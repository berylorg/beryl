use beryl_home_store::{CommandOutcome, CommitReceipt, CurrentDomainCommand, HomeStore};

use crate::cas_projection::OrdinaryTurnExecutionError;

/// Dispatches one exact-record-fenced contribution after writer admission captures its physical
/// revisions.
///
/// Every execute failure is returned so the caller can reconcile its exact mutation before
/// deciding whether to surface it.
pub(super) fn dispatch(
    store: &HomeStore,
    command: CurrentDomainCommand,
) -> Result<(), OrdinaryTurnExecutionError> {
    dispatch_with_receipt(store, command).map(|_| ())
}

pub(super) fn dispatch_with_receipt(
    store: &HomeStore,
    command: CurrentDomainCommand,
) -> Result<CommitReceipt, OrdinaryTurnExecutionError> {
    match store.execute_current(command) {
        CommandOutcome::NotCommitted { evidence } => Err(
            OrdinaryTurnExecutionError::HomeCommandNotCommitted(evidence),
        ),
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
            local_finalization: _,
        } => Ok(receipt),
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(later_failure),
            local_finalization: _,
        } => Err(OrdinaryTurnExecutionError::HomeCommandCommitted {
            receipt,
            later_failure,
        }),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            Err(OrdinaryTurnExecutionError::HomeCommandIndeterminate { failure })
        }
    }
}
