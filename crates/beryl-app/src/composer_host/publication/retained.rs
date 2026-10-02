use super::*;
use beryl_home_store::{
    CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization,
    HomeCandidateRecoveryAccess, ReconciliationFailure, ReconciliationResolution,
};

#[derive(Debug)]
pub(crate) enum RetainedComposerCommandOutcome {
    NotCommitted(CommandError),
    Committed {
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
    },
    Indeterminate {
        failure: CommandError,
        handle: ReconciliationHandle,
        result: Option<Result<ReconciliationResolution, ReconciliationFailure>>,
    },
}

impl RetainedComposerCommandOutcome {
    pub(crate) fn new(outcome: CommandOutcome) -> Self {
        match outcome {
            CommandOutcome::NotCommitted { evidence } => Self::NotCommitted(evidence),
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => Self::Committed {
                receipt,
                later_failure,
                local_finalization,
            },
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => Self::Indeterminate {
                failure,
                handle: reconciliation.install_and_handle(),
                result: None,
            },
        }
    }

    pub(crate) fn known_commit(&self) -> Option<bool> {
        match self {
            Self::NotCommitted(_) => Some(false),
            Self::Committed { .. } => Some(true),
            Self::Indeterminate {
                result: Some(Ok(ReconciliationResolution::ExactOld)),
                ..
            } => Some(false),
            Self::Indeterminate {
                result: Some(Ok(ReconciliationResolution::ExactNew { .. })),
                ..
            } => Some(true),
            _ => None,
        }
    }

    pub(crate) fn reconcile(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
    ) -> Result<bool, String> {
        if let Self::Indeterminate { handle, result, .. } = self {
            let settled = match result {
                None => access.reconcile(handle),
                Some(Err(_)) => access.retry_reconciliation(handle),
                Some(Ok(_)) => {
                    return self
                        .known_commit()
                        .ok_or_else(|| "composer publication reconciliation is not exact".into());
                }
            };
            *result = Some(settled);
        }
        self.known_commit()
            .ok_or_else(|| "composer publication outcome is unresolved".into())
    }

    pub(crate) fn committed_classification(&self) -> Option<CommandOutcome> {
        match self {
            Self::Committed { receipt, .. } => Some(CommandOutcome::Committed {
                receipt: receipt.clone(),
                later_failure: None,
                local_finalization: None,
            }),
            Self::Indeterminate {
                result: Some(Ok(ReconciliationResolution::ExactNew { receipt })),
                ..
            } => Some(CommandOutcome::Committed {
                receipt: receipt.clone(),
                later_failure: None,
                local_finalization: None,
            }),
            _ => None,
        }
    }

    pub(crate) fn noncommitted_classification(&self) -> Option<CommandOutcome> {
        if self.known_commit() != Some(false) {
            return None;
        }
        Some(CommandOutcome::NotCommitted {
            evidence: if matches!(
                self,
                Self::NotCommitted(CommandError::CancelledBeforeAdmission)
            ) {
                CommandError::CancelledBeforeAdmission
            } else {
                CommandError::ReentrantWriter
            },
        })
    }
}

pub(in crate::composer_host) struct RetainedComposerPublication {
    pub(in crate::composer_host) binding: ComposerHostBinding,
    pub(in crate::composer_host) prepared: PreparedPublication,
    pub(in crate::composer_host) outcome: RetainedComposerCommandOutcome,
}

impl SyndicComposerHost {
    pub(in crate::composer_host) fn retain_publication_command(
        &mut self,
        binding: ComposerHostBinding,
        prepared: PreparedPublication,
        outcome: CommandOutcome,
    ) {
        self.publication.retained = Some(Box::new(RetainedComposerPublication {
            binding,
            prepared,
            outcome: RetainedComposerCommandOutcome::new(outcome),
        }));
    }
}
