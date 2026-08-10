#[test]
fn dropping_unused_session_reclaims_mounted_connection_permits() {
    let (_directory, _faults, _state, _shutdowns, service) = service();
    let first_server = admission_server::NormalTerminalServer::spawn_admission_only();
    let first_connector = ManagedBackendClientConnector::for_lifecycle_test(
        first_server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let first = service
        .admit_lifecycle_test_candidate(
            &first_connector,
            RuntimeId::from_bytes([77; 16]),
            CasProcessGeneration::new(77_001).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    first_server.wait_for_admission();
    assert_eq!(service.worker_pool_diagnostics().available(), 2);

    drop(first);
    first_server.join();
    wait_until("unused mounted connection permits to return", || {
        service.worker_pool_diagnostics().available() == 4
    });

    let replacement_server = admission_server::NormalTerminalServer::spawn_admission_only();
    let replacement_connector = ManagedBackendClientConnector::for_lifecycle_test(
        replacement_server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let replacement = service
        .admit_lifecycle_test_candidate(
            &replacement_connector,
            RuntimeId::from_bytes([78; 16]),
            CasProcessGeneration::new(77_002).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    replacement_server.wait_for_admission();
    drop(replacement);
    replacement_server.join();
    wait_until("replacement mounted connection permits to return", || {
        service.worker_pool_diagnostics().available() == 4
    });

    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
}

#[test]
fn sequential_connection_churn_reaps_finished_registry_entries() {
    let (_directory, _faults, _state, _shutdowns, service) = service();
    for index in 0_u8..12 {
        let server = admission_server::NormalTerminalServer::spawn_admission_only();
        let connector = ManagedBackendClientConnector::for_lifecycle_test(
            server.endpoint(),
            admission_server::AUTHORIZATION,
        );
        let session = service
            .admit_lifecycle_test_candidate(
                &connector,
                RuntimeId::from_bytes([index.saturating_add(100); 16]),
                CasProcessGeneration::new(78_000 + u64::from(index)).unwrap(),
                Path::new(r"C:\work\beryl"),
                Duration::from_secs(10),
            )
            .unwrap();
        server.wait_for_admission();
        assert!(service.registered_connection_count_for_test() <= 1);

        drop(session);
        server.join();
        wait_until("churned connection permits to return", || {
            service.worker_pool_diagnostics().available() == 4
        });
    }
    assert!(service.registered_connection_count_for_test() <= 1);
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
}

#[test]
fn concurrent_connection_shutdown_replays_one_clean_settlement() {
    let (_directory, _faults, _state, shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([132; 16]),
            CasProcessGeneration::new(78_132).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    let barrier = Arc::new(std::sync::Barrier::new(3));
    let first_connection = Arc::clone(session.connection());
    let first_barrier = Arc::clone(&barrier);
    let first = std::thread::spawn(move || {
        first_barrier.wait();
        first_connection.shutdown()
    });
    let second_connection = Arc::clone(session.connection());
    let second_barrier = Arc::clone(&barrier);
    let second = std::thread::spawn(move || {
        second_barrier.wait();
        second_connection.shutdown()
    });
    barrier.wait();

    assert!(first.join().unwrap().is_ok());
    assert!(second.join().unwrap().is_ok());
    assert!(session.connection().shutdown().is_ok());
    drop(session);
    server.join();
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn implicit_ordinary_service_drop_does_not_wait_for_cleanup_authority() {
    let (_directory, _faults, _state, shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([113; 16]),
            CasProcessGeneration::new(78_113).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();
    let cleanup = session
        .connection()
        .acquire_cleanup_owner()
        .unwrap()
        .unwrap();
    let (finished, completion) = std::sync::mpsc::sync_channel(1);
    let dropper = std::thread::spawn(move || {
        drop(service);
        finished.send(()).unwrap();
    });

    completion
        .recv_timeout(Duration::from_secs(1))
        .expect("implicit service drop must not wait for cleanup authority");
    assert_eq!(shutdowns.load(Ordering::SeqCst), 0);

    drop(cleanup);
    drop(session);
    server.join();
    dropper.join().unwrap();
}

#[test]
fn implicit_failure_service_drop_escrows_without_waiting_for_gate_drain() {
    let (directory, faults, state, shutdowns, service) = service();
    let held_command = service.command_authorizer.authorize().unwrap();
    fail_home_through_live_command(&service, state, &faults);
    let (finished, completion) = std::sync::mpsc::sync_channel(1);
    let dropper = std::thread::spawn(move || {
        drop(service);
        finished.send(()).unwrap();
    });

    completion
        .recv_timeout(Duration::from_secs(1))
        .expect("failure-winning implicit drop must not wait for gate drain");
    assert_eq!(shutdowns.load(Ordering::SeqCst), 0);
    assert!(matches!(
        HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        )),
        Err(HomeOpenError::Busy { .. })
    ));

    drop(held_command);
    dropper.join().unwrap();
}

#[test]
fn ordinary_close_detaches_home_ownership_from_a_retained_session_shell() {
    let (directory, _faults, _state, _shutdowns, service) = service();
    let server = admission_server::NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        admission_server::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([80; 16]),
            CasProcessGeneration::new(77_004).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    server.wait_for_admission();

    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();

    let reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .expect("a stale admitted-session shell cannot retain the explicitly closed home");
    reopened.close().unwrap();
    drop(session);
}
