use beryl_backend::{
    FileUpdateChange, ItemDeltaPayload, PatchChangeKind, ThreadItemKind, TurnStreamEvent,
    parse_turn_stream_event,
};
use serde_json::{Value, json};

const THREAD_ID: &str = "thread-delta";
const TURN_ID: &str = "turn-delta";
const ITEM_ID: &str = "item-delta";

fn params(extra: Value) -> Value {
    let mut value = json!({
        "threadId": THREAD_ID,
        "turnId": TURN_ID,
        "itemId": ITEM_ID
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    value
}

fn parse_delta(method: &str, extra: Value) -> beryl_backend::ItemDelta {
    let event = parse_turn_stream_event(method, Some(params(extra)))
        .expect("delta notification must parse")
        .expect("delta notification must be recognized");
    let TurnStreamEvent::ItemDelta(delta) = event else {
        panic!("expected item delta for {method}");
    };
    delta
}

fn assert_exact_delta(method: &str, extra: Value, expected: ItemDeltaPayload) {
    let delta = parse_delta(method, extra);
    assert_eq!(delta.thread_id().as_str(), THREAD_ID);
    assert_eq!(delta.turn_id().as_str(), TURN_ID);
    assert_eq!(delta.item_id().as_str(), ITEM_ID);
    assert_eq!(delta.expected_item_kind(), expected.expected_item_kind());
    assert_eq!(delta.payload(), &expected);
}

#[test]
fn every_pinned_delta_method_preserves_its_exact_fields_and_kind() {
    let changes = vec![
        FileUpdateChange {
            path: "added.rs".to_string(),
            diff: "+added".to_string(),
            kind: PatchChangeKind::Add,
        },
        FileUpdateChange {
            path: "deleted.rs".to_string(),
            diff: "-deleted".to_string(),
            kind: PatchChangeKind::Delete,
        },
        FileUpdateChange {
            path: "old.rs".to_string(),
            diff: "-old\n+new".to_string(),
            kind: PatchChangeKind::Update {
                move_path: Some("new.rs".to_string()),
            },
        },
    ];
    let cases = [
        (
            "item/agentMessage/delta",
            json!({"delta": "agent bytes"}),
            ItemDeltaPayload::AgentMessage {
                delta: "agent bytes".to_string(),
            },
        ),
        (
            "item/plan/delta",
            json!({"delta": "plan bytes"}),
            ItemDeltaPayload::Plan {
                delta: "plan bytes".to_string(),
            },
        ),
        (
            "item/reasoning/summaryPartAdded",
            json!({"summaryIndex": 0}),
            ItemDeltaPayload::ReasoningSummaryPartAdded { summary_index: 0 },
        ),
        (
            "item/reasoning/summaryTextDelta",
            json!({"summaryIndex": 17, "delta": "summary bytes"}),
            ItemDeltaPayload::ReasoningSummaryText {
                summary_index: 17,
                delta: "summary bytes".to_string(),
            },
        ),
        (
            "item/reasoning/textDelta",
            json!({"contentIndex": 23, "delta": "private raw reasoning"}),
            ItemDeltaPayload::ReasoningTextObserved { content_index: 23 },
        ),
        (
            "item/commandExecution/outputDelta",
            json!({"delta": "stdout and stderr"}),
            ItemDeltaPayload::CommandExecutionOutput {
                delta: "stdout and stderr".to_string(),
            },
        ),
        (
            "item/fileChange/outputDelta",
            json!({"delta": "legacy patch output"}),
            ItemDeltaPayload::FileChangeOutput {
                delta: "legacy patch output".to_string(),
            },
        ),
        (
            "item/fileChange/patchUpdated",
            json!({
                "changes": [
                    {"path": "added.rs", "diff": "+added", "kind": {"type": "add"}},
                    {"path": "deleted.rs", "diff": "-deleted", "kind": {"type": "delete"}},
                    {"path": "old.rs", "diff": "-old\n+new", "kind": {"type": "update", "move_path": "new.rs"}}
                ]
            }),
            ItemDeltaPayload::FileChangePatchUpdated { changes },
        ),
        (
            "item/mcpToolCall/progress",
            json!({"message": "MCP exact progress message"}),
            ItemDeltaPayload::McpToolCallProgress {
                message: "MCP exact progress message".to_string(),
            },
        ),
    ];

    for (method, extra, expected) in cases {
        assert_exact_delta(method, extra, expected);
    }
}

#[test]
fn every_delta_method_rejects_missing_common_and_payload_fields() {
    let cases = [
        (
            "item/agentMessage/delta",
            json!({"delta": "x"}),
            &["delta"][..],
        ),
        ("item/plan/delta", json!({"delta": "x"}), &["delta"][..]),
        (
            "item/reasoning/summaryPartAdded",
            json!({"summaryIndex": 0}),
            &["summaryIndex"][..],
        ),
        (
            "item/reasoning/summaryTextDelta",
            json!({"summaryIndex": 0, "delta": "x"}),
            &["summaryIndex", "delta"][..],
        ),
        (
            "item/reasoning/textDelta",
            json!({"contentIndex": 0, "delta": "private"}),
            &["contentIndex", "delta"][..],
        ),
        (
            "item/commandExecution/outputDelta",
            json!({"delta": "x"}),
            &["delta"][..],
        ),
        (
            "item/fileChange/outputDelta",
            json!({"delta": "x"}),
            &["delta"][..],
        ),
        (
            "item/fileChange/patchUpdated",
            json!({"changes": []}),
            &["changes"][..],
        ),
        (
            "item/mcpToolCall/progress",
            json!({"message": "working"}),
            &["message"][..],
        ),
    ];

    for (method, extra, payload_fields) in cases {
        let base = params(extra);
        for field in ["threadId", "turnId", "itemId"]
            .into_iter()
            .chain(payload_fields.iter().copied())
        {
            let mut missing = base.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                parse_turn_stream_event(method, Some(missing)).is_err(),
                "{method} must reject missing {field}"
            );
        }
    }
}

#[test]
fn reasoning_indices_accept_exact_bounds_and_reject_non_indices() {
    let maximum = usize::try_from(i64::MAX).expect("supported targets represent i64::MAX");
    assert_exact_delta(
        "item/reasoning/summaryPartAdded",
        json!({"summaryIndex": i64::MAX}),
        ItemDeltaPayload::ReasoningSummaryPartAdded {
            summary_index: maximum,
        },
    );
    assert_exact_delta(
        "item/reasoning/summaryTextDelta",
        json!({"summaryIndex": i64::MAX, "delta": "last"}),
        ItemDeltaPayload::ReasoningSummaryText {
            summary_index: maximum,
            delta: "last".to_string(),
        },
    );
    assert_exact_delta(
        "item/reasoning/textDelta",
        json!({"contentIndex": i64::MAX, "delta": "private"}),
        ItemDeltaPayload::ReasoningTextObserved {
            content_index: maximum,
        },
    );

    for (method, field) in [
        ("item/reasoning/summaryPartAdded", "summaryIndex"),
        ("item/reasoning/summaryTextDelta", "summaryIndex"),
        ("item/reasoning/textDelta", "contentIndex"),
    ] {
        for invalid in [
            json!(-1),
            json!(i64::MIN),
            json!(u64::MAX),
            json!(1.5),
            json!("1"),
            Value::Null,
        ] {
            let mut extra = json!({"delta": "value"});
            extra[field] = invalid;
            assert!(parse_turn_stream_event(method, Some(params(extra))).is_err());
        }
    }
}

#[test]
fn delta_payload_types_and_patch_nested_fields_fail_closed() {
    for (method, extra) in [
        ("item/agentMessage/delta", json!({"delta": 7})),
        ("item/plan/delta", json!({"delta": false})),
        (
            "item/reasoning/summaryTextDelta",
            json!({"summaryIndex": 0, "delta": []}),
        ),
        (
            "item/reasoning/textDelta",
            json!({"contentIndex": 0, "delta": {}}),
        ),
        ("item/commandExecution/outputDelta", json!({"delta": null})),
        ("item/fileChange/outputDelta", json!({"delta": 1})),
        ("item/fileChange/patchUpdated", json!({"changes": {}})),
        ("item/mcpToolCall/progress", json!({"message": ["working"]})),
    ] {
        assert!(parse_turn_stream_event(method, Some(params(extra))).is_err());
    }

    let patch = json!({
        "changes": [{"path": "a", "diff": "+x", "kind": {"type": "update", "move_path": "b"}}]
    });
    for (pointer, field) in [
        ("/changes/0", "path"),
        ("/changes/0", "diff"),
        ("/changes/0", "kind"),
        ("/changes/0/kind", "type"),
    ] {
        let mut malformed = patch.clone();
        malformed
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            parse_turn_stream_event("item/fileChange/patchUpdated", Some(params(malformed)))
                .is_err()
        );
    }
}

#[test]
fn mcp_progress_remains_a_typed_mcp_delta_not_tool_activity_summary() {
    let delta = parse_delta(
        "item/mcpToolCall/progress",
        json!({"message": "exact provider progress"}),
    );
    assert_eq!(delta.expected_item_kind(), ThreadItemKind::McpToolCall);
    assert_eq!(
        delta.into_payload(),
        ItemDeltaPayload::McpToolCallProgress {
            message: "exact provider progress".to_string(),
        }
    );
}
