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
        .admit(
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
    assert_eq!(gate.state(), &InputGateState::Idle);
    assert_eq!(turn.lifecycle(), TurnLifecycle::Incomplete);
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
        state,
    } = home;

    let slot = SessionSlot::default();
    let pause = ProviderPause::default();
    let provider_pause = pause.clone();
    let service = ProjectionConnectionService::new(
        store,
        storage,
        ProjectionServiceConfig::try_new(128, 8).unwrap(),
        Box::new(PausingCheckoutProvider {
            checkout: ready_provider(slot.clone(), state.assets()),
            session: slot.clone(),
            pause: provider_pause,
        }),
    )
    .unwrap();
    wait_until("initial recovered generation-test park", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.recovered_pending_execution_unavailable() == 1
            && diagnostics.workers_active() == 0)
            .then_some(())
    });

    let server = NormalTerminalServer::spawn_admission_only();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit(
            &connector,
            fixture.runtime_id,
            CasProcessGeneration::new(63_200).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    server.wait_for_admission();
    let initial_generation = service.home_generation();
    service.notify_scheduled_ordinary_execution_ready();
    pause.wait_until_paused();

    recover_home_generation_before_promotion(&service, storage, &faults, &fixture.ids);
    assert!(service.health().generation().unwrap() > initial_generation);
    let recovered_storage = SyndicStorage::reacquire(&service).unwrap();
    pause.release();
    wait_until("obsolete recovered scheduler failure", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.fatal() && diagnostics.workers_active() == 0).then_some(())
    });
    wait_until("generation-invalidated session return", || {
        slot.is_ready().then_some(())
    });
    assert_recovered_pending(&service, recovered_storage, &fixture);

    assert!(matches!(
        service.close(),
        Err(ProjectionConnectionServiceCloseError::SchedulerShutdown)
    ));
    server.join();
    assert!(!slot.is_ready());
}
