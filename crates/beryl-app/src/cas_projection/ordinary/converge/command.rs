use beryl_home_store::{CommandError, CurrentDomainCommand, HomeStore};

/// Dispatches one exact-record-fenced contribution after writer admission captures its physical
/// revisions.
///
/// Every execute failure is returned so the caller can reconcile its exact mutation before
/// deciding whether to surface it.
pub(super) fn dispatch(
    store: &HomeStore,
    command: CurrentDomainCommand,
) -> Result<(), CommandError> {
    store.execute_current(command).map(|_| ())
}
