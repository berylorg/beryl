use beryl_backend::ManagedBackendClientConnector;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use beryl_home_store::{
    HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{CasProcessGeneration, RuntimeId};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};
use syndic_storage::SyndicStorage;

use super::*;
use crate::cas_projection::MinimumTurnCaptureReserve;

mod terminal_server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/normal_terminal/server.rs"
    ));
}

#[derive(Clone)]
struct ShutdownProbe(Arc<AtomicUsize>);

impl ScheduledOrdinaryExecutionProvider for ShutdownProbe {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn service() -> (
    tempfile::TempDir,
    FaultController,
    BerylState,
    Arc<AtomicUsize>,
    ProjectionConnectionService,
) {
    service_with_worker_capacity(4)
}

fn service_with_worker_capacity(
    worker_capacity: u64,
) -> (
    tempfile::TempDir,
    FaultController,
    BerylState,
    Arc<AtomicUsize>,
    ProjectionConnectionService,
) {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut home = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let shutdowns = Arc::new(AtomicUsize::new(0));
    let service = ProjectionConnectionService::new(
        Default::default(),
        home,
        storage,
        ProjectionServiceConfig::try_new(
            8,
            worker_capacity,
            MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap(),
        Box::new(ShutdownProbe(Arc::clone(&shutdowns))),
    )
    .unwrap();
    (directory, faults, state, shutdowns, service)
}

fn fail_home(service: &ProjectionConnectionService, state: BerylState, faults: &FaultController) {
    let live = service.live_home_command().unwrap();
    let home = live.home();
    let update = SettingUpdate::new(
        SettingKey::DeveloperInstructions,
        ExpectedSettingRevision::Absent,
        SettingValue::developer_instructions("terminal persistent failure").unwrap(),
    );
    let contribution = state.settings().apply(
        state.settings().revision(home).unwrap(),
        ApplySettings::new(vec![update]).unwrap(),
    );
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    faults.panic_next(FaultPoint::BeforeCommit);
    let outcome = catch_unwind(AssertUnwindSafe(|| home.execute(command)));
    assert!(outcome.is_err());
    assert_eq!(home.health().state(), HomeHealthState::Failed);
    drop(live);
}

fn wait_until(description: &str, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        std::thread::yield_now();
    }
}

#[test]
fn persistent_failure_close_returns_only_terminal_evidence_and_disposes_workers() {
    let (_directory, faults, state, shutdowns, service) = service();
    let home_id = service.home_id();
    let home_generation = service.home_generation();
    let service_generation = service.service_generation();
    let stop_revision = service.stop_work_revision().unwrap();
    let control_revision = service.control_work_revision().unwrap();
    let compaction_revision = service.compaction_work_revision().unwrap();
    let mut foreign_compaction_revision = compaction_revision.clone();
    foreign_compaction_revision.owner = Arc::new(());
    let foreign_cursor = crate::cas_projection::CompactionWorkCursor {
        revision: foreign_compaction_revision,
        after: 1,
    };
    let compaction_limits =
        crate::cas_projection::CompactionWorkPageLimits::new(1, 65_536).unwrap();
    assert_eq!(
        service.compaction_work_page(
            &compaction_revision,
            Some(&foreign_cursor),
            compaction_limits
        ),
        Err(crate::cas_projection::CompactionWorkError::ForeignRevision)
    );
    let mut different_cut = compaction_revision.clone();
    different_cut.stamp += 1;
    let different_cursor = crate::cas_projection::CompactionWorkCursor {
        revision: different_cut,
        after: 1,
    };
    assert_eq!(
        service.compaction_work_page(
            &compaction_revision,
            Some(&different_cursor),
            compaction_limits
        ),
        Err(crate::cas_projection::CompactionWorkError::ForeignRevision)
    );
    assert_eq!(
        service.compaction_work_revision().unwrap(),
        compaction_revision
    );
    fail_home(&service, state, &faults);
    wait_until("the persistent-failure cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    assert_eq!(
        service.control_work_revision(),
        Err(crate::cas_projection::ControlWorkError::Stop(
            crate::cas_projection::StopWorkError::Closed
        ))
    );
    assert_eq!(
        service.validate_control_work_revision(&control_revision),
        Err(crate::cas_projection::ControlWorkError::Stop(
            crate::cas_projection::StopWorkError::Closed
        ))
    );
    assert_eq!(
        service.control_work_page(
            &control_revision,
            None,
            crate::cas_projection::ControlWorkPageLimits::new(1, 65_536).unwrap()
        ),
        Err(crate::cas_projection::ControlWorkError::Stop(
            crate::cas_projection::StopWorkError::Closed
        ))
    );
    assert_eq!(
        service.compaction_work_revision(),
        Err(crate::cas_projection::CompactionWorkError::Closed)
    );
    assert_eq!(
        service.validate_compaction_work_revision(&compaction_revision),
        Err(crate::cas_projection::CompactionWorkError::Closed)
    );
    assert_eq!(
        service.compaction_work_page(&compaction_revision, None, compaction_limits),
        Err(crate::cas_projection::CompactionWorkError::Closed)
    );
    assert_eq!(
        service.stop_work_revision(),
        Err(crate::cas_projection::StopWorkError::Closed)
    );
    assert_eq!(
        service.validate_stop_work_revision(&stop_revision),
        Err(crate::cas_projection::StopWorkError::Closed)
    );
    assert_eq!(
        service.stop_work_page(
            &stop_revision,
            None,
            crate::cas_projection::StopWorkPageLimits::new(1, 65_536).unwrap()
        ),
        Err(crate::cas_projection::StopWorkError::Closed)
    );

    let evidence = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(evidence) => evidence,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the persistent-failure winner must return terminal evidence")
        }
    };

    assert_eq!(evidence.home_id(), home_id);
    assert_eq!(evidence.home_generation(), home_generation);
    assert_eq!(evidence.service_generation(), service_generation);
    assert_eq!(
        evidence.completion(),
        PersistentFailureCutCompletion::Finished
    );
    assert_eq!(
        evidence.cut_snapshot().state(),
        PersistentFailureCutState::Finished
    );
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn ordinary_close_remains_exact_and_shuts_provider_once() {
    let (_directory, _faults, _state, shutdowns, service) = service();
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn persistent_failure_close_joins_and_detaches_an_admitted_connection() {
    let (_directory, faults, state, shutdowns, service) = service();
    let server = terminal_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        terminal_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([99; 16]),
            CasProcessGeneration::new(99_001).unwrap(),
            Path::new(r"C:\work\beryl"),
            terminal_server::TIMEOUT,
        )
        .unwrap();
    let retirement = session.connection_retirement_handle_for_test();
    server.wait_for_admission();
    assert!(service.worker_pool_diagnostics().active() >= 2);

    fail_home(&service, state, &faults);
    wait_until("the admitted-connection failure cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });

    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(_)
    ));
    assert!(retirement.is_retired());
    assert!(retirement.is_detached());
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    drop(session);
    drop(retirement);
    server.join();
}

#[test]
fn terminal_close_reports_an_unclean_ingester_receipt_after_full_detach() {
    let (_directory, faults, state, shutdowns, service) = service();
    let server = terminal_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        terminal_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([100; 16]),
            CasProcessGeneration::new(100_001).unwrap(),
            Path::new(r"C:\work\beryl"),
            terminal_server::TIMEOUT,
        )
        .unwrap();
    let retirement = session.connection_retirement_handle_for_test();
    server.wait_for_admission();
    retirement.fail_next_ingester_join();

    fail_home(&service, state, &faults);
    wait_until("the unclean-ingester failure cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });

    assert!(matches!(
        service.close(),
        Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
    ));
    assert!(retirement.is_retired());
    assert!(retirement.is_detached());
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    drop(session);
    drop(retirement);
    server.join();
}

#[test]
fn terminal_close_recovers_a_poisoned_ingester_handle_before_reporting_failure() {
    let (_directory, faults, state, shutdowns, service) = service();
    let server = terminal_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        terminal_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([101; 16]),
            CasProcessGeneration::new(101_001).unwrap(),
            Path::new(r"C:\work\beryl"),
            terminal_server::TIMEOUT,
        )
        .unwrap();
    let retirement = session.connection_retirement_handle_for_test();
    server.wait_for_admission();
    retirement.poison_ingester_handle();

    fail_home(&service, state, &faults);
    wait_until("the poisoned-ingester failure cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });

    assert!(matches!(
        service.close(),
        Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
    ));
    assert!(retirement.is_retired());
    assert!(retirement.is_detached());
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    drop(session);
    drop(retirement);
    server.join();
}

fn admitted_connection(
    service: &ProjectionConnectionService,
    generation: u64,
) -> (
    terminal_server::NormalTerminalServer,
    super::super::AdmittedProjectionSession,
) {
    let server = terminal_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        terminal_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([102; 16]),
            CasProcessGeneration::new(generation).unwrap(),
            Path::new(r"C:\work\beryl"),
            terminal_server::TIMEOUT,
        )
        .unwrap();
    server.wait_for_admission();
    (server, session)
}

#[path = "persistent_failure_capture.rs"]
mod capture;

#[test]
fn persistent_failure_cut_preserves_detached_failed_join_and_disposes_later_connection() {
    let (_directory, faults, state, shutdowns, service) = service();
    let (failed_server, failed_session) = admitted_connection(&service, 102_001);
    let failed = Arc::clone(failed_session.connection());
    failed.fail_next_ingester_join_for_test();
    drop(failed_session);
    assert!(failed.shutdown().is_err());
    assert!(failed.is_detached());
    failed_server.join();
    let (later_server, later_session) = admitted_connection(&service, 102_002);
    let later = Arc::clone(later_session.connection());
    assert!(!later.is_detached());
    fail_home(&service, state, &faults);
    wait_until("the failure-preserving cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    {
        let registry = service.connections.lock().unwrap();
        assert_eq!(registry.len(), 2);
        assert!(
            registry
                .iter()
                .any(|connection| Arc::ptr_eq(connection, &failed))
        );
    }
    assert!(matches!(
        service.close(),
        Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
    ));
    assert!(later.is_detached());
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    drop(later_session);
    drop(failed);
    drop(later);
    later_server.join();
}

#[test]
fn persistent_failure_close_reports_invalid_registry_ownership_after_joining() {
    for poison in [true, false] {
        let (_directory, faults, state, shutdowns, service) = service();
        let (server, session) = admitted_connection(&service, 102_003);
        let connection = Arc::clone(session.connection());
        fail_home(&service, state, &faults);
        wait_until("the cut before invalidating cleanup ownership", || {
            service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
        });
        if poison {
            service.connections.poison_for_test();
        } else {
            service.connections.exhaust_revision_for_test();
        }
        assert!(matches!(
            service.close(),
            Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
        ));
        assert!(connection.is_detached());
        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
        drop(session);
        drop(connection);
        server.join();
    }
}
