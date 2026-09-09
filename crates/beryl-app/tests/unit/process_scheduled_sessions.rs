use super::*;

#[path = "process_session_idle_retirement.rs"]
mod idle_retirement;
use crate::cas_projection::{
    ProcessScheduledExecutionProvider, ScheduledExecutionSessions,
    ScheduledSessionRegistrationError,
};

fn owned_service(
    capacity: u64,
) -> (
    tempfile::TempDir,
    ProjectionConnectionService,
    BerylState,
    ScheduledExecutionSessions,
) {
    let directory = tempfile::tempdir().unwrap();
    let mut home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(
            8,
            capacity,
            MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap(),
        Box::new(provider),
    )
    .unwrap();
    (directory, service, state, sessions)
}

fn admitted_session(
    service: &ProjectionConnectionService,
    generation: u64,
) -> (AdmittedProjectionSession, NormalTerminalServer) {
    let server = NormalTerminalServer::spawn_admission_only();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([91; 16]),
            CasProcessGeneration::new(generation).unwrap(),
            Path::new(r"C:\work\scheduled-ordinary"),
            TIMEOUT,
        )
        .unwrap();
    server.wait_for_admission();
    (session, server)
}

fn tools() -> Box<dyn OrdinaryDynamicToolAuthority> {
    Box::new(ToolAuthority {
        lifecycle: LifecycleHandler,
        branch: BranchHandler,
    })
}

fn issue(
    service: &ProjectionConnectionService,
    thread: SyndicThreadId,
    binding: ExecutionBinding,
) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
    let worker = service.try_acquire_scheduled_ordinary_worker().unwrap();
    let flight = service.begin_scheduled_ordinary_flight(thread).unwrap();
    service.issue_scheduled_ordinary_execution(thread, binding, worker, flight)
}

fn issued(
    service: &ProjectionConnectionService,
    thread: SyndicThreadId,
) -> ScheduledOrdinaryExecutionLease {
    match issue(
        service,
        thread,
        execution_binding(RuntimeId::from_bytes([91; 16])),
    )
    .unwrap()
    {
        ScheduledOrdinaryAdmissionResult::Issued(lease) => lease,
        _ => panic!("exact production checkout must be available"),
    }
}

#[test]
fn process_session_return_waits_for_both_authorities_and_reuses_one_reservation() {
    let (directory, service, state, sessions) = owned_service(6);
    let (session, server) = admitted_session(&service, 72_001);
    let thread = SyndicThreadId::from_bytes([201; 16]);
    let binding = execution_binding(RuntimeId::from_bytes([91; 16]));
    sessions
        .register(
            thread,
            binding.clone(),
            session,
            explicit_policy(),
            state.assets(),
            tools(),
        )
        .unwrap();
    let wrong_root = ExecutionBinding::new(
        binding.runtime_id(),
        RootId::from_bytes([202; 16]),
        binding.root_path().clone(),
    );
    assert!(matches!(
        issue(&service, thread, wrong_root).unwrap(),
        ScheduledOrdinaryAdmissionResult::Unavailable(
            ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady
        )
    ));
    assert_eq!(sessions.diagnostics().available, 1);
    let lease = issued(&service, thread);
    assert!(matches!(
        service.begin_scheduled_ordinary_flight(thread),
        Err(ProjectionCoordinatorError::ProjectionInFlight { .. })
    ));
    let ScheduledOrdinaryExecutionLease {
        session,
        tools,
        _worker,
        flight,
        ..
    } = lease;
    drop(_worker);
    drop(flight);
    drop(session);
    assert_eq!(sessions.diagnostics().available, 0);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    drop(tools);
    assert_eq!(sessions.diagnostics().available, 1);
    for _ in 0..12 {
        drop(issued(&service, thread));
        let snapshot = sessions.diagnostics();
        assert_eq!(snapshot.available, 1);
        assert_eq!(snapshot.checked_out, 0);
        assert_eq!(snapshot.retained, 1);
        assert_eq!(snapshot.high_water, 1);
    }
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    assert_eq!(sessions.diagnostics().retained, 0);
    drop(directory);
}

#[test]
fn process_session_capacity_includes_retired_connections_with_outstanding_checkouts() {
    let (directory, service, state, sessions) = owned_service(6);
    assert_eq!(sessions.diagnostics().capacity, 3);
    let mut leases = Vec::new();
    let mut servers = Vec::new();
    for ordinal in 0..3 {
        let (session, server) = admitted_session(&service, 72_010 + ordinal);
        let thread = SyndicThreadId::from_bytes([210 + ordinal as u8; 16]);
        sessions
            .register(
                thread,
                execution_binding(RuntimeId::from_bytes([91; 16])),
                session,
                explicit_policy(),
                state.assets(),
                tools(),
            )
            .unwrap();
        let mut lease = issued(&service, thread);
        lease.session().invalidate_connection();
        leases.push(lease);
        servers.push(server);
        let deadline = Instant::now() + TIMEOUT;
        while service.worker_pool_diagnostics().available() != 6 - leases.len() {
            assert!(
                Instant::now() < deadline,
                "retired connection did not release its workers"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    let snapshot = sessions.diagnostics();
    assert_eq!(snapshot.retained, 3);
    assert_eq!(snapshot.checked_out, 3);
    assert_eq!(snapshot.retiring, 3);
    let (session, server) = admitted_session(&service, 72_013);
    servers.push(server);
    assert_eq!(
        sessions.register(
            SyndicThreadId::from_bytes([213; 16]),
            execution_binding(RuntimeId::from_bytes([91; 16])),
            session,
            explicit_policy(),
            state.assets(),
            tools(),
        ),
        Err(ScheduledSessionRegistrationError::CapacityFull),
    );
    assert_eq!(sessions.diagnostics().high_water, 3);
    drop(leases);
    assert_eq!(sessions.diagnostics().retained, 0);
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    for server in servers {
        server.join();
    }
    drop(directory);
}

#[test]
fn process_session_registration_rejects_foreign_service_and_issuance_rejects_foreign_assets() {
    let (directory, service, state, sessions) = owned_service(6);
    let (foreign_directory, foreign_service, foreign_state, _) = owned_service(6);
    let (session, foreign_server) = admitted_session(&foreign_service, 72_020);
    let thread = SyndicThreadId::from_bytes([220; 16]);
    assert_eq!(
        sessions.register(
            thread,
            execution_binding(RuntimeId::from_bytes([91; 16])),
            session,
            explicit_policy(),
            state.assets(),
            tools(),
        ),
        Err(ScheduledSessionRegistrationError::SessionAuthorityUnavailable),
    );
    let (session, server) = admitted_session(&service, 72_021);
    let registration = sessions
        .register(
            thread,
            execution_binding(RuntimeId::from_bytes([91; 16])),
            session,
            explicit_policy(),
            foreign_state.assets(),
            tools(),
        )
        .unwrap();
    assert!(matches!(
        issue(
            &service,
            thread,
            execution_binding(RuntimeId::from_bytes([91; 16]))
        ),
        Err(ScheduledOrdinaryAdmissionError::AssetAuthority { .. })
    ));
    assert_eq!(sessions.diagnostics().available, 1);
    assert!(sessions.retire(registration));
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert!(matches!(
        foreign_service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    foreign_server.join();
    assert_eq!(sessions.diagnostics().retained, 0);
    drop(directory);
    drop(foreign_directory);
}
