use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use beryl_home_store::{
    HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    ApplySettings, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};

use super::*;
use crate::cas_projection::{
    PersistentFailureCutHandoff, PersistentFailureCutState, PersistentFailureGeneration,
    PersistentFailureNotificationStatus, ScheduledOrdinaryAdmission,
    ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
};

struct FactoryProbe {
    epochs: Arc<AtomicUsize>,
    provider_shutdowns: Arc<AtomicUsize>,
    factory_shutdowns: Arc<AtomicUsize>,
}

struct ProviderProbe {
    shutdowns: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProviderFactory for FactoryProbe {
    fn create_epoch(
        &mut self,
        _context: ScheduledOrdinaryProviderEpochContext,
    ) -> Result<
        Box<dyn ScheduledOrdinaryExecutionProvider>,
        Box<dyn std::error::Error + Send + Sync + 'static>,
    > {
        self.epochs.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(ProviderProbe {
            shutdowns: Arc::clone(&self.provider_shutdowns),
        }))
    }

    fn shutdown(&mut self) {
        assert_eq!(
            self.provider_shutdowns.load(Ordering::SeqCst),
            self.epochs.load(Ordering::SeqCst),
            "every issued epoch view is fenced before the stable provider pool"
        );
        self.factory_shutdowns.fetch_add(1, Ordering::SeqCst);
    }
}

impl ScheduledOrdinaryExecutionProvider for ProviderProbe {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    faults: FaultController,
    state: BerylState,
    supervisor: RunningSessionRecoverySupervisor,
    epochs: Arc<AtomicUsize>,
    provider_shutdowns: Arc<AtomicUsize>,
    factory_shutdowns: Arc<AtomicUsize>,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let mut home = HomeStore::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        SyndicStorage::register(&mut home).unwrap();
        let state = BerylState::register(&mut home).unwrap();
        let epochs = Arc::new(AtomicUsize::new(0));
        let provider_shutdowns = Arc::new(AtomicUsize::new(0));
        let factory_shutdowns = Arc::new(AtomicUsize::new(0));
        let supervisor = RunningSessionRecoverySupervisor::start(
            home,
            ProjectionServiceConfig::try_new(8, 4).unwrap(),
            Box::new(FactoryProbe {
                epochs: Arc::clone(&epochs),
                provider_shutdowns: Arc::clone(&provider_shutdowns),
                factory_shutdowns: Arc::clone(&factory_shutdowns),
            }),
        )
        .unwrap();
        Self {
            _directory: directory,
            faults,
            state,
            supervisor,
            epochs,
            provider_shutdowns,
            factory_shutdowns,
        }
    }
}

#[test]
fn scoped_leases_share_one_pointer_exact_current_service() {
    let fixture = Fixture::new();
    let first = fixture.supervisor.acquire().unwrap();
    let second = fixture.supervisor.acquire().unwrap();

    assert!(std::ptr::eq(&*first, &*second));
    let _published_state = first.state();
    assert_eq!(fixture.supervisor.diagnostics().active_service_leases(), 2);
    assert_eq!(fixture.epochs.load(Ordering::SeqCst), 1);

    drop((first, second));
    fixture.supervisor.shutdown().unwrap();
    assert_eq!(fixture.provider_shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.factory_shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn successful_same_generation_verification_preserves_current_service() {
    let fixture = Fixture::new();
    let before = fixture.supervisor.acquire().unwrap();
    let service_pointer = std::ptr::from_ref::<ProjectionConnectionService>(&*before);
    let service_generation = before.service_generation();
    let home_generation = before.home_generation();
    let verification = fixture.faults.block_next(FaultPoint::BeforeVerification);
    let settings = fixture.state.settings();
    let update = SettingUpdate::new(
        SettingKey::DraftAutosaveInterval,
        ExpectedSettingRevision::Absent,
        SettingValue::draft_autosave_interval_seconds(1),
    );
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let command_home = before.live_home_command().unwrap();
    let mut command = HomeCommand::new(command_home.home().home_revision().unwrap());
    command
        .add(settings.apply(
            settings.revision(command_home.home()).unwrap(),
            ApplySettings::new(vec![update]).unwrap(),
        ))
        .unwrap();
    command_home.home().execute(command).unwrap_err();
    assert_eq!(
        command_home.home().health().state(),
        HomeHealthState::Verifying
    );
    let provider_home = before.retained_home_for_recovery();
    let provider_permit = before.live_command_authorizer().authorize().unwrap();
    let provider_join = provider_permit
        .verification_join(&provider_home, before.home_id(), home_generation)
        .unwrap();
    before.signal_accepted_next_ready_for_test();

    assert!(
        verification.wait_until_reached(Duration::from_secs(5)),
        "a scheduler verifying observation signals the supervisor flight"
    );
    wait_until_named("scheduler verification pause", || {
        before
            .accepted_input_scheduler_diagnostics()
            .verification_pauses()
            == 1
    });
    let paused_scheduler = before.accepted_input_scheduler_diagnostics();
    assert!(!paused_scheduler.fatal());
    assert!(!paused_scheduler.stopped());
    assert!(before.live_command_authorizer().is_open());
    assert_eq!(before.service_generation(), service_generation);
    assert_eq!(before.home_generation(), home_generation);
    assert_eq!(
        std::ptr::from_ref::<ProjectionConnectionService>(&*before),
        service_pointer
    );
    let paused_pass_count = paused_scheduler.pass_count();
    drop(command_home);
    drop(before);
    let during = fixture.supervisor.acquire().unwrap();

    verification.release();
    assert!(
        provider_join
            .settle_after_operation()
            .unwrap()
            .verified_current()
    );
    assert!(provider_permit.is_current());
    wait_until_named("verification success", || {
        fixture.supervisor.diagnostics().verification_successes() == 1
    });
    wait_until_named("scheduler resume pass", || {
        during.accepted_input_scheduler_diagnostics().pass_count() > paused_pass_count
    });
    drop(during);
    let after = fixture.supervisor.acquire().unwrap();
    assert_eq!(after.service_generation(), service_generation);
    assert_eq!(after.home_generation(), home_generation);
    assert_eq!(
        std::ptr::from_ref::<ProjectionConnectionService>(&*after),
        service_pointer
    );
    let scheduler = after.accepted_input_scheduler_diagnostics();
    assert!(
        !scheduler.fatal(),
        "same-generation verification must not poison the current scheduler: {scheduler:?}"
    );
    assert!(
        !scheduler.stopped(),
        "same-generation verification must leave the current scheduler running: {scheduler:?}"
    );
    drop(after);
    drop((provider_permit, provider_home));

    fixture.supervisor.shutdown().unwrap();
    assert_eq!(fixture.epochs.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.provider_shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.factory_shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn failed_verification_completes_provider_waiter_before_command_drain() {
    let fixture = Fixture::new();
    let service = fixture.supervisor.acquire().unwrap();
    let home = service.retained_home_for_recovery();
    let home_generation = service.home_generation();
    let permit = service.live_command_authorizer().authorize().unwrap();
    let settings = fixture.state.settings();
    let update = SettingUpdate::new(
        SettingKey::DraftAutosaveInterval,
        ExpectedSettingRevision::Absent,
        SettingValue::draft_autosave_interval_seconds(2),
    );
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(settings.apply(
            settings.revision(&home).unwrap(),
            ApplySettings::new(vec![update]).unwrap(),
        ))
        .unwrap();
    home.execute(command).unwrap_err();
    assert_eq!(home.health().state(), HomeHealthState::Verifying);

    let join = permit
        .verification_join(&home, service.home_id(), home_generation)
        .unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeVerification);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::VerificationSignaled
    );
    assert_eq!(
        join.settle_after_operation(),
        Err(crate::cas_projection::LiveCommandAdmissionError::Closed)
    );
    assert!(!permit.is_current());
    wait_until(|| fixture.supervisor.diagnostics().recovering());
    assert_eq!(
        fixture.supervisor.diagnostics().recovery_cycles(),
        0,
        "the provider waiter completes before the still-owned command permit can drain"
    );

    drop((permit, home, service));
    wait_until(|| fixture.supervisor.diagnostics().recovery_cycles() == 1);
    fixture.supervisor.shutdown().unwrap();
}

#[test]
fn stale_slot_validation_completes_exact_provider_waiter() {
    let fixture = Fixture::new();
    let service = fixture.supervisor.acquire().unwrap();
    let home = service.retained_home_for_recovery();
    let home_generation = service.home_generation();
    let permit = service.live_command_authorizer().authorize().unwrap();
    let notification = service.persistent_failure_notification();
    let settings = fixture.state.settings();
    let update = SettingUpdate::new(
        SettingKey::DraftAutosaveInterval,
        ExpectedSettingRevision::Absent,
        SettingValue::draft_autosave_interval_seconds(3),
    );
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(settings.apply(
            settings.revision(&home).unwrap(),
            ApplySettings::new(vec![update]).unwrap(),
        ))
        .unwrap();
    home.execute(command).unwrap_err();
    let join = permit
        .verification_join(&home, service.home_id(), home_generation)
        .unwrap();
    let verification = fixture.faults.block_next(FaultPoint::BeforeVerification);
    assert_eq!(
        notification.notify(),
        PersistentFailureNotificationStatus::VerificationSignaled
    );
    assert!(verification.wait_until_reached(Duration::from_secs(5)));

    assert!(
        fixture
            .supervisor
            .slot
            .complete_same_generation_verification(
                &home,
                home_generation,
                ProjectionServiceGeneration::allocate().unwrap(),
                &notification,
            )
            .is_none()
    );
    assert_eq!(
        join.settle_after_operation(),
        Err(crate::cas_projection::LiveCommandAdmissionError::Closed)
    );
    assert!(!permit.is_current());
    wait_until(|| {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Cutting
    });

    verification.release();
    drop(permit);
    wait_until(|| {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    drop((home, service));
    assert!(fixture.supervisor.shutdown().is_err());
}

#[test]
fn shutdown_completes_provider_waiter_before_service_drain() {
    let fixture = Fixture::new();
    let service = fixture.supervisor.acquire().unwrap();
    let home = service.retained_home_for_recovery();
    let home_generation = service.home_generation();
    let permit = service.live_command_authorizer().authorize().unwrap();
    let settings = fixture.state.settings();
    let update = SettingUpdate::new(
        SettingKey::DraftAutosaveInterval,
        ExpectedSettingRevision::Absent,
        SettingValue::draft_autosave_interval_seconds(4),
    );
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(settings.apply(
            settings.revision(&home).unwrap(),
            ApplySettings::new(vec![update]).unwrap(),
        ))
        .unwrap();
    home.execute(command).unwrap_err();
    let join = permit
        .verification_join(&home, service.home_id(), home_generation)
        .unwrap();
    let verification = fixture.faults.block_next(FaultPoint::BeforeVerification);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::VerificationSignaled
    );
    assert!(verification.wait_until_reached(Duration::from_secs(5)));

    let supervisor = fixture.supervisor;
    let shutdown = std::thread::spawn(move || supervisor.shutdown());
    assert_eq!(
        join.settle_after_operation(),
        Err(crate::cas_projection::LiveCommandAdmissionError::Unavailable)
    );
    assert!(permit.is_current());

    verification.release();
    drop((permit, home, service));
    shutdown.join().unwrap().unwrap();
}

#[test]
fn two_sequential_failed_generations_publish_on_the_same_retained_home() {
    let fixture = Fixture::new();
    let initial = fixture.supervisor.acquire().unwrap();
    let home_id = initial.home_id();
    let retained_home = initial.retained_home_for_recovery();
    let retained_home_pointer = Arc::as_ptr(&retained_home);
    let mut home_generation = initial.home_generation();
    let mut service_generation = initial.service_generation();
    drop((retained_home, initial));

    for cycle in 1..=2 {
        fail_current_generation(&fixture, cycle);
        wait_until(|| fixture.supervisor.diagnostics().recovery_cycles() == cycle);

        let recovered = fixture.supervisor.acquire().unwrap();
        let current_home = recovered.retained_home_for_recovery();
        assert_eq!(recovered.home_id(), home_id);
        assert_eq!(Arc::as_ptr(&current_home), retained_home_pointer);
        assert!(recovered.home_generation() > home_generation);
        assert!(recovered.service_generation() > service_generation);
        home_generation = recovered.home_generation();
        service_generation = recovered.service_generation();
        drop((current_home, recovered));
        assert_eq!(fixture.epochs.load(Ordering::SeqCst), cycle as usize + 1);
        assert_eq!(
            fixture.provider_shutdowns.load(Ordering::SeqCst),
            cycle as usize
        );
    }

    fixture.supervisor.shutdown().unwrap();
    assert_eq!(fixture.provider_shutdowns.load(Ordering::SeqCst), 3);
    assert_eq!(fixture.factory_shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn shutdown_wake_consumed_by_recovery_retry_exits_the_worker() {
    let fixture = Fixture::new();
    let service = fixture.supervisor.acquire().unwrap();
    let cut_identity = (
        service.home_id(),
        service.home_generation(),
        service.service_generation(),
        PersistentFailureGeneration::FIRST,
    );
    drop(service);
    fixture.faults.fail_times(FaultPoint::BeforeReopen, 8);
    fail_current_generation(&fixture, 1);
    wait_until(|| {
        let diagnostics = fixture.supervisor.diagnostics();
        diagnostics.recovering() && diagnostics.current_service_generation().is_none()
    });
    assert!(PersistentFailureCutHandoff::escrow_registered_for_test(
        cut_identity.0,
        cut_identity.1,
        cut_identity.2,
        cut_identity.3,
    ));

    let provider_shutdowns = Arc::clone(&fixture.provider_shutdowns);
    let factory_shutdowns = Arc::clone(&fixture.factory_shutdowns);
    let supervisor = fixture.supervisor;
    let (finished, receiver) = mpsc::sync_channel(1);
    let shutdown = std::thread::spawn(move || {
        let result = supervisor.shutdown();
        let _ = finished.send(result);
    });

    receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    shutdown.join().unwrap();
    assert_eq!(provider_shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(factory_shutdowns.load(Ordering::SeqCst), 1);
    assert!(!PersistentFailureCutHandoff::escrow_registered_for_test(
        cut_identity.0,
        cut_identity.1,
        cut_identity.2,
        cut_identity.3,
    ));
}

fn fail_current_generation(fixture: &Fixture, cycle: u64) {
    let service = fixture.supervisor.acquire().unwrap();
    let state = service.state();
    let live = service.live_home_command().unwrap();
    let update = SettingUpdate::new(
        SettingKey::DeveloperInstructions,
        ExpectedSettingRevision::Absent,
        SettingValue::developer_instructions(format!("Phase 86 cycle {cycle}")).unwrap(),
    );
    let mut command = HomeCommand::new(live.home().home_revision().unwrap());
    command
        .add(state.settings().apply(
            state.settings().revision(live.home()).unwrap(),
            ApplySettings::new(vec![update]).unwrap(),
        ))
        .unwrap();
    fixture.faults.panic_next(FaultPoint::BeforeCommit);

    assert!(catch_unwind(AssertUnwindSafe(|| live.home().execute(command))).is_err());
    assert_eq!(live.home().health().state(), HomeHealthState::Failed);
    drop(live);
    drop(service);
}

fn wait_until(predicate: impl FnMut() -> bool) {
    wait_until_named("supervisor state", predicate);
}

fn wait_until_named(name: &str, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out waiting for {name}");
        std::thread::yield_now();
    }
}
