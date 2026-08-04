#[test]
fn phase82_scrambled_two_connection_set_adopts_both_stable_cores_together() {
    let (_directory, faults, state, _shutdowns, service) = service_with_worker_capacity(8);
    let first_server =
        admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
    let second_server =
        admission_server::NormalTerminalServer::spawn_admission_only_controlled_close();
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
            RuntimeId::from_bytes([194; 16]),
            CasProcessGeneration::new(82_494).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    first_server.wait_for_admission();
    let second_session = service
        .admit_lifecycle_test_candidate(
            &second_connector,
            RuntimeId::from_bytes([195; 16]),
            CasProcessGeneration::new(82_495).unwrap(),
            Path::new(r"C:\work\beryl"),
            Duration::from_secs(10),
        )
        .unwrap();
    second_server.wait_for_admission();

    let first = Arc::clone(first_session.connection());
    let second = Arc::clone(second_session.connection());
    let first_pointer = Arc::as_ptr(&first) as usize;
    let second_pointer = Arc::as_ptr(&second) as usize;
    let first_stable = first.identity_observation();
    let second_stable = second.identity_observation();
    let first_old_epoch = first.epoch_identity_for_adoption_test().unwrap();
    let second_old_epoch = second.epoch_identity_for_adoption_test().unwrap();
    let first_old_epoch_pointer = first.epoch_pointer_for_adoption_test().unwrap();
    let second_old_epoch_pointer = second.epoch_pointer_for_adoption_test().unwrap();

    fail_home_through_live_command(&service, state, &faults);
    assert_eq!(
        service.persistent_failure_notification().notify(),
        PersistentFailureNotificationStatus::Joined
    );
    drop(first_session);
    drop(second_session);
    wait_until("the two-connection adoption cut to finish", || {
        service.persistent_failure_cut_snapshot().state() == PersistentFailureCutState::Finished
    });
    let handoff = match service.close().unwrap() {
        ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
        ProjectionConnectionServiceCloseOutcome::Closed => {
            panic!("both retained connections must remain recovery-owned")
        }
    };
    let inventory = handoff.into_recovery_inventory().unwrap();
    let registry = inventory.retained_connection_registry();
    let retained_home = Arc::clone(inventory.retained_home());
    let config = inventory.retained_service_config();
    let quarantine = inventory.into_pending_projection_quarantine().unwrap();
    assert_eq!(quarantine.metadata().retained_connection_count(), 2);

    {
        let mut registered = registry.lock().unwrap();
        registered.sort_unstable_by(|left, right| {
            right
                .identity_observation()
                .connection_generation()
                .cmp(&left.identity_observation().connection_generation())
        });
        assert!(
            registered[0].identity_observation().connection_generation()
                > registered[1].identity_observation().connection_generation()
        );
    }

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

    assert_eq!(Arc::as_ptr(&first) as usize, first_pointer);
    assert_eq!(Arc::as_ptr(&second) as usize, second_pointer);
    assert_eq!(first.identity_observation(), first_stable);
    assert_eq!(second.identity_observation(), second_stable);
    let first_new_epoch = first.epoch_identity_for_adoption_test().unwrap();
    let second_new_epoch = second.epoch_identity_for_adoption_test().unwrap();
    assert_ne!(
        first.epoch_pointer_for_adoption_test().unwrap(),
        first_old_epoch_pointer
    );
    assert_ne!(
        second.epoch_pointer_for_adoption_test().unwrap(),
        second_old_epoch_pointer
    );
    assert_eq!(first_new_epoch.home_generation(), recovery.generation());
    assert_eq!(second_new_epoch.home_generation(), recovery.generation());
    assert_eq!(
        first_new_epoch.service_generation(),
        second_new_epoch.service_generation()
    );
    assert!(first_new_epoch.service_generation() > first_old_epoch.service_generation());
    assert!(second_new_epoch.service_generation() > second_old_epoch.service_generation());
    assert_eq!(adopted.adopted_connection_count_for_test(), 2);
    assert!(adopted.startup_fence_is_closed_for_test());

    drop(adopted);
    drop(registry);
    drop(first);
    drop(second);
    first_server.assert_quiet_and_close();
    second_server.assert_quiet_and_close();
    first_server.join();
    second_server.join();
}
