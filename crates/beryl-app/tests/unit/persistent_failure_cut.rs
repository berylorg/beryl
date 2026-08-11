use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use beryl_backend::ManagedBackendClientConnector;

use beryl_home_store::{
    HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};
use syndic_storage::SyndicStorage;
use beryl_model::{CasProcessGeneration, RuntimeId};

use super::*;

mod terminal_server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/phase37_normal_terminal/server.rs"
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
        home,
        storage,
        ProjectionServiceConfig::try_new(8, 4).unwrap(),
        Box::new(ShutdownProbe(Arc::clone(&shutdowns))),
    )
    .unwrap();
    (directory, faults, state, shutdowns, service)
}

fn fail_home(
    service: &ProjectionConnectionService,
    state: BerylState,
    faults: &FaultController,
) {
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
    fail_home(&service, state, &faults);
    wait_until("the persistent-failure cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });

    let evidence = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(evidence) => evidence,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the persistent-failure winner must return terminal evidence")
        }
    };

    assert_eq!(evidence.home_id(), home_id);
    assert_eq!(evidence.home_generation(), home_generation);
    assert_eq!(evidence.service_generation(), service_generation);
    assert_eq!(evidence.completion(), PersistentFailureCutCompletion::Finished);
    assert_eq!(evidence.cut_snapshot().state(), PersistentFailureCutState::Finished);
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
