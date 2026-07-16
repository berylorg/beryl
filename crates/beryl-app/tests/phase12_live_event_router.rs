#[path = "phase10_projection/backend.rs"]
mod backend;
#[path = "phase10_projection/syndic.rs"]
mod syndic;

use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

use beryl_app::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest,
    LiveEventConnectionState, LiveEventPoll, LiveEventTarget, LiveEventTargetCloseReason,
    LoadedProjectionReleaseError, LoadedProjectionReleaseOutcome, ProjectionCancellationToken,
    ProjectionSessionAdmissionError,
};
use beryl_backend::{
    ApprovalResponseDisposition, ThreadStartOptions, ThreadUnsubscribeStatus, TurnStreamEvent,
};
use beryl_model::{CasProcessGeneration, CasTurnId};
use serde_json::json;
use syndic_storage::SyndicTimestamp;

use backend::{FakeAppServer, ProjectionStep, TIMEOUT, UnsubscribeReply};
use syndic::{Fixture, execution_binding};

fn process(value: u64) -> CasProcessGeneration {
    CasProcessGeneration::new(value).unwrap()
}

fn request(fixture: &Fixture, thread: beryl_model::SyndicThreadId) -> CasProjectionRequest {
    CasProjectionRequest::new(
        thread,
        fixture.selected_path(thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(1_000_000),
        SyndicTimestamp::from_unix_millis(10_000),
        TIMEOUT,
    )
}

fn obtain(
    fixture: &Fixture,
    coordinator: &CasProjectionCoordinator,
    session: &mut AdmittedProjectionSession,
    thread: beryl_model::SyndicThreadId,
) -> beryl_app::cas_projection::LoadedCasProjection {
    coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            session,
            &request(fixture, thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap()
}

fn event(target: &LiveEventTarget) -> beryl_app::cas_projection::RoutedLiveEvent {
    match target.poll(TIMEOUT) {
        LiveEventPoll::Event(event) => event,
        other => panic!("expected routed event, got {other:?}"),
    }
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for router state"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn buffered_notification_is_routed_before_request_response_returns() {
    let mut fixture = Fixture::new(30);
    fixture.submit_text("first pending");
    let second_thread = fixture.create_ordinary_pending(31, "second pending");
    let first_target = "phase12-buffered-first";
    let second_target = "phase12-buffered-second";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh {
            target: first_target,
        },
        ProjectionStep::Fresh {
            target: second_target,
        },
        ProjectionStep::UnsubscribeAfterNotification {
            target: second_target,
            method: "turn/started",
            params: json!({
                "threadId": first_target,
                "turn": {
                    "id": "phase12-buffered-turn",
                    "status": "inProgress",
                    "items": []
                }
            }),
            reply: UnsubscribeReply::Status("unsubscribed"),
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(30));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let first = obtain(&fixture, &coordinator, &mut session, fixture.thread)
        .into_pending_live_event_target()
        .unwrap();
    let second = obtain(&fixture, &coordinator, &mut session, second_thread);

    assert_eq!(
        second.release().unwrap(),
        LoadedProjectionReleaseOutcome::Unsubscribe(ThreadUnsubscribeStatus::Unsubscribed)
    );
    let routed = event(&first);
    assert_eq!(routed.thread_id().as_str(), first_target);
    assert_eq!(routed.turn_id().unwrap().as_str(), "phase12-buffered-turn");
    first
        .confirm_turn(CasTurnId::new("phase12-buffered-turn").unwrap())
        .unwrap();

    drop(first);
    server.join();
}

#[test]
fn interleaved_approval_is_denied_and_routed_before_request_success() {
    let mut fixture = Fixture::new(38);
    fixture.submit_text("approval owner");
    let second_thread = fixture.create_ordinary_pending(50, "unsubscribe owner");
    let approval_thread = "phase12-approval-thread";
    let approval_turn = "phase12-approval-turn";
    let second_target = "phase12-approval-second";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh {
            target: approval_thread,
        },
        ProjectionStep::Fresh {
            target: second_target,
        },
        ProjectionStep::UnsubscribeAfterApprovalRequest {
            target: second_target,
            approval_thread,
            approval_turn,
            approval_item: "phase12-approval-item",
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(38));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let first = obtain(&fixture, &coordinator, &mut session, fixture.thread)
        .into_active_live_event_target(CasTurnId::new(approval_turn).unwrap())
        .unwrap();
    let second = obtain(&fixture, &coordinator, &mut session, second_thread);

    assert_eq!(
        second.release().unwrap(),
        LoadedProjectionReleaseOutcome::Unsubscribe(ThreadUnsubscribeStatus::Unsubscribed)
    );
    match event(&first).event() {
        TurnStreamEvent::ApprovalRequested(request) => {
            assert_eq!(request.thread_id(), Some(approval_thread));
            assert_eq!(request.turn_id(), Some(approval_turn));
            assert_eq!(request.item_id(), Some("phase12-approval-item"));
            assert_eq!(
                request.response_disposition(),
                ApprovalResponseDisposition::AutoDenied
            );
        }
        other => panic!("expected routed approval request, got {other:?}"),
    }

    drop(first);
    server.join();
}

#[test]
fn target_local_buffered_failure_suppresses_success_without_retiring_the_connection() {
    let mut fixture = Fixture::new(52);
    fixture.submit_text("first pending");
    let second_thread = fixture.create_ordinary_pending(53, "second pending");
    let first_target = "phase12-target-failure-first";
    let first_turn = "phase12-target-failure-expected";
    let conflicting_turn = "phase12-target-failure-conflict";
    let second_target = "phase12-target-failure-second";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh {
            target: first_target,
        },
        ProjectionStep::Fresh {
            target: second_target,
        },
        ProjectionStep::UnsubscribeAfterNotification {
            target: second_target,
            method: "turn/started",
            params: json!({
                "threadId": first_target,
                "turn": {
                    "id": conflicting_turn,
                    "status": "inProgress",
                    "items": []
                }
            }),
            reply: UnsubscribeReply::Status("unsubscribed"),
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(52));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let first = obtain(&fixture, &coordinator, &mut session, fixture.thread)
        .into_active_live_event_target(CasTurnId::new(first_turn).unwrap())
        .unwrap();
    let second = obtain(&fixture, &coordinator, &mut session, second_thread);

    assert!(matches!(
        second.release(),
        Err(LoadedProjectionReleaseError::LiveEventRouting {
            ref thread_id,
            reason: LiveEventTargetCloseReason::ConflictingTurnIdentity,
        }) if thread_id.as_str() == first_target
    ));
    assert!(matches!(
        first.poll(TIMEOUT),
        LiveEventPoll::Closed(LiveEventTargetCloseReason::ConflictingTurnIdentity)
    ));
    assert_eq!(
        session.live_event_snapshot().unwrap().state(),
        LiveEventConnectionState::Active
    );

    drop(first);
    server.join();
}

#[test]
fn buffered_routing_failure_suppresses_the_matching_request_success() {
    let mut fixture = Fixture::new(36);
    fixture.submit_text("first pending");
    let second_thread = fixture.create_ordinary_pending(37, "second pending");
    let first_target = "phase12-failure-first";
    let second_target = "phase12-failure-second";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh {
            target: first_target,
        },
        ProjectionStep::Fresh {
            target: second_target,
        },
        ProjectionStep::UnsubscribeAfterNotification {
            target: second_target,
            method: "thread/name/updated",
            params: json!({ "threadId": "", "threadName": "invalid" }),
            reply: UnsubscribeReply::Status("unsubscribed"),
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(36));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let first = obtain(&fixture, &coordinator, &mut session, fixture.thread)
        .into_pending_live_event_target()
        .unwrap();
    let second = obtain(&fixture, &coordinator, &mut session, second_thread);

    assert_eq!(
        second.release().unwrap(),
        LoadedProjectionReleaseOutcome::ConnectionRetired
    );
    assert!(matches!(
        first.poll(TIMEOUT),
        LiveEventPoll::Closed(LiveEventTargetCloseReason::InvalidEventIdentity)
    ));
    assert_eq!(
        session.live_event_snapshot().unwrap().state(),
        LiveEventConnectionState::Retired(LiveEventTargetCloseReason::InvalidEventIdentity)
    );

    drop(first);
    server.join();
}

#[test]
fn quiet_polling_survives_late_events_and_separates_account_facts() {
    let mut fixture = Fixture::new(32);
    fixture.submit_text("late pending");
    let cas_thread = "phase12-late-thread";
    let server = FakeAppServer::spawn(vec![ProjectionStep::Fresh { target: cas_thread }]);
    let mut session = server.admit(execution_binding().runtime_id(), process(32));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let target = obtain(&fixture, &coordinator, &mut session, fixture.thread)
        .into_pending_live_event_target()
        .unwrap();

    wait_until(|| {
        session
            .live_event_snapshot()
            .is_ok_and(|snapshot| snapshot.quiet_poll_count() > 0)
    });
    server.send_notification(
        "turn/started",
        json!({
            "threadId": cas_thread,
            "turn": {
                "id": "phase12-late-turn",
                "status": "inProgress",
                "items": []
            }
        }),
    );
    assert_eq!(
        event(&target).turn_id().unwrap().as_str(),
        "phase12-late-turn"
    );

    server.send_notification(
        "account/rateLimits/updated",
        json!({
            "rateLimits": {
                "limitId": "codex",
                "primary": { "usedPercent": 17 }
            }
        }),
    );
    wait_until(|| {
        session
            .live_event_process_snapshot()
            .is_ok_and(|snapshot| snapshot.account_event_count() == 1)
    });
    let router_snapshot = session.live_event_snapshot().unwrap();
    let process_snapshot = session.live_event_process_snapshot().unwrap();
    assert_eq!(router_snapshot.target_count(), 1);
    assert_eq!(
        process_snapshot
            .account_rate_limits()
            .and_then(|limits| limits.limit_id.as_deref()),
        Some("codex")
    );
    assert_eq!(
        process_snapshot.account_source_connection_generation(),
        Some(router_snapshot.connection_generation())
    );
    assert!(matches!(
        target.poll(Duration::from_millis(25)),
        LiveEventPoll::Quiet
    ));

    server.send_notification(
        "thread/name/updated",
        json!({ "threadId": "", "threadName": "invalid" }),
    );
    assert!(matches!(
        target.poll(TIMEOUT),
        LiveEventPoll::Closed(LiveEventTargetCloseReason::InvalidEventIdentity)
    ));
    assert_eq!(
        session.live_event_snapshot().unwrap().state(),
        LiveEventConnectionState::Retired(LiveEventTargetCloseReason::InvalidEventIdentity)
    );

    drop(target);
    server.join();
}

#[test]
fn another_connection_on_the_same_runtime_cannot_receive_target_events() {
    let mut first_fixture = Fixture::new(33);
    first_fixture.submit_text("first connection pending");
    let mut second_fixture = Fixture::new(34);
    second_fixture.submit_text("second connection pending");
    let first_thread = "phase12-connection-one";
    let second_thread = "phase12-connection-two";
    let first_server = FakeAppServer::spawn(vec![ProjectionStep::Fresh {
        target: first_thread,
    }]);
    let second_server = FakeAppServer::spawn(vec![ProjectionStep::Fresh {
        target: second_thread,
    }]);
    let runtime = execution_binding().runtime_id();
    let mut first_session = first_server.admit(runtime, process(33));
    let mut second_session = second_server.admit(runtime, process(33));
    let first_coordinator =
        CasProjectionCoordinator::for_healthy_home(&first_fixture.store).unwrap();
    let second_coordinator =
        CasProjectionCoordinator::for_healthy_home(&second_fixture.store).unwrap();
    let first_target = obtain(
        &first_fixture,
        &first_coordinator,
        &mut first_session,
        first_fixture.thread,
    )
    .into_active_live_event_target(CasTurnId::new("phase12-turn-one").unwrap())
    .unwrap();
    let second_target = obtain(
        &second_fixture,
        &second_coordinator,
        &mut second_session,
        second_fixture.thread,
    )
    .into_active_live_event_target(CasTurnId::new("phase12-turn-two").unwrap())
    .unwrap();

    let second_connection_generation = second_session
        .live_event_snapshot()
        .unwrap()
        .connection_generation();
    second_server.send_notification(
        "account/rateLimits/updated",
        json!({
            "rateLimits": {
                "limitId": "shared-process",
                "primary": { "usedPercent": 23 }
            }
        }),
    );
    wait_until(|| {
        first_session
            .live_event_process_snapshot()
            .is_ok_and(|snapshot| snapshot.account_event_count() == 1)
    });
    let process_snapshot = first_session.live_event_process_snapshot().unwrap();
    assert_eq!(process_snapshot.active_connection_count(), 2);
    assert_eq!(
        process_snapshot.account_source_connection_generation(),
        Some(second_connection_generation)
    );
    assert_eq!(
        process_snapshot
            .account_rate_limits()
            .and_then(|limits| limits.limit_id.as_deref()),
        Some("shared-process")
    );

    second_server.send_notification(
        "thread/status/changed",
        json!({
            "threadId": first_thread,
            "status": { "type": "active", "activeFlags": [] }
        }),
    );
    wait_until(|| {
        second_session
            .live_event_snapshot()
            .is_ok_and(|snapshot| snapshot.unmatched_event_count() == 1)
    });
    assert!(matches!(
        first_target.poll(Duration::from_millis(25)),
        LiveEventPoll::Quiet
    ));
    assert!(matches!(
        second_target.poll(Duration::from_millis(25)),
        LiveEventPoll::Quiet
    ));

    first_server.send_notification(
        "thread/status/changed",
        json!({
            "threadId": first_thread,
            "status": { "type": "active", "activeFlags": [] }
        }),
    );
    assert!(matches!(
        event(&first_target).event(),
        TurnStreamEvent::ThreadStatusChanged { .. }
    ));

    drop(first_target);
    drop(second_target);
    first_server.join();
    second_server.join();
}

#[test]
fn projection_admission_rejects_request_only_sessions() {
    let server = FakeAppServer::spawn_for_admission_rejection();
    let backend = server.connect_request_only();
    let runtime_id = execution_binding().runtime_id();
    let process_generation = process(35);
    let error = AdmittedProjectionSession::admit(
        backend,
        runtime_id,
        process_generation,
        Path::new(backend::EXECUTION_ROOT),
        TIMEOUT,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        ProjectionSessionAdmissionError::FullTurnStreamRequired {
            runtime_id: observed_runtime,
            process_generation: observed_process,
        } if observed_runtime == runtime_id && observed_process == process_generation
    ));
    server.join();
}
