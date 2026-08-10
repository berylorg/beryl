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
