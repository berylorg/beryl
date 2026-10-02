use super::*;

pub(super) fn open_with_faults(
    path: &std::path::Path,
    faults: &FaultController,
) -> (HomeStore, BerylState) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    (home, state)
}

#[test]
fn removal_and_recovery_preserve_noncommit_postcommit_and_indeterminate_custody() {
    for recovery_fault in [false, true] {
        for point in [
            FaultPoint::BeforeCommit,
            FaultPoint::AfterPersist,
            FaultPoint::AfterCommitBeforePersist,
        ] {
            let directory = tempfile::tempdir().unwrap();
            let faults = FaultController::new();
            let (home, state) = open_with_faults(directory.path(), &faults);
            let session = state.session();
            seed(&home, &session, 3);
            let before = snapshot(&home, &session);
            let evidence = session
                .capture_window_removal(&home, before.windows()[0].window_id())
                .unwrap();
            if !recovery_fault {
                faults.fail_next(point);
            }
            let removal = remove(&home, &session, &evidence);
            let mut reconciliation = None;
            if !recovery_fault {
                match (point, removal) {
                    (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { .. }) => {}
                    (
                        FaultPoint::AfterPersist,
                        CommandOutcome::Committed {
                            later_failure: Some(_),
                            local_finalization: Some(_),
                            ..
                        },
                    ) => {}
                    (
                        FaultPoint::AfterCommitBeforePersist,
                        CommandOutcome::Indeterminate {
                            reconciliation: pending,
                            ..
                        },
                    ) => {
                        reconciliation = Some(pending.install_and_handle());
                    }
                    (_, outcome) => panic!("unexpected removal outcome: {outcome:?}"),
                }
            } else {
                committed(removal);
            }
            let mut candidate = recover(home);
            let fresh = BerylState::reacquire_candidate(&candidate)
                .unwrap()
                .session();
            let access = candidate.recovery_access().unwrap();
            if let Some(handle) = reconciliation {
                assert!(matches!(
                    access.reconcile(&handle).unwrap(),
                    ReconciliationResolution::ExactNew { .. }
                ));
            }
            if !recovery_fault && point == FaultPoint::BeforeCommit {
                assert_eq!(
                    fresh
                        .classify_window_removal_candidate(&access, &evidence)
                        .unwrap(),
                    SessionWindowRemovalState::Original
                );
                assert_same(
                    &before,
                    &fresh.minimal_bootstrap_candidate(&access).unwrap().unwrap(),
                );
            } else {
                let contribution = recovery_contribution(&access, &fresh, &evidence);
                if recovery_fault {
                    faults.fail_next(point);
                }
                let outcome = execute(&access, contribution);
                let mut recovery_reconciliation = None;
                match (recovery_fault, point, outcome) {
                    (true, FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { .. }) => {}
                    (
                        true,
                        FaultPoint::AfterPersist,
                        CommandOutcome::Committed {
                            later_failure: Some(_),
                            local_finalization: Some(_),
                            ..
                        },
                    ) => {}
                    (
                        true,
                        FaultPoint::AfterCommitBeforePersist,
                        CommandOutcome::Indeterminate { reconciliation, .. },
                    ) => {
                        recovery_reconciliation = Some(reconciliation.install_and_handle());
                    }
                    (false, _, outcome) => committed(outcome),
                    (_, _, outcome) => panic!("unexpected recovery outcome: {outcome:?}"),
                }
                drop(access);
                let home = candidate.abort();
                candidate = home.recover_same_home().unwrap();
                let fresh = BerylState::reacquire_candidate(&candidate)
                    .unwrap()
                    .session();
                let access = candidate.recovery_access().unwrap();
                if recovery_fault && point == FaultPoint::BeforeCommit {
                    assert_eq!(
                        fresh
                            .classify_window_removal_candidate(&access, &evidence)
                            .unwrap(),
                        SessionWindowRemovalState::Removed
                    );
                    committed(execute(
                        &access,
                        recovery_contribution(&access, &fresh, &evidence),
                    ));
                }
                if let Some(handle) = recovery_reconciliation {
                    faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
                    assert!(access.reconcile(&handle).is_err());
                    assert_eq!(access.pending_reconciliations().len(), 1);
                    drop(access);
                    let home = candidate.abort();
                    candidate = home.recover_same_home().unwrap();
                    let fresh = BerylState::reacquire_candidate(&candidate)
                        .unwrap()
                        .session();
                    let access = candidate.recovery_access().unwrap();
                    assert!(matches!(
                        access.retry_reconciliation(&handle).unwrap(),
                        ReconciliationResolution::ExactNew { .. }
                    ));
                    assert!(access.pending_reconciliations().is_empty());
                    assert_eq!(
                        fresh
                            .classify_window_removal_candidate(&access, &evidence)
                            .unwrap(),
                        SessionWindowRemovalState::Recovered
                    );
                    assert_recovered(
                        &before,
                        &fresh.minimal_bootstrap_candidate(&access).unwrap().unwrap(),
                        &evidence,
                    );
                    drop(access);
                    candidate.publish().unwrap().close().unwrap();
                    continue;
                }
                assert_eq!(
                    fresh
                        .classify_window_removal_candidate(&access, &evidence)
                        .unwrap(),
                    SessionWindowRemovalState::Recovered
                );
                assert_recovered(
                    &before,
                    &fresh.minimal_bootstrap_candidate(&access).unwrap().unwrap(),
                    &evidence,
                );
                drop(access);
                candidate.publish().unwrap().close().unwrap();
                continue;
            }
            drop(access);
            candidate.publish().unwrap().close().unwrap();
        }
    }
}

#[test]
fn failed_capture_confirmation_and_candidate_reopening_return_no_recovery_authority() {
    {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let (home, state) = open_with_faults(directory.path(), &faults);
        seed(&home, &state.session(), 1);
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(
            state
                .session()
                .capture_window_removal(&home, WindowId::from_bytes([0; 16]))
                .is_err()
        );
        home.close().unwrap();
    }
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (home, state) = open_with_faults(directory.path(), &faults);
    let session = state.session();
    seed(&home, &session, 1);
    let evidence = session
        .capture_window_removal(&home, WindowId::from_bytes([0; 16]))
        .unwrap();
    committed(remove(&home, &session, &evidence));
    home.inject_retained_maintenance_terminal();
    assert!(BerylState::reacquire(&home).is_err());
    faults.fail_next(FaultPoint::BeforeReopen);
    let failure = home.recover_same_home().unwrap_err();
    let home = failure.into_store();
    let mut candidate = home.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&candidate)
        .unwrap()
        .session();
    let access = candidate.recovery_access().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        fresh
            .classify_window_removal_candidate(&access, &evidence)
            .is_err()
    );
    drop(access);
    let home = candidate.abort();
    let mut candidate = home.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&candidate)
        .unwrap()
        .session();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        fresh
            .classify_window_removal_candidate(&access, &evidence)
            .unwrap(),
        SessionWindowRemovalState::Removed
    );
    drop(access);
    candidate.publish().unwrap().close().unwrap();
}
