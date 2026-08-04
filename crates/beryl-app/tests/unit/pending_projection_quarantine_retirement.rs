#[test]
fn connection_quarantine_owner_fences_retirement_across_registry_commit() {
    let (_directory, faults, state, _shutdowns, service) = service_with_worker_capacity(6);
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let runtime_id = RuntimeId::from_bytes([150; 16]);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(79_150).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let connection = Arc::clone(session.connection());
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(service.home.as_deref().unwrap()).unwrap();
    let owner = SyndicThreadId::from_bytes([151; 16]);
    let cas_thread_id = CasThreadId::new("phase-79-retired-candidate").unwrap();
    let lease =
        phase79_register_candidate_lease(&service, &connection, cas_thread_id.clone(), owner);
    let projection = LoadedCasProjection::new(
        &coordinator,
        owner,
        BindingRevision::new(1).unwrap(),
        phase79_execution_binding(runtime_id, 152),
        cas_thread_id,
        lease,
        phase79_lineage(),
    );
    let promotion = connection
        .reserve_scheduled_promotion()
        .unwrap()
        .expect("the candidate connection admits one exact promotion barrier");
    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);

    fail_home_through_live_command(&service, state, &faults);
    retainer.retain_promotion(promotion);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(projection);
    drop(session);
    wait_until(
        "candidate and promotion barrier to enter one finished cut",
        || {
            let snapshot = service.persistent_failure_cut_snapshot();
            snapshot.state() == PersistentFailureCutState::Finished
                && snapshot.retained_projection_count() == 1
                && snapshot.retained_promotion_count() == 1
        },
    );

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("candidate and barrier authority must remain failure-retained")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let scope = [connection.identity_observation()];
    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&scope)
            .unwrap()
            .observations()
            .len(),
        1
    );
    let (reached_tx, reached_rx) = std::sync::mpsc::sync_channel(0);
    let (resume_tx, resume_rx) = std::sync::mpsc::sync_channel(0);
    inventory.observe_next_connection_owner_install_for_test(reached_tx, resume_rx);
    let conversion = std::thread::spawn(move || inventory.into_pending_projection_quarantine());
    reached_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("conversion installs the exact connection quarantine owner");
    assert!(matches!(
        connection.retire_authority_for_recovery_test().unwrap(),
        crate::cas_projection::connection::ConnectionRetirementOutcome::FailureRetained(_)
    ));
    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&scope)
            .unwrap()
            .observations()
            .len(),
        1,
        "the quarantine owner must fence retirement before registry commit"
    );
    resume_tx.send(()).unwrap();
    let quarantine = conversion.join().unwrap().unwrap();
    assert_eq!(quarantine.metadata().candidate_count(), 1);
    assert!(!quarantine.metadata().is_promotable());
    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&scope)
            .unwrap()
            .observations()
            .len(),
        1,
        "the retained candidate token must survive retirement and registry commit"
    );
    drop(server);
}

#[test]
fn missing_target_result_is_rejected_before_registry_mutation() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let runtime_id = RuntimeId::from_bytes([153; 16]);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(79_153).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let connection = Arc::clone(session.connection());
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
    let owner = SyndicThreadId::from_bytes([154; 16]);
    let cas_thread_id = CasThreadId::new("phase-79-missing-target-result").unwrap();
    let lease = connection
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
        phase79_execution_binding(runtime_id, 155),
        cas_thread_id,
        lease,
        phase79_lineage(),
    );
    let mut target = projection
        .into_active_live_event_target(
            beryl_model::CasTurnId::new("phase-79-missing-target-result-turn").unwrap(),
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
    wait_until("failed target to enter exact failure retention", || {
        let snapshot = service.persistent_failure_cut_snapshot();
        snapshot.state() == PersistentFailureCutState::Finished
            && snapshot.retained_projection_count() == 1
    });

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the failed target and loaded token must remain failure-retained")
        }
    };
    assert!(handoff.orphan_one_retained_target_result_for_test());
    let inventory = handoff.into_recovery_inventory().unwrap();
    let counts = inventory.metadata().sealed_counts().unwrap();
    assert_eq!(counts.target_projection_count(), 1);
    assert_eq!(counts.target_result_count(), 0);
    let scope = [connection.identity_observation()];
    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&scope)
            .unwrap()
            .observations()
            .len(),
        1
    );

    let error = inventory.into_pending_projection_quarantine().unwrap_err();
    assert_eq!(
        error.reason(),
        crate::cas_projection::PersistentFailurePendingProjectionQuarantineReason::TargetDispositionMismatch
    );
    let inventory_metadata = error.inventory().metadata();
    assert!(!inventory_metadata.retention_poisoned());
    assert!(!inventory_metadata.is_promotable());
    assert!(!error.metadata().is_promotable());
    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&scope)
            .unwrap()
            .observations()
            .len(),
        1,
        "aggregate guard mismatch must be proven before loaded-registry mutation"
    );
    drop(server);
}

#[test]
fn publication_crossing_checkout_is_retained_by_the_installed_owning_error() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([156; 16]),
            CasProcessGeneration::new(79_156).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let lease = phase79_register_candidate_lease(
        &service,
        session.connection(),
        CasThreadId::new("phase-79-checkout-crossing-publication").unwrap(),
        SyndicThreadId::from_bytes([157; 16]),
    );

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(session);
    wait_until("checkout-crossing cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the live loaded token must keep the failed service retained")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let (reached_tx, reached_rx) = std::sync::mpsc::sync_channel(0);
    let (resume_tx, resume_rx) = std::sync::mpsc::sync_channel(0);
    inventory.observe_next_pending_quarantine_checkout_for_test(reached_tx, resume_rx);
    let conversion = std::thread::spawn(move || inventory.into_pending_projection_quarantine());
    reached_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("conversion reaches the checked-out stage");

    assert!(matches!(
        lease.release().unwrap(),
        LoadedProjectionReleaseOutcome::ConnectionRetired
    ));
    resume_tx.send(()).unwrap();
    let error = conversion.join().unwrap().unwrap_err();
    let metadata = error.metadata();
    assert_eq!(metadata.late_publication_count(), 1);
    assert_eq!(metadata.local_disposition_count(), 1);
    assert!(!metadata.is_promotable());
    drop(server);
}

#[test]
fn phase79_conversion_source_has_no_external_work_boundary() {
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let relative_paths = [
        "src/cas_projection/persistent_failure/quarantine/conversion.rs",
        "src/cas_projection/persistent_failure/quarantine/conversion/preflight.rs",
        "src/cas_projection/persistent_failure/quarantine/conversion/normalization.rs",
    ];
    let forbidden = [
        "ManagedBackend",
        "ConnectionRequestSession",
        "call_ordered",
        "unsubscribe",
        "LiveCommandPermit",
        "LiveCommandAuthorizer",
        "HomeCommand",
        "HomeStore",
        "SyndicStorage",
        "ProjectionConnectionService",
        "transfer_quarantined",
        "reserve_reacquisition",
        "register_new",
        "acquire_existing",
        "rebind",
        "publish_",
    ];
    for relative_path in relative_paths {
        let source = std::fs::read_to_string(crate_root.join(relative_path))
            .expect("Phase 79 conversion source is readable");
        for forbidden in forbidden {
            assert!(
                !source.contains(forbidden),
                "{relative_path} crossed the inert conversion boundary with {forbidden}"
            );
        }
    }
}
