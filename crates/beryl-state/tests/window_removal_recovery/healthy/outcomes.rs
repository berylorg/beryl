use super::*;

#[test]
fn healthy_restoration_preserves_separate_noncommit_commit_and_indeterminate_outcomes() {
    for point in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterPersist,
        FaultPoint::AfterCommitBeforePersist,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let (home, state) = super::super::outcomes::open_with_faults(directory.path(), &faults);
        let session = state.session();
        seed(&home, &session, 3);
        let before = snapshot(&home, &session);
        let evidence = session
            .capture_window_removal(&home, before.windows()[0].window_id())
            .unwrap();
        let removal = remove(&home, &session, &evidence);
        let CommandOutcome::Committed {
            receipt: removal_receipt,
            later_failure: None,
            ..
        } = removal
        else {
            panic!("unexpected removal outcome: {removal:?}");
        };
        let restore = contribution(&home, &session, &evidence);
        faults.fail_next(point);
        let restored = support::execute(&home, restore);
        let mut reconciliation = None;
        match (point, restored) {
            (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { .. }) => {}
            (
                FaultPoint::AfterPersist,
                CommandOutcome::Committed {
                    receipt,
                    later_failure: Some(_),
                    local_finalization: Some(_),
                    ..
                },
            ) => {
                assert_eq!(
                    receipt.home_revision(),
                    removal_receipt.home_revision().checked_next().unwrap()
                );
            }
            (
                FaultPoint::AfterCommitBeforePersist,
                CommandOutcome::Indeterminate {
                    reconciliation: pending,
                    ..
                },
            ) => {
                reconciliation = Some(pending.install_and_handle());
            }
            (_, outcome) => panic!("unexpected healthy restoration outcome: {outcome:?}"),
        }
        if home.health().state() == beryl_home_store::HomeHealthState::Healthy {
            assert_eq!(
                session.classify_window_removal(&home, &evidence).unwrap(),
                if point == FaultPoint::BeforeCommit {
                    SessionWindowRemovalState::Removed
                } else {
                    SessionWindowRemovalState::Recovered
                }
            );
        } else {
            assert!(session.classify_window_removal(&home, &evidence).is_err());
        }
        let mut candidate = recover(home);
        let fresh = BerylState::reacquire_candidate(&candidate)
            .unwrap()
            .session();
        let access = candidate.recovery_access().unwrap();
        if let Some(handle) = reconciliation {
            assert_eq!(access.pending_reconciliations().len(), 1);
            assert!(matches!(
                access.reconcile(&handle).unwrap(),
                ReconciliationResolution::ExactNew { .. }
            ));
            assert!(access.pending_reconciliations().is_empty());
        }
        assert_eq!(
            fresh
                .classify_window_removal_candidate(&access, &evidence)
                .unwrap(),
            if point == FaultPoint::BeforeCommit {
                SessionWindowRemovalState::Removed
            } else {
                SessionWindowRemovalState::Recovered
            }
        );
        if point == FaultPoint::BeforeCommit {
            committed(execute(
                &access,
                recovery_contribution(&access, &fresh, &evidence),
            ));
        }
        assert_recovered(
            &before,
            &fresh.minimal_bootstrap_candidate(&access).unwrap().unwrap(),
            &evidence,
        );
        assert!(
            fresh
                .recover_removed_window_candidate(
                    &access,
                    fresh.revision_candidate(&access).unwrap(),
                    &evidence
                )
                .is_err()
        );
        drop(access);
        candidate.publish().unwrap().close().unwrap();
    }
}

#[test]
fn healthy_classification_read_failure_produces_no_restoration_authority() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (home, state) = super::super::outcomes::open_with_faults(directory.path(), &faults);
    let session = state.session();
    seed(&home, &session, 3);
    let evidence = session
        .capture_window_removal(&home, WindowId::from_bytes([0; 16]))
        .unwrap();
    committed(remove(&home, &session, &evidence));
    let revision = session.revision(&home).unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        session
            .recover_removed_window(&home, revision, &evidence)
            .is_err()
    );
    let mut candidate = recover(home);
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
