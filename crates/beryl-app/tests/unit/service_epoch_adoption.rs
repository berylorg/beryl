#[test]
fn phase82_exact_empty_set_adopts_one_unpublished_recovered_service() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let old_home_generation = service.home_generation();
    let old_service_generation = service.service_generation();

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("the empty Phase 82 cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the failed service must yield its retained recovery authority")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    assert!(quarantine.metadata().is_promotable());
    assert_eq!(quarantine.metadata().retained_connection_count(), 0);

    let recovery = retained_home.recover_same_home().unwrap();
    assert!(recovery.generation() > old_home_generation);
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        Arc::clone(&retained_home),
        config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    assert!(replacement.startup_gate().is_closed());
    let revision_before_adoption = retained_home.home_revision().unwrap();

    let adopted = quarantine.adopt_unpublished_service(replacement).unwrap();
    let metadata = adopted.metadata();
    assert_eq!(metadata.home_id(), retained_home.home_id());
    assert_eq!(metadata.old_home_generation(), old_home_generation);
    assert_eq!(metadata.new_home_generation(), recovery.generation());
    assert_eq!(metadata.old_service_generation(), old_service_generation);
    assert!(metadata.new_service_generation() > old_service_generation);
    assert_eq!(metadata.connection_count(), 0);
    assert_eq!(metadata.candidate_count(), 0);
    assert_eq!(metadata.group_count(), 0);
    assert_eq!(metadata.local_disposition_count(), 0);
    assert_eq!(
        retained_home.home_revision().unwrap(),
        revision_before_adoption
    );
    let startup_gate = adopted.startup_gate_for_test();
    assert!(startup_gate.is_closed());
    drop(adopted);
    assert!(!startup_gate.is_closed());
}

#[test]
fn phase82_one_connection_replaces_epoch_while_preserving_stable_core_and_old_capacity() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([182; 16]),
            CasProcessGeneration::new(82_182).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());
    let stable_identity = connection.identity_observation();
    let old_epoch = connection.epoch_identity_for_adoption_test().unwrap();
    let old_worker_count = service.worker_pool_diagnostics().active();

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(session);
    wait_until("the retained connection's Phase 82 cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the failed connection service must remain recovery-owned")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let retained_home = Arc::clone(inventory.retained_home());
    let old_workers = inventory.retained_worker_pool();
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    assert_eq!(quarantine.metadata().retained_connection_count(), 1);
    assert_eq!(quarantine.metadata().candidate_count(), 0);

    let recovery = retained_home.recover_same_home().unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        retained_home,
        config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    let adopted = quarantine.adopt_unpublished_service(replacement).unwrap();

    assert_eq!(connection.identity_observation(), stable_identity);
    let new_epoch = connection.epoch_identity_for_adoption_test().unwrap();
    assert_eq!(new_epoch.home_id(), old_epoch.home_id());
    assert_eq!(new_epoch.home_generation(), recovery.generation());
    assert!(new_epoch.home_generation() > old_epoch.home_generation());
    assert!(new_epoch.service_generation() > old_epoch.service_generation());
    assert!(adopted.startup_fence_is_closed_for_test());
    assert_eq!(adopted.adopted_connection_count_for_test(), 1);
    assert_eq!(
        adopted.replacement_worker_diagnostics_for_test().active(),
        2
    );
    assert_eq!(old_workers.diagnostics().active(), old_worker_count);

    drop(adopted);
    drop(connection);
    server.assert_quiet_and_close();
    server.join();
}

#[test]
fn phase82_candidate_hold_moves_to_replacement_pool_without_rebinding_registry_identity() {
    let (_directory, faults, state, _shutdowns, service) = service_with_worker_capacity(6);
    let server = admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let runtime_id = RuntimeId::from_bytes([183; 16]);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(82_183).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());
    let stable_identity = connection.identity_observation();
    let owner = SyndicThreadId::from_bytes([184; 16]);
    let cas_thread_id = CasThreadId::new("phase-82-candidate-hold").unwrap();
    let lease =
        phase79_register_candidate_lease(&service, &connection, cas_thread_id.clone(), owner);
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(service.home.as_deref().unwrap()).unwrap();
    let projection = LoadedCasProjection::new(
        &coordinator,
        owner,
        BindingRevision::new(1).unwrap(),
        phase79_execution_binding(runtime_id, 185),
        cas_thread_id,
        lease,
        phase79_lineage(),
    );
    wait_until("the candidate hold to charge the old service", || {
        service.worker_pool_diagnostics().active() == 3
    });

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(projection);
    drop(session);
    wait_until("the candidate to enter the Phase 82 cut", || {
        let snapshot = service.persistent_failure_cut_snapshot();
        snapshot.state() == PersistentFailureCutState::Finished
            && snapshot.retained_projection_count() == 1
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the retained candidate must suppress ordinary close")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let retained_home = Arc::clone(inventory.retained_home());
    let old_workers = inventory.retained_worker_pool();
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    assert_eq!(quarantine.metadata().candidate_count(), 1);
    assert_eq!(quarantine.metadata().retained_connection_count(), 1);
    let scope = [stable_identity];
    let registry_before = crate::cas_projection::connection::registry::recovery_audit(&scope)
        .unwrap()
        .into_observations();

    retained_home.recover_same_home().unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        retained_home,
        config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    let adopted = quarantine.adopt_unpublished_service(replacement).unwrap();

    assert_eq!(old_workers.diagnostics().active(), 3);
    assert_eq!(
        adopted.replacement_worker_diagnostics_for_test().active(),
        3
    );
    assert_eq!(
        crate::cas_projection::connection::registry::recovery_audit(&scope)
            .unwrap()
            .into_observations(),
        registry_before
    );
    assert_eq!(connection.identity_observation(), stable_identity);
    assert!(adopted.startup_fence_is_closed_for_test());

    drop(adopted);
    drop(connection);
    server.assert_quiet_and_close();
    server.join();
}

#[test]
fn phase82_unpublished_service_construction_does_not_consume_a_recovered_read() {
    let (_directory, faults, state, _shutdowns, service) = service();

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    wait_until("the dormant-construction cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the failed service must yield recovery authority")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    retained_home.recover_same_home().unwrap();

    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        Arc::clone(&retained_home),
        config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();

    assert!(replacement.startup_gate().is_closed());
    assert!(retained_home.home_revision().is_err());
    drop(replacement);
    drop(quarantine);
}

#[test]
fn phase82_poisoned_hub_inert_transition_detaches_and_retains_the_exact_epoch() {
    let (_directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([186; 16]),
            CasProcessGeneration::new(82_186).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());
    let old_epoch = connection.epoch_pointer_for_adoption_test().unwrap();

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(session);
    wait_until("the poison-transition failure cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the retained connection must remain recovery-owned")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let cut = crate::cas_projection::persistent_failure::PersistentFailureCutIdentity::new(
        quarantine.home_id(),
        quarantine.home_generation(),
        quarantine.service_generation(),
        quarantine.failure_generation(),
    );

    connection.poison_forwarding_epoch_barrier_for_test();
    let inert = connection.make_adoption_inert_in_place(cut);

    assert!(inert.retains_epoch_pointer_for_test(old_epoch));
    assert!(connection.forwarding_epoch_is_inert_and_detached_for_test());
    wait_until(
        "the detached poisoned epoch ingester to observe cancellation",
        || inert.ingester_is_finished_for_test(),
    );

    inert.dispose();
    connection
        .dispose_inert_driver_after_adoption_failure()
        .unwrap();
    server.join();
    drop(quarantine);
    drop(connection);
}

mod failure_disposition {
    use super::*;
    use crate::cas_projection::PersistentFailurePendingProjectionQuarantineReason;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/service_epoch_adoption/failure_disposition.rs"
    ));
}

mod publication_fence {
    use super::*;
    use crate::cas_projection::PersistentFailurePendingProjectionQuarantineReason;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/service_epoch_adoption/publication_fence.rs"
    ));
}

mod preflight_mismatch {
    use super::*;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/service_epoch_adoption/preflight_mismatch.rs"
    ));
}

mod multi_connection_success {
    use super::*;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/service_epoch_adoption/multi_connection_success.rs"
    ));
}

mod command_frontier {
    use super::*;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/service_epoch_adoption/command_frontier.rs"
    ));
}

mod resource_failure_ownership {
    use super::*;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/service_epoch_adoption/resource_failure_ownership.rs"
    ));
}
