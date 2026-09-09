use super::*;
use beryl_app::cas_projection::{ProcessWorkError, ProcessWorkFacts, ScheduledSessionWorkState};

#[test]
fn targeted_work_tracks_admitted_sessions_without_attention_or_unadmitted_backlog() {
    let (mut fixture, sessions) = fixture(231);
    let queued = seed_runtime_next_input_without_wake(&mut fixture, 231);
    let unadmitted = fixture.create_ordinary_pending(233, "unadmitted pending");
    fixture.thread = fixture.create_ordinary(235);
    let attention = ProcessLifecycleAttentionPool::new();
    let server = NormalTerminalServer::spawn_admission_only();
    let registration = install_session(
        &fixture,
        &sessions,
        fixture.thread,
        server.endpoint(),
        74_101,
    );
    server.wait_for_admission();
    let cancellation = ProjectionCancellationToken::new();
    let idle = fixture
        .store
        .required_session_work_for_test(&sessions, &cancellation)
        .unwrap();
    assert_eq!(idle.len(), 1);
    assert_eq!(idle[0].0, fixture.thread);
    assert_eq!(idle[0].1.state(), ScheduledSessionWorkState::Available);
    assert_eq!(idle[0].1.execution_binding(), &syndic::execution_binding());
    assert_eq!(idle[0].2, ProcessWorkFacts::default());
    assert!(idle.len() <= sessions.diagnostics().capacity);
    let attempt = attention
        .track_accepted_yield(
            fixture.home().home_id(),
            fixture.thread,
            SyndicTurnId::from_bytes([231; 16]),
            LifecycleYieldOutcome::PhaseNeedsReview,
        )
        .unwrap();
    assert!(matches!(
        attention.report_terminal(&attempt),
        LifecycleAttentionAdmission::Admitted(_)
    ));
    assert_eq!(page(&fixture, &sessions, &attention).total_threads(), 3);
    assert_eq!(
        fixture
            .store
            .required_session_work_for_test(&sessions, &cancellation)
            .unwrap()[0]
            .2,
        ProcessWorkFacts::default()
    );

    let lease = match fixture
        .store
        .checkout_scheduled_session_for_test(fixture.thread, syndic::execution_binding())
        .unwrap()
    {
        ScheduledOrdinaryAdmissionResult::Issued(lease) => lease,
        _ => panic!("admitted session must check out"),
    };
    let running = fixture
        .store
        .required_session_work_for_test(&sessions, &cancellation)
        .unwrap();
    assert_eq!(
        running[0].1.registration_serial(),
        idle[0].1.registration_serial()
    );
    assert_eq!(running[0].1.state(), ScheduledSessionWorkState::CheckedOut);
    assert!(running[0].2.executing);
    let admitted = fixture.thread;
    let before = fixture.home().home_revision().unwrap();
    let rows = fixture
        .store
        .required_session_work_for_test(&sessions, &cancellation)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, admitted);
    assert_ne!(rows[0].0, unadmitted);
    assert!(!rows[0].2.queued);
    assert!(rows[0].2.executing);
    assert_eq!(fixture.home().home_revision().unwrap(), before);
    assert!(matches!(
        accepted_route_state(&fixture.home(), &fixture.storage, &queued),
        AcceptedRouteEffectiveState::NextTurn(_)
    ));
    assert!(sessions.retire(registration));
    drop(lease);
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    drop(directory);
}

#[test]
fn admitted_pending_work_is_observed_without_publishing_presentation_metadata() {
    let (mut fixture, sessions) = fixture(237);
    let server = NormalTerminalServer::spawn_admission_only();
    let registration = install_session(
        &fixture,
        &sessions,
        fixture.thread,
        server.endpoint(),
        74_102,
    );
    server.wait_for_admission();
    let lease = match fixture
        .store
        .checkout_scheduled_session_for_test(fixture.thread, syndic::execution_binding())
        .unwrap()
    {
        ScheduledOrdinaryAdmissionResult::Issued(lease) => lease,
        _ => panic!("admitted session must check out"),
    };
    let _ = fixture.submit_text("pending while session checkout is held");
    let before = fixture.home().home_revision().unwrap();
    let rows = fixture
        .store
        .required_session_work_for_test(&sessions, &ProjectionCancellationToken::new())
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].2.pending);
    assert!(rows[0].2.executing);
    assert_eq!(fixture.home().home_revision().unwrap(), before);
    assert!(sessions.retire(registration));
    drop(lease);
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    drop(directory);
}

#[test]
fn an_admitted_session_without_a_durable_gate_cannot_be_classified_idle() {
    let (fixture, sessions) = fixture(239);
    let server = NormalTerminalServer::spawn_admission_only();
    let missing = SyndicThreadId::from_bytes([241; 16]);
    let registration = install_session(&fixture, &sessions, missing, server.endpoint(), 74_103);
    server.wait_for_admission();
    assert!(matches!(
        fixture
            .store
            .required_session_work_for_test(&sessions, &ProjectionCancellationToken::new()),
        Err(ProcessWorkError::Durable(
            syndic_storage::SyndicReadError::Invariant(_)
        ))
    ));
    assert_eq!(sessions.diagnostics().available, 1);
    assert!(sessions.retire(registration));
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    server.join();
    drop(directory);
}
