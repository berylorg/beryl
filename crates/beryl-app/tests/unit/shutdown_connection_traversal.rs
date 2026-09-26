include!("shutdown_support.rs");

use crate::cas_projection::{AdmittedProjectionSession, ConnectionWorkError, ProcessWorkError};
use beryl_backend::ManagedBackendClientConnector;
use beryl_model::CasProcessGeneration;
use std::{
    path::Path,
    sync::{Arc, Weak},
};

#[path = "../normal_terminal/server.rs"]
mod server;
use server::{AUTHORIZATION, NormalTerminalServer, TIMEOUT};

#[path = "shutdown_connection_traversal/cleanup.rs"]
mod cleanup;

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
fn failed_inventory_exceeds_worker_capacity_with_one_borrowed_handle() {
    let fixture = Fixture::idle();
    let mut retained = Vec::new();
    for generation in 73_000..73_009 {
        let server = NormalTerminalServer::spawn_admission_only();
        let session = admit(&fixture, &server, generation);
        let connection = Arc::clone(session.connection());
        connection.fail_next_ingester_join_for_test();
        drop(session);
        assert!(connection.shutdown().is_err());
        assert!(connection.is_detached());
        retained.push(Arc::downgrade(&connection));
        drop(connection);
        server.join();
    }
    let registry = &fixture.service.connections;
    assert_eq!(registry.lock().unwrap().len(), 9);
    let observation = registry.work_boundary().try_observe().unwrap();
    let baseline = retained.iter().map(Weak::strong_count).collect::<Vec<_>>();
    let mut visited = 0;
    registry
        .visit_connections::<ConnectionWorkError>(|connection| {
            let guard = registry.lock().unwrap();
            assert!(Arc::ptr_eq(connection, &guard[visited]));
            for (index, weak) in retained.iter().enumerate() {
                assert_eq!(
                    weak.strong_count(),
                    baseline[index] + usize::from(index == visited)
                );
            }
            visited += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(visited, 9);
    registry
        .work_boundary()
        .try_elect(&observation, || ())
        .unwrap();
    {
        let mut membership = registry.lock().unwrap();
        membership.reverse();
        assert!(matches!(
            registry.work_boundary().try_observe(),
            Err(crate::cas_projection::runtime_work::RuntimeWorkError::Busy)
        ));
        membership.reverse();
    }
    assert!(matches!(
        registry.work_boundary().try_elect(&observation, || ()),
        Err(crate::cas_projection::runtime_work::RuntimeWorkError::Stale)
    ));
    visited = 0;
    registry
        .visit_cleanup_connections(
            crate::cas_projection::service_registry::ConnectionCleanupMode::Dispose,
            None,
            |connection| {
                let guard = registry.lock().unwrap();
                assert!(Arc::ptr_eq(connection, &guard[visited]));
                for (index, weak) in retained.iter().enumerate() {
                    assert_eq!(
                        weak.strong_count(),
                        baseline[index] + usize::from(index == visited)
                    );
                }
                visited += 1;
                Ok(crate::cas_projection::service_registry::ConnectionCleanupDisposition::Retain)
            },
        )
        .unwrap();
    assert_eq!(visited, 9);
    assert!(
        !fixture
            .service
            .shutdown_work_revision(&fixture.sessions)
            .unwrap()
            .requires_connection_cleanup()
    );
    assert!(matches!(
        fixture
            .service
            .poll_shutdown_connection_cleanup(&ProjectionCancellationToken::new()),
        Err(ProcessWorkError::Projection(
            crate::cas_projection::ProjectionCoordinatorError::ProjectionWorkerStopped
        ))
    ));
    assert_eq!(registry.lock().unwrap().len(), 9);
}

#[test]
fn connection_membership_observation_tracks_mutable_retired_owners_without_workers() {
    use crate::cas_projection::runtime_work::RuntimeWorkError;
    let fixture = Fixture::idle();
    let boundary = fixture.service.connections.work_boundary();
    let mut owners = Vec::new();
    for generation in 74_000..74_003 {
        let before = boundary.try_observe().unwrap();
        let server = NormalTerminalServer::spawn_admission_only_controlled_close();
        let session = admit(&fixture, &server, generation);
        assert!(matches!(
            boundary.try_elect(&before, || ()),
            Err(RuntimeWorkError::Stale)
        ));
        let cleanup = session
            .connection()
            .acquire_cleanup_owner()
            .unwrap()
            .unwrap();
        server.assert_quiet_and_close();
        server.join();
        drop(session);
        let deadline = std::time::Instant::now() + TIMEOUT;
        while fixture.service.worker_pool_diagnostics().active() != 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "connection workers did not return"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        owners.push(cleanup);
    }
    assert_eq!(fixture.service.connections.lock().unwrap().len(), 3);
    assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
    while let Some(owner) = owners.pop() {
        let before = boundary.try_observe().unwrap();
        drop(owner);
        assert!(matches!(
            boundary.try_elect(&before, || ()),
            Err(RuntimeWorkError::Stale)
        ));
    }
    let observation = boundary.try_observe().unwrap();
    boundary.try_elect(&observation, || ()).unwrap();
}

#[test]
fn empty_inventory_checks_cancellation_poison_and_revision_exhaustion() {
    let fixture = Fixture::idle();
    let registry = &fixture.service.connections;
    registry
        .visit_connections::<ConnectionWorkError>(|_| panic!("empty registry callback"))
        .unwrap();
    assert!(
        fixture
            .service
            .poll_shutdown_connection_cleanup(&ProjectionCancellationToken::new())
            .unwrap()
    );
    let cancelled = ProjectionCancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        fixture.service.poll_shutdown_connection_cleanup(&cancelled),
        Err(ProcessWorkError::Cancelled)
    ));
    registry.exhaust_revision_for_test();
    assert!(matches!(
        registry.visit_connections::<ConnectionWorkError>(|_| Ok(())),
        Err(ConnectionWorkError::RevisionUnavailable)
    ));
    registry.poison_for_test();
    assert!(matches!(
        registry.visit_connections::<ConnectionWorkError>(|_| Ok(())),
        Err(ConnectionWorkError::Poisoned)
    ));
}

#[test]
fn final_callback_membership_changes_including_aba_reject_completion() {
    let fixture = Fixture::idle();
    let server = NormalTerminalServer::spawn_admission_only();
    let session = admit(&fixture, &server, 73_010);
    let registry = &fixture.service.connections;
    let cancelled = ProjectionCancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        fixture.service.poll_shutdown_connection_cleanup(&cancelled),
        Err(ProcessWorkError::Cancelled)
    ));
    for mutation in 0..3 {
        let result = registry.visit_connections::<ConnectionWorkError>(|connection| {
            let mut guard = registry.lock().unwrap();
            match mutation {
                0 => guard.push(Arc::clone(connection)),
                1 => {
                    guard.pop();
                }
                _ => {
                    let last = guard.pop().unwrap();
                    guard.push(last);
                }
            }
            Ok(())
        });
        assert!(matches!(result, Err(ConnectionWorkError::StaleRevision)));
    }
    let result = registry.visit_connections::<ProcessWorkError>(|_| {
        registry.lock().unwrap().reverse();
        Err(ProcessWorkError::Cancelled)
    });
    assert!(matches!(result, Err(ProcessWorkError::Cancelled)));
    let connection = Arc::clone(session.connection());
    drop(session);
    connection.shutdown().unwrap();
    drop(connection);
    server.join();
}
