#![cfg(feature = "test-faults")]

#[path = "phase10_projection/syndic.rs"]
mod syndic;

#[path = "phase37_normal_terminal/loss.rs"]
mod loss;
#[path = "phase37_normal_terminal/server.rs"]
mod server;
#[path = "phase37_normal_terminal/steering_loss.rs"]
mod steering_loss;
#[path = "phase37_normal_terminal/verification.rs"]
mod verification;

use std::{
    io::ErrorKind,
    net::TcpListener,
    path::Path,
    sync::mpsc::TryRecvError,
    thread,
    time::{Duration, Instant},
};

use beryl_app::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
    cas_projection::{
        CasProjectionCoordinator, HardStopCoordinationOutcome, HardStopTargetDisposition,
        HardStopTargetKind, OrdinaryDynamicToolContext, OrdinaryDynamicToolHandlers,
        OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest, ProjectionCoordinatorError,
        ProjectionSessionAdmissionError, StopCoordinationError, StopCoordinationOutcome,
    },
};
use beryl_backend::{
    BackendWebSocketEndpoint, DynamicToolCallResponse, ExactHardStopLimitation,
    ManagedBackendClientConnector, ThreadStartOptions, TurnStartOptions,
};
use beryl_model::{CasProcessGeneration, RuntimeId};
use syndic_storage::InputGateState;
use syndic_storage::SyndicTimestamp;

use server::{AUTHORIZATION, NormalTerminalServer, SUBMITTED_TEXT, TIMEOUT};
use syndic::{Fixture, execution_binding};
use verification::{ProjectionExpectation, assert_connection_quiescent, assert_durable_success};

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

static PHASE37_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Default)]
struct NoopLifecycle {
    calls: usize,
}

impl LifecycleYieldRequestHandler for NoopLifecycle {
    fn respond_lifecycle_yield(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: LifecycleYieldRequest,
    ) -> DynamicToolCallResponse {
        self.calls += 1;
        DynamicToolCallResponse::success_text("unused lifecycle handler")
    }
}

#[derive(Default)]
struct NoopBranch {
    calls: usize,
}

impl BranchDiscussionResolutionRequestHandler for NoopBranch {
    fn respond_branch_discussion_resolution(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        _request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        self.calls += 1;
        DynamicToolCallResponse::success_text("unused branch handler")
    }
}

#[test]
fn raw_websocket_ordinary_success_reaches_durable_terminal() {
    let _guard = PHASE37_TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(137);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn();

    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(37_137).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let active_workers = fixture.store.worker_pool_diagnostics();
    assert_eq!(active_workers.capacity(), 128);
    assert_eq!(active_workers.available(), 126);
    assert_eq!(active_workers.active(), 2);
    assert!(
        (2..=3).contains(&active_workers.high_water()),
        "the connection pair may overlap the one-permit startup scheduler scan"
    );
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection_request = beryl_app::cas_projection::CasProjectionRequest::new(
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
            &fixture.store,
            fixture.storage,
            &mut session,
            &projection_request,
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    let expected_projection = ProjectionExpectation::capture(&projection);

    let execution_request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    let mut lifecycle = NoopLifecycle::default();
    let mut branch = NoopBranch::default();
    let outcome = coordinator
        .execute_ordinary_turn(
            &fixture.store,
            fixture.storage,
            fixture.state.assets(),
            projection,
            &fixture.cancellation,
            &execution_request,
            OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
        )
        .unwrap();
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = outcome else {
        panic!("ordinary success did not return the real terminal outcome: {outcome:?}")
    };
    assert_eq!(lifecycle.calls, 0);
    assert_eq!(branch.calls, 0);
    assert_eq!(status, syndic_storage::TurnEndStatus::complete());

    let durable = assert_durable_success(&fixture, submitted.turn, status);
    expected_projection.assert_returned(&projection, durable.binding_revision);
    assert_connection_quiescent(&session, server::terminal_wire().len());

    session.invalidate_connection();
    let released_workers = fixture.store.worker_pool_diagnostics();
    assert_eq!(released_workers.capacity(), 128);
    assert_eq!(released_workers.available(), 128);
    assert_eq!(released_workers.active(), 0);
    assert!(
        (2..=3).contains(&released_workers.high_water()),
        "the bounded next-turn scan may complete before or overlap the connection pair"
    );
    assert!(!projection.is_live().unwrap());
    drop(projection);
    drop(session);
    server.join();

    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

#[test]
fn hard_stop_uses_same_driver_cleanup_and_holds_terminal_release_until_result() {
    let _guard = PHASE37_TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(141);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn_hard_stop_terminal();

    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(37_141).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection_request = beryl_app::cas_projection::CasProjectionRequest::new(
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
            &fixture.store,
            fixture.storage,
            &mut session,
            &projection_request,
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();

    let execution_request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    let (hard_result, ordinary_result) = thread::scope(|scope| {
        let (ordinary_sender, ordinary_receiver) = std::sync::mpsc::sync_channel(1);
        let ordinary_store = &fixture.store;
        let ordinary_cancellation = &fixture.cancellation;
        let ordinary_coordinator = &coordinator;
        let assets = fixture.state.assets();
        scope.spawn(move || {
            let mut lifecycle = NoopLifecycle::default();
            let mut branch = NoopBranch::default();
            let outcome = ordinary_coordinator.execute_ordinary_turn(
                ordinary_store,
                fixture.storage,
                assets,
                projection,
                ordinary_cancellation,
                &execution_request,
                OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
            );
            ordinary_sender.send(outcome).unwrap();
        });

        server.wait_for_active_command();
        wait_for_source_event_count(&fixture, submitted.turn, 4);

        let (hard_sender, hard_receiver) = std::sync::mpsc::sync_channel(1);
        let hard_store = &fixture.store;
        let syndic_thread_id = fixture.thread;
        scope.spawn(move || {
            hard_sender
                .send(hard_store.hard_stop_selected_operation(syndic_thread_id))
                .unwrap();
        });

        server.wait_for_interrupt();
        assert_pending(&hard_receiver, "hard result before primary acceptance");
        assert_pending(
            &ordinary_receiver,
            "ordinary result before terminal ingress",
        );
        server.accept_interrupt();
        server.wait_for_cleanup();
        assert_pending(&hard_receiver, "hard result before cleanup acceptance");
        assert_pending(
            &ordinary_receiver,
            "ordinary result before terminal ingress",
        );

        server.wait_for_terminal_before_cleanup_response();
        wait_for_finalizing_history(&fixture, submitted.turn);
        assert_pending(
            &hard_receiver,
            "hard result while cleanup response is withheld",
        );
        assert_pending(
            &ordinary_receiver,
            "ordinary final idle release while cleanup response is withheld",
        );
        server.accept_cleanup();

        let hard = hard_receiver
            .recv_timeout(TIMEOUT)
            .expect("hard caller must receive its bounded result");
        let ordinary = ordinary_receiver
            .recv_timeout(TIMEOUT)
            .expect("ordinary caller must finish after hard finalization release");
        (hard, ordinary)
    });

    let HardStopCoordinationOutcome::Finished(hard_result) = hard_result.unwrap() else {
        panic!("exact live operation must admit bounded hard stop")
    };
    assert_eq!(hard_result.targets().len(), 1);
    assert_eq!(
        hard_result.targets()[0].target(),
        HardStopTargetKind::CoarseThreadCleanup
    );
    assert_eq!(
        hard_result.targets()[0].disposition(),
        HardStopTargetDisposition::RequestAccepted
    );
    let limitations = hard_result.limitations();
    assert_eq!(
        limitations[0].limitation(),
        ExactHardStopLimitation::ChildOrSubagentInterruptionUnsupported
    );
    assert_eq!(limitations[0].omitted_active(), 0);
    assert!(!limitations[0].count_overflowed());
    assert_eq!(
        limitations[1].limitation(),
        ExactHardStopLimitation::IndividualTurnProcessTerminationIdentityUnsafe
    );
    assert_eq!(limitations[1].omitted_active(), 1);
    assert!(!limitations[1].count_overflowed());

    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = ordinary_result.unwrap()
    else {
        panic!("terminal ingress must return the ordinary projection")
    };
    assert_eq!(status, syndic_storage::TurnEndStatus::complete());
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, syndic::point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);

    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

#[test]
fn unclassified_interrupt_completion_unknown_retires_exact_authority_immediately() {
    let _guard = PHASE37_TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(143);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn_hard_stop_unclassified_rejection();

    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(37_143).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let retirement = session.connection_retirement_handle_for_test();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection_request = beryl_app::cas_projection::CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(37_300),
        TIMEOUT,
    );
    let projection = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &projection_request,
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();

    let execution_request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    let (hard_result, ordinary_result) = thread::scope(|scope| {
        let (ordinary_sender, ordinary_receiver) = std::sync::mpsc::sync_channel(1);
        let ordinary_store = &fixture.store;
        let ordinary_cancellation = &fixture.cancellation;
        let ordinary_coordinator = &coordinator;
        let assets = fixture.state.assets();
        scope.spawn(move || {
            let mut lifecycle = NoopLifecycle::default();
            let mut branch = NoopBranch::default();
            ordinary_sender
                .send(ordinary_coordinator.execute_ordinary_turn(
                    ordinary_store,
                    fixture.storage,
                    assets,
                    projection,
                    ordinary_cancellation,
                    &execution_request,
                    OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
                ))
                .unwrap();
        });

        server.wait_for_active_command();
        wait_for_source_event_count(&fixture, submitted.turn, 4);
        let hard_result = fixture.store.hard_stop_selected_operation(fixture.thread);
        server.wait_for_interrupt();
        wait_for_connection_retirement(&retirement);
        assert!(matches!(
            fixture.store.stop_selected_operation(fixture.thread),
            Err(StopCoordinationError::TargetUnavailable)
        ));
        let ordinary_result = ordinary_receiver
            .recv_timeout(TIMEOUT)
            .expect("connection retirement must converge the ordinary turn");
        (hard_result, ordinary_result)
    });

    let HardStopCoordinationOutcome::Finished(hard_result) = hard_result.unwrap() else {
        panic!("the admitted hard caller must receive its frozen bounded result")
    };
    assert_eq!(hard_result.targets().len(), 1);
    assert_eq!(
        hard_result.targets()[0].disposition(),
        HardStopTargetDisposition::UnavailableWithoutDispatch
    );
    assert!(matches!(
        ordinary_result.unwrap(),
        OrdinaryTurnExecutionOutcome::Incomplete { .. }
    ));

    session.invalidate_connection();
    drop(session);
    drop(retirement);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

#[test]
fn accepted_soft_stop_later_hard_escalation_reuses_primary_and_cleanup() {
    let _guard = PHASE37_TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(142);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn_soft_then_hard_stop_terminal();

    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(37_142).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection_request = beryl_app::cas_projection::CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(37_200),
        TIMEOUT,
    );
    let projection = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &projection_request,
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();

    let execution_request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    let (hard_result, ordinary_result) = thread::scope(|scope| {
        let (ordinary_sender, ordinary_receiver) = std::sync::mpsc::sync_channel(1);
        let ordinary_store = &fixture.store;
        let ordinary_cancellation = &fixture.cancellation;
        let ordinary_coordinator = &coordinator;
        let assets = fixture.state.assets();
        scope.spawn(move || {
            let mut lifecycle = NoopLifecycle::default();
            let mut branch = NoopBranch::default();
            let outcome = ordinary_coordinator.execute_ordinary_turn(
                ordinary_store,
                fixture.storage,
                assets,
                projection,
                ordinary_cancellation,
                &execution_request,
                OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
            );
            ordinary_sender.send(outcome).unwrap();
        });

        server.wait_for_active_command();
        wait_for_source_event_count(&fixture, submitted.turn, 4);

        let (soft_sender, soft_receiver) = std::sync::mpsc::sync_channel(1);
        let soft_store = &fixture.store;
        let syndic_thread_id = fixture.thread;
        scope.spawn(move || {
            soft_sender
                .send(soft_store.stop_selected_operation(syndic_thread_id))
                .unwrap();
        });
        server.wait_for_interrupt();
        assert_pending(&soft_receiver, "soft result before primary acceptance");
        server.accept_interrupt();
        let soft = soft_receiver
            .recv_timeout(TIMEOUT)
            .expect("accepted soft stop must settle");
        assert!(matches!(
            soft.unwrap(),
            StopCoordinationOutcome::Stopping {
                primary_owner: true,
                ..
            }
        ));

        let (hard_sender, hard_receiver) = std::sync::mpsc::sync_channel(1);
        let hard_store = &fixture.store;
        scope.spawn(move || {
            hard_sender
                .send(hard_store.hard_stop_selected_operation(syndic_thread_id))
                .unwrap();
        });
        server.wait_for_cleanup();
        assert_pending(&hard_receiver, "late hard result before cleanup acceptance");
        server.accept_cleanup();

        let hard = hard_receiver
            .recv_timeout(TIMEOUT)
            .expect("late hard caller must receive its bounded result");
        let ordinary = ordinary_receiver
            .recv_timeout(TIMEOUT)
            .expect("ordinary caller must finish after terminal ingress");
        (hard, ordinary)
    });

    let HardStopCoordinationOutcome::Finished(hard_result) = hard_result.unwrap() else {
        panic!("accepted soft stop must remain eligible for late hard escalation")
    };
    assert_accepted_hard_result(&hard_result);

    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = ordinary_result.unwrap()
    else {
        panic!("terminal ingress must return the ordinary projection")
    };
    assert_eq!(status, syndic_storage::TurnEndStatus::complete());
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, syndic::point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);

    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

#[test]
fn minimum_worker_capacity_denies_before_connect_and_reuses_after_retirement() {
    let _guard = PHASE37_TEST_LOCK.lock().unwrap();
    let fixture = Fixture::new_with_worker_capacity(138, 4);
    let first_server = NormalTerminalServer::spawn_admission_only();
    let first_connector =
        ManagedBackendClientConnector::for_lifecycle_test(first_server.endpoint(), AUTHORIZATION);
    let first_runtime = RuntimeId::from_bytes([138; 16]);
    let first_process = CasProcessGeneration::new(37_138).unwrap();
    let first = fixture
        .store
        .admit(
            &first_connector,
            first_runtime,
            first_process,
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    first_server.wait_for_admission();

    let active = fixture.store.worker_pool_diagnostics();
    assert_eq!(active.capacity(), 4);
    assert_eq!(active.available(), 2);
    assert_eq!(active.active(), 2);
    assert!(
        (2..=3).contains(&active.high_water()),
        "the connection pair may overlap the one-permit startup scheduler scan"
    );
    assert_eq!(active.denied_pairs(), 0);

    let unused_listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    unused_listener.set_nonblocking(true).unwrap();
    let unused_endpoint =
        BackendWebSocketEndpoint::loopback(unused_listener.local_addr().unwrap().port());
    let denied_connector =
        ManagedBackendClientConnector::for_lifecycle_test(unused_endpoint, AUTHORIZATION);
    let denied_runtime = RuntimeId::from_bytes([139; 16]);
    let denied_process = CasProcessGeneration::new(37_139).unwrap();
    let denied = fixture
        .store
        .admit(
            &denied_connector,
            denied_runtime,
            denied_process,
            Path::new(EXECUTION_ROOT),
            Duration::from_millis(100),
        )
        .unwrap_err();
    assert_eq!(denied.runtime_id(), denied_runtime);
    assert_eq!(denied.process_generation(), denied_process);
    assert!(matches!(
        denied,
        ProjectionSessionAdmissionError::ConnectionOwnership {
            source: ProjectionCoordinatorError::ProjectionWorkerCapacityFull { available: 2 },
            ..
        }
    ));
    let denied_workers = fixture.store.worker_pool_diagnostics();
    assert_eq!(denied_workers.available(), 2);
    assert_eq!(denied_workers.active(), 2);
    assert!((2..=3).contains(&denied_workers.high_water()));
    assert_eq!(denied_workers.denied_pairs(), 1);
    match unused_listener.accept() {
        Err(error) if error.kind() == ErrorKind::WouldBlock => {}
        Ok((stream, _)) => {
            drop(stream);
            panic!("worker denial must occur before the connector opens a socket")
        }
        Err(error) => panic!("unused connector listener failed unexpectedly: {error}"),
    }

    first.invalidate_connection();
    drop(first);
    first_server.join();
    let released = fixture.store.worker_pool_diagnostics();
    assert_eq!(released.available(), 4);
    assert_eq!(released.active(), 0);
    assert!((2..=3).contains(&released.high_water()));
    assert_eq!(released.denied_pairs(), 1);

    let later_server = NormalTerminalServer::spawn_admission_only();
    let later_connector =
        ManagedBackendClientConnector::for_lifecycle_test(later_server.endpoint(), AUTHORIZATION);
    let later = fixture
        .store
        .admit(
            &later_connector,
            RuntimeId::from_bytes([140; 16]),
            CasProcessGeneration::new(37_140).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    later_server.wait_for_admission();
    let reused = fixture.store.worker_pool_diagnostics();
    assert_eq!(reused.available(), 2);
    assert_eq!(reused.active(), 2);
    assert!((2..=3).contains(&reused.high_water()));
    assert_eq!(reused.denied_pairs(), 1);

    later.invalidate_connection();
    drop(later);
    later_server.join();
    let rereleased = fixture.store.worker_pool_diagnostics();
    assert_eq!(rereleased.available(), 4);
    assert_eq!(rereleased.active(), 0);
    assert!((2..=3).contains(&rereleased.high_water()));
    assert_eq!(rereleased.denied_pairs(), 1);

    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}

#[test]
fn raw_websocket_connection_loss_converges_to_durable_incomplete() {
    let _guard = PHASE37_TEST_LOCK.lock().unwrap();
    loss::run();
}

#[test]
fn delayed_steering_correlation_failure_converges_production_projection_loss() {
    let _guard = PHASE37_TEST_LOCK.lock().unwrap();
    steering_loss::run();
}

fn assert_accepted_hard_result(result: &beryl_app::cas_projection::BoundedHardStopResult) {
    assert_eq!(result.targets().len(), 1);
    assert_eq!(
        result.targets()[0].target(),
        HardStopTargetKind::CoarseThreadCleanup
    );
    assert_eq!(
        result.targets()[0].disposition(),
        HardStopTargetDisposition::RequestAccepted
    );
    let limitations = result.limitations();
    assert_eq!(
        limitations[0].limitation(),
        ExactHardStopLimitation::ChildOrSubagentInterruptionUnsupported
    );
    assert_eq!(limitations[0].omitted_active(), 0);
    assert!(!limitations[0].count_overflowed());
    assert_eq!(
        limitations[1].limitation(),
        ExactHardStopLimitation::IndividualTurnProcessTerminationIdentityUnsafe
    );
    assert_eq!(limitations[1].omitted_active(), 1);
    assert!(!limitations[1].count_overflowed());
}

fn assert_pending<T>(receiver: &std::sync::mpsc::Receiver<T>, description: &str) {
    match receiver.try_recv() {
        Err(TryRecvError::Empty) => {}
        Ok(_) => panic!("{description}"),
        Err(TryRecvError::Disconnected) => panic!("{description}: result channel disconnected"),
    }
}

fn wait_for_source_event_count(fixture: &Fixture, turn: beryl_model::SyndicTurnId, expected: u64) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let state = fixture
            .storage
            .turn_state(&fixture.store, turn, syndic::point_limit())
            .unwrap()
            .unwrap();
        if state.source_event_count() >= expected {
            assert_eq!(state.open_item_count(), 1);
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the active command publication"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn wait_for_connection_retirement(
    retirement: &beryl_app::cas_projection::test_faults::ProjectionConnectionRetirementHandle,
) {
    let deadline = Instant::now() + TIMEOUT;
    while !retirement.is_retired() {
        assert!(
            Instant::now() < deadline,
            "completion-unknown interrupt did not retire exact connection authority"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn wait_for_finalizing_history(fixture: &Fixture, turn: beryl_model::SyndicTurnId) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let gate = fixture
            .storage
            .input_gate(&fixture.store, fixture.thread, syndic::point_limit())
            .unwrap()
            .unwrap();
        let state = fixture
            .storage
            .turn_state(&fixture.store, turn, syndic::point_limit())
            .unwrap()
            .unwrap();
        if gate.state() == &InputGateState::FinalizingHistory(turn)
            && state.lifecycle().is_proven_terminal()
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for terminal ingress to reach durable finalizing history"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
