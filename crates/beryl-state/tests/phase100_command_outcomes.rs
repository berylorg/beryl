mod support;

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, HomeCommand,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::tempdir;

#[test]
fn state_commands_preserve_exact_outcomes_and_project_only_committed_receipts() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = beryl_home_store::HomeStore::open_with_faults(
        beryl_home_store::HomeOpenOptions::new(
            directory.path(),
            beryl_home_store::HomeSchemaVersion::CURRENT,
        ),
        faults.clone(),
    )
    .unwrap();
    let state = beryl_state::BerylState::register(&mut store).unwrap();

    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let mut cancelled =
        HomeCommand::new(store.home_revision().unwrap()).with_cancellation(cancellation);
    cancelled
        .add(state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&store).unwrap(),
            support::host_runtime(1, 1, r"C:\Codex\cancelled.exe", r"C:\cancelled"),
        ))
        .unwrap();
    match store.execute(cancelled) {
        CommandOutcome::NotCommitted {
            evidence: CommandError::CancelledBeforeAdmission,
        } => {}
        outcome => panic!("expected pre-admission cancellation, got {outcome:?}"),
    }

    let expected = state.runtime_roots().revision(&store).unwrap();
    let mut committed = HomeCommand::new(store.home_revision().unwrap());
    committed
        .add(state.runtime_roots().create_runtime_with_home_root(
            expected,
            support::host_runtime(2, 2, r"C:\Codex\committed.exe", r"C:\committed"),
        ))
        .unwrap();
    match store.execute(committed) {
        CommandOutcome::Committed {
            receipt,
            later_failure,
            local_finalization: _,
        } => {
            assert!(later_failure.is_none());
            assert_eq!(
                state
                    .runtime_roots()
                    .committed_revision(&store, &receipt)
                    .unwrap(),
                Some(expected.checked_next().unwrap()),
            );
        }
        outcome => panic!("expected committed command, got {outcome:?}"),
    }

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let mut indeterminate = HomeCommand::new(store.home_revision().unwrap());
    indeterminate
        .add(state.runtime_roots().create_runtime_with_home_root(
            state.runtime_roots().revision(&store).unwrap(),
            support::host_runtime(3, 3, r"C:\Codex\indeterminate.exe", r"C:\indeterminate"),
        ))
        .unwrap();
    match store.execute(indeterminate) {
        CommandOutcome::Indeterminate {
            failure: CommandError::Persistence { .. },
            reconciliation: _reconciliation,
        } => {}
        outcome => panic!("expected indeterminate command, got {outcome:?}"),
    }
}
