use super::*;
use beryl_app::cas_projection::{
    OrdinaryTurnExecutionError, OrdinaryTurnExecutionFailure,
    test_faults::{TerminalHistoryBarrierStage, install_terminal_history_barrier},
};
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use syndic_storage::TurnLifecycle;

#[test]
fn terminal_completion_distinguishes_commit_noncommit_and_indeterminate() {
    let _guard = TEST_LOCK.lock().unwrap();
    for fault in [
        None,
        Some(FaultPoint::BeforeCommit),
        Some(FaultPoint::AfterCommitBeforePersist),
        Some(FaultPoint::AfterPersist),
    ] {
        verify_completion(fault, false);
    }
}

#[test]
fn source_loss_completion_survives_direct_execution_flight_release() {
    let _guard = TEST_LOCK.lock().unwrap();
    verify_completion(None, true);
}

fn verify_completion(fault: Option<FaultPoint>, source_loss: bool) {
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(139, faults.clone());
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = if source_loss {
        NormalTerminalServer::spawn_connection_loss()
    } else {
        NormalTerminalServer::spawn()
    };
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(37_139).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let request = beryl_app::cas_projection::CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(37_000),
        TIMEOUT,
    );
    let projection = coordinator
        .obtain_projection(
            &*fixture.home(),
            &fixture.storage,
            &mut session,
            &request,
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    let barrier = install_terminal_history_barrier(
        fixture.thread,
        TerminalHistoryBarrierStage::BeforeGateRelease,
    );
    let home = fixture.home();
    let (outcome, observer) = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let mut lifecycle = NoopLifecycle::default();
            let mut branch = NoopBranch::default();
            coordinator.execute_ordinary_turn(
                &home,
                &fixture.storage,
                &fixture.state.assets(),
                projection,
                &fixture.cancellation,
                &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
                OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
            )
        });
        barrier.wait();
        let observer = coordinator
            .terminal_completion_for_test(fixture.thread)
            .unwrap()
            .unwrap();
        assert_eq!(observer.turn_id(), submitted.turn);
        assert_eq!(observer.lifecycle(), None);
        if let Some(fault) = fault {
            faults.fail_next(fault);
        }
        barrier.release();
        (worker.join().unwrap(), observer)
    });
    let expected_lifecycle = if source_loss {
        TurnLifecycle::Incomplete
    } else {
        TurnLifecycle::Complete
    };
    match fault {
        None => assert!(outcome.is_ok(), "terminal execution failed: {outcome:?}"),
        Some(FaultPoint::BeforeCommit) => assert!(
            matches!(
                outcome,
                Err(OrdinaryTurnExecutionFailure::AfterActivation {
                    source: OrdinaryTurnExecutionError::HomeCommandNotCommitted(_),
                })
            ),
            "noncommit classification changed: {outcome:?}"
        ),
        Some(FaultPoint::AfterCommitBeforePersist) => assert!(
            matches!(
                outcome,
                Err(OrdinaryTurnExecutionFailure::AfterActivation {
                    source: OrdinaryTurnExecutionError::HomeCommandIndeterminate { .. },
                })
            ),
            "indeterminate classification changed: {outcome:?}"
        ),
        Some(FaultPoint::AfterPersist) => assert!(
            matches!(
                outcome,
                Err(OrdinaryTurnExecutionFailure::AfterActivation {
                    source: OrdinaryTurnExecutionError::HomeCommandCommitted { .. },
                })
            ),
            "committed failure classification changed: {outcome:?}"
        ),
        _ => unreachable!(),
    }
    assert_eq!(
        observer.lifecycle(),
        if fault.is_none() || fault == Some(FaultPoint::AfterPersist) {
            Some(expected_lifecycle)
        } else {
            None
        }
    );
    assert!(
        coordinator
            .terminal_completion_for_test(fixture.thread)
            .unwrap()
            .is_none()
    );
    for handle in home.pending_reconciliations() {
        assert!(matches!(
            home.reconcile(&handle).unwrap(),
            beryl_home_store::ReconciliationResolution::ExactNew { .. }
        ));
    }
    assert_eq!(
        observer.lifecycle(),
        if fault.is_none() || fault == Some(FaultPoint::AfterPersist) {
            Some(expected_lifecycle)
        } else {
            None
        }
    );
    session.invalidate_connection();
    drop(outcome);
    drop(home);
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}
