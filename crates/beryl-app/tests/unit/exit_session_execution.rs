use beryl_home_store::{
    ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};

#[test]
fn exit_session_execution_commits_complete_set_once() {
    let (_directory, home, session) = open(3);
    let before = session.revision(&home).unwrap();
    let result = execute_exit_session(&home, &session, placements(3)).unwrap();
    let ExitSessionExecution::Committed {
        receipt,
        later_failure,
        local_finalization,
    } = result
    else {
        panic!("{result:?}");
    };
    assert!(later_failure.is_none());
    assert!(local_finalization.is_none());
    assert_ne!(session.revision(&home).unwrap(), before);
    assert_eq!(
        session.committed_revision(&home, &receipt).unwrap(),
        Some(session.revision(&home).unwrap())
    );
    let snapshot = session.minimal_bootstrap(&home).unwrap().unwrap();
    assert_eq!(
        snapshot.header().exit_intent(),
        SessionExitIntent::OrderlyExit
    );
    for record in snapshot.windows() {
        let expected = placements(3)
            .into_iter()
            .find(|(id, _)| *id == record.window_id())
            .unwrap()
            .1;
        assert_eq!(record.placement(), &expected);
    }
    let revision = home.home_revision().unwrap();
    assert!(matches!(
        execute_exit_session(&home, &session, placements(3)),
        Err(ExitSessionPreparationError::NotRunning)
    ));
    assert_eq!(home.home_revision().unwrap(), revision);
    home.close().unwrap();
}

#[test]
fn exit_session_execution_preserves_definitive_noncommit() {
    let faults = FaultController::new();
    let (directory, home, session) = open_with_faults(1, faults.clone());
    let before = format!("{:?}", session.minimal_bootstrap(&home).unwrap());
    let revision = home.home_revision().unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    let result = execute_exit_session(&home, &session, placements(1)).unwrap();
    assert!(
        matches!(result, ExitSessionExecution::NotCommitted { .. }),
        "{result:?}"
    );
    assert!(home.pending_reconciliations().is_empty());
    assert!(home.home_revision().is_err());
    home.close().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let session = state.session();
    assert_eq!(home.home_revision().unwrap(), revision);
    assert_eq!(
        format!("{:?}", session.minimal_bootstrap(&home).unwrap()),
        before
    );
    home.close().unwrap();
}

#[test]
fn exit_session_execution_preserves_postcommit_failure_and_capability() {
    for kind in [std::io::ErrorKind::Other, std::io::ErrorKind::StorageFull] {
        let faults = FaultController::new();
        let (directory, home, session) = open_with_faults(1, faults.clone());
        faults.fail_next_with_kind(FaultPoint::AfterPersist, kind);
        let result = execute_exit_session(&home, &session, placements(1)).unwrap();
        let ExitSessionExecution::Committed {
            later_failure,
            local_finalization,
            ..
        } = result
        else {
            panic!("{result:?}");
        };
        assert!(later_failure.is_some());
        assert!(local_finalization.is_some());
        assert!(home.pending_reconciliations().is_empty());
        drop(local_finalization);
        home.close().unwrap();
        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let state = BerylState::register(&mut candidate).unwrap();
        let home = candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(
            state
                .session()
                .minimal_bootstrap(&home)
                .unwrap()
                .unwrap()
                .header()
                .exit_intent(),
            SessionExitIntent::OrderlyExit
        );
        home.close().unwrap();
    }
}

#[test]
fn exit_session_execution_installs_ambiguity_and_retains_failed_reconciliation() {
    let faults = FaultController::new();
    let (_directory, home, session) = open_with_faults(3, faults.clone());
    let (_foreign_directory, foreign, _) = open(1);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let result = execute_exit_session(&home, &session, placements(3)).unwrap();
    let ExitSessionExecution::Indeterminate(reconciliation) = result else {
        panic!("{result:?}");
    };
    assert_eq!(home.pending_reconciliations().len(), 1);
    let result = reconciliation.reconcile(&foreign);
    let ExitSessionReconciled::Pending { reconciliation, .. } = result else {
        panic!("{result:?}");
    };
    assert_eq!(home.pending_reconciliations().len(), 1);
    assert!(foreign.pending_reconciliations().is_empty());
    faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
    let result = reconciliation.reconcile(&home);
    let ExitSessionReconciled::Pending {
        reconciliation,
        failure,
    } = result
    else {
        panic!("{result:?}");
    };
    let failure = failure.to_string();
    let result = reconciliation.reconcile(&home);
    let ExitSessionReconciled::Pending {
        reconciliation,
        failure: memoized,
    } = result
    else {
        panic!("{result:?}");
    };
    assert_eq!(memoized.to_string(), failure);
    let handle = home.pending_reconciliations().pop().unwrap();
    assert!(matches!(
        home.retry_reconciliation(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    let result = reconciliation.reconcile(&home);
    let ExitSessionReconciled::ExactNew {
        receipt,
        original_failure,
    } = result
    else {
        panic!("{result:?}");
    };
    assert!(!original_failure.to_string().is_empty());
    assert_eq!(
        session.committed_revision(&home, &receipt).unwrap(),
        Some(session.revision(&home).unwrap())
    );
    assert!(home.pending_reconciliations().is_empty());
    assert_eq!(
        session
            .minimal_bootstrap(&home)
            .unwrap()
            .unwrap()
            .header()
            .exit_intent(),
        SessionExitIntent::OrderlyExit
    );
    foreign.close().unwrap();
    home.close().unwrap();
}

#[test]
fn exit_session_execution_abandonment_keeps_installed_reconciliation() {
    let faults = FaultController::new();
    let (_directory, home, session) = open_with_faults(1, faults.clone());
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let result = execute_exit_session(&home, &session, placements(1)).unwrap();
    assert!(matches!(result, ExitSessionExecution::Indeterminate(_)));
    drop(result);
    let mut pending = home.pending_reconciliations();
    assert_eq!(pending.len(), 1);
    assert!(matches!(
        home.retry_reconciliation(&pending.pop().unwrap()).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert!(home.pending_reconciliations().is_empty());
    home.close().unwrap();
}
