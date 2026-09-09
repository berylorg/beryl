#![cfg(all(feature = "test-faults", target_os = "windows"))]

#[allow(dead_code)]
#[path = "support/managed_runtime.rs"]
mod support;

#[allow(dead_code)]
#[path = "normal_terminal/server.rs"]
mod protocol;

use beryl_app::cas_projection::{
    AdmittedProjectionSession, ProjectionConnectionServiceCloseOutcome, RuntimeFailure,
    RuntimeInterestKind, RuntimeInterestStatus,
};
use beryl_backend::ManagedBackendClientConnector;
use beryl_model::{CasProcessGeneration, RuntimeId};
use support::{Fixture, ProcessWitness, TIMEOUT, ready, wait_until};

#[test]
fn retired_connection_detaches_without_later_activity_and_preserves_active_sibling() {
    let mut fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    let initial = ready(&view);
    let first = fixture
        .acquire(1, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let first = fixture
        .service()
        .admit_runtime_session(first, TIMEOUT)
        .unwrap();
    let retired = first.connection_retirement_handle_for_test();
    let second = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let second = fixture
        .service()
        .admit_runtime_session(second, TIMEOUT)
        .unwrap();
    let active = second.connection_retirement_handle_for_test();
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 6);

    drop(first);
    wait_until(|| retired.is_detached());
    assert!(retired.is_retired());
    assert!(!active.is_retired());
    assert!(!active.is_detached());
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 4);
    assert_eq!(ready(&view), initial);
    assert_eq!(fixture.token_count(), 1);

    drop(second);
    wait_until(|| active.is_detached());
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 2);
    drop(view);
    wait_until(|| fixture.token_count() == 0);
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
}

#[derive(Clone, Copy)]
enum PoisonedCleanup {
    WorkerDisposition,
    IngesterHandle,
    ShutdownSettlement,
}

fn failed_cleanup_releases_without_external_activity(poison: PoisonedCleanup) {
    let mut fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let session = fixture
        .service()
        .admit_runtime_session(work, TIMEOUT)
        .unwrap();
    let retired = session.connection_retirement_handle_for_test();
    match poison {
        PoisonedCleanup::WorkerDisposition => retired.poison_worker_disposition(),
        PoisonedCleanup::IngesterHandle => retired.poison_ingester_handle(),
        PoisonedCleanup::ShutdownSettlement => retired.poison_shutdown_settlement(),
    }

    drop(session);
    wait_until(|| {
        fixture.token_count() == 0 && fixture.service().worker_pool_diagnostics().active() == 0
    });
    process.assert_exited();
    assert!(retired.is_detached());
    assert_eq!(
        view.status(),
        RuntimeInterestStatus::Unavailable(RuntimeFailure::AppRetirement)
    );
    drop(view);
    let _ = fixture.service.take().unwrap().close();
}

#[test]
fn poisoned_worker_disposition_is_consumed_without_later_activity() {
    failed_cleanup_releases_without_external_activity(PoisonedCleanup::WorkerDisposition);
}

#[test]
fn poisoned_ingester_handle_is_consumed_without_later_activity() {
    failed_cleanup_releases_without_external_activity(PoisonedCleanup::IngesterHandle);
}

#[test]
fn poisoned_shutdown_settlement_is_consumed_without_later_activity() {
    failed_cleanup_releases_without_external_activity(PoisonedCleanup::ShutdownSettlement);
}

fn unmanaged_session(
    fixture: &Fixture,
) -> (protocol::NormalTerminalServer, AdmittedProjectionSession) {
    let server = protocol::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        protocol::AUTHORIZATION,
    );
    let session = fixture
        .service()
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([77; 16]),
            CasProcessGeneration::new(77_001).unwrap(),
            &fixture.root(1),
            TIMEOUT,
        )
        .unwrap();
    server.wait_for_admission();
    (server, session)
}

#[test]
fn runtime_sweep_does_not_consume_foreign_runtime_or_process_retirement() {
    let mut fixture = Fixture::new();
    let (server, session) = unmanaged_session(&fixture);
    let retired = session.connection_retirement_handle_for_test();
    retired.poison_worker_disposition();
    drop(session);
    assert!(retired.is_retired());
    let exact_runtime = RuntimeId::from_bytes([77; 16]);
    let exact_process = CasProcessGeneration::new(77_001).unwrap();
    fixture
        .service()
        .poll_runtime_retirements_for_test(RuntimeId::from_bytes([78; 16]), exact_process)
        .unwrap();
    fixture
        .service()
        .poll_runtime_retirements_for_test(
            exact_runtime,
            CasProcessGeneration::new(77_002).unwrap(),
        )
        .unwrap();
    assert!(!retired.is_detached());
    assert!(fixture.service().worker_pool_diagnostics().active() >= 1);

    wait_until(|| {
        fixture
            .service()
            .poll_runtime_retirements_for_test(exact_runtime, exact_process)
            == Err(RuntimeFailure::AppRetirement)
    });
    assert!(retired.is_detached());
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 0);
    let _ = fixture.service.take().unwrap().close();
    server.join();
}

#[test]
fn runtime_sweep_defers_contended_settlement_and_consumes_after_release() {
    let mut fixture = Fixture::new();
    let (server, session) = unmanaged_session(&fixture);
    let retired = session.connection_retirement_handle_for_test();
    drop(session);
    assert!(retired.is_retired());
    let poll = || {
        fixture.service().poll_runtime_retirements_for_test(
            RuntimeId::from_bytes([77; 16]),
            CasProcessGeneration::new(77_001).unwrap(),
        )
    };
    retired.with_shutdown_settlement(|| {
        assert!(poll().is_ok());
        assert!(!retired.is_detached());
    });
    wait_until(|| {
        poll().unwrap();
        retired.is_detached()
    });
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 0);
    assert!(matches!(
        fixture.service.take().unwrap().close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
}

#[test]
fn failed_retirement_waits_for_persistent_failure_cut_before_consuming_resources() {
    let mut fixture = Fixture::new();
    let view = fixture.acquire(1, RuntimeInterestKind::View).unwrap();
    ready(&view);
    let process = ProcessWitness::open(fixture.evidence(1)["pid"].as_u64().unwrap() as u32);
    let work = fixture
        .acquire(2, RuntimeInterestKind::RequiredWork)
        .unwrap();
    let session = fixture
        .service()
        .admit_runtime_session(work, TIMEOUT)
        .unwrap();
    let retired = session.connection_retirement_handle_for_test();
    retired.poison_worker_disposition();
    let admitted = fixture.service().live_home_command().unwrap();
    fixture.fail_home();
    drop(session);
    wait_until(|| fixture.service().runtime_retirement_waiters_for_test() == 1);
    assert!(!retired.is_detached());
    assert!(process.running());
    assert_eq!(fixture.service().worker_pool_diagnostics().active(), 4);

    drop(admitted);
    wait_until(|| {
        fixture.token_count() == 0 && fixture.service().worker_pool_diagnostics().active() == 0
    });
    process.assert_exited();
    assert!(retired.is_detached());
    drop(view);
    let _ = fixture.service.take().unwrap().close();
}
