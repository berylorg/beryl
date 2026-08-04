struct OneConnectionPreflightFixture {
    _directory: tempfile::TempDir,
    server: admission_server::NormalTerminalServer,
    connection: Arc<ProjectionConnection>,
    registry: Arc<crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry>,
    retained_home: Arc<HomeStore>,
    config: ProjectionServiceConfig,
    quarantine: crate::cas_projection::PersistentFailurePendingProjectionQuarantine,
}

fn one_connection_preflight_fixture(identity_byte: u8) -> OneConnectionPreflightFixture {
    let (directory, faults, state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([identity_byte; 16]),
            CasProcessGeneration::new(u64::from(identity_byte) + 82_300).unwrap(),
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
    wait_until("the preflight-mismatch cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("the retained connection must remain recovery-owned")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let registry = inventory.retained_connection_registry();
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    assert_eq!(quarantine.metadata().retained_connection_count(), 1);

    OneConnectionPreflightFixture {
        _directory: directory,
        server,
        connection,
        registry,
        retained_home,
        config,
        quarantine,
    }
}

fn assert_preflight_failure(
    fixture: OneConnectionPreflightFixture,
    replacement: UnpublishedProjectionConnectionService,
    expected: PersistentFailureServiceAdoptionReason,
) {
    let OneConnectionPreflightFixture {
        _directory,
        server,
        connection,
        registry,
        retained_home,
        config: _,
        quarantine,
    } = fixture;
    let startup = Arc::clone(replacement.startup_gate());
    assert!(startup.is_closed());

    let error = quarantine
        .adopt_unpublished_service(replacement)
        .unwrap_err();
    assert_eq!(error.reason(), expected);
    assert_eq!(error.metadata().connection_count(), 1);
    assert!(connection.forwarding_epoch_is_inert_and_detached_for_test());

    let (startup_tx, startup_rx) = std::sync::mpsc::sync_channel(1);
    let startup_wait = Arc::clone(&startup);
    let waiter = std::thread::spawn(move || {
        startup_tx.send(startup_wait.wait()).unwrap();
    });
    let opened = match startup_rx.recv_timeout(Duration::from_secs(1)) {
        Ok(opened) => opened,
        Err(error) => {
            startup.cancel();
            waiter.join().unwrap();
            panic!("the failed unpublished-service startup fence did not settle: {error}");
        }
    };
    waiter.join().unwrap();
    assert!(
        !opened,
        "a failed adoption must never publish its replacement"
    );

    drop(error);
    drop(registry);
    drop(connection);
    drop(retained_home);
    server.assert_quiet_and_close();
    server.join();
    drop(_directory);
}

#[test]
fn phase82_service_config_mismatch_returns_one_inert_owner_before_first_park() {
    let fixture = one_connection_preflight_fixture(190);
    fixture.retained_home.recover_same_home().unwrap();
    let mismatched_config = ProjectionServiceConfig::try_new(9, 4).unwrap();
    assert_ne!(mismatched_config, fixture.config);
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        Arc::clone(&fixture.retained_home),
        mismatched_config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();

    assert_preflight_failure(
        fixture,
        replacement,
        PersistentFailureServiceAdoptionReason::ServiceConfigMismatch,
    );
}

#[test]
fn phase82_foreign_home_instance_returns_one_inert_owner_before_first_park() {
    let fixture = one_connection_preflight_fixture(191);
    let foreign_directory = tempfile::tempdir().unwrap();
    let mut foreign_home = HomeStore::open(HomeOpenOptions::new(
        foreign_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let _foreign_storage = SyndicStorage::register(&mut foreign_home).unwrap();
    let _foreign_state = BerylState::register(&mut foreign_home).unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        Arc::new(foreign_home),
        fixture.config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();

    assert_preflight_failure(
        fixture,
        replacement,
        PersistentFailureServiceAdoptionReason::HomeInstanceMismatch,
    );
}

#[test]
fn phase82_duplicate_stable_registry_member_rejects_the_whole_set_before_first_park() {
    let fixture = one_connection_preflight_fixture(192);
    fixture.retained_home.recover_same_home().unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        Arc::clone(&fixture.retained_home),
        fixture.config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    fixture
        .registry
        .lock()
        .unwrap()
        .push(Arc::clone(&fixture.connection));

    assert_preflight_failure(
        fixture,
        replacement,
        PersistentFailureServiceAdoptionReason::DuplicateConnection,
    );
}

#[test]
fn phase82_missing_stable_registry_member_rejects_the_whole_set_before_first_park() {
    let fixture = one_connection_preflight_fixture(193);
    fixture.retained_home.recover_same_home().unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        Arc::clone(&fixture.retained_home),
        fixture.config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    fixture.registry.lock().unwrap().clear();

    assert_preflight_failure(
        fixture,
        replacement,
        PersistentFailureServiceAdoptionReason::ConnectionSetMismatch,
    );
}
