struct PoisonedAdoptionFixture {
    _directory: tempfile::TempDir,
    error: PersistentFailureServiceAdoptionError,
    connection: Arc<ProjectionConnection>,
}

fn poisoned_adoption_fixture(
    server: &admission_server::NormalTerminalServer,
    identity_byte: u8,
) -> PoisonedAdoptionFixture {
    let (directory, faults, state, _shutdowns, service) = service();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([identity_byte; 16]),
            CasProcessGeneration::new(u64::from(identity_byte) + 82_000).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(session);
    wait_until("the poisoned-adoption cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the retained connection must remain recovery-owned")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    retained_home.recover_same_home().unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        retained_home,
        config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();

    connection.poison_forwarding_epoch_barrier_for_test();
    let error = quarantine
        .adopt_unpublished_service(replacement)
        .unwrap_err();
    assert_eq!(
        error.reason(),
        PersistentFailureServiceAdoptionReason::DriverPark
    );
    PoisonedAdoptionFixture {
        _directory: directory,
        error,
        connection,
    }
}

fn first_reservation_failure_fixture(
    server: &admission_server::NormalTerminalServer,
    identity_byte: u8,
) -> PoisonedAdoptionFixture {
    let (directory, faults, state, _shutdowns, service) = service();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([identity_byte; 16]),
            CasProcessGeneration::new(u64::from(identity_byte) + 93_000).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(session);
    wait_until("the first-reservation failure cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the retained connection must remain recovery-owned")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let home_id = quarantine.home_id();
    retained_home.recover_same_home().unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        retained_home,
        config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();

    super::super::adoption::fail_next_first_inert_reservation_for_test(home_id);
    let error = quarantine
        .adopt_unpublished_service(replacement)
        .unwrap_err();
    assert_eq!(
        error.reason(),
        PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable
    );
    assert!(error.inventory_reescrow_is_disarmed_for_test());
    assert!(connection.forwarding_epoch_is_inert_and_attached_for_test());
    PoisonedAdoptionFixture {
        _directory: directory,
        error,
        connection,
    }
}

fn armed_post_preflight_failure_fixture(
    server: &admission_server::NormalTerminalServer,
    identity_byte: u8,
    arm: impl FnOnce(
        crate::cas_projection::persistent_failure::PersistentFailureCutIdentity,
        &Arc<ProjectionConnection>,
    ),
    expected: PersistentFailureServiceAdoptionReason,
) -> PoisonedAdoptionFixture {
    let (directory, faults, state, _shutdowns, service) = service();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([identity_byte; 16]),
            CasProcessGeneration::new(u64::from(identity_byte) + 93_200).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(session);
    wait_until("the post-preflight failure cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the retained connection must remain recovery-owned")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let cut = crate::cas_projection::persistent_failure::PersistentFailureCutIdentity::new(
        quarantine.home_id(),
        quarantine.home_generation(),
        quarantine.service_generation(),
        quarantine.failure_generation(),
    );
    retained_home.recover_same_home().unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        retained_home,
        config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();

    arm(cut, &connection);
    let error = quarantine
        .adopt_unpublished_service(replacement)
        .unwrap_err();
    assert_eq!(error.reason(), expected);
    assert!(error.inventory_reescrow_is_disarmed_for_test());
    assert!(connection.forwarding_epoch_is_inert_and_detached_for_test());
    PoisonedAdoptionFixture {
        _directory: directory,
        error,
        connection,
    }
}

#[test]
fn phase82_explicit_inert_disposition_releases_and_joins_the_parked_driver() {
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let fixture = poisoned_adoption_fixture(&server, 187);

    fixture.error.dispose().unwrap();
    drop(fixture.connection);
    server.join();
}

#[test]
fn phase82_implicit_inert_drop_is_bounded_and_emits_no_backend_frame() {
    let server = admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let fixture = poisoned_adoption_fixture(&server, 188);
    let (dropped_tx, dropped_rx) = std::sync::mpsc::sync_channel(1);

    let disposer = std::thread::spawn(move || {
        drop(fixture.error);
        dropped_tx.send(fixture.connection).unwrap();
    });
    let connection = dropped_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("implicit inert-owner drop must never wait for parked workers");
    disposer.join().unwrap();

    drop(connection);
    server.assert_quiet_and_close();
    server.join();
}

#[test]
fn phase93_first_reservation_failure_retains_endpoint_until_explicit_disposition() {
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let fixture = first_reservation_failure_fixture(&server, 190);

    fixture.error.dispose().unwrap();
    assert!(
        fixture
            .connection
            .forwarding_epoch_is_inert_and_detached_for_test()
    );
    drop(fixture.connection);
    server.join();
}

#[test]
fn phase93_first_reservation_failure_implicit_drop_is_bounded_and_nonretryable() {
    let server = admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let fixture = first_reservation_failure_fixture(&server, 191);
    let (dropped_tx, dropped_rx) = std::sync::mpsc::sync_channel(1);

    let disposer = std::thread::spawn(move || {
        drop(fixture.error);
        dropped_tx.send(fixture.connection).unwrap();
    });
    let connection = dropped_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("first-reservation inert-owner drop must remain bounded");
    disposer.join().unwrap();
    assert!(connection.forwarding_epoch_is_inert_and_attached_for_test());

    drop(connection);
    server.assert_quiet_and_close();
    server.join();
}

#[test]
fn phase93_old_ingester_join_failure_returns_one_explicitly_disposable_inert_owner() {
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let fixture = armed_post_preflight_failure_fixture(
        &server,
        192,
        |_cut, connection| {
            connection
                .fail_current_epoch_ingester_join_for_test()
                .unwrap();
        },
        PersistentFailureServiceAdoptionReason::OldIngesterJoin,
    );

    assert!(matches!(
        fixture.error.dispose(),
        Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
    ));
    drop(fixture.connection);
    server.join();
}

#[test]
fn phase93_post_join_panic_returns_one_explicitly_disposable_unexpected_unwind_owner() {
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let fixture = armed_post_preflight_failure_fixture(
        &server,
        193,
        |cut, _connection| {
            super::super::adoption::panic_next_adoption_after_old_ingester_join_for_test(
                cut.home_id,
            );
        },
        PersistentFailureServiceAdoptionReason::UnexpectedUnwind,
    );

    fixture.error.dispose().unwrap();
    drop(fixture.connection);
    server.join();
}

#[test]
fn phase82_late_authority_before_commit_returns_one_inert_unpublished_owner() {
    let (directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([189; 16]),
            CasProcessGeneration::new(82_189).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(session);
    wait_until("the late-authority adoption cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the retained connection must remain recovery-owned")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    let home_id = quarantine.home_id();
    retained_home.recover_same_home().unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        retained_home,
        config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    super::super::adoption::retain_late_authority_before_next_adoption_commit_for_test(home_id);

    let error = quarantine
        .adopt_unpublished_service(replacement)
        .unwrap_err();
    assert_eq!(
        error.reason(),
        PersistentFailureServiceAdoptionReason::LatePublication(
            PersistentFailurePendingProjectionQuarantineReason::LatePublication
        )
    );
    assert!(connection.forwarding_epoch_is_inert_and_detached_for_test());

    drop(error);
    drop(connection);
    drop(directory);
    server.assert_quiet_and_close();
    server.join();
}
