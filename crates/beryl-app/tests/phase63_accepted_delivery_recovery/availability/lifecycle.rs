use super::*;

#[test]
fn shutdown_retires_an_active_recovered_worker_and_converges_possible_dispatch() {
    let home = seeded_home();
    let fixture = install_recovered_pending(&home, 199);
    let directory = close_seeded(home);
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let (service, _storage, _) = restart_service_with(directory.path(), |state| {
        Box::new(ready_provider(provider_slot, state.assets()))
    });
    wait_until("initial recovered session-unavailable park", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.recovered_pending_execution_unavailable() == 1
            && diagnostics.workers_active() == 0)
            .then_some(())
    });

    let server =
        NormalTerminalServer::spawn_resume_delayed_rejection(fixture.active.cas_thread.as_str());
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            fixture.runtime_id,
            CasProcessGeneration::new(63_199).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let retirement = session.connection_retirement_handle_for_test();
    slot.replace(session);
    service.notify_scheduled_ordinary_execution_ready();
    server.wait_for_projection();
    server.wait_for_turn_start();
    wait_until("active recovered worker", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.workers_active() == 1 && diagnostics.workers_started() == 1).then_some(())
    });

    let close_worker = thread::spawn(move || service.close());
    wait_until("recovered shutdown connection retirement", || {
        retirement.is_retired().then_some(())
    });
    server.release_turn_start_rejection();
    drop(retirement);
    close_worker
        .join()
        .expect("recovered shutdown worker did not panic")
        .expect("recovered shutdown must join without scheduler failure");
    server.join();
    assert!(!slot.is_ready());

    let (reopened, storage, _) = reopen_registered(directory.path());
    let gate = storage
        .input_gate(&reopened, fixture.ids.thread, point_limit())
        .unwrap()
        .unwrap();
    let turn = storage
        .turn_state(&reopened, fixture.promoted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.state(),
        &InputGateState::AwaitingSteering(fixture.promoted.turn)
    );
    assert_eq!(turn.lifecycle(), TurnLifecycle::Pending);
    reopened.close().unwrap();
}

#[test]
fn newer_healthy_home_generation_invalidates_the_old_recovered_scheduler() {
    let faults = FaultController::new();
    let directory = tempfile::tempdir().unwrap();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let state = BerylState::register(&mut store).unwrap();
    let home = SeededHome {
        directory,
        store,
        storage,
        state,
    };
    let fixture = install_recovered_pending(&home, 200);
    let SeededHome {
        directory: _directory,
        store,
        storage,
        state: _,
    } = home;

    let slot = SessionSlot::default();
    let pause = ProviderPause::default();
    let supervisor = RunningSessionRecoverySupervisor::start(
        store,
        ProjectionServiceConfig::try_new(128, 8).unwrap(),
        Box::new(PausingProviderFactory {
            slot: slot.clone(),
            pause: pause.clone(),
            epochs: 0,
        }),
    )
    .unwrap();
    wait_until("initial recovered generation-test park", || {
        let service = supervisor.acquire().ok()?;
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.recovered_pending_execution_unavailable() == 1
            && diagnostics.workers_active() == 0)
            .then_some(())
    });
    let service = supervisor.acquire().unwrap();

    let server = NormalTerminalServer::spawn_admission_only();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            fixture.runtime_id,
            CasProcessGeneration::new(63_200).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    server.wait_for_admission();
    let home_id = service.home_id();
    let initial_home_generation = service.home_generation();
    let initial_service_generation = service.service_generation();
    let initial_service_pointer = std::ptr::from_ref::<ProjectionConnectionService>(&*service);
    service.notify_scheduled_ordinary_execution_ready();
    pause.wait_until_paused();

    fail_home_generation_before_promotion(&supervisor, storage, &faults, &fixture.ids);
    pause.release();
    drop(service);

    let recovered = wait_until("supervisor-owned recovered scheduler replacement", || {
        (supervisor.diagnostics().recovery_cycles() == 1)
            .then(|| supervisor.acquire().ok())
            .flatten()
    });
    assert_eq!(recovered.home_id(), home_id);
    assert!(recovered.home_generation() > initial_home_generation);
    assert!(recovered.service_generation() > initial_service_generation);
    assert_ne!(
        std::ptr::from_ref::<ProjectionConnectionService>(&*recovered),
        initial_service_pointer
    );
    assert_eq!(
        supervisor.diagnostics().current_home_generation(),
        Some(recovered.home_generation())
    );
    assert_eq!(
        supervisor.diagnostics().current_service_generation(),
        Some(recovered.service_generation())
    );
    {
        let command_home = recovered.live_home_command().unwrap();
        let recovered_storage = SyndicStorage::reacquire(command_home.home()).unwrap();
        assert_recovered_pending(command_home.home(), recovered_storage, &fixture);
    }
    wait_until("replacement recovered scheduler park", || {
        let diagnostics = recovered.accepted_input_scheduler_diagnostics();
        (diagnostics.recovered_pending_execution_unavailable() >= 1
            && diagnostics.workers_active() == 0)
            .then_some(())
    });
    assert!(!recovered.accepted_input_scheduler_diagnostics().fatal());
    assert!(slot.is_ready());

    drop(recovered);
    supervisor.shutdown().unwrap();
    server.join();
    assert!(!slot.is_ready());
}
