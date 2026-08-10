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
    one_connection_preflight_fixture_with_server_mode(identity_byte, true)
}

fn one_connection_preflight_fixture_with_server_mode(
    identity_byte: u8,
    controlled_close: bool,
) -> OneConnectionPreflightFixture {
    let (directory, faults, state, _shutdowns, service) = service();
    let server = if controlled_close {
        admission_server::NormalTerminalServer::spawn_admission_only_controlled_close()
    } else {
        admission_server::NormalTerminalServer::spawn_admission_only()
    };
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

#[test]
fn phase93_registry_drift_before_fenced_commit_returns_one_inert_owner() {
    let fixture = one_connection_preflight_fixture_with_server_mode(194, false);
    fixture.retained_home.recover_same_home().unwrap();
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        Arc::clone(&fixture.retained_home),
        fixture.config,
        Box::new(ShutdownProbe {
            count: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    let home_id = fixture.quarantine.home_id();
    let quarantine = fixture.quarantine;
    let pause = super::super::adoption::pause_next_adoption_before_commit_for_test(home_id);
    let adoption =
        std::thread::spawn(move || quarantine.adopt_unpublished_service(replacement));

    pause.wait_until_reached();
    fixture.registry.lock().unwrap().clear();
    pause.release();
    let error = adoption.join().unwrap().unwrap_err();

    assert_eq!(
        error.reason(),
        PersistentFailureServiceAdoptionReason::ConnectionSetMismatch
    );
    assert!(error.inventory_reescrow_is_disarmed_for_test());
    assert!(fixture.connection.forwarding_epoch_is_inert_and_detached_for_test());
    error.dispose().unwrap();
    drop(fixture.registry);
    drop(fixture.connection);
    drop(fixture.retained_home);
    fixture.server.join();
    drop(fixture._directory);
}

#[derive(Clone, Copy)]
enum Phase95AdversarialTopology {
    ExtraConnectionOwner,
    RetiredConnectionOwner,
    ForeignFailureCutOwner,
}

#[derive(Clone)]
struct Phase95ProviderProbe {
    issues: Arc<AtomicUsize>,
    shutdowns: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProvider for Phase95ProviderProbe {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.issues.fetch_add(1, Ordering::SeqCst);
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
    }
}

struct Phase95SecondarySettlement {
    _directory: tempfile::TempDir,
    server: admission_server::NormalTerminalServer,
    connection: Arc<ProjectionConnection>,
    registry: Arc<crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry>,
    retained_home: Arc<HomeStore>,
}

fn phase95_assert_startup_cancelled(startup: Arc<ServiceStartupGate>) {
    let (settled_tx, settled_rx) = std::sync::mpsc::sync_channel(1);
    let waiter = std::thread::spawn(move || settled_tx.send(startup.wait()).unwrap());
    assert!(!settled_rx.recv_timeout(Duration::from_secs(1)).unwrap());
    waiter.join().unwrap();
}

fn phase95_adversarial_topology_fixture(
    adversary: Phase95AdversarialTopology,
    primary_identity: u8,
    secondary_identity: Option<u8>,
    expected_reason: PersistentFailureServiceAdoptionReason,
    expected_owner_count: usize,
) {
    let primary = one_connection_preflight_fixture_with_server_mode(primary_identity, false);
    let secondary = secondary_identity
        .map(|identity| one_connection_preflight_fixture_with_server_mode(identity, false));
    let (secondary_quarantine, secondary_settlement) = match secondary {
        Some(OneConnectionPreflightFixture {
            _directory,
            server,
            connection,
            registry,
            retained_home,
            config: _,
            quarantine,
        }) => (
            Some(quarantine),
            Some(Phase95SecondarySettlement {
                _directory,
                server,
                connection,
                registry,
                retained_home,
            }),
        ),
        None => (None, None),
    };
    let OneConnectionPreflightFixture {
        _directory,
        server,
        connection,
        registry,
        retained_home,
        config,
        quarantine,
    } = primary;

    retained_home.recover_same_home().unwrap();
    let issues = Arc::new(AtomicUsize::new(0));
    let shutdowns = Arc::new(AtomicUsize::new(0));
    let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
        Arc::clone(&retained_home),
        config,
        Box::new(Phase95ProviderProbe {
            issues: Arc::clone(&issues),
            shutdowns: Arc::clone(&shutdowns),
        }),
    )
    .unwrap();
    let startup = Arc::clone(replacement.startup_gate());
    let arm = match adversary {
        Phase95AdversarialTopology::ExtraConnectionOwner => quarantine
            .arm_extra_connection_owner_for_adoption_test(
                secondary_quarantine.expect("the extra owner comes from a sealed quarantine"),
            ),
        Phase95AdversarialTopology::RetiredConnectionOwner => {
            assert!(secondary_quarantine.is_none());
            quarantine.arm_retired_connection_owner_for_adoption_test()
        }
        Phase95AdversarialTopology::ForeignFailureCutOwner => quarantine
            .arm_foreign_failure_cut_owner_for_adoption_test(
                secondary_quarantine.expect("the foreign owner comes from a sealed quarantine"),
            ),
    };

    let error = quarantine
        .adopt_unpublished_service(replacement)
        .unwrap_err();
    assert!(arm.was_consumed());
    assert_eq!(error.reason(), expected_reason);
    assert_eq!(error.metadata().connection_count(), 1);
    assert_eq!(error.metadata().candidate_count(), 0);
    assert!(error.inventory_reescrow_is_disarmed_for_test());
    let diagnostics = error.adversarial_topology_diagnostics_for_test();
    assert_eq!(diagnostics.reached_owner_count(), expected_owner_count);
    assert_eq!(
        diagnostics.retained_topology_owner_count(),
        expected_owner_count
    );
    assert_eq!(
        diagnostics.reached_connection_count(),
        expected_owner_count
    );
    assert_eq!(diagnostics.inert_attachment_count(), expected_owner_count);
    assert_eq!(diagnostics.connection_state_count(), 0);
    assert!(diagnostics.secondary_inventory_reescrow_disarmed());
    assert!(diagnostics.all_reached_connections_inert());
    assert!(diagnostics.startup_fence_never_opened());
    assert!(diagnostics.startup_fence_cancelled());
    assert!(!diagnostics.publication_committed());
    assert!(connection.forwarding_epoch_is_inert_and_detached_for_test());
    if let Some(secondary) = secondary_settlement.as_ref() {
        assert!(
            secondary
                .connection
                .forwarding_epoch_is_inert_and_detached_for_test()
        );
    }
    assert_eq!(issues.load(Ordering::SeqCst), 0);
    assert_eq!(shutdowns.load(Ordering::SeqCst), 0);
    phase95_assert_startup_cancelled(startup);

    error.dispose().unwrap();
    assert_eq!(issues.load(Ordering::SeqCst), 0);
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    server.join();

    drop(registry);
    drop(connection);
    drop(retained_home);
    drop(_directory);
    if let Some(secondary) = secondary_settlement {
        secondary.server.join();
        drop(secondary.registry);
        drop(secondary.connection);
        drop(secondary.retained_home);
        drop(secondary._directory);
    }
}

#[test]
fn phase95_extra_connection_owner_rejects_and_retains_the_complete_set() {
    phase95_adversarial_topology_fixture(
        Phase95AdversarialTopology::ExtraConnectionOwner,
        195,
        Some(196),
        PersistentFailureServiceAdoptionReason::ConnectionSetMismatch,
        2,
    );
}

#[test]
fn phase95_retired_connection_owner_rejects_before_parking() {
    phase95_adversarial_topology_fixture(
        Phase95AdversarialTopology::RetiredConnectionOwner,
        197,
        None,
        PersistentFailureServiceAdoptionReason::ConnectionUnavailable,
        1,
    );
}

#[test]
fn phase95_foreign_failure_cut_owner_rejects_and_retains_both_owners() {
    phase95_adversarial_topology_fixture(
        Phase95AdversarialTopology::ForeignFailureCutOwner,
        198,
        Some(199),
        PersistentFailureServiceAdoptionReason::ConnectionUnavailable,
        2,
    );
}
