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

use std::{io::ErrorKind, net::TcpListener, path::Path, thread, time::Duration};

use beryl_app::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    LifecycleYieldRequest, LifecycleYieldRequestHandler,
    cas_projection::{
        CasProjectionCoordinator, OrdinaryDynamicToolContext, OrdinaryDynamicToolHandlers,
        OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest, ProjectionCoordinatorError,
        ProjectionSessionAdmissionError,
    },
};
use beryl_backend::{
    BackendWebSocketEndpoint, DynamicToolCallResponse, ManagedBackendClientConnector,
    ThreadStartOptions, TurnStartOptions,
};
use beryl_model::{CasProcessGeneration, RuntimeId};
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
        .admit_lifecycle_test_candidate(
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
        .admit_lifecycle_test_candidate(
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
        .admit_lifecycle_test_candidate(
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
        .admit_lifecycle_test_candidate(
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
