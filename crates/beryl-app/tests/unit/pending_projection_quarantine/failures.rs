#[test]
fn sibling_witness_disagreement_never_publishes_a_partial_group() {
    let (_directory, faults, state, _shutdowns, service) = service_with_worker_capacity(6);
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let runtime_id = RuntimeId::from_bytes([138; 16]);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(79_138).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let coordinator =
        CasProjectionCoordinator::for_healthy_home(service.home.as_deref().unwrap()).unwrap();
    let owner = SyndicThreadId::from_bytes([139; 16]);
    let cas_thread_id = CasThreadId::new("phase-79-witness-disagreement").unwrap();
    let first_lease = phase79_register_candidate_lease(
        &service,
        session.connection(),
        cas_thread_id.clone(),
        owner,
    );
    let second_lease =
        phase79_acquire_candidate_sibling(&service, session.connection(), &cas_thread_id, owner);
    let execution_binding = phase79_execution_binding(runtime_id, 140);
    let projections = vec![
        LoadedCasProjection::new(
            &coordinator,
            owner,
            BindingRevision::new(1).unwrap(),
            execution_binding.clone(),
            cas_thread_id.clone(),
            first_lease,
            phase79_lineage(),
        ),
        LoadedCasProjection::new(
            &coordinator,
            owner,
            BindingRevision::new(2).unwrap(),
            execution_binding,
            cas_thread_id,
            second_lease,
            phase79_lineage(),
        ),
    ];

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(projections);
    drop(session);
    wait_until(
        "both disagreeing sibling witnesses to enter retention",
        || {
            let snapshot = service.persistent_failure_cut_snapshot();
            snapshot.state() == PersistentFailureCutState::Finished
                && snapshot.retained_projection_count() == 2
        },
    );

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("disagreeing candidate witnesses must suppress ordinary close")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    assert_eq!(
        inventory
            .metadata()
            .sealed_counts()
            .unwrap()
            .complete_candidate_count(),
        2
    );
    let error = inventory.into_pending_projection_quarantine().unwrap_err();
    assert_eq!(
        error.reason(),
        crate::cas_projection::PersistentFailurePendingProjectionQuarantineReason::WitnessDisagreement
    );
    assert_eq!(
        error
            .inventory()
            .metadata()
            .sealed_counts()
            .unwrap()
            .complete_candidate_count(),
        2
    );
    drop(server);
}

#[test]
fn orphaned_retained_promotion_barrier_returns_an_owning_topology_error() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([141; 16]),
            CasProcessGeneration::new(79_141).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let promotion = session
        .connection()
        .reserve_scheduled_promotion()
        .unwrap()
        .expect("the live connection admits one exact promotion barrier");
    let retainer = service
        .persistent_failure
        .as_ref()
        .unwrap()
        .projection_retainer(service.home_id, service.home_generation);

    fail_home_through_live_command(&service, state, &faults);
    retainer.retain_promotion(promotion);
    drop(session);
    wait_until("the promotion-owning cut to finish", || {
        let snapshot = service.persistent_failure_cut_snapshot();
        snapshot.state() == PersistentFailureCutState::Finished
            && snapshot.retained_promotion_count() == 1
    });

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the retained promotion barrier must suppress ordinary close")
        }
    };
    assert!(handoff.orphan_one_retained_promotion_for_test());
    let inventory = handoff.into_recovery_inventory().unwrap();
    let sealed_counts = inventory.metadata().sealed_counts().unwrap();
    assert_eq!(sealed_counts.connection_count(), 1);
    assert_eq!(sealed_counts.promotion_count(), 0);
    assert!(inventory.metadata().is_promotable());

    let error = inventory.into_pending_projection_quarantine().unwrap_err();
    assert_eq!(
        error.reason(),
        crate::cas_projection::PersistentFailurePendingProjectionQuarantineReason::BarrierDispositionMismatch
    );
    assert_eq!(
        error.inventory().metadata().sealed_counts().unwrap(),
        sealed_counts
    );
    drop(server);
}

#[test]
fn recovery_owner_drop_revokes_its_token_under_registry_poison() {
    let (_directory, _faults, _state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([142; 16]),
            CasProcessGeneration::new(79_142).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let connection = Arc::clone(session.connection());
    let baseline_workers = service.worker_pool_diagnostics().active();
    let lease = phase79_register_candidate_lease(
        &service,
        &connection,
        CasThreadId::new("phase-79-poison-safe-owner-drop").unwrap(),
        SyndicThreadId::from_bytes([143; 16]),
    );
    let scope = [connection.identity_observation()];
    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&scope)
            .unwrap()
            .observations()
            .len(),
        1
    );
    let owner = lease.into_pending_projection_lease_owner();
    wait_until("the recovery owner to retain worker admission", || {
        service.worker_pool_diagnostics().active() == baseline_workers + 1
    });

    crate::cas_projection::connection::registry::poison_loaded_registry_for_recovery_drop_test();
    drop(owner);
    crate::cas_projection::connection::registry::clear_loaded_registry_poison_for_test();

    assert!(
        crate::cas_projection::connection::registry::recovery_audit(&scope)
            .unwrap()
            .observations()
            .is_empty()
    );
    assert_eq!(service.worker_pool_diagnostics().active(), baseline_workers);
    drop(session);
    drop(connection);
    drop(service);
    drop(server);
}

#[test]
fn wrong_retained_connection_set_returns_an_owning_identity_error() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([144; 16]),
            CasProcessGeneration::new(79_144).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("the connection-set mismatch cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the failed mounted connection must remain owned")
        }
    };
    assert!(handoff.orphan_one_retained_connection_for_test());
    let inventory = handoff.into_recovery_inventory().unwrap();
    assert_eq!(
        inventory
            .metadata()
            .sealed_counts()
            .unwrap()
            .connection_count(),
        0
    );
    let error = inventory.into_pending_projection_quarantine().unwrap_err();
    assert_eq!(
        error.reason(),
        crate::cas_projection::PersistentFailurePendingProjectionQuarantineReason::ConnectionIdentityMismatch
    );
    assert!(!error.metadata().is_promotable());
    drop(session);
    drop(server);
}

#[test]
fn registry_poison_returns_an_owning_inert_quarantine_error() {
    let (_directory, faults, state, _shutdowns, service) = service();
    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("the registry-poison cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the failed service must retain its empty cut")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    assert!(inventory.metadata().is_promotable());

    crate::cas_projection::connection::registry::poison_loaded_registry_for_recovery_drop_test();
    let error = inventory.into_pending_projection_quarantine().unwrap_err();
    crate::cas_projection::connection::registry::clear_loaded_registry_poison_for_test();

    assert_eq!(
        error.reason(),
        crate::cas_projection::PersistentFailurePendingProjectionQuarantineReason::RegistryUnavailable
    );
    let metadata = error.metadata();
    assert_eq!(metadata.group_count(), 0);
    assert_eq!(metadata.candidate_count(), 0);
    assert_eq!(metadata.retained_connection_count(), 0);
    assert_eq!(metadata.local_disposition_count(), 0);
    assert!(!metadata.is_promotable());
}
