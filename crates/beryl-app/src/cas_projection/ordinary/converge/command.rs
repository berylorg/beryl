use beryl_home_store::{CommandOutcome, CurrentDomainCommand, HomeStore};

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
    match store.execute_current(command) {
        CommandOutcome::NotCommitted { evidence } => Err(
            OrdinaryTurnExecutionError::HomeCommandNotCommitted(evidence),
        ),
        CommandOutcome::Committed {
            receipt: _,
            later_failure: None,
        } => Ok(()),
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(later_failure),
        } => Err(OrdinaryTurnExecutionError::HomeCommandCommitted {
            receipt,
            later_failure,
        }),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => Err(OrdinaryTurnExecutionError::HomeCommandIndeterminate {
            failure,
            reconciliation,
        }),
    }
}
