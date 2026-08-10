mod phase93_admission_server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/phase37_normal_terminal/server.rs"
    ));
}

#[test]
fn phase93_supervisor_retains_exact_empty_adoption_without_publication_or_execution() {
    let fixture = Fixture::new();
    let initial = fixture.supervisor.acquire().unwrap();
    let home_id = initial.home_id();
    let failed_home_generation = initial.home_generation();
    let failed_service_generation = initial.service_generation();
    let retained_home = initial.retained_home_for_recovery();
    let retained_home_pointer = Arc::as_ptr(&retained_home);
    drop(initial);

    fail_current_generation(&fixture, 93);
    wait_until_named("Phase 93 adopted owner", || {
        fixture
            .supervisor
            .phase93_adoption_observation_for_test()
            .is_some()
    });
    wait_until(|| fixture.supervisor.diagnostics().terminal_failures() == 1);

    let observation = fixture
        .supervisor
        .phase93_adoption_observation_for_test()
        .expect("the stopped recovery worker retains its adopted-but-unpublished owner");
    assert_eq!(observation.metadata.home_id(), home_id);
    assert_eq!(
        observation.metadata.old_home_generation(),
        failed_home_generation
    );
    assert!(observation.metadata.new_home_generation() > failed_home_generation);
    assert_eq!(
        observation.metadata.old_service_generation(),
        failed_service_generation
    );
    assert!(observation.metadata.new_service_generation() > failed_service_generation);
    assert_eq!(observation.metadata.connection_count(), 0);
    assert_eq!(observation.metadata.candidate_count(), 0);
    assert_eq!(observation.metadata.group_count(), 0);
    assert_eq!(observation.metadata.local_disposition_count(), 0);
    assert_eq!(observation.adopted_connection_count, 0);
    assert!(observation.startup_fence_closed);

    assert!(fixture.supervisor.acquire().is_err());
    assert_eq!(
        fixture.supervisor.diagnostics().current_home_generation(),
        None
    );
    assert_eq!(fixture.epochs.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.provider_issues.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.provider_shutdowns.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.factory_shutdowns.load(Ordering::SeqCst), 0);

    assert!(matches!(
        fixture.supervisor.shutdown(),
        Err(RunningSessionRecoveryShutdownError::TerminalRecovery)
    ));
    assert_eq!(Arc::as_ptr(&retained_home), retained_home_pointer);
    assert_eq!(retained_home.home_id(), home_id);
    assert_eq!(retained_home.health().state(), HomeHealthState::Healthy);
    assert!(retained_home.health().generation().unwrap() > failed_home_generation);
    assert_eq!(fixture.provider_shutdowns.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.factory_shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn phase93_supervisor_adopts_one_stable_core_and_retains_it_until_shutdown() {
    let fixture = Fixture::new();
    let server = phase93_admission_server::NormalTerminalServer::spawn_admission_only();
    let initial = fixture.supervisor.acquire().unwrap();
    let failed_home_generation = initial.home_generation();
    let failed_service_generation = initial.service_generation();
    let connector = beryl_backend::ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        phase93_admission_server::AUTHORIZATION,
    );
    let session = initial
        .admit_lifecycle_test_candidate(
            &connector,
            beryl_model::RuntimeId::from_bytes([193; 16]),
            beryl_model::CasProcessGeneration::new(93_193).unwrap(),
            std::path::Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let connection = Arc::clone(session.connection());
    let stable_identity = connection.identity_observation();
    let old_epoch = connection.epoch_identity_for_adoption_test().unwrap();
    drop(initial);

    fail_current_generation(&fixture, 94);
    drop(session);
    wait_until_named("Phase 93 nonempty adopted owner", || {
        fixture
            .supervisor
            .phase93_adoption_observation_for_test()
            .is_some()
    });
    wait_until(|| fixture.supervisor.diagnostics().terminal_failures() == 1);

    let observation = fixture
        .supervisor
        .phase93_adoption_observation_for_test()
        .expect("the recovery worker retains its nonempty adopted owner");
    assert_eq!(observation.metadata.connection_count(), 1);
    assert_eq!(observation.adopted_connection_count, 1);
    assert!(observation.startup_fence_closed);
    assert_eq!(
        observation.metadata.old_home_generation(),
        failed_home_generation
    );
    assert_eq!(
        observation.metadata.old_service_generation(),
        failed_service_generation
    );
    assert_eq!(connection.identity_observation(), stable_identity);
    let adopted_epoch = connection.epoch_identity_for_adoption_test().unwrap();
    assert!(adopted_epoch.home_generation() > old_epoch.home_generation());
    assert!(adopted_epoch.service_generation() > old_epoch.service_generation());
    assert!(fixture.supervisor.acquire().is_err());
    assert_eq!(fixture.provider_issues.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.provider_shutdowns.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.factory_shutdowns.load(Ordering::SeqCst), 0);

    assert!(matches!(
        fixture.supervisor.shutdown(),
        Err(RunningSessionRecoveryShutdownError::TerminalRecovery)
    ));
    assert!(connection.forwarding_epoch_is_inert_and_detached_for_test());
    assert_eq!(fixture.provider_shutdowns.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.factory_shutdowns.load(Ordering::SeqCst), 1);
    drop(connection);
    server.join();
}
