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
