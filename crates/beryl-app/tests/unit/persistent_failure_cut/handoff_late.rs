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
