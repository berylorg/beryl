use super::*;
use crate::cas_projection::service_registry::{
    ConnectionCleanupDisposition, ConnectionCleanupError, ConnectionCleanupMode,
};

#[test]
fn poisoned_and_exhausted_runtime_disposal_join_later_connections_after_a_failed_join() {
    for poison in [true, false] {
        let fixture = Fixture::with_worker_capacity(false, 8);
        let first_server = NormalTerminalServer::spawn_admission_only();
        let first = admit(&fixture, &first_server, 73_100);
        let first_connection = Arc::clone(first.connection());
        let second_server = NormalTerminalServer::spawn_admission_only();
        let second = admit(&fixture, &second_server, 73_100);
        let second_connection = Arc::clone(second.connection());
        let retirement = fixture
            .service
            .admission_context()
            .unwrap()
            .runtime_retirement(
                RuntimeId::from_bytes([71; 16]),
                CasProcessGeneration::new(73_100).unwrap(),
            );
        first_connection.fail_next_ingester_join_for_test();
        drop(first);
        drop(second);
        if poison {
            fixture.service.connections.poison_for_test();
        } else {
            fixture.service.connections.exhaust_revision_for_test();
        }
        assert!(!retirement.retire());
        assert!(first_connection.is_detached());
        assert!(second_connection.is_detached());
        assert_eq!(fixture.service.worker_pool_diagnostics().active(), 0);
        let registry = fixture
            .service
            .connections
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert_eq!(registry.len(), 1);
        assert!(Arc::ptr_eq(&registry[0], &first_connection));
        drop(registry);
        drop(retirement);
        drop(first_connection);
        drop(second_connection);
        first_server.join();
        second_server.join();
    }
}

#[test]
fn consuming_disposal_accounts_for_own_removal_and_continues_after_external_drift() {
    for change_membership in [false, true] {
        let fixture = Fixture::with_worker_capacity(false, 8);
        let first_server = NormalTerminalServer::spawn_admission_only();
        let first = admit(&fixture, &first_server, 73_101);
        let first_connection = Arc::clone(first.connection());
        let second_server = NormalTerminalServer::spawn_admission_only();
        let second = admit(&fixture, &second_server, 73_102);
        let second_connection = Arc::clone(second.connection());
        drop(first);
        drop(second);
        let registry = &fixture.service.connections;
        let mut visited = 0;
        let outcome = registry.visit_cleanup_connections(
            ConnectionCleanupMode::Dispose,
            None,
            |connection| {
                let mut guard = registry.lock().unwrap();
                assert!(guard.iter().any(|entry| Arc::ptr_eq(entry, connection)));
                if change_membership && visited == 0 {
                    guard.reverse();
                }
                drop(guard);
                visited += 1;
                connection.shutdown_for_runtime_retirement().unwrap();
                Ok(ConnectionCleanupDisposition::RemoveClean)
            },
        );
        assert_eq!(
            outcome,
            if change_membership {
                Err(ConnectionCleanupError::Failed)
            } else {
                Ok(())
            }
        );
        assert_eq!(visited, 2);
        assert_eq!(registry.lock().unwrap().len(), 0);
        assert!(first_connection.is_detached());
        assert!(second_connection.is_detached());
        drop(first_connection);
        drop(second_connection);
        first_server.join();
        second_server.join();
    }
}

#[test]
fn opportunistic_cleanup_defers_contention_and_membership_changes() {
    let fixture = Fixture::idle();
    let registry = &fixture.service.connections;
    let guard = registry.lock().unwrap();
    std::thread::scope(|scope| {
        let (sent, received) = std::sync::mpsc::sync_channel(1);
        let worker = scope.spawn(move || {
            sent.send(registry.visit_cleanup_connections(
                ConnectionCleanupMode::Inspect,
                None,
                |_| panic!("contended inspection callback"),
            ))
            .unwrap();
        });
        let result = received.recv_timeout(std::time::Duration::from_secs(2));
        drop(guard);
        worker.join().unwrap();
        assert_eq!(result.unwrap(), Err(ConnectionCleanupError::Deferred));
    });
    let server = NormalTerminalServer::spawn_admission_only();
    let session = admit(&fixture, &server, 73_103);
    let connection = Arc::clone(session.connection());
    assert_eq!(
        registry.visit_cleanup_connections(ConnectionCleanupMode::Inspect, None, |_| {
            registry.lock().unwrap().reverse();
            Ok(ConnectionCleanupDisposition::Retain)
        }),
        Err(ConnectionCleanupError::Deferred)
    );
    registry.exhaust_revision_for_test();
    assert_eq!(
        registry.visit_cleanup_connections(ConnectionCleanupMode::Inspect, None, |_| {
            panic!("exhausted inspection callback")
        }),
        Err(ConnectionCleanupError::Failed)
    );
    registry.poison_for_test();
    assert_eq!(
        registry.visit_cleanup_connections(ConnectionCleanupMode::Inspect, None, |_| {
            panic!("poisoned inspection callback")
        }),
        Err(ConnectionCleanupError::Failed)
    );
    drop(session);
    connection.shutdown().unwrap();
    drop(connection);
    server.join();
}

#[test]
fn implicit_shutdown_drains_registry_and_joins_connections_when_registry_is_poisoned() {
    let fixture = Fixture::with_worker_capacity(false, 8);
    let first_server = NormalTerminalServer::spawn_admission_only();
    let first = admit(&fixture, &first_server, 73_104);
    let first_connection = Arc::clone(first.connection());
    let second_server = NormalTerminalServer::spawn_admission_only();
    let second = admit(&fixture, &second_server, 73_105);
    let second_connection = Arc::clone(second.connection());
    let registry = Arc::clone(&fixture.service.connections);
    registry.poison_for_test();
    drop(fixture.service);
    assert!(first_connection.is_retired());
    assert!(second_connection.is_retired());
    assert_eq!(
        registry
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .len(),
        0
    );
    drop(first);
    drop(second);
    assert!(first_connection.is_detached());
    assert!(second_connection.is_detached());
    drop(first_connection);
    drop(second_connection);
    first_server.join();
    second_server.join();
}

#[test]
fn disposal_rejects_late_membership_without_expanding_its_initial_scope() {
    let fixture = Fixture::with_worker_capacity(false, 8);
    let first_server = NormalTerminalServer::spawn_admission_only();
    let first = admit(&fixture, &first_server, 73_106);
    let first_connection = Arc::clone(first.connection());
    let second_server = NormalTerminalServer::spawn_admission_only();
    let second = admit(&fixture, &second_server, 73_107);
    let second_connection = Arc::clone(second.connection());
    let registry = &fixture.service.connections;
    let mut later = Some(registry.lock().unwrap().pop().unwrap());
    assert!(Arc::ptr_eq(later.as_ref().unwrap(), &second_connection));
    let mut visited = 0;
    assert_eq!(
        registry.visit_cleanup_connections(ConnectionCleanupMode::Dispose, None, |connection| {
            assert!(Arc::ptr_eq(connection, &first_connection));
            visited += 1;
            registry.lock().unwrap().push(later.take().unwrap());
            Ok(ConnectionCleanupDisposition::Retain)
        }),
        Err(ConnectionCleanupError::Failed)
    );
    assert_eq!(visited, 1);
    assert_eq!(registry.lock().unwrap().len(), 2);
    drop(first);
    drop(second);
    first_connection.shutdown().unwrap();
    second_connection.shutdown().unwrap();
    drop(first_connection);
    drop(second_connection);
    first_server.join();
    second_server.join();
}

#[test]
fn final_service_close_reports_invalid_registry_ownership_after_joining() {
    for poison in [true, false] {
        let fixture = Fixture::idle();
        let server = NormalTerminalServer::spawn_admission_only();
        let session = admit(&fixture, &server, 73_108);
        let connection = Arc::clone(session.connection());
        drop(session);
        if poison {
            fixture.service.connections.poison_for_test();
        } else {
            fixture.service.connections.exhaust_revision_for_test();
        }
        assert!(fixture.service.close().is_err());
        assert!(connection.is_detached());
        drop(connection);
        server.join();
    }
}
