fn phase79_lineage() -> CasLineageProof {
    CasLineageProof::native(
        NativeCasLineage::Fresh,
        CasRepresentedPrefixProof::new(
            None,
            ThreadRevision::new(1).unwrap(),
            empty_selected_path_digest(),
        ),
    )
    .unwrap()
}

fn phase79_execution_binding(runtime_id: RuntimeId, root: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        runtime_id,
        RootId::from_bytes([root; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            r"C:\work\beryl",
        )
        .unwrap(),
    )
}

fn phase79_register_candidate_lease(
    service: &ProjectionConnectionService,
    connection: &Arc<ProjectionConnection>,
    cas_thread_id: CasThreadId,
    owner: SyndicThreadId,
) -> LoadedProjectionLease {
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
    let lease = connection
        .register_new(cas_thread_id, owner, Duration::from_secs(10), Some(&issuer))
        .unwrap();
    drop(worker);
    lease
}

fn phase79_acquire_candidate_sibling(
    service: &ProjectionConnectionService,
    connection: &Arc<ProjectionConnection>,
    cas_thread_id: &CasThreadId,
    owner: SyndicThreadId,
) -> LoadedProjectionLease {
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
    let lease = match connection
        .acquire_existing(cas_thread_id, owner, Duration::from_secs(10), Some(&issuer))
        .unwrap()
    {
        crate::cas_projection::connection::ExistingLease::Exact(lease) => lease,
        _ => panic!("the exact loaded projection must mint one sibling lease"),
    };
    drop(worker);
    lease
}

#[test]
fn pending_quarantine_preserves_all_sibling_leases_and_connection_groups() {
    let (_directory, faults, state, _shutdowns, service) = service_with_worker_capacity(10);
    let first_server = admission_server::NormalTerminalServer::spawn_admission_only();
    let second_server = admission_server::NormalTerminalServer::spawn_admission_only();
    let first_runtime = RuntimeId::from_bytes([130; 16]);
    let second_runtime = RuntimeId::from_bytes([131; 16]);
    let first_connector = ManagedBackendClientConnector::for_lifecycle_test(
        first_server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let second_connector = ManagedBackendClientConnector::for_lifecycle_test(
        second_server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let first_session = service
        .admit_lifecycle_test_candidate(
            &first_connector,
            first_runtime,
            CasProcessGeneration::new(79_130).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    first_server.wait_for_admission();
    let second_session = service
        .admit_lifecycle_test_candidate(
            &second_connector,
            second_runtime,
            CasProcessGeneration::new(79_131).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    second_server.wait_for_admission();
    let first_connection = Arc::clone(first_session.connection());
    let second_connection = Arc::clone(second_session.connection());

    let coordinator =
        CasProjectionCoordinator::for_healthy_home(service.home.as_deref().unwrap()).unwrap();
    let baseline_workers = service.worker_pool_diagnostics().active();
    let first_owner = SyndicThreadId::from_bytes([132; 16]);
    let first_thread = CasThreadId::new("phase-79-sibling-group").unwrap();
    let first_thread_for_close = first_thread.clone();
    let first_lease = phase79_register_candidate_lease(
        &service,
        first_session.connection(),
        first_thread.clone(),
        first_owner,
    );
    let sibling_lease = phase79_acquire_candidate_sibling(
        &service,
        first_session.connection(),
        &first_thread,
        first_owner,
    );
    let second_owner = SyndicThreadId::from_bytes([133; 16]);
    let second_thread = CasThreadId::new("phase-79-second-connection-group").unwrap();
    let second_lease = phase79_register_candidate_lease(
        &service,
        second_session.connection(),
        second_thread.clone(),
        second_owner,
    );
    let first_binding = phase79_execution_binding(first_runtime, 134);
    let second_binding = phase79_execution_binding(second_runtime, 135);
    let projections = vec![
        LoadedCasProjection::new(
            &coordinator,
            first_owner,
            BindingRevision::new(1).unwrap(),
            first_binding.clone(),
            first_thread.clone(),
            first_lease,
            phase79_lineage(),
        ),
        LoadedCasProjection::new(
            &coordinator,
            first_owner,
            BindingRevision::new(1).unwrap(),
            first_binding,
            first_thread,
            sibling_lease,
            phase79_lineage(),
        ),
        LoadedCasProjection::new(
            &coordinator,
            second_owner,
            BindingRevision::new(2).unwrap(),
            second_binding,
            second_thread,
            second_lease,
            phase79_lineage(),
        ),
    ];
    wait_until("three candidate admissions to remain bounded", || {
        service.worker_pool_diagnostics().active() == baseline_workers + 3
    });

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(projections);
    drop(first_session);
    drop(second_session);
    wait_until(
        "all candidate siblings to enter the exact failure cut",
        || {
            let snapshot = service.persistent_failure_cut_snapshot();
            snapshot.state() == PersistentFailureCutState::Finished
                && snapshot.retained_projection_count() == 3
        },
    );

    let worker_pool = service.workers.clone();
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("candidate groups must suppress ordinary close")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let counts = inventory.metadata().sealed_counts().unwrap();
    assert_eq!(counts.complete_candidate_count(), 3);
    assert_eq!(counts.connection_count(), 2);
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let metadata = quarantine.metadata();
    assert!(metadata.is_promotable());
    assert_eq!(metadata.group_count(), 2);
    assert_eq!(metadata.candidate_count(), 3);
    assert_eq!(metadata.retained_connection_count(), 2);
    assert_eq!(metadata.local_disposition_count(), 0);
    assert_eq!(worker_pool.diagnostics().active(), baseline_workers + 3);

    let first_scope = [first_connection.identity_observation()];
    let second_scope = [second_connection.identity_observation()];
    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&first_scope)
            .unwrap()
            .observations()
            .len(),
        2
    );
    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&second_scope)
            .unwrap()
            .observations()
            .len(),
        1
    );

    let closed = first_connection
        .record_thread_closed(&first_thread_for_close)
        .unwrap();
    assert!(!closed.connection_retired());
    assert!(closed.registry_authority_revoked());
    assert!(
        crate::cas_projection::connection::registry::recovery_audit(&first_scope)
            .unwrap()
            .observations()
            .is_empty()
    );
    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&second_scope)
            .unwrap()
            .observations()
            .len(),
        1
    );
    let after_close = quarantine.metadata();
    assert!(after_close.is_promotable());
    assert_eq!(after_close.group_count(), 2);
    assert_eq!(after_close.candidate_count(), 3);

    let duplicate = first_connection
        .record_thread_closed(&first_thread_for_close)
        .unwrap();
    assert!(!duplicate.connection_retired());
    assert!(!duplicate.registry_authority_revoked());
    assert!(quarantine.metadata().is_promotable());

    first_connection.poison_authority_for_recovery_test();
    assert!(!quarantine.metadata().is_promotable());
    drop(first_server);
    drop(second_server);
}

#[test]
fn missing_registry_sibling_returns_owning_error_and_rejects_second_conversion() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([136; 16]),
            CasProcessGeneration::new(79_136).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let lease = phase79_register_candidate_lease(
        &service,
        session.connection(),
        CasThreadId::new("phase-79-missing-registry-sibling").unwrap(),
        SyndicThreadId::from_bytes([137; 16]),
    );
    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("the live sibling's cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });

    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the live registry sibling must remain inside failed-service ownership")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let sealed_counts = inventory.metadata().sealed_counts().unwrap();
    assert_eq!(sealed_counts.connection_count(), 1);
    assert_eq!(sealed_counts.complete_candidate_count(), 0);
    assert_eq!(sealed_counts.raw_loaded_lease_count(), 0);

    let first_error = inventory.into_pending_projection_quarantine().unwrap_err();
    assert_eq!(
        first_error.reason(),
        crate::cas_projection::PersistentFailurePendingProjectionQuarantineReason::MissingSiblingToken
    );
    assert_eq!(
        first_error.inventory().metadata().sealed_counts().unwrap(),
        sealed_counts
    );
    let second_error = first_error
        .into_inventory()
        .into_pending_projection_quarantine()
        .unwrap_err();
    assert_eq!(
        second_error.reason(),
        crate::cas_projection::PersistentFailurePendingProjectionQuarantineReason::InventoryNotPromotable
    );

    assert!(matches!(
        lease.release().unwrap(),
        LoadedProjectionReleaseOutcome::ConnectionRetired
    ));
    let late = second_error.inventory().metadata();
    assert_eq!(late.sealed_counts(), Some(sealed_counts));
    assert_eq!(late.retained_counts().raw_loaded_lease_count(), 0);
    assert_eq!(late.late_publication_count(), 1);
    assert!(!late.is_promotable());
    let quarantine = second_error.metadata();
    assert_eq!(quarantine.group_count(), 0);
    assert_eq!(quarantine.candidate_count(), 0);
    assert_eq!(quarantine.retained_connection_count(), 1);
    assert_eq!(quarantine.local_disposition_count(), 1);
    assert_eq!(quarantine.late_publication_count(), 1);
    assert!(!quarantine.is_promotable());
    let inventory = second_error.into_inventory();
    assert_eq!(inventory.metadata().retained_counts(), Default::default());
    drop(session);
    drop(server);
}

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
