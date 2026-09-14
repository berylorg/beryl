use super::*;
use crate::cas_projection::AdmittedProjectionSession;
use beryl_backend::ManagedBackendClientConnector;
use beryl_model::{CasProcessGeneration, CasThreadId};
use std::{path::Path, sync::Arc};

#[path = "../../normal_terminal/server.rs"]
mod server;
use server::{AUTHORIZATION, NormalTerminalServer, TIMEOUT};

fn admit(
    fixture: &Fixture,
    server: &NormalTerminalServer,
    generation: u64,
) -> AdmittedProjectionSession {
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = fixture
        .service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([71; 16]),
            CasProcessGeneration::new(generation).unwrap(),
            Path::new(r"C:\work\beryl"),
            TIMEOUT,
        )
        .unwrap();
    server.wait_for_admission();
    session
}

#[test]
fn threadless_cleanup_waits_for_its_owner() {
    let fixture = Fixture::idle();
    let server = NormalTerminalServer::spawn_admission_only();
    let session = admit(&fixture, &server, 72_200);
    let connection = Arc::clone(session.connection());
    let cleanup = connection.acquire_cleanup_owner().unwrap().unwrap();
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    assert_eq!(poll(&fixture, id), ShutdownProgress::Waiting);
    drop(cleanup);
    assert_eq!(finish_pass(&fixture, id), ShutdownProgress::Ready);
    drop(session);
    connection.shutdown().unwrap();
    drop(connection);
    server.join();
}

#[test]
fn detached_failed_join_cannot_disappear_behind_a_clear_cleanup_flag() {
    let fixture = Fixture::idle();
    let server = NormalTerminalServer::spawn_admission_only();
    let session = admit(&fixture, &server, 72_201);
    let connection = Arc::clone(session.connection());
    connection.fail_next_ingester_join_for_test();
    drop(session);
    assert!(connection.shutdown().is_err());
    assert!(connection.is_detached());
    assert!(
        !fixture
            .service
            .shutdown_work_revision(&fixture.sessions)
            .unwrap()
            .requires_connection_cleanup()
    );
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    assert_eq!(
        finish_pass(&fixture, id),
        ShutdownProgress::Failed {
            reason: ShutdownFailure::CleanupFailed,
            reopened: true
        }
    );
    drop(connection);
    server.join();
}

#[test]
fn shared_returned_projection_remains_cleanup_until_the_last_lease_returns() {
    let fixture = Fixture::new();
    let server = NormalTerminalServer::spawn_unsubscribe_failure();
    let session = admit(&fixture, &server, 72_202);
    let connection = Arc::clone(session.connection());
    let cas = CasThreadId::new("shared-shutdown-projection").unwrap();
    let first = connection
        .register_new(
            cas.clone(),
            beryl_backend::ThreadSessionMetadata::default(),
            fixture.thread,
            TIMEOUT,
        )
        .unwrap();
    let crate::cas_projection::connection::ExistingLease::Exact(second) = connection
        .acquire_existing(&cas, fixture.thread, TIMEOUT)
        .unwrap()
    else {
        panic!("shared lease unavailable");
    };
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    assert_eq!(poll(&fixture, id), ShutdownProgress::Waiting);
    first.release().unwrap();
    assert_eq!(poll(&fixture, id), ShutdownProgress::Waiting);
    assert!(second.release().is_err());
    drop(session);
    assert_eq!(finish_pass(&fixture, id), ShutdownProgress::Ready);
    drop(connection);
    server.join();
}
