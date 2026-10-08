use super::*;
use beryl_home_store::{
    CommandOutcome, HomeOpenOptions, HomeSchemaVersion, ReconciliationResolution,
    test_faults::{FaultController, FaultPoint, FaultScope},
};
use std::{sync::Arc, thread, time::Duration};

fn fixture(path: &std::path::Path, faults: &FaultController) -> Fixture {
    Fixture::publish(
        HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap(),
    )
}

#[test]
fn queued_joined_command_captures_both_revisions_after_prior_writer() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = Arc::new(fixture(directory.path(), &faults));
    let first_cut = faults.block_next(FaultPoint::BeforeCommit);
    let command = fixture.command();
    thread::scope(|scope| {
        let first_fixture = Arc::clone(&fixture);
        let first = scope.spawn(move || {
            committed(
                first_fixture.store.execute_current(
                    first_fixture
                        .beta
                        .current_command(PutBytes::<BetaDomain>::new(3, b"prior".to_vec())),
                ),
            )
        });
        assert!(first_cut.wait_until_reached(Duration::from_secs(10)));
        let next_fixture = Arc::clone(&fixture);
        let next = scope.spawn(move || next_fixture.store.execute_current_home(command));
        first_cut.release();
        first.join().unwrap();
        let receipt = committed(next.join().unwrap());
        assert_eq!(receipt.home_revision().get(), 3);
        assert_eq!(
            fixture
                .store
                .receipt_domain_revision(&receipt, &fixture.alpha)
                .unwrap()
                .unwrap()
                .get(),
            2
        );
        assert_eq!(
            fixture
                .store
                .receipt_domain_revision(&receipt, &fixture.beta)
                .unwrap()
                .unwrap()
                .get(),
            3
        );
    });
    assert_eq!(
        read(&fixture.store, &fixture.alpha, 1),
        Some(b"alpha".to_vec())
    );
    assert_eq!(
        read(&fixture.store, &fixture.beta, 2),
        Some(b"beta".to_vec())
    );
}

#[test]
fn cancellation_after_writer_admission_does_not_hide_committed_join() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = Arc::new(fixture(directory.path(), &faults));
    let cancellation = CommandCancellation::new();
    let command = fixture.command().with_cancellation(cancellation.clone());
    let cut = faults.block_next(FaultPoint::BeforeCommit);
    thread::scope(|scope| {
        let writer = scope.spawn(|| fixture.store.execute_current_home(command));
        assert!(cut.wait_until_reached(Duration::from_secs(10)));
        cancellation.cancel();
        cut.release();
        committed(writer.join().unwrap());
    });
    assert_eq!(
        read(&fixture.store, &fixture.alpha, 1),
        Some(b"alpha".to_vec())
    );
    assert_eq!(
        read(&fixture.store, &fixture.beta, 2),
        Some(b"beta".to_vec())
    );
}

#[test]
fn primary_typed_fault_scope_is_preserved_for_joined_command() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = fixture(directory.path(), &faults);
    faults.fail_next_in_scope(FaultPoint::BeforeCommit, FaultScope::of::<PutIfMissing>());
    committed(fixture.store.execute_current_home(fixture.command()));
    let mut command =
        CurrentHomeCommand::new(fixture.alpha.current_command(PutIfMissing { key: 9 }));
    command
        .add(
            fixture
                .beta
                .current_command(PutBytes::<BetaDomain>::new(8, b"secondary".to_vec())),
        )
        .unwrap();
    assert!(matches!(
        not_committed(fixture.store.execute_current_home(command)),
        CommandError::Commit { .. }
    ));
    let candidate = fixture.store.recover_same_home().unwrap();
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let beta = candidate.domain_handle::<BetaDomain>().unwrap();
    let store = candidate.publish().unwrap();
    assert_eq!(read(&store, &alpha, 9), None);
    assert_eq!(read(&store, &beta, 8), None);

    let mut stale_secondary = CurrentHomeCommand::new(
        alpha.current_command(PutBytes::<AlphaDomain>::new(9, b"fresh".to_vec())),
    );
    stale_secondary
        .add(
            fixture
                .beta
                .current_command(PutBytes::<BetaDomain>::new(10, b"stale".to_vec())),
        )
        .unwrap();
    assert!(matches!(
        not_committed(store.execute_current_home(stale_secondary)),
        CommandError::ForeignDomain { .. }
    ));
    assert_eq!(read(&store, &alpha, 9), None);
}

#[test]
fn indeterminate_join_reconciles_original_receipt_for_both_domains() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = fixture(directory.path(), &faults);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match fixture.store.execute_current_home(fixture.command()) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected original indeterminate custody: {other:?}"),
    };
    let receipt = match fixture.store.reconcile(&handle).unwrap() {
        ReconciliationResolution::ExactNew { receipt } => receipt,
        other => panic!("expected exact joined new state: {other:?}"),
    };
    assert_eq!(
        fixture
            .store
            .receipt_domain_revision(&receipt, &fixture.alpha)
            .unwrap()
            .unwrap()
            .get(),
        2
    );
    assert_eq!(
        fixture
            .store
            .receipt_domain_revision(&receipt, &fixture.beta)
            .unwrap()
            .unwrap()
            .get(),
        2
    );
    assert!(fixture.store.pending_reconciliations().is_empty());
    assert_eq!(
        read(&fixture.store, &fixture.alpha, 1),
        Some(b"alpha".to_vec())
    );
    assert_eq!(
        read(&fixture.store, &fixture.beta, 2),
        Some(b"beta".to_vec())
    );
}

#[test]
fn journal_failure_reconciles_exact_old_join_after_generation_recovery() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let fixture = fixture(directory.path(), &faults);
    let journal_fault = beryl_home_store::test_faults::fail_next_journal_write();
    let handle = match fixture.store.execute_current_home(fixture.command()) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected original journal ambiguity: {other:?}"),
    };
    drop(journal_fault);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    let candidate = fixture.store.recover_same_home().unwrap();
    let failure = candidate.publish().unwrap_err();
    assert!(matches!(
        failure.error(),
        beryl_home_store::HomeCandidateError::PendingReconciliation { count: 1 }
    ));
    let mut candidate = failure.into_parts().1;
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .reconcile(&handle)
            .unwrap(),
        ReconciliationResolution::ExactOld
    );
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let beta = candidate.domain_handle::<BetaDomain>().unwrap();
    let store = candidate.publish().unwrap();
    assert_eq!(read(&store, &alpha, 1), None);
    assert_eq!(read(&store, &beta, 2), None);
    assert!(store.pending_reconciliations().is_empty());
    let mut stale = CurrentHomeCommand::new(
        fixture
            .alpha
            .current_command(PutBytes::<AlphaDomain>::new(7, b"stale".to_vec())),
    );
    stale
        .add(beta.current_command(PutBytes::<BetaDomain>::new(8, b"fresh".to_vec())))
        .unwrap();
    assert!(matches!(
        not_committed(store.execute_current_home(stale)),
        CommandError::ForeignDomain { .. }
    ));
    assert_eq!(read(&store, &beta, 8), None);
}
