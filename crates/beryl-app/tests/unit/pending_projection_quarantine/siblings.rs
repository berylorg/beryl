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
