use std::{io::ErrorKind, net::TcpListener, path::Path, time::Instant};

use beryl_app::cas_projection::{
    CasProjectionCoordinator, CasProjectionRequest, OrdinaryDynamicToolHandlers,
    OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest, ProjectionCoordinatorError,
    ProjectionSessionAdmissionError, StopCoordinationOutcome, StopWorkError, StopWorkPageLimits,
    StopWorkRecord, test_faults::install_stop_handoff_barrier,
};
use beryl_backend::{
    BackendWebSocketEndpoint, ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions,
};
use beryl_model::{CasProcessGeneration, RuntimeId, SyndicTurnId};
use syndic_storage::{InputGateState, SyndicTimestamp};

use super::{
    EXECUTION_ROOT, NoopBranch, NoopLifecycle, TEST_LOCK,
    server::{AUTHORIZATION, NormalTerminalServer, SUBMITTED_TEXT, TIMEOUT},
    syndic::{Fixture, execution_binding, point_limit},
};

#[test]
fn lost_stop_caller_retains_capacity_until_rejected_handoff_disposal() {
    let _guard = TEST_LOCK.lock().unwrap();
    run(false, false);
}

#[test]
fn lost_stop_caller_retains_capacity_until_unwind_disposal() {
    let _guard = TEST_LOCK.lock().unwrap();
    run(true, false);
}

#[test]
fn settled_stop_retains_both_workers_through_backend_cleanup() {
    let _guard = TEST_LOCK.lock().unwrap();
    run(false, true);
}

fn run(unwind: bool, cleanup: bool) {
    let mut fixture = Fixture::new_with_worker_capacity(158, 4);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = if cleanup {
        NormalTerminalServer::spawn_accepted_stop()
    } else {
        NormalTerminalServer::spawn_controlled_connection_loss()
    };
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(37_158).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&*fixture.home()).unwrap();
    let request = CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(37_100),
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

    std::thread::scope(|scope| {
        let fixture = &fixture;
        let capture = scope.spawn(move || {
            coordinator
                .execute_ordinary_turn(
                    &*fixture.home(),
                    &fixture.storage,
                    &fixture.state.assets(),
                    projection,
                    &fixture.cancellation,
                    &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
                    OrdinaryDynamicToolHandlers::new(
                        &mut NoopLifecycle::default(),
                        &mut NoopBranch::default(),
                    ),
                )
                .unwrap()
        });
        wait_for_active(fixture, submitted.turn);
        let idle_revision = fixture.store.stop_work_revision().unwrap();
        assert!(
            fixture
                .store
                .stop_work_page(
                    &idle_revision,
                    None,
                    StopWorkPageLimits::new(10, 65_536).unwrap()
                )
                .unwrap()
                .records()
                .is_empty()
        );
        let barrier = if cleanup {
            beryl_app::cas_projection::test_faults::install_stop_cleanup_barrier(fixture.thread)
        } else {
            install_stop_handoff_barrier(fixture.thread)
        };
        let stop = scope.spawn(|| fixture.store.stop_selected_operation(fixture.thread));
        barrier.wait();
        assert_eq!(
            fixture.store.validate_stop_work_revision(&idle_revision),
            Err(StopWorkError::StaleRevision)
        );
        assert_stop_custody(fixture, cleanup, true);
        assert!(fixture.store.has_local_stop_for_test(fixture.thread));
        assert!(matches!(
            fixture
                .store
                .stop_selected_operation(fixture.thread)
                .unwrap(),
            StopCoordinationOutcome::Stopping {
                primary_owner: false,
                ..
            }
        ));
        let abandonment =
            beryl_app::cas_projection::test_faults::install_live_event_target_abandonment(
                &session,
                fixture.thread,
            );
        assert!(abandonment.wait_until_abandoned(TIMEOUT));
        if !cleanup {
            server.close_connection();
        }
        assert!(matches!(
            capture.join().unwrap(),
            OrdinaryTurnExecutionOutcome::Incomplete { .. }
        ));
        assert!(!fixture.store.has_local_stop_for_test(fixture.thread));
        let owned_revision = assert_stop_custody(fixture, cleanup, false);
        assert!(
            !stop.is_finished(),
            "loss must converge while the admitted caller is paused"
        );
        let retirement = if cleanup {
            let retirement = scope.spawn(|| session.invalidate_connection());
            let deadline = Instant::now() + TIMEOUT;
            while !session.ingester_finished_for_test() {
                assert!(
                    Instant::now() < deadline,
                    "ingester did not exit during driver cleanup"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert_eq!(
                session.retained_worker_units_for_test(),
                Some((true, true)),
                "the original ingester admission must survive its worker exit until unbind"
            );
            assert!(
                !retirement.is_finished(),
                "driver cleanup must still block its join"
            );
            Some(retirement)
        } else {
            session.invalidate_connection();
            None
        };
        wait_for_available(fixture, 2);
        assert_eq!(fixture.store.worker_pool_diagnostics().active(), 2);
        if !cleanup {
            assert_replacement_denied(fixture);
        }
        barrier.release(unwind);
        let result = stop.join();
        if unwind {
            assert!(result.is_err());
        } else if !cleanup {
            assert!(result.unwrap().is_err());
        } else {
            let outcome = result.unwrap();
            assert!(
                outcome.is_err()
                    || matches!(
                        outcome,
                        Ok(StopCoordinationOutcome::Stopping {
                            primary_owner: true,
                            ..
                        })
                    )
            );
        }
        if let Some(retirement) = retirement {
            retirement.join().unwrap();
        }
        wait_for_available(fixture, 4);
        assert_eq!(fixture.store.worker_pool_diagnostics().active(), 0);
        assert_eq!(fixture.store.worker_pool_diagnostics().available(), 4);
        assert_eq!(
            fixture.store.validate_stop_work_revision(&owned_revision),
            Err(StopWorkError::StaleRevision)
        );
        let revision = fixture.store.stop_work_revision().unwrap();
        assert!(
            fixture
                .store
                .stop_work_page(
                    &revision,
                    None,
                    StopWorkPageLimits::new(10, 65_536).unwrap()
                )
                .unwrap()
                .records()
                .is_empty()
        );
    });
    drop(session);
    server.join();

    let replacement_server = NormalTerminalServer::spawn_admission_only();
    let replacement_connector = ManagedBackendClientConnector::for_lifecycle_test(
        replacement_server.endpoint(),
        AUTHORIZATION,
    );
    let replacement = fixture
        .store
        .admit_lifecycle_test_candidate(
            &replacement_connector,
            RuntimeId::from_bytes([159; 16]),
            CasProcessGeneration::new(37_159).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    replacement_server.wait_for_admission();
    replacement.invalidate_connection();
    drop(replacement);
    replacement_server.join();
    wait_for_available(&fixture, 4);
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    drop(directory);
}

fn assert_stop_custody(
    fixture: &Fixture,
    driver: bool,
    local: bool,
) -> beryl_app::cas_projection::StopWorkRevision {
    let revision = fixture.store.stop_work_revision().unwrap();
    let limits = StopWorkPageLimits::new(10, 65_536).unwrap();
    let page = fixture
        .store
        .stop_work_page(&revision, None, limits)
        .unwrap();
    assert_eq!(
        page,
        fixture
            .store
            .stop_work_page(&revision, None, limits)
            .unwrap()
    );
    assert!(page.next_cursor().is_none());
    let [StopWorkRecord::Stop(fact)] = page.records() else {
        panic!("one exact stop custody must remain visible");
    };
    assert_eq!(fact.target.thread_id(), fixture.thread);
    assert_eq!(fact.primary_custody, !driver);
    assert_eq!(fact.driver_custody, driver);
    assert_eq!(fact.local_dispatch.is_some(), local);
    revision
}

pub(super) fn wait_for_active(fixture: &Fixture, turn: SyndicTurnId) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let gate = fixture
            .storage
            .input_gate(&*fixture.home(), fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        let state = fixture
            .storage
            .turn_state(&*fixture.home(), turn, point_limit())
            .unwrap()
            .unwrap();
        if matches!(gate.state(), InputGateState::Steerable(actual) if *actual == turn)
            && state.source_event_count() >= 3
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "ordinary turn did not become durably active"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn wait_for_available(fixture: &Fixture, available: usize) {
    let deadline = Instant::now() + TIMEOUT;
    while fixture.store.worker_pool_diagnostics().available() != available {
        assert!(Instant::now() < deadline, "worker capacity did not settle");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn assert_replacement_denied(fixture: &Fixture) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    let result = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([160; 16]),
            CasProcessGeneration::new(37_160).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap_err();
    assert!(matches!(
        result,
        ProjectionSessionAdmissionError::ConnectionOwnership {
            source: ProjectionCoordinatorError::ProjectionWorkerCapacityFull { available: 2 },
            ..
        }
    ));
    match listener.accept() {
        Err(error) if error.kind() == ErrorKind::WouldBlock => {}
        _ => panic!("retained stop capacity must deny replacement before connecting"),
    }
}
