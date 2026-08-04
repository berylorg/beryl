use super::*;

#[test]
fn recovered_projection_flight_waits_for_its_lane_release_without_self_spin() {
    let home = seeded_home();
    let fixture = install_recovered_pending(&home, 198);
    let directory = close_seeded(home);
    let attempts = Arc::new(AtomicUsize::new(0));
    let provider_attempts = Arc::clone(&attempts);
    let (service, storage, state) = restart_service(
        directory.path(),
        Box::new(CountingUnavailableProvider {
            attempts: provider_attempts,
        }),
    );
    wait_until("initial recovered provider park", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (attempts.load(Ordering::SeqCst) == 1
            && diagnostics.workers_active() == 0
            && !diagnostics.recovered_pending_retained_source_cursor())
        .then_some(())
    });

    let server =
        NormalTerminalServer::spawn_resume_delayed_rejection(fixture.active.cas_thread.as_str());
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = service
        .admit(
            &connector,
            fixture.runtime_id,
            CasProcessGeneration::new(63_198).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let binding = storage
        .current_binding(&service, fixture.ids.thread, point_limit())
        .unwrap()
        .unwrap();
    let projection = CasProjectionCoordinator::for_healthy_home(&service)
        .unwrap()
        .obtain_projection(
            &service,
            storage,
            &mut session,
            &CasProjectionRequest::new(
                fixture.ids.thread,
                binding.binding().selected_path(),
                execution_binding(fixture.runtime_id),
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                time(63_198),
                TIMEOUT,
            ),
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    server.wait_for_projection();

    let cancellation = ProjectionCancellationToken::new();
    let execution_request = OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT);
    thread::scope(|scope| {
        let execution = scope.spawn(|| {
            let coordinator = CasProjectionCoordinator::for_healthy_home(&service).unwrap();
            let mut lifecycle = NoopLifecycle;
            let mut branch = NoopBranch;
            coordinator.execute_ordinary_turn(
                &service,
                storage,
                state.assets(),
                projection,
                &cancellation,
                &execution_request,
                OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
            )
        });
        server.wait_for_turn_start();
        let active = storage
            .current_binding(&service, fixture.ids.thread, point_limit())
            .unwrap()
            .unwrap();
        let BindingState::Active(active) = active.binding().state() else {
            panic!("external ordinary execution must hold active durable authority");
        };
        cancel_activation(
            &service,
            storage,
            fixture.ids.thread,
            fixture.promoted.turn,
            active.snapshot_id(),
        );

        let before = service.accepted_input_scheduler_diagnostics();
        let attempts_before = attempts.load(Ordering::SeqCst);
        service.notify_scheduled_ordinary_execution_ready();
        wait_until("recovered same-thread projection-flight wait", || {
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            (diagnostics.recovered_pending_flight_waits() > before.recovered_pending_flight_waits())
                .then_some(diagnostics)
        });
        let waiting = service.accepted_input_scheduler_diagnostics();
        assert_eq!(waiting.next_flight_waits(), before.next_flight_waits());
        assert_eq!(attempts.load(Ordering::SeqCst), attempts_before);
        thread::sleep(Duration::from_millis(100));
        let still_waiting = service.accepted_input_scheduler_diagnostics();
        assert_eq!(
            still_waiting.recovered_pending_flight_waits(),
            waiting.recovered_pending_flight_waits()
        );
        assert_eq!(
            still_waiting.next_flight_waits(),
            waiting.next_flight_waits()
        );
        assert_eq!(attempts.load(Ordering::SeqCst), attempts_before);

        server.release_turn_start_rejection();
        let execution = execution.join().unwrap();
        assert!(matches!(
            execution,
            Ok(OrdinaryTurnExecutionOutcome::NotStarted { .. })
                | Err(OrdinaryTurnExecutionFailure::AfterActivation { .. })
        ));
        wait_until("recovered flight-release retry", || {
            let diagnostics = service.accepted_input_scheduler_diagnostics();
            (attempts.load(Ordering::SeqCst) == attempts_before + 1
                && diagnostics.workers_active() == 0
                && !diagnostics.recovered_pending_retained_source_cursor())
            .then_some(())
        });
    });
    thread::sleep(Duration::from_millis(100));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_recovered_pending(&service, storage, &fixture);
    session.invalidate_connection();
    drop(session);
    service.close().unwrap();
    server.join();
}
