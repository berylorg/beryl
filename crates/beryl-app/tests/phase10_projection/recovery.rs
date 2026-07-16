use super::*;

#[test]
fn rejected_injection_is_abandoned_and_retry_uses_another_fresh_thread() {
    let mut fixture = Fixture::new(4);
    let first = fixture.submit_text("history user");
    fixture.complete_with_assistant(first, "history assistant");
    fixture.submit_text("pending user");
    fixture.retire_current_binding(fixture.thread);
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Recover {
            target: "phase10-rejected-target",
            injection: InjectionReply::Reject,
        },
        ProjectionStep::Recover {
            target: "phase10-retry-target",
            injection: InjectionReply::Success,
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(4));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let request = request(&fixture, fixture.thread);

    let error = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ProjectionExecutionError::InjectionRejected { ref thread_id, .. }
            if thread_id.as_str() == "phase10-rejected-target"
    ));
    let stale = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Stale(stale) = stale.binding().state() else {
        panic!("rejected injection target must become stale provenance")
    };
    assert_eq!(stale.cas_thread_id().as_str(), "phase10-rejected-target");
    assert_eq!(stale.observed_prefix(), None);
    assert!(stale.loaded_generation().is_some());

    let projection = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(projection.cas_thread_id().as_str(), "phase10-retry-target");
    assert!(matches!(
        projection.lineage_proof(),
        CasLineageProof::RecoveredInjection(_)
    ));
    server.join();
}

#[test]
fn lost_recovered_generation_forces_another_fresh_injection_not_resume() {
    let mut fixture = Fixture::new(5);
    let first = fixture.submit_text("history user");
    fixture.complete_with_assistant(first, "history assistant");
    fixture.submit_text("pending user");
    fixture.retire_current_binding(fixture.thread);
    let first_server = FakeAppServer::spawn(vec![ProjectionStep::Recover {
        target: "phase10-recovered-process-one",
        injection: InjectionReply::Success,
    }]);
    let mut first_session = first_server.admit(execution_binding().runtime_id(), process(5));
    let first_coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let request = request(&fixture, fixture.thread);
    let first_projection = first_coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut first_session,
            &request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(
        first_projection.cas_thread_id().as_str(),
        "phase10-recovered-process-one"
    );
    first_server.join();
    drop(first_session);

    let second_server = FakeAppServer::spawn(vec![ProjectionStep::Recover {
        target: "phase10-recovered-process-two",
        injection: InjectionReply::Success,
    }]);
    let mut second_session = second_server.admit(execution_binding().runtime_id(), process(6));
    let second_coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let second_projection = second_coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut second_session,
            &request,
            &ProjectionCancellationToken::new(),
        )
        .unwrap();
    assert_eq!(
        second_projection.cas_thread_id().as_str(),
        "phase10-recovered-process-two"
    );
    assert_ne!(
        second_projection.loaded_session_generation(),
        first_projection.loaded_session_generation()
    );
    second_server.join();
}

#[test]
fn ambiguous_injection_transport_loss_retires_the_consumed_target() {
    let mut fixture = Fixture::new(8);
    let first = fixture.submit_text("history user");
    fixture.complete_with_assistant(first, "history assistant");
    fixture.submit_text("pending user");
    fixture.retire_current_binding(fixture.thread);
    let server = FakeAppServer::spawn(vec![ProjectionStep::Recover {
        target: "phase10-disconnected-target",
        injection: InjectionReply::Disconnect,
    }]);
    let mut session = server.admit(execution_binding().runtime_id(), process(9));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();

    let error = coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request(&fixture, fixture.thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ProjectionExecutionError::InjectionTransportLost { ref thread_id, .. }
            if thread_id.as_str() == "phase10-disconnected-target"
    ));
    let current = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Stale(stale) = current.binding().state() else {
        panic!("ambiguous injection target must be retired")
    };
    assert_eq!(
        stale.cas_thread_id().as_str(),
        "phase10-disconnected-target"
    );
    assert_eq!(stale.observed_prefix(), None);
    server.join();
}

#[test]
fn cancellation_and_runtime_mismatch_stop_before_projection_dispatch() {
    let mut cancelled_fixture = Fixture::new(6);
    cancelled_fixture.submit_text("cancelled pending");
    let cancelled_server = FakeAppServer::spawn(Vec::new());
    let mut cancelled_session =
        cancelled_server.admit(execution_binding().runtime_id(), process(7));
    let cancelled_coordinator =
        CasProjectionCoordinator::for_healthy_home(&cancelled_fixture.store).unwrap();
    let cancellation = ProjectionCancellationToken::new();
    cancellation.cancel();
    assert!(matches!(
        cancelled_coordinator.obtain_projection(
            &cancelled_fixture.store,
            cancelled_fixture.storage,
            &mut cancelled_session,
            &request(&cancelled_fixture, cancelled_fixture.thread),
            &cancellation,
        ),
        Err(ProjectionExecutionError::Cancelled)
    ));
    cancelled_server.join();

    let mut mismatch_fixture = Fixture::new(7);
    mismatch_fixture.submit_text("mismatch pending");
    let mismatch_server = FakeAppServer::spawn(Vec::new());
    let admitted_runtime = RuntimeId::from_bytes([99; 16]);
    let mut mismatch_session = mismatch_server.admit(admitted_runtime, process(8));
    let mismatch_coordinator =
        CasProjectionCoordinator::for_healthy_home(&mismatch_fixture.store).unwrap();
    assert!(matches!(
        mismatch_coordinator.obtain_projection(
            &mismatch_fixture.store,
            mismatch_fixture.storage,
            &mut mismatch_session,
            &request(&mismatch_fixture, mismatch_fixture.thread),
            &ProjectionCancellationToken::new(),
        ),
        Err(ProjectionExecutionError::RuntimeMismatch { admitted, .. })
            if admitted == admitted_runtime
    ));
    mismatch_server.join();
}

#[test]
fn ephemeral_projection_options_stop_before_projection_dispatch() {
    let mut fixture = Fixture::new(8);
    fixture.submit_text("ephemeral projection must be refused");
    let server = FakeAppServer::spawn(Vec::new());
    let mut session = server.admit(execution_binding().runtime_id(), process(9));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let request = CasProjectionRequest::new(
        fixture.thread,
        fixture.selected_path(fixture.thread),
        execution_binding(),
        ThreadStartOptions::ephemeral(),
        Some(1_000_000),
        SyndicTimestamp::from_unix_millis(10_001),
        TIMEOUT,
    );

    assert!(matches!(
        coordinator.obtain_projection(
            &fixture.store,
            fixture.storage,
            &mut session,
            &request,
            &ProjectionCancellationToken::new(),
        ),
        Err(ProjectionExecutionError::EphemeralProjectionThread)
    ));
    server.join();
}
