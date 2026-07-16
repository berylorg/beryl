use std::sync::atomic::Ordering;

use beryl_backend::{
    CompletedTurn, CompletedTurnStatus, RateLimitSnapshot, ThreadStatus, TurnStatus,
    TurnStreamEvent, parse_dynamic_tool_call_request, parse_turn_stream_event,
};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
    CasTurnId, RuntimeId, SyndicThreadId,
};

use super::{
    EventRouter, LIVE_EVENT_TARGET_QUEUE_BYTE_LIMIT, LIVE_EVENT_TARGET_QUEUE_COUNT_LIMIT,
    LiveEventConnectionState, LiveEventTargetCloseReason, LiveEventTargetHandoffError,
    RouteOutcome, TargetHandoffRequirement, TargetRegistration,
};
use crate::cas_projection::connection::registry::LoadedThreadKey;
use serde_json::json;

fn process() -> CasProcessGeneration {
    CasProcessGeneration::new(7).unwrap()
}

fn router(connection_generation: u64) -> EventRouter {
    EventRouter::new(
        RuntimeId::from_bytes([connection_generation as u8; 16]),
        process(),
        connection_generation,
    )
    .unwrap()
}

fn router_for(runtime: u8, connection_generation: u64) -> EventRouter {
    EventRouter::new(
        RuntimeId::from_bytes([runtime; 16]),
        process(),
        connection_generation,
    )
    .unwrap()
}

fn register(
    router: &EventRouter,
    cas_thread_id: &str,
    owner: u8,
    loaded_thread_generation: u64,
    turn_id: Option<&str>,
) -> TargetRegistration {
    let cas_thread_id = CasThreadId::new(cas_thread_id).unwrap();
    router
        .register(
            LoadedThreadKey {
                runtime_id: router.runtime_id,
                process_generation: router.process_generation,
                cas_thread_id,
            },
            SyndicThreadId::from_bytes([owner; 16]),
            CasLoadedSessionGeneration::new(
                router.process_generation,
                CasLoadedThreadGeneration::new(loaded_thread_generation).unwrap(),
            ),
            turn_id.map(|turn_id| CasTurnId::new(turn_id).unwrap()),
        )
        .unwrap()
}

fn thread_status(thread_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::ThreadStatusChanged {
        thread_id: thread_id.to_owned(),
        status: ThreadStatus::Idle,
    }
}

fn turn_started(thread_id: &str, turn_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::TurnStarted {
        thread_id: CasThreadId::new(thread_id).unwrap(),
        turn_id: CasTurnId::new(turn_id).unwrap(),
        status: TurnStatus::InProgress,
    }
}

fn turn_completed(thread_id: &str, turn_id: &str) -> TurnStreamEvent {
    TurnStreamEvent::TurnCompleted {
        thread_id: CasThreadId::new(thread_id).unwrap(),
        turn: CompletedTurn {
            id: CasTurnId::new(turn_id).unwrap(),
            status: CompletedTurnStatus::Completed,
            error: None,
        },
    }
}

fn agent_delta(thread_id: &str, turn_id: &str, item_id: &str) -> TurnStreamEvent {
    parse_turn_stream_event(
        "item/agentMessage/delta",
        Some(json!({
            "threadId": thread_id,
            "turnId": turn_id,
            "itemId": item_id,
            "delta": "ignored"
        })),
    )
    .unwrap()
    .unwrap()
}

fn take_event(registration: &TargetRegistration) -> super::RoutedLiveEvent {
    let queued = registration.receiver.try_recv().unwrap();
    registration.queued_count.fetch_sub(1, Ordering::AcqRel);
    registration
        .queued_bytes
        .fetch_sub(queued.retained_bytes, Ordering::AcqRel);
    queued.event
}

#[test]
fn targets_are_exact_within_and_across_connection_routers() {
    let first_router = router_for(101, 1);
    let second_router = router_for(101, 2);
    let first = register(&first_router, "cas-thread-one", 1, 1, None);
    let second = register(&first_router, "cas-thread-two", 2, 2, None);
    let same_remote_id_elsewhere = register(&second_router, "cas-thread-one", 3, 3, None);

    assert!(matches!(
        first_router.route(turn_started("cas-thread-two", "turn-two"), 20),
        RouteOutcome::Continue
    ));
    assert!(matches!(
        first_router.route(turn_started("cas-thread-one", "turn-one"), 21),
        RouteOutcome::Continue
    ));
    assert!(matches!(
        second_router.route(thread_status("cas-thread-one"), 22),
        RouteOutcome::Continue
    ));

    assert_eq!(take_event(&first).turn_id().unwrap().as_str(), "turn-one");
    assert_eq!(take_event(&second).turn_id().unwrap().as_str(), "turn-two");
    assert_eq!(
        take_event(&same_remote_id_elsewhere).thread_id().as_str(),
        "cas-thread-one"
    );
    assert!(first.receiver.try_recv().is_err());
    assert!(second.receiver.try_recv().is_err());
}

#[test]
fn provisional_target_binds_only_from_turn_start_and_rejects_conflicts() {
    let bound_router = router(3);
    let registration = register(&bound_router, "cas-thread", 4, 4, None);

    assert!(matches!(
        bound_router.route(turn_started("cas-thread", "turn-one"), 30),
        RouteOutcome::Continue
    ));
    assert_eq!(
        take_event(&registration).turn_id().unwrap().as_str(),
        "turn-one"
    );

    let outcome = bound_router.route(agent_delta("cas-thread", "turn-other", "item-other"), 31);
    assert!(matches!(outcome, RouteOutcome::InvalidateTarget(_)));
    assert_eq!(
        *registration.terminal.lock().unwrap(),
        Some(LiveEventTargetCloseReason::ConflictingTurnIdentity)
    );

    let early_router = router(4);
    let early = register(&early_router, "cas-early", 5, 5, None);
    let outcome = early_router.route(
        agent_delta("cas-early", "turn-before-start", "item-before-start"),
        32,
    );
    assert!(matches!(outcome, RouteOutcome::InvalidateTarget(_)));
    assert_eq!(
        *early.terminal.lock().unwrap(),
        Some(LiveEventTargetCloseReason::EventBeforeTurnStart)
    );
}

#[test]
fn target_queue_enforces_record_and_retained_byte_bounds() {
    let count_router = router(5);
    let count_target = register(&count_router, "cas-count", 6, 6, Some("turn-count"));
    for _ in 0..LIVE_EVENT_TARGET_QUEUE_COUNT_LIMIT {
        assert!(matches!(
            count_router.route(thread_status("cas-count"), 1),
            RouteOutcome::Continue
        ));
    }
    assert!(matches!(
        count_router.route(thread_status("cas-count"), 1),
        RouteOutcome::InvalidateTarget(_)
    ));
    assert_eq!(
        *count_target.terminal.lock().unwrap(),
        Some(LiveEventTargetCloseReason::QueueOverflow)
    );
    let count_snapshot = count_router.snapshot().unwrap();
    assert_eq!(count_snapshot.overflow_count(), 1);
    assert_eq!(count_snapshot.target_count(), 0);

    let byte_router = router(6);
    let byte_target = register(&byte_router, "cas-bytes", 7, 7, Some("turn-bytes"));
    assert!(matches!(
        byte_router.route(
            thread_status("cas-bytes"),
            LIVE_EVENT_TARGET_QUEUE_BYTE_LIMIT + 1,
        ),
        RouteOutcome::InvalidateTarget(_)
    ));
    assert_eq!(
        *byte_target.terminal.lock().unwrap(),
        Some(LiveEventTargetCloseReason::QueueOverflow)
    );
    let byte_snapshot = byte_router.snapshot().unwrap();
    assert_eq!(byte_snapshot.queued_event_bytes(), 0);
    assert_eq!(byte_snapshot.overflow_count(), 1);
}

#[test]
fn account_facts_are_process_scoped_and_malformed_identity_retires() {
    let router = router_for(107, 7);
    let observer = router_for(107, 70);
    let rate_limits = RateLimitSnapshot {
        limit_id: Some("codex".to_owned()),
        limit_name: Some("Codex".to_owned()),
        ..Default::default()
    };
    assert!(matches!(
        router.route(
            TurnStreamEvent::AccountRateLimitsUpdated {
                rate_limits: rate_limits.clone(),
            },
            10,
        ),
        RouteOutcome::Continue
    ));
    let snapshot = observer.process_snapshot().unwrap();
    assert_eq!(snapshot.account_rate_limits(), Some(&rate_limits));
    assert_eq!(snapshot.account_event_count(), 1);
    assert_eq!(snapshot.account_source_connection_generation(), Some(7));
    assert_eq!(snapshot.active_connection_count(), 2);

    assert!(matches!(
        router.route(
            TurnStreamEvent::ThreadNameUpdated {
                thread_id: String::new(),
                thread_name: Some("invalid".to_owned()),
            },
            10,
        ),
        RouteOutcome::RetireConnection(LiveEventTargetCloseReason::InvalidEventIdentity)
    ));
    router.retire(LiveEventTargetCloseReason::InvalidEventIdentity);
    assert_eq!(router.snapshot().unwrap().rejected_event_count(), 1);
    let process_snapshot = observer.process_snapshot().unwrap();
    assert_eq!(process_snapshot.active_connection_count(), 1);
    let fact = process_snapshot.latest_connection_fact().unwrap();
    assert_eq!(fact.connection_generation(), 7);
    assert_eq!(
        fact.state(),
        LiveEventConnectionState::Retired(LiveEventTargetCloseReason::InvalidEventIdentity)
    );
}

#[test]
fn abandoned_receiver_is_distinct_from_capacity_overflow() {
    let router = router(8);
    let registration = register(&router, "cas-abandoned", 8, 8, Some("turn-abandoned"));
    let terminal = registration.terminal.clone();
    drop(registration.receiver);

    assert!(matches!(
        router.route(thread_status("cas-abandoned"), 1),
        RouteOutcome::InvalidateTarget(_)
    ));
    assert_eq!(
        *terminal.lock().unwrap(),
        Some(LiveEventTargetCloseReason::ReceiverAbandoned)
    );
    assert_eq!(router.snapshot().unwrap().overflow_count(), 0);
}

#[test]
fn late_event_after_target_retirement_is_unmatched_not_reassigned() {
    let router = router(9);
    let retired = register(&router, "cas-retired", 9, 9, Some("turn-retired"));
    let survivor = register(&router, "cas-survivor", 10, 10, Some("turn-survivor"));
    assert!(!router.unregister(&retired, LiveEventTargetCloseReason::ReceiverAbandoned));

    let retired_thread_id = CasThreadId::new("cas-retired").unwrap();
    let replacement_error = router
        .register(
            LoadedThreadKey {
                runtime_id: router.runtime_id,
                process_generation: router.process_generation,
                cas_thread_id: retired_thread_id.clone(),
            },
            SyndicThreadId::from_bytes([11; 16]),
            CasLoadedSessionGeneration::new(
                router.process_generation,
                CasLoadedThreadGeneration::new(11).unwrap(),
            ),
            Some(CasTurnId::new("turn-replacement").unwrap()),
        )
        .unwrap_err();
    assert!(matches!(
        replacement_error,
        super::LiveEventTargetRegistrationError::TargetGenerationRetired { thread_id }
            if thread_id == retired_thread_id
    ));

    assert!(matches!(
        router.route(thread_status("cas-retired"), 1),
        RouteOutcome::Continue
    ));
    assert!(survivor.receiver.try_recv().is_err());
    let snapshot = router.snapshot().unwrap();
    assert_eq!(snapshot.unmatched_event_count(), 1);
    assert_eq!(snapshot.target_count(), 1);
    assert_eq!(snapshot.retired_thread_lane_count(), 1);
}

#[test]
fn retired_thread_lane_fence_is_bounded_by_connection_retirement() {
    let router = router(10);
    for index in 0..=super::RETIRED_THREAD_LANE_LIMIT {
        let registration = register(
            &router,
            &format!("cas-retired-{index}"),
            index as u8,
            index as u64 + 1,
            Some(&format!("turn-retired-{index}")),
        );
        let retired =
            router.unregister(&registration, LiveEventTargetCloseReason::ReceiverAbandoned);
        assert_eq!(retired, index == super::RETIRED_THREAD_LANE_LIMIT);
    }

    let snapshot = router.snapshot().unwrap();
    assert_eq!(
        snapshot.state(),
        LiveEventConnectionState::Retired(LiveEventTargetCloseReason::RetiredThreadLaneCapacity)
    );
    assert_eq!(
        snapshot.retired_thread_lane_count(),
        super::RETIRED_THREAD_LANE_LIMIT
    );
    assert_eq!(snapshot.target_count(), 0);
}

#[test]
fn proven_terminal_handoff_requires_the_terminal_event_to_be_drained() {
    let router = router(11);
    let registration = register(&router, "cas-handoff", 11, 11, None);
    router.authorize_turn_start(&registration.proof()).unwrap();
    assert!(matches!(
        router.route(turn_started("cas-handoff", "turn-handoff"), 1),
        RouteOutcome::Continue
    ));
    assert!(matches!(
        router.route(turn_completed("cas-handoff", "turn-handoff"), 1),
        RouteOutcome::Continue
    ));

    assert!(matches!(
        router.handoff_target(&registration, TargetHandoffRequirement::ProvenTerminal),
        Err(LiveEventTargetHandoffError::QueuedEvents { count: 2, .. })
    ));
    take_event(&registration);
    take_event(&registration);
    router
        .handoff_target(&registration, TargetHandoffRequirement::ProvenTerminal)
        .unwrap();

    let snapshot = router.snapshot().unwrap();
    assert_eq!(snapshot.target_count(), 0);
    assert_eq!(snapshot.retired_thread_lane_count(), 0);
    let replacement = register(&router, "cas-handoff", 11, 11, None);
    assert_eq!(
        replacement.loaded_generation,
        registration.loaded_generation
    );
}

#[test]
fn not_started_handoff_requires_dispatch_authorization_and_an_empty_queue() {
    let router = router(12);
    let premature = register(&router, "cas-premature", 12, 12, None);
    assert!(matches!(
        router.handoff_target(&premature, TargetHandoffRequirement::NotStarted),
        Err(LiveEventTargetHandoffError::TargetMayHaveStarted)
    ));
    assert!(!router.unregister(&premature, LiveEventTargetCloseReason::ReceiverAbandoned));

    let queued = register(&router, "cas-not-started-queued", 13, 13, None);
    router.authorize_turn_start(&queued.proof()).unwrap();
    assert!(matches!(
        router.route(thread_status("cas-not-started-queued"), 1),
        RouteOutcome::Continue
    ));
    assert!(matches!(
        router.handoff_target(&queued, TargetHandoffRequirement::NotStarted),
        Err(LiveEventTargetHandoffError::QueuedEvents { count: 1, .. })
    ));
    take_event(&queued);
    router
        .handoff_target(&queued, TargetHandoffRequirement::NotStarted)
        .unwrap();
    assert_eq!(router.snapshot().unwrap().retired_thread_lane_count(), 1);
}

#[test]
fn any_event_after_exact_turn_completion_fails_and_fences_the_target() {
    let router = router(13);
    let registration = register(&router, "cas-post-terminal", 14, 14, Some("turn-terminal"));
    assert!(matches!(
        router.route(turn_completed("cas-post-terminal", "turn-terminal"), 1),
        RouteOutcome::Continue
    ));
    let outcome = router.route(thread_status("cas-post-terminal"), 1);
    assert!(matches!(outcome, RouteOutcome::InvalidateTarget(_)));
    assert_eq!(
        *registration.terminal.lock().unwrap(),
        Some(LiveEventTargetCloseReason::EventAfterTurnCompletion)
    );
    assert_eq!(router.snapshot().unwrap().retired_thread_lane_count(), 1);
}

#[test]
fn dynamic_tool_response_authority_requires_the_exact_routed_request() {
    let router = router(14);
    let exact = register(&router, "cas-tool", 15, 15, Some("turn-tool"));
    let request = parse_dynamic_tool_call_request(
        json!(700),
        "item/tool/call",
        Some(json!({
            "threadId": "cas-tool",
            "turnId": "turn-tool",
            "callId": "call-tool",
            "tool": "yield",
            "arguments": {}
        })),
    )
    .unwrap()
    .unwrap();
    assert!(matches!(
        router.route(
            TurnStreamEvent::DynamicToolCallRequested(request.clone()),
            10,
        ),
        RouteOutcome::Continue
    ));
    router
        .authorize_dynamic_tool_response(&exact.proof(), &request)
        .unwrap();

    let conflicting = register(
        &router,
        "cas-tool-conflict",
        16,
        16,
        Some("turn-tool-conflict"),
    );
    assert!(matches!(
        router.authorize_dynamic_tool_response(&conflicting.proof(), &request),
        Err(super::TargetAuthorizationFailure::Target(
            LiveEventTargetCloseReason::ConflictingTurnIdentity
        ))
    ));
    assert_eq!(
        *conflicting.terminal.lock().unwrap(),
        Some(LiveEventTargetCloseReason::ConflictingTurnIdentity)
    );
}
