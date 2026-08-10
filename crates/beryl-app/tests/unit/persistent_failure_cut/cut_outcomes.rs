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
