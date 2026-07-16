use beryl_backend::{
    CodexErrorInfo, CompletedTurnStatus, ItemDeltaPayload, TerminalNonSteerableTurnKind,
    TurnStatus, TurnStreamEvent, parse_turn_stream_event,
};
use serde_json::{Value, json};

fn parse_event(method: &str, params: Value) -> TurnStreamEvent {
    parse_turn_stream_event(method, Some(params))
        .expect("notification must be valid")
        .expect("notification must be supported")
}

#[test]
fn every_known_notification_rejects_absent_params() {
    for method in [
        "thread/status/changed",
        "thread/started",
        "thread/closed",
        "turn/started",
        "turn/completed",
        "item/started",
        "item/completed",
        "item/agentMessage/delta",
        "item/plan/delta",
        "item/reasoning/summaryPartAdded",
        "item/reasoning/summaryTextDelta",
        "item/reasoning/textDelta",
        "item/commandExecution/outputDelta",
        "item/fileChange/outputDelta",
        "item/fileChange/patchUpdated",
        "item/mcpToolCall/progress",
        "thread/name/updated",
        "thread/tokenUsage/updated",
        "account/rateLimits/updated",
        "codex/event/collab_agent_spawn_end",
    ] {
        let error = parse_turn_stream_event(method, None)
            .expect_err("a known notification without params must fail closed");
        assert!(error.to_string().contains(method));
    }
}

#[test]
fn unknown_notification_without_params_remains_ignorable() {
    assert_eq!(
        parse_turn_stream_event("future/notification", None)
            .expect("an unknown notification remains non-fatal"),
        None
    );
}

#[test]
fn turn_started_and_completed_expose_only_typed_status_authority() {
    let started = parse_event(
        "turn/started",
        json!({
            "threadId": "thread-1",
            "turn": {"id": "turn-1", "status": "inProgress", "items": []}
        }),
    );
    assert!(matches!(
        started,
        TurnStreamEvent::TurnStarted {
            status: TurnStatus::InProgress,
            ..
        }
    ));

    let completed = parse_event(
        "turn/completed",
        json!({
            "threadId": "thread-1",
            "turn": {
                "id": "turn-1",
                "status": "failed",
                "items": [],
                "itemsView": "notLoaded",
                "error": {
                    "message": "review turn cannot be steered",
                    "codexErrorInfo": {
                        "activeTurnNotSteerable": {"turnKind": "review"}
                    }
                }
            }
        }),
    );
    let TurnStreamEvent::TurnCompleted { thread_id, turn } = completed else {
        panic!("expected terminal turn");
    };
    assert_eq!(thread_id.as_str(), "thread-1");
    assert_eq!(turn.id.as_str(), "turn-1");
    assert_eq!(turn.status, CompletedTurnStatus::Failed);
    assert_eq!(
        turn.error.and_then(|error| error.codex_error_info),
        Some(CodexErrorInfo::ActiveTurnNotSteerable {
            turn_kind: TerminalNonSteerableTurnKind::Review,
        })
    );

    assert!(
        parse_turn_stream_event(
            "turn/completed",
            Some(json!({
                "threadId": "thread-1",
                "turn": {"id": "turn-1", "status": "inProgress"}
            }))
        )
        .is_err()
    );
}

#[test]
fn every_item_delta_carries_its_exact_expected_kind() {
    let cases = [
        (
            "item/agentMessage/delta",
            json!({"delta": "answer"}),
            ItemDeltaPayload::AgentMessage {
                delta: "answer".to_string(),
            },
        ),
        (
            "item/plan/delta",
            json!({"delta": "step"}),
            ItemDeltaPayload::Plan {
                delta: "step".to_string(),
            },
        ),
        (
            "item/reasoning/summaryPartAdded",
            json!({"summaryIndex": 2}),
            ItemDeltaPayload::ReasoningSummaryPartAdded { summary_index: 2 },
        ),
        (
            "item/reasoning/summaryTextDelta",
            json!({"summaryIndex": 3, "delta": "summary"}),
            ItemDeltaPayload::ReasoningSummaryText {
                summary_index: 3,
                delta: "summary".to_string(),
            },
        ),
        (
            "item/reasoning/textDelta",
            json!({"contentIndex": 4, "delta": "private"}),
            ItemDeltaPayload::ReasoningTextObserved { content_index: 4 },
        ),
        (
            "item/commandExecution/outputDelta",
            json!({"delta": "output"}),
            ItemDeltaPayload::CommandExecutionOutput {
                delta: "output".to_string(),
            },
        ),
        (
            "item/fileChange/outputDelta",
            json!({"delta": "output"}),
            ItemDeltaPayload::FileChangeOutput {
                delta: "output".to_string(),
            },
        ),
        (
            "item/fileChange/patchUpdated",
            json!({"changes": []}),
            ItemDeltaPayload::FileChangePatchUpdated {
                changes: Vec::new(),
            },
        ),
        (
            "item/mcpToolCall/progress",
            json!({"message": "working"}),
            ItemDeltaPayload::McpToolCallProgress {
                message: "working".to_string(),
            },
        ),
    ];

    for (method, extra, expected_payload) in cases {
        let mut params = json!({
            "threadId": "thread-1",
            "turnId": "turn-1",
            "itemId": "item-1"
        });
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());

        let TurnStreamEvent::ItemDelta(delta) = parse_event(method, params) else {
            panic!("expected item delta for {method}");
        };
        assert_eq!(delta.thread_id().as_str(), "thread-1");
        assert_eq!(delta.turn_id().as_str(), "turn-1");
        assert_eq!(delta.item_id().as_str(), "item-1");
        assert_eq!(
            delta.expected_item_kind(),
            expected_payload.expected_item_kind()
        );
        assert_eq!(delta.payload(), &expected_payload);
    }
}

#[test]
fn negative_protocol_indices_are_rejected_before_publication() {
    for (method, index_field) in [
        ("item/reasoning/summaryPartAdded", "summaryIndex"),
        ("item/reasoning/summaryTextDelta", "summaryIndex"),
        ("item/reasoning/textDelta", "contentIndex"),
    ] {
        let mut params = json!({
            "threadId": "thread-1",
            "turnId": "turn-1",
            "itemId": "reasoning-1",
            "delta": "hidden"
        });
        params[index_field] = json!(-1);
        assert!(parse_turn_stream_event(method, Some(params)).is_err());
    }
}

#[test]
fn raw_reasoning_delta_publishes_no_reasoning_text_or_activity_detail() {
    let event = parse_event(
        "item/reasoning/textDelta",
        json!({
            "threadId": "thread-1",
            "turnId": "turn-1",
            "itemId": "reasoning-1",
            "contentIndex": 0,
            "delta": "private chain of thought"
        }),
    );

    assert!(event.activity().is_none());
    assert!(event.tool_activity().is_none());
    let TurnStreamEvent::ItemDelta(delta) = event else {
        panic!("expected typed reasoning delta");
    };
    assert_eq!(
        delta.payload(),
        &ItemDeltaPayload::ReasoningTextObserved { content_index: 0 }
    );
}
