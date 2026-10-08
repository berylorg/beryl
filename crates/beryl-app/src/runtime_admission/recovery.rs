use super::*;
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeGeneration};

#[derive(Clone, Debug)]
pub(crate) struct FirstConversationFacts {
    home: BerylHomeId,
    generation: HomeGeneration,
    window: SessionWindowRecord,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
    runtime: RuntimeId,
    root: RootId,
}

impl FirstConversationFacts {
    pub(crate) fn window(&self) -> &SessionWindowRecord {
        &self.window
    }
    pub(crate) fn thread_id(&self) -> SyndicThreadId {
        self.thread
    }
    pub(crate) fn draft_id(&self) -> SyndicDraftId {
        self.draft
    }
    pub(crate) fn runtime_id(&self) -> RuntimeId {
        self.runtime
    }
    pub(crate) fn root_id(&self) -> RootId {
        self.root
    }
    pub(crate) fn home_id(&self) -> BerylHomeId {
        self.home
    }
    pub(crate) fn home_generation(&self) -> HomeGeneration {
        self.generation
    }
}

pub(crate) struct FirstConversationAdmissionRecovery {
    facts: FirstConversationFacts,
    outcome: RecoveryOutcome,
}

enum RecoveryOutcome {
    Committed {
        _receipt: CommitReceipt,
        _later_failure: Option<CommandError>,
        _local: Option<CommittedLocalFinalization>,
    },
    Pending {
        handle: ReconciliationHandle,
        _failure: CommandError,
    },
    NotCommitted,
    Unavailable {
        _handle: ReconciliationHandle,
    },
}

impl std::fmt::Debug for FirstConversationAdmissionRecovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FirstConversationAdmissionRecovery")
            .field("facts", &self.facts)
            .finish_non_exhaustive()
    }
}

impl RuntimeAdmissionOutcome {
    pub(crate) fn first_conversation_facts(&self) -> Option<FirstConversationFacts> {
        let admission = match self {
            Self::Committed { admission, .. } => admission,
            Self::Indeterminate { reconciliation, .. } => &reconciliation.admission,
            _ => return None,
        };
        let onboarding = admission.facts.onboarding()?;
        Some(FirstConversationFacts {
            home: admission.store.home_id(),
            generation: admission.store.health().generation()?,
            window: onboarding.window().clone(),
            thread: onboarding.thread_id(),
            draft: onboarding.draft_id(),
            runtime: admission.facts.runtime_id(),
            root: admission.facts.root_id(),
        })
    }

    pub(crate) fn into_first_conversation_recovery(
        self,
    ) -> Result<FirstConversationAdmissionRecovery, Self> {
        let Some(facts) = self.first_conversation_facts() else {
            return Err(self);
        };
        let outcome = match self {
            Self::Committed {
                receipt,
                later_failure,
                local_finalization,
                ..
            } => RecoveryOutcome::Committed {
                _receipt: receipt,
                _later_failure: later_failure,
                _local: local_finalization,
            },
            Self::Indeterminate {
                failure,
                reconciliation,
            } => RecoveryOutcome::Pending {
                handle: reconciliation.handle,
                _failure: failure,
            },
            _ => unreachable!(),
        };
        Ok(FirstConversationAdmissionRecovery { facts, outcome })
    }
}

impl FirstConversationAdmissionRecovery {
    pub(crate) fn facts(&self) -> &FirstConversationFacts {
        &self.facts
    }
    pub(crate) fn committed(&self) -> bool {
        matches!(self.outcome, RecoveryOutcome::Committed { .. })
    }
    pub(crate) fn settle(
        &mut self,
        access: &HomeCandidateRecoveryAccess<'_>,
    ) -> Result<bool, String> {
        if access.home_id() != self.facts.home || access.generation() == self.facts.generation {
            return Err("first conversation admission belongs to another home candidate".into());
        }
        let RecoveryOutcome::Pending { handle, .. } = &self.outcome else {
            return match self.outcome {
                RecoveryOutcome::Committed { .. } => Ok(true),
                RecoveryOutcome::NotCommitted => Ok(false),
                _ => {
                    Err("first conversation admission retains terminal unavailable custody".into())
                }
            };
        };
        let handle = handle.clone();
        let result = access
            .retry_reconciliation(&handle)
            .map_err(|e| e.to_string())?;
        self.outcome = match result {
            ReconciliationResolution::ExactOld => RecoveryOutcome::NotCommitted,
            ReconciliationResolution::ExactNew { receipt } => RecoveryOutcome::Committed {
                _receipt: receipt,
                _later_failure: None,
                _local: None,
            },
            ReconciliationResolution::ExactSuccessor { .. }
            | ReconciliationResolution::Collision => {
                RecoveryOutcome::Unavailable { _handle: handle }
            }
        };
        self.settle(access)
    }
}
