use super::{ExitSessionPreparationError, ExitSessionPublication, prepare_exit_session_command};
use beryl_home_store::{
    CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization, HomeStore,
    ReconciliationFailure, ReconciliationHandle, ReconciliationResolution,
};
use beryl_model::{WindowId, WindowPlacement};
use beryl_state::SessionState;

mod candidate;

#[derive(Debug)]
#[must_use]
pub(crate) enum ExitSessionExecution {
    NotCommitted {
        evidence: CommandError,
        publication: Box<ExitSessionPublication>,
    },
    Committed {
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
        publication: Box<ExitSessionPublication>,
    },
    Indeterminate(ExitSessionReconciliation),
}

#[derive(Debug)]
#[must_use]
pub(crate) struct ExitSessionReconciliation {
    original_failure: CommandError,
    handle: ReconciliationHandle,
    publication: Box<ExitSessionPublication>,
    candidate_resolution: Option<Result<ReconciliationResolution, ReconciliationFailure>>,
}

#[derive(Debug)]
#[must_use]
pub(crate) enum ExitSessionReconciled {
    ExactOld {
        original_failure: CommandError,
        publication: Box<ExitSessionPublication>,
    },
    ExactNew {
        original_failure: CommandError,
        receipt: CommitReceipt,
        publication: Box<ExitSessionPublication>,
    },
    Pending {
        failure: ReconciliationFailure,
        reconciliation: ExitSessionReconciliation,
    },
    Blocked {
        original_failure: CommandError,
        resolution: ReconciliationResolution,
        publication: Box<ExitSessionPublication>,
    },
}

pub(crate) fn execute_exit_session(
    home: &HomeStore,
    session: &SessionState,
    placements: Vec<(WindowId, WindowPlacement)>,
) -> Result<ExitSessionExecution, ExitSessionPreparationError> {
    let prepared = prepare_exit_session_command(home, session, placements)?;
    let publication = prepared.publication;
    Ok(match home.execute(prepared.command) {
        CommandOutcome::NotCommitted { evidence } => ExitSessionExecution::NotCommitted {
            evidence,
            publication,
        },
        CommandOutcome::Committed {
            receipt,
            later_failure,
            local_finalization,
        } => ExitSessionExecution::Committed {
            publication,
            receipt,
            later_failure,
            local_finalization,
        },
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => ExitSessionExecution::Indeterminate(ExitSessionReconciliation {
            candidate_resolution: None,
            publication,
            handle: reconciliation.install_and_handle(),
            original_failure: failure,
        }),
    })
}

impl ExitSessionReconciliation {
    pub(crate) fn publication(&self) -> &ExitSessionPublication {
        &self.publication
    }

    pub(crate) fn reconcile(self, home: &HomeStore) -> ExitSessionReconciled {
        match home.reconcile(&self.handle) {
            Ok(ReconciliationResolution::ExactOld) => ExitSessionReconciled::ExactOld {
                publication: self.publication,
                original_failure: self.original_failure,
            },
            Ok(ReconciliationResolution::ExactNew { receipt }) => ExitSessionReconciled::ExactNew {
                publication: self.publication,
                original_failure: self.original_failure,
                receipt,
            },
            Ok(resolution) => ExitSessionReconciled::Blocked {
                publication: self.publication,
                original_failure: self.original_failure,
                resolution,
            },
            Err(failure) => ExitSessionReconciled::Pending {
                failure,
                reconciliation: self,
            },
        }
    }
}

impl ExitSessionExecution {
    pub(crate) fn publication(&self) -> &ExitSessionPublication {
        match self {
            Self::NotCommitted { publication, .. } | Self::Committed { publication, .. } => {
                publication
            }
            Self::Indeterminate(pending) => pending.publication(),
        }
    }
}

impl ExitSessionReconciled {
    pub(crate) fn publication(&self) -> &ExitSessionPublication {
        match self {
            Self::ExactOld { publication, .. }
            | Self::ExactNew { publication, .. }
            | Self::Blocked { publication, .. } => publication,
            Self::Pending { reconciliation, .. } => reconciliation.publication(),
        }
    }
}
