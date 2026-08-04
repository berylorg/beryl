use std::{
    num::NonZeroUsize,
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use beryl_backend::ManagedBackendClientConnector;
use beryl_home_store::{
    HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{CasProcessGeneration, RuntimeId};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};
use syndic_storage::SyndicStorage;

use super::*;
use crate::cas_projection::{
    ProjectionConnectionService, ProjectionServiceConfig, ScheduledOrdinaryAdmission,
    ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
    persistent_failure::{
        MasterCommandGate, PersistentFailureGeneration, ProjectionServiceGeneration,
    },
    service_config::ProjectionWorkerPool,
    service_startup::ServiceStartupGate,
};

mod admission_server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/phase37_normal_terminal/server.rs"
    ));
}

struct DriverTestExecutionProvider;

impl ScheduledOrdinaryExecutionProvider for DriverTestExecutionProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

struct AdoptionIdentityFixture {
    _directory: tempfile::TempDir,
    old_epoch: ConnectionEpochIdentity,
    replacement_epoch: ConnectionEpochIdentity,
    cut: PersistentFailureCutIdentity,
    frontier: PersistentFailureCommandFrontier,
}

fn adoption_identities() -> AdoptionIdentityFixture {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut home = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let home_id = home.home_id();
    let old_home_generation = home.health().generation().unwrap();
    let update = SettingUpdate::new(
        SettingKey::DeveloperInstructions,
        ExpectedSettingRevision::Absent,
        SettingValue::developer_instructions("phase 82 driver adoption").unwrap(),
    );
    let contribution = state.settings().apply(
        state.settings().revision(&home).unwrap(),
        ApplySettings::new(vec![update]).unwrap(),
    );
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    faults.panic_next(FaultPoint::BeforeCommit);
    assert!(catch_unwind(AssertUnwindSafe(|| home.execute(command))).is_err());
    let replacement_home_generation = home.recover_same_home().unwrap().generation();
    let old_service = ProjectionServiceGeneration::allocate().unwrap();
    let replacement_service = ProjectionServiceGeneration::allocate().unwrap();
    let failure_generation = PersistentFailureGeneration::FIRST;
    let gate = MasterCommandGate::new(old_service, None);
    assert!(
        gate.elect_persistent_failure_for_test(failure_generation)
            .unwrap()
    );
    let frontier = gate
        .authorizer()
        .persistent_failure_frontier(old_service, failure_generation)
        .unwrap();
    let cut = PersistentFailureCutIdentity::new(
        home_id,
        old_home_generation,
        old_service,
        failure_generation,
    );
    AdoptionIdentityFixture {
        _directory: directory,
        old_epoch: ConnectionEpochIdentity::new(home_id, old_home_generation, old_service),
        replacement_epoch: ConnectionEpochIdentity::new(
            home_id,
            replacement_home_generation,
            replacement_service,
        ),
        cut,
        frontier,
    }
}

fn connection_worker(pool: &ProjectionWorkerPool) -> ProjectionWorkerPermit {
    let mut pair = pool.try_acquire_pair().unwrap();
    let worker = pair.take_driver();
    drop(pair);
    worker
}

fn live_driver_service() -> (tempfile::TempDir, ProjectionConnectionService) {
    let directory = tempfile::tempdir().unwrap();
    let mut home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    BerylState::register(&mut home).unwrap();
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(8, 4).unwrap(),
        Box::new(DriverTestExecutionProvider),
    )
    .unwrap();
    (directory, service)
}

fn spawn_driver_cycle(
    slot: Arc<DriverAdoptionSlot>,
    worker: ProjectionWorkerPermit,
) -> (mpsc::Receiver<bool>, thread::JoinHandle<()>) {
    let (outcome_sender, outcome_receiver) = mpsc::sync_channel(1);
    let handle = thread::spawn(move || {
        let mut worker = Some(worker);
        loop {
            match slot.begin_cycle(&mut worker) {
                DriverAdoptionPoll::Work(guard) => {
                    drop(guard);
                    thread::yield_now();
                }
                DriverAdoptionPoll::Park { attempt } => {
                    let resumed = matches!(
                        slot.park_and_wait(attempt, &mut worker),
                        DriverParkWaitOutcome::Resumed(_)
                    );
                    outcome_sender.send(resumed).unwrap();
                    return;
                }
                DriverAdoptionPoll::AwaitDisposition => {
                    assert!(matches!(slot.wait_inert(), DriverParkWaitOutcome::Disposed));
                    outcome_sender.send(false).unwrap();
                    return;
                }
                DriverAdoptionPoll::Disposed => {
                    outcome_sender.send(false).unwrap();
                    return;
                }
            }
        }
    });
    (outcome_receiver, handle)
}

fn park_control(identity: &AdoptionIdentityFixture) -> DriverParkControl {
    DriverParkControl::new(identity.old_epoch, identity.cut, identity.frontier)
}

fn wait_until_starting(slot: &DriverAdoptionSlot) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if slot
            .state
            .lock()
            .is_ok_and(|state| matches!(&*state, DriverAdoptionState::Starting { .. }))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "driver did not reach the shared startup fence"
        );
        thread::yield_now();
    }
}

#[test]
fn phase82_dropping_parked_driver_escrows_the_stable_slot_without_resuming() {
    let identity = adoption_identities();
    let pool = ProjectionWorkerPool::new(NonZeroUsize::new(8).unwrap());
    let slot = DriverAdoptionSlot::new();
    let (outcome, driver) = spawn_driver_cycle(Arc::clone(&slot), connection_worker(&pool));

    let parked = slot.park(park_control(&identity)).unwrap();
    drop(parked);

    assert!(matches!(
        outcome.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    assert!(slot.is_inert());
    assert_eq!(pool.diagnostics().active(), 1);
    drop(driver);
    drop(slot);
    assert_eq!(pool.diagnostics().active(), 1);
}

#[test]
fn phase82_dropping_bound_driver_token_disables_instead_of_resuming() {
    let identity = adoption_identities();
    let pool = ProjectionWorkerPool::new(NonZeroUsize::new(8).unwrap());
    let slot = DriverAdoptionSlot::new();
    let (outcome, driver) = spawn_driver_cycle(Arc::clone(&slot), connection_worker(&pool));
    let replacement_worker = connection_worker(&pool);
    let startup = ServiceStartupGate::closed_gate();

    let parked = slot.park(park_control(&identity)).unwrap();
    let (old_worker, token) = parked.into_parts();
    let adopted = token
        .bind_replacement(identity.replacement_epoch, replacement_worker, startup)
        .unwrap();
    drop(adopted);

    assert!(matches!(
        outcome.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    assert!(slot.is_inert());
    drop(old_worker);
    assert_eq!(pool.diagnostics().active(), 1);
    drop(driver);
    drop(slot);
    assert_eq!(pool.diagnostics().active(), 1);
}

#[test]
fn phase82_bound_replacement_waits_at_the_shared_startup_fence() {
    let identity = adoption_identities();
    let pool = ProjectionWorkerPool::new(NonZeroUsize::new(8).unwrap());
    let slot = DriverAdoptionSlot::new();
    let (outcome, driver) = spawn_driver_cycle(Arc::clone(&slot), connection_worker(&pool));
    let replacement_worker = connection_worker(&pool);
    let startup = ServiceStartupGate::closed_gate();

    let parked = slot.park(park_control(&identity)).unwrap();
    let (old_worker, token) = parked.into_parts();
    token
        .bind_replacement(
            identity.replacement_epoch,
            replacement_worker,
            Arc::clone(&startup),
        )
        .unwrap()
        .arm_for_publication(&startup)
        .unwrap();

    wait_until_starting(&slot);
    assert!(matches!(outcome.try_recv(), Err(mpsc::TryRecvError::Empty)));
    startup.cancel();
    assert!(matches!(
        outcome.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    assert!(slot.is_inert());
    drop(old_worker);
    assert_eq!(pool.diagnostics().active(), 1);
    drop(driver);
    drop(slot);
    assert_eq!(pool.diagnostics().active(), 1);
}

#[test]
fn phase82_disable_before_the_first_driver_cycle_never_grants_work() {
    let identity = adoption_identities();
    let pool = ProjectionWorkerPool::new(NonZeroUsize::new(8).unwrap());
    let slot = DriverAdoptionSlot::new();
    let mut worker = Some(connection_worker(&pool));

    slot.disable_for_failure(identity.cut);
    assert!(matches!(
        slot.begin_cycle(&mut worker),
        DriverAdoptionPoll::AwaitDisposition
    ));
    assert!(worker.is_none());
    assert_eq!(pool.diagnostics().active(), 1);

    assert!(slot.release_inert_for_disposition());
    assert_eq!(pool.diagnostics().active(), 0);
    assert!(matches!(
        slot.begin_cycle(&mut worker),
        DriverAdoptionPoll::Disposed
    ));
}

#[test]
fn phase82_guarded_hub_loss_quiesces_before_exact_cut_disable_without_a_close_frame() {
    let (_directory, service) = live_driver_service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([191; 16]),
            CasProcessGeneration::new(82_191).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());
    let epoch = connection.current_epoch().unwrap();
    let cut = PersistentFailureCutIdentity::new(
        epoch.identity.home_id(),
        epoch.identity.home_generation(),
        epoch.identity.service_generation(),
        PersistentFailureGeneration::FIRST,
    );
    let pause = super::super::install_driver_work_guard_pause(
        connection.identity_observation().connection_generation(),
    );
    pause.wait_until_reached();

    connection.poison_forwarding_epoch_barrier_for_test();
    let inert_connection = Arc::clone(&connection);
    let (inert_reached, inert_observation) = mpsc::sync_channel(1);
    let (dispose_inert, dispose_inert_receiver) = mpsc::sync_channel(1);
    let inert_owner = thread::spawn(move || {
        let inert = inert_connection.make_adoption_inert_in_place(cut);
        inert_reached.send(()).unwrap();
        dispose_inert_receiver.recv().unwrap();
        inert.dispose();
    });

    assert!(matches!(
        inert_observation.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    pause.release();
    inert_observation
        .recv_timeout(Duration::from_secs(5))
        .expect("exact-cut inert conversion follows guarded quiescence");
    assert!(connection.forwarding_epoch_is_inert_and_detached_for_test());

    drop(session);
    server.assert_quiet_and_close();
    server.join();

    dispose_inert.send(()).unwrap();
    inert_owner.join().unwrap();
    connection
        .dispose_inert_driver_after_adoption_failure()
        .unwrap();
    drop((connection, service));
}
