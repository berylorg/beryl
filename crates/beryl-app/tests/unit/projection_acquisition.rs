use super::*;
use crate::cas_projection::{
    CasProjectionRequest, ProjectionExecutionError,
    acquisition::ProjectionAcquisition,
    test_faults::{AcquisitionBarrierStage, install_acquisition_barrier},
};
use crate::process_admission::{ProcessAdmissionError, ProcessExecutionAdmissionError};
use beryl_backend::{BackendWebSocketEndpoint, ThreadStartOptions};
use std::{io::ErrorKind, net::TcpListener, thread};
use syndic_storage::SelectedPathProof;

#[path = "persistent_failure_projection_disposal.rs"]
mod persistent_failure_disposal;

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

mod server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/normal_terminal/server.rs"
    ));
}

fn connector(endpoint: BackendWebSocketEndpoint) -> ManagedBackendClientConnector {
    ManagedBackendClientConnector::for_lifecycle_test(endpoint, server::AUTHORIZATION)
}

fn admit(
    fixture: &Fixture,
    connector: &ManagedBackendClientConnector,
) -> Result<AdmittedProjectionSession, ProjectionSessionAdmissionError> {
    fixture.service.admit_lifecycle_test_candidate(
        connector,
        RuntimeId::from_bytes([71; 16]),
        CasProcessGeneration::new(91_001).unwrap(),
        Path::new(r"C:\work\beryl"),
        server::TIMEOUT,
    )
}

fn request(fixture: &Fixture) -> CasProjectionRequest {
    let command = fixture.service.live_home_command().unwrap();
    let thread = fixture
        .service
        .storage
        .thread(command.home(), fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let binding = fixture
        .service
        .storage
        .thread_execution(command.home(), fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    CasProjectionRequest::new(
        fixture.thread,
        SelectedPathProof::new(
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
        ),
        binding.execution().clone(),
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(4),
        server::TIMEOUT,
    )
}

#[test]
fn post_fence_public_session_admission_creates_no_connection() {
    let fixture = Fixture::idle();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let connector = connector(BackendWebSocketEndpoint::loopback(
        listener.local_addr().unwrap().port(),
    ));
    let fence = fixture.gate.fence().unwrap();
    let before = fixture.service.worker_pool_diagnostics().active();
    assert!(matches!(
        fixture.service.admit(
            &connector,
            RuntimeId::from_bytes([71; 16]),
            CasProcessGeneration::new(91_002).unwrap(),
            Path::new(r"C:\work\beryl"),
            server::TIMEOUT,
        ),
        Err(ProjectionSessionAdmissionError::ConnectionOwnership {
            source: ProjectionCoordinatorError::AcquisitionFenced(ProcessAdmissionError::Fenced),
            ..
        })
    ));
    assert_eq!(listener.accept().unwrap_err().kind(), ErrorKind::WouldBlock);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), before);
    assert_eq!(fence.validate_settled_for(&fixture.gate), Ok(()));
}

#[test]
fn admitted_connection_keeps_custody_across_fence_until_publication() {
    let fixture = Fixture::idle();
    let server = server::NormalTerminalServer::spawn_admission_only();
    let connector = connector(server.endpoint());
    let barrier = install_acquisition_barrier(
        fixture.service.service_generation(),
        AcquisitionBarrierStage::SessionPrepared,
    );
    let (session, fence) = thread::scope(|scope| {
        let worker = scope.spawn(|| admit(&fixture, &connector));
        barrier.wait();
        let fence = fixture.gate.fence().unwrap();
        assert_eq!(
            fence.validate_settled_for(&fixture.gate),
            Err(ProcessAdmissionError::Unsettled)
        );
        assert_eq!(fence.reopen_if(true), Err(ProcessAdmissionError::Unsettled));
        barrier.release();
        (worker.join().unwrap().unwrap(), fence)
    });
    server.wait_for_admission();
    assert_eq!(fence.validate_settled_for(&fixture.gate), Ok(()));
    assert_eq!(fixture.service.registered_connection_count_for_test(), 1);
    drop(session);
    let _ = fixture.service.close().unwrap();
    server.join();
}

#[test]
fn failed_connection_acquisition_releases_its_count_after_cleanup() {
    let fixture = Fixture::idle();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let connector = connector(BackendWebSocketEndpoint::loopback(
        listener.local_addr().unwrap().port(),
    ));
    drop(listener);
    let barrier = install_acquisition_barrier(
        fixture.service.service_generation(),
        AcquisitionBarrierStage::SessionPrepared,
    );
    thread::scope(|scope| {
        let worker = scope.spawn(|| admit(&fixture, &connector));
        barrier.wait();
        let fence = fixture.gate.fence().unwrap();
        assert_eq!(
            fence.validate_settled_for(&fixture.gate),
            Err(ProcessAdmissionError::Unsettled)
        );
        barrier.release();
        assert!(matches!(
            worker.join().unwrap(),
            Err(ProjectionSessionAdmissionError::CandidateConnection { .. })
        ));
        assert_eq!(fence.validate_settled_for(&fixture.gate), Ok(()));
        assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
        fence.reopen_if(true).unwrap();
    });
}

#[test]
fn post_fence_projection_cannot_start_a_provider_thread() {
    let fixture = Fixture::idle();
    let server = server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let mut session = admit(&fixture, &connector(server.endpoint())).unwrap();
    server.wait_for_admission();
    let request = request(&fixture);
    let fence = fixture.gate.fence().unwrap();
    let command = fixture.service.live_home_command().unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(command.home()).unwrap();
    assert!(matches!(
        coordinator.obtain_projection(
            command.home(),
            &fixture.service.storage,
            &mut session,
            &request,
            &ProjectionCancellationToken::new(),
        ),
        Err(ProjectionExecutionError::Coordinator(
            ProjectionCoordinatorError::AcquisitionFenced(ProcessAdmissionError::Fenced)
        ))
    ));
    assert_eq!(fence.validate_settled_for(&fixture.gate), Ok(()));
    drop(command);
    server.assert_quiet_and_close();
    server.join();
    drop(session);
    let _ = fixture.service.close().unwrap();
}

#[test]
fn admitted_projection_publishes_behind_fence_without_losing_custody() {
    let fixture = Fixture::new();
    let server = server::NormalTerminalServer::spawn_projection_only();
    let mut session = admit(&fixture, &connector(server.endpoint())).unwrap();
    server.wait_for_admission();
    let request = request(&fixture);
    let barrier = install_acquisition_barrier(
        fixture.service.service_generation(),
        AcquisitionBarrierStage::ProjectionAdmitted,
    );
    let command = fixture.service.live_home_command().unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(command.home()).unwrap();
    let projection = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            coordinator.obtain_projection(
                command.home(),
                &fixture.service.storage,
                &mut session,
                &request,
                &ProjectionCancellationToken::new(),
            )
        });
        barrier.wait();
        let fence = fixture.gate.fence().unwrap();
        assert_eq!(
            fence.validate_settled_for(&fixture.gate),
            Err(ProcessAdmissionError::Unsettled)
        );
        barrier.release();
        let projection = worker.join().unwrap().unwrap();
        assert_eq!(fence.validate_settled_for(&fixture.gate), Ok(()));
        projection
    });
    server.wait_for_projection();
    projection.release().unwrap();
    drop(command);
    drop(session);
    let _ = fixture.service.close().unwrap();
    server.join();
}

#[test]
fn cancellation_of_admitted_projection_settles_without_provider_effects() {
    let fixture = Fixture::new();
    let server = server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let mut session = admit(&fixture, &connector(server.endpoint())).unwrap();
    server.wait_for_admission();
    let request = request(&fixture);
    let cancellation = ProjectionCancellationToken::new();
    let barrier = install_acquisition_barrier(
        fixture.service.service_generation(),
        AcquisitionBarrierStage::ProjectionAdmitted,
    );
    let command = fixture.service.live_home_command().unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(command.home()).unwrap();
    thread::scope(|scope| {
        let worker = scope.spawn(|| {
            coordinator.obtain_projection(
                command.home(),
                &fixture.service.storage,
                &mut session,
                &request,
                &cancellation,
            )
        });
        barrier.wait();
        let fence = fixture.gate.fence().unwrap();
        assert_eq!(
            fence.validate_settled_for(&fixture.gate),
            Err(ProcessAdmissionError::Unsettled)
        );
        cancellation.cancel();
        barrier.release();
        assert!(matches!(
            worker.join().unwrap(),
            Err(ProjectionExecutionError::Cancelled)
        ));
        assert_eq!(fence.validate_settled_for(&fixture.gate), Ok(()));
    });
    drop(command);
    server.assert_quiet_and_close();
    server.join();
    drop(session);
    let _ = fixture.service.close().unwrap();
}

#[test]
fn old_unadmitted_command_cannot_acquire_after_reopening() {
    let fixture = Fixture::idle();
    let commands = fixture.service.live_command_authorizer();
    let command = commands.authorize().unwrap();
    let fence = fixture.gate.fence().unwrap();
    fence.reopen_if(true).unwrap();
    assert!(matches!(
        ProjectionAcquisition::admit_from(&commands, &command),
        Err(ProcessExecutionAdmissionError::Process(
            ProcessAdmissionError::Stale
        ))
    ));
    let acquisition = ProjectionAcquisition::admit(&commands).unwrap();
    let fence = fixture.gate.fence().unwrap();
    let child = acquisition.clone();
    drop(acquisition);
    assert_eq!(
        fence.validate_settled_for(&fixture.gate),
        Err(ProcessAdmissionError::Unsettled)
    );
    drop(child);
    assert_eq!(fence.validate_settled_for(&fixture.gate), Ok(()));
}

#[test]
fn failed_publication_retains_acquisition_through_cleanup_and_pending_reconciliation() {
    use crate::cas_projection::ProjectionPublicationFailure;
    use beryl_home_store::test_faults::FaultPoint;

    let fixture = Fixture::new();
    let server = server::NormalTerminalServer::spawn_projection_controlled_cleanup();
    let mut session = admit(&fixture, &connector(server.endpoint())).unwrap();
    server.wait_for_admission();
    let request = request(&fixture);
    let barrier = install_acquisition_barrier(
        fixture.service.service_generation(),
        AcquisitionBarrierStage::ProjectionAdmitted,
    );
    let command = fixture.service.live_home_command().unwrap();
    let coordinator = CasProjectionCoordinator::for_healthy_home(command.home()).unwrap();
    thread::scope(|scope| {
        let worker = scope.spawn(|| {
            coordinator.obtain_projection(
                command.home(),
                &fixture.service.storage,
                &mut session,
                &request,
                &ProjectionCancellationToken::new(),
            )
        });
        barrier.wait();
        let fence = fixture.gate.fence().unwrap();
        fixture
            .faults
            .fail_next(FaultPoint::AfterCommitBeforePersist);
        barrier.release();
        server.wait_for_projection();
        server.wait_for_unsubscribe();
        assert_eq!(
            fence.validate_settled_for(&fixture.gate),
            Err(ProcessAdmissionError::Unsettled)
        );
        assert_eq!(fence.reopen_if(true), Err(ProcessAdmissionError::Unsettled));
        assert!(!command.home().pending_reconciliations().is_empty());
        server.release_unsubscribe();
        let error = worker.join().unwrap().err().expect("publication must fail");
        let ProjectionExecutionError::AbandonmentFailed { primary, .. } = error else {
            panic!("publication must retain its abandonment failure: {error:?}");
        };
        let ProjectionExecutionError::Publication(source) = *primary else {
            panic!("expected publication failure: {primary:?}");
        };
        assert!(matches!(
            *source,
            ProjectionPublicationFailure::CommandIndeterminate { .. }
        ));
        assert_eq!(fence.validate_settled_for(&fixture.gate), Ok(()));
        assert!(!command.home().pending_reconciliations().is_empty());
        assert!(fixture.read(&fence).unwrap().is_none());
        for handle in command.home().pending_reconciliations() {
            assert!(matches!(
                command.home().reconcile(&handle).unwrap(),
                beryl_home_store::ReconciliationResolution::ExactNew { .. }
            ));
        }
        assert!(command.home().pending_reconciliations().is_empty());
    });
    drop(command);
    drop(session);
    let _ = fixture.service.close().unwrap();
    server.join();
}
