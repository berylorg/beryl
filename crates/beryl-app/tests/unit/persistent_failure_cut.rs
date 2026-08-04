use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use beryl_home_store::{
    HomeCommand, HomeHealthState, HomeOpenError, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    BindingRevision, CasThreadId, PathFlavor, RootId, RuntimeMode, RuntimeNativePath,
    ThreadRevision,
};
use beryl_state::{
    ApplySettings, BerylState, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
};
use syndic_storage::{
    CasLineageProof, CasRepresentedPrefixProof, NativeCasLineage, SyndicStorage,
    empty_selected_path_digest,
};

use super::*;
use crate::cas_projection::{
    LoadedCasProjection, LoadedProjectionReleaseOutcome, PersistentFailureNotificationStatus,
    PersistentFailureRecoveryInventoryError, connection::LoadedProjectionLease,
    persistent_failure::PersistentFailureServiceEscrowReservation,
};

mod admission_server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/phase37_normal_terminal/server.rs"
    ));
}

#[derive(Clone)]
struct ShutdownProbe {
    count: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProvider for ShutdownProbe {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        self.count.fetch_add(1, Ordering::SeqCst);
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
        home,
        storage,
        ProjectionServiceConfig::try_new(8, worker_capacity).unwrap(),
        Box::new(ShutdownProbe {
            count: Arc::clone(&shutdowns),
        }),
    )
    .unwrap();
    wait_until(
        "the initial recovered-pending scheduler pass to settle",
        || {
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            diagnostics.recovered_pending_pass_count() >= 1
                && diagnostics.workers_active() == 0
                && service.worker_pool_diagnostics().active() == 0
                && !diagnostics.fatal()
        },
    );
    (directory, faults, state, shutdowns, service)
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

fn fail_home_through_live_command(
    service: &ProjectionConnectionService,
    state: BerylState,
    faults: &FaultController,
) {
    let (live, command) = prepare_failure_command(service, state, "persistent failure cut");
    let home = live.home();
    faults.panic_next(FaultPoint::BeforeCommit);

    let panicked = catch_unwind(AssertUnwindSafe(|| home.execute(command)));
    assert!(panicked.is_err());
    assert_eq!(home.health().state(), HomeHealthState::Failed);
    assert!(matches!(
        service.persistent_failure_cut_snapshot().state(),
        PersistentFailureCutState::Armed | PersistentFailureCutState::Cutting
    ));
    drop(live);
}

fn prepare_failure_command<'a>(
    service: &'a ProjectionConnectionService,
    state: BerylState,
    instructions: &'static str,
) -> (LiveHomeCommand<'a>, HomeCommand) {
    let live = service.live_home_command().unwrap();
    let home = live.home();
    let update = SettingUpdate::new(
        SettingKey::DeveloperInstructions,
        ExpectedSettingRevision::Absent,
        SettingValue::developer_instructions(instructions).unwrap(),
    );
    let contribution = state.settings().apply(
        state.settings().revision(home).unwrap(),
        ApplySettings::new(vec![update]).unwrap(),
    );
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    (live, command)
}

#[test]
fn same_generation_failed_read_is_valid_scheduler_quiescence_for_zero_inventory() {
    let (directory, faults, state, shutdowns, service) = service();
    let notification = service.persistent_failure_notification();
    let scheduler_signal = service.scheduler_signal.clone();
    let armed = service.persistent_failure_cut_snapshot();
    let home_id = service.home_id;
    let home_generation = service.home_generation;

    let (live, command) = prepare_failure_command(&service, state, "scheduler failure provenance");
    let home = live.home();
    let blocked_read = faults.block_next(FaultPoint::BeforeReadConfirmation);
    scheduler_signal.wake(AcceptedInputWakeReason::Recovery);
    if !blocked_read.wait_until_reached(Duration::from_secs(5)) {
        blocked_read.release();
        panic!("the accepted-input scheduler did not reach the deterministic read cut");
    }
    faults.panic_next(FaultPoint::BeforeCommit);
    let panicked = catch_unwind(AssertUnwindSafe(|| home.execute(command)));
    assert!(panicked.is_err());
    assert_eq!(home.health().state(), HomeHealthState::Failed);
    drop(live);
    blocked_read.release();

    for _ in 0..8 {
        assert_eq!(
            notification.notify(),
            crate::cas_projection::PersistentFailureNotificationStatus::Joined
        );
    }
    wait_until(
        "the failed scheduler read and exact persistent-failure cut",
        || {
            service.accepted_input_scheduler_diagnostics().fatal()
                && service.persistent_failure_cut_snapshot().state()
                    == PersistentFailureCutState::Finished
        },
    );

    let finished = service.persistent_failure_cut_snapshot();
    assert_eq!(finished.service_generation(), armed.service_generation());
    assert_eq!(finished.failure_generation().unwrap().get(), 1);
    assert_eq!(finished.target_count(), 0);
    assert_eq!(finished.retained_projection_count(), 0);
    assert!(!service.is_accepting_for_test());
    assert!(service.live_home_command().is_err());
    assert_eq!(shutdowns.load(Ordering::SeqCst), 0);

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("typed persistent failure must win the close election")
        }
    };
    assert_eq!(handoff.home_id(), home_id);
    assert_eq!(handoff.home_generation(), home_generation);
    assert_eq!(handoff.service_generation(), armed.service_generation());
    assert_eq!(handoff.failure_generation().get(), 1);
    assert_eq!(
        handoff.completion(),
        PersistentFailureCutCompletion::Finished
    );
    assert_eq!(
        handoff.cut_snapshot().state(),
        PersistentFailureCutState::Finished
    );
    assert_eq!(shutdowns.load(Ordering::SeqCst), 0);

    assert!(matches!(
        HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        )),
        Err(HomeOpenError::Busy { .. })
    ));
    let (seal_observer, seal_observation) = std::sync::mpsc::sync_channel(1);
    PersistentFailureCutHandoff::observe_next_retention_seal_for_test(seal_observer);
    let inventory = handoff.into_recovery_inventory().unwrap();
    let metadata = inventory.metadata();
    assert!(
        seal_observation
            .recv_timeout(Duration::from_secs(1))
            .expect("inventory conversion reached its coordinator retention seal")
    );
    assert!(scheduler_signal.diagnostics().stopped());
    assert!(metadata.is_promotable());
    assert_eq!(metadata.late_publication_count(), 0);
    assert_eq!(metadata.sealed_counts(), Some(Default::default()));
    assert_eq!(metadata.retained_counts(), Default::default());

    assert!(inventory.escrow_is_checked_out_for_test());
    assert!(matches!(
        PersistentFailureServiceEscrowReservation::reserve(
            inventory.home_id(),
            inventory.home_generation(),
            inventory.service_generation(),
        ),
        Err(ProjectionCoordinatorError::PersistentFailureEscrowIdentityAlreadyReserved)
    ));
    assert!(matches!(
        HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        )),
        Err(HomeOpenError::Busy { .. })
    ));
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let quarantine_metadata = quarantine.metadata();
    assert!(quarantine_metadata.is_promotable());
    assert_eq!(quarantine_metadata.group_count(), 0);
    assert_eq!(quarantine_metadata.candidate_count(), 0);
    assert_eq!(quarantine_metadata.retained_connection_count(), 0);
    assert_eq!(quarantine_metadata.local_disposition_count(), 0);
    drop(quarantine);
    assert!(matches!(
        PersistentFailureServiceEscrowReservation::reserve(
            home_id,
            home_generation,
            armed.service_generation(),
        ),
        Err(ProjectionCoordinatorError::PersistentFailureEscrowIdentityAlreadyReserved)
    ));
    assert!(matches!(
        HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        )),
        Err(HomeOpenError::Busy { .. })
    ));
}

#[test]
fn unrelated_local_scheduler_failure_dominates_a_later_persistent_failure_cut() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let notification = service.persistent_failure_notification();
    let (live, command) =
        prepare_failure_command(&service, state, "local scheduler failure precedence");
    let home = live.home();

    service.command_gate.close_for_local_failure();
    service
        .scheduler_signal
        .wake(AcceptedInputWakeReason::Recovery);
    wait_until(
        "the unrelated local scheduler failure to stop the scheduler",
        || {
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            diagnostics.fatal() && diagnostics.stopped()
        },
    );
    assert_eq!(home.health().state(), HomeHealthState::Healthy);

    faults.panic_next(FaultPoint::BeforeCommit);
    let panicked = catch_unwind(AssertUnwindSafe(|| home.execute(command)));
    assert!(panicked.is_err());
    assert_eq!(home.health().state(), HomeHealthState::Failed);
    drop(live);
    assert_eq!(
        notification.notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("the later persistent-failure cut", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the later persistent failure must retain its exact service")
        }
    };
    let inventory = match handoff.into_recovery_inventory() {
        Err(PersistentFailureRecoveryInventoryError::SchedulerFatal(inventory)) => inventory,
        outcome => panic!("the unrelated local failure must remain dominant: {outcome:?}"),
    };
    assert!(!inventory.metadata().is_promotable());
    assert_eq!(
        inventory.metadata().sealed_counts(),
        Some(Default::default())
    );
}

#[test]
fn local_failure_after_scheduler_exit_still_makes_inventory_nonpromotable() {
    let (_directory, faults, state, _shutdowns, service) = service();
    fail_home_through_live_command(&service, state, &faults);
    wait_until(
        "the persistent-failure cut before scheduler quiescence",
        || service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished,
    );
    service.scheduler_signal.request_shutdown();
    wait_until("the cut-correlated scheduler exit", || {
        service.accepted_input_scheduler_diagnostics().stopped()
    });

    service.command_gate.close_for_local_failure();
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the persistent failure must retain its exact service")
        }
    };
    let inventory = match handoff.into_recovery_inventory() {
        Err(PersistentFailureRecoveryInventoryError::SchedulerFatal(inventory)) => inventory,
        outcome => panic!("the post-exit local failure must remain visible: {outcome:?}"),
    };
    assert_eq!(
        inventory.metadata().sealed_counts(),
        Some(Default::default())
    );
    assert!(!inventory.metadata().is_promotable());
}

#[test]
fn poisoned_command_gate_after_scheduler_exit_returns_an_owning_inventory() {
    let (_directory, faults, state, _shutdowns, service) = service();
    fail_home_through_live_command(&service, state, &faults);
    wait_until(
        "the persistent-failure cut before poisoned-gate quiescence",
        || service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished,
    );
    service.scheduler_signal.request_shutdown();
    wait_until("the scheduler exit before command-gate poison", || {
        service.accepted_input_scheduler_diagnostics().stopped()
    });

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the persistent failure must retain its exact service gate")
        }
    };
    handoff.poison_command_gate_for_test();
    let inventory = match handoff.into_recovery_inventory() {
        Err(PersistentFailureRecoveryInventoryError::CommandGatePoisoned(inventory)) => inventory,
        outcome => panic!("the poisoned command gate must remain explicit: {outcome:?}"),
    };
    assert_eq!(
        inventory.metadata().sealed_counts(),
        Some(Default::default())
    );
    assert!(!inventory.metadata().is_promotable());
}

#[test]
fn poisoned_scheduler_owner_is_joined_and_sealed_nonpromotable() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let notification = service.persistent_failure_notification();
    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        notification.notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("the poisoned-owner fixture's failure cut", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the failed service must retain its scheduler owner")
        }
    };
    handoff.poison_scheduler_owner_for_test();
    let inventory = match handoff.into_recovery_inventory() {
        Err(PersistentFailureRecoveryInventoryError::SchedulerPoisoned(inventory)) => inventory,
        outcome => panic!("the poisoned owner must remain explicit: {outcome:?}"),
    };
    let metadata = inventory.metadata();
    assert_eq!(metadata.sealed_counts(), Some(Default::default()));
    assert!(!metadata.is_promotable());
}

#[test]
fn poisoned_retention_boundary_returns_an_owning_nonpromotable_inventory() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let notification = service.persistent_failure_notification();
    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        notification.notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("the poisoned-retention fixture's failure cut", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the failed service must retain its capability state")
        }
    };
    handoff.poison_retention_for_test();
    let inventory = match handoff.into_recovery_inventory() {
        Err(PersistentFailureRecoveryInventoryError::RetentionSealRejected(inventory)) => inventory,
        outcome => panic!("the poisoned retention boundary must remain explicit: {outcome:?}"),
    };
    let metadata = inventory.metadata();
    assert!(metadata.sealed_counts().is_none());
    assert!(metadata.retention_poisoned());
    assert!(!metadata.is_promotable());
}

#[test]
fn scheduler_main_panic_joins_blocked_projection_before_inventory_seal() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let runtime_id = RuntimeId::from_bytes([120; 16]);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(78_120).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let coordinator =
        CasProjectionCoordinator::for_healthy_home(service.home.as_deref().unwrap()).unwrap();
    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);
    let worker = service
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
        .unwrap();
    let issuer = worker.preactivation_surrender_issuer(retainer).unwrap();
    let owner = SyndicThreadId::from_bytes([121; 16]);
    let cas_thread_id = CasThreadId::new("phase-78-scheduler-main-panic").unwrap();
    let lease = session
        .connection()
        .register_new(
            cas_thread_id.clone(),
            owner,
            Duration::from_secs(10),
            Some(&issuer),
        )
        .unwrap();
    let projection = LoadedCasProjection::new(
        &coordinator,
        owner,
        BindingRevision::new(1).unwrap(),
        ExecutionBinding::new(
            runtime_id,
            RootId::from_bytes([122; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                r"C:\work\beryl",
            )
            .unwrap(),
        ),
        cas_thread_id,
        lease,
        CasLineageProof::native(
            NativeCasLineage::Fresh,
            CasRepresentedPrefixProof::new(
                None,
                ThreadRevision::new(1).unwrap(),
                empty_selected_path_digest(),
            ),
        )
        .unwrap(),
    );

    let (live, command) = prepare_failure_command(&service, state, "scheduler-main containment");
    let home = live.home();
    let worker_pause =
        service.install_blocked_scheduler_projection_worker_for_test(projection, worker);
    assert!(worker_pause.wait_until_registered(Duration::from_secs(5)));
    let scheduler_panic = service.install_accepted_input_scheduler_panic_for_test();
    assert!(scheduler_panic.wait_until_panicking(Duration::from_secs(5)));

    faults.panic_next(FaultPoint::BeforeCommit);
    let failed = catch_unwind(AssertUnwindSafe(|| home.execute(command)));
    assert!(failed.is_err());
    assert_eq!(home.health().state(), HomeHealthState::Failed);
    drop(live);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until(
        "the cut to finish while its scheduler child is paused",
        || service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished,
    );

    drop(session);
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the failed service must retain its scheduler and projection")
        }
    };
    let conversion = std::thread::spawn(move || handoff.into_recovery_inventory());
    assert!(scheduler_panic.wait_until_join_requested(Duration::from_secs(5)));
    assert!(
        !conversion.is_finished(),
        "retention sealed before the blocked scheduler child was joined"
    );

    worker_pause.release();
    let inventory = match conversion.join().unwrap() {
        Err(PersistentFailureRecoveryInventoryError::SchedulerPanicked(inventory)) => inventory,
        outcome => panic!("scheduler-main panic must return its owning inventory: {outcome:?}"),
    };
    let metadata = inventory.metadata();
    let sealed = metadata
        .sealed_counts()
        .expect("panic containment must join every child before sealing");
    assert_eq!(sealed.complete_candidate_count(), 1);
    assert_eq!(metadata.retained_counts(), sealed);
    assert_eq!(metadata.late_publication_count(), 0);
    assert!(!metadata.is_promotable());
    drop(server);
}

#[test]
fn dropping_unused_session_reclaims_mounted_connection_permits() {
    let (_directory, _faults, _state, _shutdowns, service) = service();
    let first_server = admission_server::NormalTerminalServer::spawn_admission_only();
    let first_connector = ManagedBackendClientConnector::for_lifecycle_test(
        first_server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let first = service
        .admit_lifecycle_test_candidate(
            &first_connector,
            RuntimeId::from_bytes([77; 16]),
            CasProcessGeneration::new(77_001).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    first_server.wait_for_admission();
    assert_eq!(service.worker_pool_diagnostics().available(), 2);

    drop(first);
    first_server.join();
    wait_until("unused mounted connection permits to return", || {
        service.worker_pool_diagnostics().available() == 4
    });

    let replacement_server = admission_server::NormalTerminalServer::spawn_admission_only();
    let replacement_connector = ManagedBackendClientConnector::for_lifecycle_test(
        replacement_server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let replacement = service
        .admit_lifecycle_test_candidate(
            &replacement_connector,
            RuntimeId::from_bytes([78; 16]),
            CasProcessGeneration::new(77_002).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    replacement_server.wait_for_admission();
    drop(replacement);
    replacement_server.join();
    wait_until("replacement mounted connection permits to return", || {
        service.worker_pool_diagnostics().available() == 4
    });

    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
}

#[test]
fn sequential_connection_churn_reaps_finished_registry_entries() {
    let (_directory, _faults, _state, _shutdowns, service) = service();
    for index in 0_u8..12 {
        let server = admission_server::NormalTerminalServer::spawn_admission_only();
        let connector = ManagedBackendClientConnector::for_lifecycle_test(
            server.endpoint(),
            admission_server::AUTHORIZATION,
        );
        let session = service
            .admit_lifecycle_test_candidate(
                &connector,
                RuntimeId::from_bytes([index.saturating_add(100); 16]),
                CasProcessGeneration::new(78_000 + u64::from(index)).unwrap(),
                Path::new(r"C:\work\beryl"),
                Duration::from_secs(10),
            )
            .unwrap();
        server.wait_for_admission();
        assert!(service.registered_connection_count_for_test() <= 1);

        drop(session);
        server.join();
        wait_until("churned connection permits to return", || {
            service.worker_pool_diagnostics().available() == 4
        });
    }
    assert!(service.registered_connection_count_for_test() <= 1);
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
}

#[test]
fn concurrent_connection_shutdown_replays_one_clean_settlement() {
    let (_directory, _faults, _state, shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([132; 16]),
            CasProcessGeneration::new(78_132).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let barrier = Arc::new(std::sync::Barrier::new(3));
    let first_connection = Arc::clone(session.connection());
    let first_barrier = Arc::clone(&barrier);
    let first = std::thread::spawn(move || {
        first_barrier.wait();
        first_connection.shutdown()
    });
    let second_connection = Arc::clone(session.connection());
    let second_barrier = Arc::clone(&barrier);
    let second = std::thread::spawn(move || {
        second_barrier.wait();
        second_connection.shutdown()
    });
    barrier.wait();

    assert!(first.join().unwrap().is_ok());
    assert!(second.join().unwrap().is_ok());
    assert!(session.connection().shutdown().is_ok());
    drop(session);
    server.join();
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn implicit_ordinary_service_drop_does_not_wait_for_cleanup_authority() {
    let (_directory, _faults, _state, shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([113; 16]),
            CasProcessGeneration::new(78_113).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let cleanup = session
        .connection()
        .acquire_cleanup_owner()
        .unwrap()
        .unwrap();
    let (finished, completion) = std::sync::mpsc::sync_channel(1);
    let dropper = std::thread::spawn(move || {
        drop(service);
        finished.send(()).unwrap();
    });

    completion
        .recv_timeout(Duration::from_secs(1))
        .expect("implicit service drop must not wait for cleanup authority");
    assert_eq!(shutdowns.load(Ordering::SeqCst), 0);

    drop(cleanup);
    drop(session);
    server.join();
    dropper.join().unwrap();
}

#[test]
fn implicit_failure_service_drop_escrows_without_waiting_for_gate_drain() {
    let (directory, faults, state, shutdowns, service) = service();
    let held_command = service.command_authorizer.authorize().unwrap();
    fail_home_through_live_command(&service, state, &faults);
    let (finished, completion) = std::sync::mpsc::sync_channel(1);
    let dropper = std::thread::spawn(move || {
        drop(service);
        finished.send(()).unwrap();
    });

    completion
        .recv_timeout(Duration::from_secs(1))
        .expect("failure-winning implicit drop must not wait for gate drain");
    assert_eq!(shutdowns.load(Ordering::SeqCst), 0);
    assert!(matches!(
        HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        )),
        Err(HomeOpenError::Busy { .. })
    ));

    drop(held_command);
    dropper.join().unwrap();
}

#[test]
fn ordinary_close_detaches_home_ownership_from_a_retained_session_shell() {
    let (directory, _faults, _state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([80; 16]),
            CasProcessGeneration::new(77_004).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();

    let reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .expect("a stale admitted-session shell cannot retain the explicitly closed home");
    reopened.close().unwrap();
    drop(session);
}

#[test]
fn worker_surrender_bounds_preactivation_while_router_targets_have_their_own_bound() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let runtime_id = RuntimeId::from_bytes([82; 16]);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(77_006).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let coordinator =
        CasProjectionCoordinator::for_healthy_home(service.home.as_deref().unwrap()).unwrap();
    let execution_binding = ExecutionBinding::new(
        runtime_id,
        RootId::from_bytes([83; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl",
        )
        .unwrap(),
    );
    let represented = CasRepresentedPrefixProof::new(
        None,
        ThreadRevision::new(1).unwrap(),
        empty_selected_path_digest(),
    );
    let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap();
    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);
    let baseline_workers = service.worker_pool_diagnostics().active();
    let mut targets = Vec::new();
    for index in 0_u8..5 {
        let worker = service
            .workers
            .try_acquire_scheduled_ordinary_or_arm()
            .unwrap();
        let issuer = worker
            .preactivation_surrender_issuer(retainer.clone())
            .unwrap();
        let owner = SyndicThreadId::from_bytes([index.saturating_add(90); 16]);
        let cas_thread_id = CasThreadId::new(format!("phase-77-target-{index}")).unwrap();
        let lease = session
            .connection()
            .register_new(
                cas_thread_id.clone(),
                owner,
                Duration::from_secs(10),
                Some(&issuer),
            )
            .unwrap();
        let projection = LoadedCasProjection::new(
            &coordinator,
            owner,
            BindingRevision::new(u64::from(index) + 1).unwrap(),
            execution_binding.clone(),
            cas_thread_id,
            lease,
            lineage,
        );
        targets.push(
            projection
                .into_active_live_event_target(
                    beryl_model::CasTurnId::new(format!("phase-77-turn-{index}")).unwrap(),
                )
                .unwrap(),
        );
        drop(worker);
        wait_until("activated target to release its worker admission", || {
            service.worker_pool_diagnostics().active() == baseline_workers
        });
    }
    assert!(targets.len() > service.config.worker_capacity().get());

    let worker = service
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
        .unwrap();
    let issuer = worker.preactivation_surrender_issuer(retainer).unwrap();
    let owner = SyndicThreadId::from_bytes([99; 16]);
    let cas_thread_id = CasThreadId::new("phase-77-preactivation").unwrap();
    let raw_lease = session
        .connection()
        .register_new(
            cas_thread_id.clone(),
            owner,
            Duration::from_secs(10),
            Some(&issuer),
        )
        .unwrap();
    drop(worker);
    wait_until(
        "preactivation surrender to retain one worker admission",
        || service.worker_pool_diagnostics().active() == baseline_workers + 1,
    );
    assert!(matches!(
        service.workers.try_acquire_scheduled_ordinary_or_arm(),
        Err(
            crate::cas_projection::service_config::ProjectionWorkerPermitError::CapacityFull { .. }
        )
    ));

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(targets);
    drop(raw_lease);
    drop(session);
    wait_until(
        "all bounded loaded capabilities to enter the failure cut",
        || {
            let snapshot = service.persistent_failure_cut_snapshot();
            snapshot.state() == PersistentFailureCutState::Finished
                && snapshot.retained_projection_count() == 6
        },
    );

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("failure-retained loaded capabilities must suppress ordinary close")
        }
    };
    assert_eq!(handoff.cut_snapshot().retained_projection_count(), 6);
    let inventory = handoff.into_recovery_inventory().unwrap();
    let counts = inventory.metadata().sealed_counts().unwrap();
    assert_eq!(counts.complete_candidate_count(), 0);
    assert_eq!(counts.target_projection_count(), 5);
    assert_eq!(counts.raw_loaded_lease_count(), 1);
    assert_eq!(counts.connection_count(), 1);
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let metadata = quarantine.metadata();
    assert!(metadata.is_promotable());
    assert_eq!(metadata.group_count(), 0);
    assert_eq!(metadata.candidate_count(), 0);
    assert_eq!(metadata.retained_connection_count(), 1);
    assert_eq!(metadata.local_disposition_count(), 11);
    drop(server);
}

#[test]
fn whole_projection_drop_chooses_the_exact_failure_side_after_starting_before_the_cut() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let runtime_id = RuntimeId::from_bytes([114; 16]);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(78_114).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let coordinator =
        CasProjectionCoordinator::for_healthy_home(service.home.as_deref().unwrap()).unwrap();
    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);
    let worker = service
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
        .unwrap();
    let issuer = worker.preactivation_surrender_issuer(retainer).unwrap();
    let owner = SyndicThreadId::from_bytes([115; 16]);
    let cas_thread_id = CasThreadId::new("phase-77-whole-drop-cut-race").unwrap();
    let lease = session
        .connection()
        .register_new(
            cas_thread_id.clone(),
            owner,
            Duration::from_secs(10),
            Some(&issuer),
        )
        .unwrap();
    let lineage = CasLineageProof::native(
        NativeCasLineage::Fresh,
        CasRepresentedPrefixProof::new(
            None,
            ThreadRevision::new(1).unwrap(),
            empty_selected_path_digest(),
        ),
    )
    .unwrap();
    let projection = LoadedCasProjection::new(
        &coordinator,
        owner,
        BindingRevision::new(1).unwrap(),
        ExecutionBinding::new(
            runtime_id,
            RootId::from_bytes([116; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                r"C:\work\beryl",
            )
            .unwrap(),
        ),
        cas_thread_id,
        lease,
        lineage,
    );
    drop(worker);

    let connection = Arc::clone(session.connection());
    let authority = connection.lock_authority_for_test();
    let (observed, settlement_started) = std::sync::mpsc::sync_channel(1);
    LoadedProjectionLease::observe_next_recovery_owner_settlement_for_test(observed);
    let dropper = std::thread::spawn(move || drop(projection));
    settlement_started
        .recv_timeout(Duration::from_secs(1))
        .expect("whole projection drop reached exact authority settlement");

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("failure election to close command admission", || {
        !service.is_accepting_for_test()
    });
    drop(authority);
    dropper.join().unwrap();
    drop(session);
    drop(connection);
    wait_until("whole projection to enter failure retention", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
            && service
                .persistent_failure
                .as_ref()
                .unwrap()
                .retained_loaded_projection_counts_for_test()
                == (1, 0)
    });

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("whole projection metadata must cross the exact failure cut")
        }
    };
    assert_eq!(handoff.cut_snapshot().retained_projection_count(), 1);
    let inventory = handoff.into_recovery_inventory().unwrap();
    assert!(inventory.metadata().is_promotable());
    let counts = inventory.metadata().sealed_counts().unwrap();
    assert_eq!(counts.complete_candidate_count(), 1);
    assert_eq!(counts.target_projection_count(), 0);
    assert_eq!(counts.raw_loaded_lease_count(), 0);
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let metadata = quarantine.metadata();
    assert!(metadata.is_promotable());
    assert_eq!(metadata.group_count(), 1);
    assert_eq!(metadata.candidate_count(), 1);
    assert_eq!(metadata.retained_connection_count(), 1);
    assert_eq!(metadata.local_disposition_count(), 0);
    drop(server);
}

#[test]
fn whole_reacquisition_anchor_drop_chooses_the_exact_failure_side_after_starting_before_the_cut() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let runtime_id = RuntimeId::from_bytes([117; 16]);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(78_117).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let coordinator =
        CasProjectionCoordinator::for_healthy_home(service.home.as_deref().unwrap()).unwrap();
    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);
    let worker = service
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
        .unwrap();
    let issuer = worker.preactivation_surrender_issuer(retainer).unwrap();
    let owner = SyndicThreadId::from_bytes([118; 16]);
    let cas_thread_id = CasThreadId::new("phase-77-whole-anchor-cut-race").unwrap();
    let lease = session
        .connection()
        .register_new(
            cas_thread_id.clone(),
            owner,
            Duration::from_secs(10),
            Some(&issuer),
        )
        .unwrap();
    let lineage = CasLineageProof::native(
        NativeCasLineage::Fresh,
        CasRepresentedPrefixProof::new(
            None,
            ThreadRevision::new(1).unwrap(),
            empty_selected_path_digest(),
        ),
    )
    .unwrap();
    let projection = LoadedCasProjection::new(
        &coordinator,
        owner,
        BindingRevision::new(1).unwrap(),
        ExecutionBinding::new(
            runtime_id,
            RootId::from_bytes([119; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                r"C:\work\beryl",
            )
            .unwrap(),
        ),
        cas_thread_id,
        lease,
        lineage,
    );
    drop(worker);
    let anchor = projection.into_same_native_reacquisition_anchor().unwrap();

    let connection = Arc::clone(session.connection());
    let authority = connection.lock_authority_for_test();
    let (observed, settlement_started) = std::sync::mpsc::sync_channel(1);
    LoadedProjectionLease::observe_next_recovery_owner_settlement_for_test(observed);
    let dropper = std::thread::spawn(move || drop(anchor));
    settlement_started
        .recv_timeout(Duration::from_secs(1))
        .expect("whole reacquisition anchor drop reached exact authority settlement");

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("failure election to close command admission", || {
        !service.is_accepting_for_test()
    });
    drop(authority);
    dropper.join().unwrap();
    drop(session);
    drop(connection);
    wait_until(
        "whole reacquisition anchor to enter failure retention",
        || {
            service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
                && service
                    .persistent_failure
                    .as_ref()
                    .unwrap()
                    .retained_reacquisition_anchor_counts_for_test()
                    == (1, 0)
        },
    );

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("whole reacquisition metadata must cross the exact failure cut")
        }
    };
    assert_eq!(handoff.cut_snapshot().retained_projection_count(), 1);
    let inventory = handoff.into_recovery_inventory().unwrap();
    let counts = inventory.metadata().sealed_counts().unwrap();
    assert_eq!(counts.reacquisition_anchor_count(), 1);
    assert_eq!(counts.raw_quarantined_anchor_count(), 0);
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let metadata = quarantine.metadata();
    assert!(metadata.is_promotable());
    assert_eq!(metadata.candidate_count(), 0);
    assert_eq!(metadata.retained_connection_count(), 1);
    assert_eq!(metadata.local_disposition_count(), 1);
    drop(server);
}

#[test]
fn failed_target_handoff_rejects_a_stale_guard_disposition() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let runtime_id = RuntimeId::from_bytes([84; 16]);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(77_007).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let coordinator =
        CasProjectionCoordinator::for_healthy_home(service.home.as_deref().unwrap()).unwrap();
    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);
    let worker = service
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
        .unwrap();
    let issuer = worker.preactivation_surrender_issuer(retainer).unwrap();
    let owner = SyndicThreadId::from_bytes([85; 16]);
    let cas_thread_id = CasThreadId::new("phase-77-failed-handoff").unwrap();
    let lease = session
        .connection()
        .register_new(
            cas_thread_id.clone(),
            owner,
            Duration::from_secs(10),
            Some(&issuer),
        )
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        None,
        ThreadRevision::new(1).unwrap(),
        empty_selected_path_digest(),
    );
    let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap();
    let projection = LoadedCasProjection::new(
        &coordinator,
        owner,
        BindingRevision::new(1).unwrap(),
        ExecutionBinding::new(
            runtime_id,
            RootId::from_bytes([86; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                r"C:\work\beryl",
            )
            .unwrap(),
        ),
        cas_thread_id,
        lease,
        lineage,
    );
    let mut target = projection
        .into_active_live_event_target(
            beryl_model::CasTurnId::new("phase-77-failed-handoff-turn").unwrap(),
        )
        .unwrap();
    drop(worker);

    assert!(target.into_proven_terminal_projection().is_err());
    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(target);
    drop(session);
    wait_until(
        "failed handoff target to enter exact failure retention",
        || {
            let snapshot = service.persistent_failure_cut_snapshot();
            snapshot.state() == PersistentFailureCutState::Finished
                && snapshot.retained_projection_count() == 1
        },
    );

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("failed target handoff must remain owned by the failure cut")
        }
    };
    assert_eq!(handoff.cut_snapshot().retained_projection_count(), 1);
    assert!(handoff.corrupt_one_target_disposition_for_test());
    let inventory = handoff.into_recovery_inventory().unwrap();
    let counts = inventory.metadata().sealed_counts().unwrap();
    assert_eq!(counts.target_projection_count(), 1);
    assert_eq!(counts.target_result_count(), 1);
    let error = inventory.into_pending_projection_quarantine().unwrap_err();
    assert_eq!(
        error.reason(),
        crate::cas_projection::PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch
    );
    assert!(!error.metadata().is_promotable());
    assert_eq!(error.metadata().retained_connection_count(), 1);
    assert_eq!(error.metadata().local_disposition_count(), 2);
    drop(server);
}

#[test]
fn explicit_raw_loaded_release_after_the_cut_retains_exact_authority() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([87; 16]),
            CasProcessGeneration::new(77_008).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);
    let worker = service
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
        .unwrap();
    let issuer = worker.preactivation_surrender_issuer(retainer).unwrap();
    let lease = session
        .connection()
        .register_new(
            CasThreadId::new("phase-77-explicit-release").unwrap(),
            SyndicThreadId::from_bytes([88; 16]),
            Duration::from_secs(10),
            Some(&issuer),
        )
        .unwrap();
    drop(worker);

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    assert!(matches!(
        lease.release().unwrap(),
        LoadedProjectionReleaseOutcome::ConnectionRetired
    ));
    drop(session);
    wait_until("post-cut explicit release to retain its raw lease", || {
        let snapshot = service.persistent_failure_cut_snapshot();
        snapshot.state() == PersistentFailureCutState::Finished
            && snapshot.retained_projection_count() == 1
    });

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("post-cut explicit release must preserve failure authority")
        }
    };
    assert_eq!(handoff.cut_snapshot().retained_projection_count(), 1);
    drop(server);
}

#[test]
fn late_loaded_lease_publication_stays_owned_and_poisoned_after_seal() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([120; 16]),
            CasProcessGeneration::new(78_120).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);
    let worker = service
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
        .unwrap();
    let issuer = worker.preactivation_surrender_issuer(retainer).unwrap();
    let lease = session
        .connection()
        .register_new(
            CasThreadId::new("phase-78-late-loaded-lease").unwrap(),
            SyndicThreadId::from_bytes([121; 16]),
            Duration::from_secs(10),
            Some(&issuer),
        )
        .unwrap();
    drop(worker);

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("the lease-owning cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the failed service must retain its still-live lease")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let sealed = inventory.metadata();
    let sealed_counts = sealed.sealed_counts().unwrap();
    assert_eq!(sealed_counts.connection_count(), 1);
    assert_eq!(sealed_counts.raw_loaded_lease_count(), 0);
    assert!(sealed.is_promotable());

    assert!(matches!(
        lease.release().unwrap(),
        LoadedProjectionReleaseOutcome::ConnectionRetired
    ));
    let late = inventory.metadata();
    assert_eq!(late.sealed_counts(), Some(sealed_counts));
    assert_eq!(late.retained_counts().raw_loaded_lease_count(), 1);
    assert_eq!(late.late_publication_count(), 1);
    assert!(!late.is_promotable());

    drop(session);
    drop(server);
}

#[test]
fn cut_retains_exact_raw_quarantine_and_reacquisition_reservation_authority() {
    let (_directory, faults, state, _shutdowns, service) = service_with_worker_capacity(6);
    let old_server = admission_server::NormalTerminalServer::spawn_admission_only();
    let replacement_server = admission_server::NormalTerminalServer::spawn_admission_only();
    let runtime_id = RuntimeId::from_bytes([89; 16]);
    let process_generation = CasProcessGeneration::new(77_009).unwrap();
    let old_connector = ManagedBackendClientConnector::for_lifecycle_test(
        old_server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let replacement_connector = ManagedBackendClientConnector::for_lifecycle_test(
        replacement_server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let old_session = service
        .admit_lifecycle_test_candidate(
            &old_connector,
            runtime_id,
            process_generation,
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    old_server.wait_for_admission();
    let replacement_session = service
        .admit_lifecycle_test_candidate(
            &replacement_connector,
            runtime_id,
            process_generation,
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    replacement_server.wait_for_admission();
    let baseline_workers = service.worker_pool_diagnostics().active();

    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);
    let worker = service
        .workers
        .try_acquire_scheduled_ordinary_or_arm()
        .unwrap();
    let issuer = worker.preactivation_surrender_issuer(retainer).unwrap();
    let lease = old_session
        .connection()
        .register_new(
            CasThreadId::new("phase-77-quarantine-reservation").unwrap(),
            SyndicThreadId::from_bytes([90; 16]),
            Duration::from_secs(10),
            Some(&issuer),
        )
        .unwrap();
    drop(worker);
    let anchor = lease.quarantine_for_reacquisition().unwrap();
    let reservation = anchor
        .reserve_replacement(replacement_session.connection(), None)
        .unwrap();
    wait_until("the raw quarantine admission to remain held", || {
        service.worker_pool_diagnostics().active() == baseline_workers + 1
    });

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(reservation);
    drop(anchor);
    drop(old_session);
    drop(replacement_session);
    wait_until(
        "raw quarantine and replacement reservation to enter exact failure retention",
        || {
            let snapshot = service.persistent_failure_cut_snapshot();
            snapshot.state() == PersistentFailureCutState::Finished
                && snapshot.retained_projection_count() == 2
        },
    );

    let worker_pool = service.workers.clone();
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("raw reacquisition authority must suppress ordinary close")
        }
    };
    assert_eq!(handoff.cut_snapshot().retained_projection_count(), 2);
    let inventory = handoff.into_recovery_inventory().unwrap();
    let counts = inventory.metadata().sealed_counts().unwrap();
    assert_eq!(counts.raw_quarantined_anchor_count(), 1);
    assert_eq!(counts.raw_reacquisition_reservation_count(), 1);
    assert_eq!(counts.connection_count(), 2);
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let metadata = quarantine.metadata();
    assert!(metadata.is_promotable());
    assert_eq!(metadata.group_count(), 0);
    assert_eq!(metadata.candidate_count(), 0);
    assert_eq!(metadata.retained_connection_count(), 2);
    assert_eq!(metadata.local_disposition_count(), 2);
    assert_eq!(worker_pool.diagnostics().active(), baseline_workers);
    drop(old_server);
    drop(replacement_server);
}

#[test]
fn failure_retains_the_exact_scheduled_promotion_barrier_for_recovery() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([81; 16]),
            CasProcessGeneration::new(77_005).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let reservation = session
        .connection()
        .reserve_scheduled_promotion()
        .unwrap()
        .expect("the exact live connection admits one promotion barrier");
    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);

    fail_home_through_live_command(&service, state, &faults);
    retainer.retain_promotion(reservation);
    drop(session);
    wait_until("the promotion-owning persistent-failure cut", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });

    let snapshot = service.persistent_failure_cut_snapshot();
    assert_eq!(snapshot.retained_promotion_count(), 1);
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("failure-retained promotion authority must suppress ordinary close")
        }
    };
    assert_eq!(handoff.cut_snapshot().retained_promotion_count(), 1);
    let inventory = handoff.into_recovery_inventory().unwrap();
    assert_eq!(
        inventory
            .metadata()
            .sealed_counts()
            .unwrap()
            .promotion_count(),
        1
    );
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let metadata = quarantine.metadata();
    assert!(metadata.is_promotable());
    assert_eq!(metadata.group_count(), 0);
    assert_eq!(metadata.retained_connection_count(), 1);
    assert_eq!(metadata.local_disposition_count(), 1);
    drop(server);
}

#[test]
fn finished_inventory_counts_the_exact_cleanup_barrier() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([122; 16]),
            CasProcessGeneration::new(78_122).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let cleanup = session
        .connection()
        .acquire_cleanup_owner()
        .unwrap()
        .unwrap();

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("cleanup settlement to observe the failure cut", || {
        !service.is_accepting_for_test()
    });
    drop(cleanup);
    drop(session);
    wait_until("the cleanup-owning failure cut", || {
        let snapshot = service.persistent_failure_cut_snapshot();
        snapshot.state() == PersistentFailureCutState::Finished
            && snapshot.retained_cleanup_count() == 1
    });

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("failure-retained cleanup authority must suppress ordinary close")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    assert_eq!(
        inventory
            .metadata()
            .sealed_counts()
            .unwrap()
            .cleanup_count(),
        1
    );
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let metadata = quarantine.metadata();
    assert!(metadata.is_promotable());
    assert_eq!(metadata.group_count(), 0);
    assert_eq!(metadata.retained_connection_count(), 1);
    assert_eq!(metadata.local_disposition_count(), 1);
    drop(server);
}

#[test]
fn router_freeze_failure_is_incomplete_and_retains_the_mounted_connection() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([79; 16]),
            CasProcessGeneration::new(77_003).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    session.connection().poison_router_for_test();

    fail_home_through_live_command(&service, state, &faults);
    drop(session);
    wait_until("the failed router cut to report incomplete", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Incomplete
    });

    assert_eq!(service.worker_pool_diagnostics().available(), 2);
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("typed persistent failure must retain an incomplete cut")
        }
    };
    assert_eq!(
        handoff.completion(),
        PersistentFailureCutCompletion::Incomplete
    );
    assert_eq!(
        handoff.cut_snapshot().state(),
        PersistentFailureCutState::Incomplete
    );
    assert_eq!(handoff.cut_snapshot().target_count(), 0);
    assert_eq!(
        handoff.cut_snapshot().service_generation(),
        handoff.service_generation()
    );
    let handoff = match handoff.into_recovery_inventory() {
        Err(PersistentFailureRecoveryInventoryError::CutIncomplete(handoff)) => handoff,
        outcome => panic!("incomplete cut changed ownership during conversion: {outcome:?}"),
    };
    assert_eq!(
        handoff.completion(),
        PersistentFailureCutCompletion::Incomplete
    );
    assert_eq!(
        handoff.cut_snapshot().state(),
        PersistentFailureCutState::Incomplete
    );
    drop(server);
}

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/pending_projection_quarantine.rs"
));

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/pending_projection_quarantine_retirement.rs"
));
