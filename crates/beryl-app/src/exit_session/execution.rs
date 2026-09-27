use super::{ExitSessionPreparationError, prepare_exit_session_command};
use beryl_home_store::{
    CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization, HomeStore,
    ReconciliationFailure, ReconciliationHandle, ReconciliationResolution,
};
use beryl_model::{WindowId, WindowPlacement};
use beryl_state::SessionState;

#[derive(Debug)]
#[must_use]
pub(crate) enum ExitSessionExecution {
    NotCommitted {
        evidence: CommandError,
    },
    Committed {
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
    },
    Indeterminate(ExitSessionReconciliation),
}

#[derive(Debug)]
#[must_use]
pub(crate) struct ExitSessionReconciliation {
    original_failure: CommandError,
    handle: ReconciliationHandle,
}

#[derive(Debug)]
#[must_use]
pub(crate) enum ExitSessionReconciled {
    ExactOld {
        original_failure: CommandError,
    },
    ExactNew {
        original_failure: CommandError,
        receipt: CommitReceipt,
    },
    Pending {
        failure: ReconciliationFailure,
        reconciliation: ExitSessionReconciliation,
    },
    Blocked {
        original_failure: CommandError,
        resolution: ReconciliationResolution,
    },
}

pub(crate) fn execute_exit_session(
    home: &HomeStore,
    session: &SessionState,
    placements: Vec<(WindowId, WindowPlacement)>,
) -> Result<ExitSessionExecution, ExitSessionPreparationError> {
    let command = prepare_exit_session_command(home, session, placements)?;
    Ok(match home.execute(command) {
        CommandOutcome::NotCommitted { evidence } => {
            ExitSessionExecution::NotCommitted { evidence }
        }
        CommandOutcome::Committed {
            receipt,
            later_failure,
            local_finalization,
        } => ExitSessionExecution::Committed {
            receipt,
            later_failure,
            local_finalization,
        },
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => ExitSessionExecution::Indeterminate(ExitSessionReconciliation {
            handle: reconciliation.install_and_handle(),
            original_failure: failure,
        }),
    })
}

impl ExitSessionReconciliation {
    pub(crate) fn reconcile(self, home: &HomeStore) -> ExitSessionReconciled {
        match home.reconcile(&self.handle) {
            Ok(ReconciliationResolution::ExactOld) => ExitSessionReconciled::ExactOld {
                original_failure: self.original_failure,
            },
            Ok(ReconciliationResolution::ExactNew { receipt }) => ExitSessionReconciled::ExactNew {
                original_failure: self.original_failure,
                receipt,
            },
            Ok(resolution) => ExitSessionReconciled::Blocked {
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
