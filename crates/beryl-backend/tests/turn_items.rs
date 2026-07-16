use beryl_backend::{
    ProtocolPhase, ThreadItem, ThreadItemKind, ThreadItemLifecycleContract, TurnStreamEvent,
    parse_turn_stream_event,
};
use serde_json::{Value, json};

fn parse_event(method: &str, params: Value) -> TurnStreamEvent {
    parse_turn_stream_event(method, Some(params))
        .expect("notification must be valid")
        .expect("notification must be supported")
}

#[test]
fn pinned_thread_item_union_accepts_exactly_the_eighteen_known_variants() {
    let cases = [
        (
            json!({
                "id": "user-1",
                "type": "userMessage",
                "content": [{"type": "text", "text": "hello"}]
            }),
            ThreadItemKind::UserMessage,
        ),
        (
            json!({
                "id": "hook-1",
                "type": "hookPrompt",
                "fragments": [{"text": "continue", "hookRunId": "run-1"}]
            }),
            ThreadItemKind::HookPrompt,
        ),
        (
            json!({"id": "agent-1", "type": "agentMessage", "text": "answer"}),
            ThreadItemKind::AgentMessage,
        ),
        (
            json!({"id": "plan-1", "type": "plan", "text": "step"}),
            ThreadItemKind::Plan,
        ),
        (
            json!({
                "id": "reasoning-1",
                "type": "reasoning",
                "summary": ["summary"],
                "content": ["private"]
            }),
            ThreadItemKind::Reasoning,
        ),
        (
            json!({
                "id": "command-1",
                "type": "commandExecution",
                "command": "cargo check",
                "cwd": "C:/work",
                "status": "completed",
                "commandActions": []
            }),
            ThreadItemKind::CommandExecution,
        ),
        (
            json!({
                "id": "file-1",
                "type": "fileChange",
                "status": "completed",
                "changes": []
            }),
            ThreadItemKind::FileChange,
        ),
        (
            json!({
                "id": "mcp-1",
                "type": "mcpToolCall",
                "server": "filesystem",
                "tool": "read_file",
                "status": "completed",
                "arguments": {}
            }),
            ThreadItemKind::McpToolCall,
        ),
        (
            json!({
                "id": "dynamic-1",
                "type": "dynamicToolCall",
                "tool": "lookup",
                "arguments": {},
                "status": "completed"
            }),
            ThreadItemKind::DynamicToolCall,
        ),
        (
            json!({
                "id": "collab-1",
                "type": "collabAgentToolCall",
                "tool": "wait",
                "status": "completed",
                "senderThreadId": "thread-parent",
                "receiverThreadIds": ["thread-child"],
                "agentsStates": {
                    "thread-child": {"status": "completed", "message": null}
                }
            }),
            ThreadItemKind::CollabAgentToolCall,
        ),
        (
            json!({
                "id": "subagent-1",
                "type": "subAgentActivity",
                "kind": "started",
                "agentThreadId": "thread-child",
                "agentPath": "agents/thread-child"
            }),
            ThreadItemKind::SubAgentActivity,
        ),
        (
            json!({"id": "web-1", "type": "webSearch", "query": "Beryl"}),
            ThreadItemKind::WebSearch,
        ),
        (
            json!({"id": "view-1", "type": "imageView", "path": "image.png"}),
            ThreadItemKind::ImageView,
        ),
        (
            json!({"id": "sleep-1", "type": "sleep", "durationMs": 10}),
            ThreadItemKind::Sleep,
        ),
        (
            json!({
                "id": "image-1",
                "type": "imageGeneration",
                "status": "completed",
                "result": "base64-image"
            }),
            ThreadItemKind::ImageGeneration,
        ),
        (
            json!({
                "id": "review-enter-1",
                "type": "enteredReviewMode",
                "review": "Review changes"
            }),
            ThreadItemKind::EnteredReviewMode,
        ),
        (
            json!({
                "id": "review-exit-1",
                "type": "exitedReviewMode",
                "review": "Review complete"
            }),
            ThreadItemKind::ExitedReviewMode,
        ),
        (
            json!({"id": "compact-1", "type": "contextCompaction"}),
            ThreadItemKind::ContextCompaction,
        ),
    ];

    assert_eq!(cases.len(), 18);
    for (value, expected_kind) in cases {
        let item: ThreadItem = serde_json::from_value(value).expect("known item must parse");
        assert_eq!(item.kind(), expected_kind);
        assert_eq!(item.item_type(), expected_kind.item_type());
        assert_eq!(
            item.lifecycle_contract(),
            expected_kind.lifecycle_contract()
        );
    }

    assert_eq!(
        ThreadItemKind::SubAgentActivity.lifecycle_contract(),
        ThreadItemLifecycleContract::CompletionOnly
    );
}

#[test]
fn unknown_and_malformed_items_fail_the_closed_boundary() {
    let unknown = serde_json::from_value::<ThreadItem>(json!({
        "id": "future-1",
        "type": "futureItem"
    }));
    assert!(unknown.is_err());

    let malformed = serde_json::from_value::<ThreadItem>(json!({
        "id": "image-1",
        "type": "imageGeneration"
    }));
    assert!(malformed.is_err());
}

#[test]
fn reasoning_retains_only_summary_content() {
    let item: ThreadItem = serde_json::from_value(json!({
        "id": "reasoning-1",
        "type": "reasoning",
        "summary": ["public summary"],
        "content": ["private chain of thought"]
    }))
    .unwrap();

    let ThreadItem::Reasoning(reasoning) = item else {
        panic!("expected reasoning item");
    };
    assert_eq!(reasoning.summary, ["public summary"]);
}

#[test]
fn agent_message_preserves_public_phase_metadata() {
    let item: ThreadItem = serde_json::from_value(json!({
        "id": "agent-1",
        "type": "agentMessage",
        "phase": "final_answer",
        "text": "done"
    }))
    .unwrap();

    let ThreadItem::AgentMessage(message) = item else {
        panic!("expected agent message");
    };
    assert_eq!(message.phase, Some(ProtocolPhase::FinalAnswer));
}

#[test]
fn subagent_activity_is_completion_only() {
    let params = json!({
        "threadId": "thread-parent",
        "turnId": "turn-parent",
        "startedAtMs": 1_752_689_600_123_u64,
        "completedAtMs": 1_752_689_600_123_u64,
        "item": {
            "id": "subagent-1",
            "type": "subAgentActivity",
            "kind": "started",
            "agentThreadId": "thread-child",
            "agentPath": "agents/thread-child"
        }
    });

    assert!(parse_turn_stream_event("item/started", Some(params.clone())).is_err());
    let event = parse_event("item/completed", params);
    let TurnStreamEvent::ItemCompleted { item, .. } = event else {
        panic!("expected completion-only item");
    };
    assert_eq!(item.kind(), ThreadItemKind::SubAgentActivity);
}

#[test]
fn standalone_image_generation_has_a_paired_typed_lifecycle() {
    for method in ["item/started", "item/completed"] {
        let timestamp_field = if method == "item/started" {
            "startedAtMs"
        } else {
            "completedAtMs"
        };
        let mut params = json!({
            "threadId": "thread-1",
            "turnId": "turn-1",
            "item": {
                "id": "image-1",
                "type": "imageGeneration",
                "status": "generating",
                "result": "base64-image",
                "savedPath": "generated/image-1.png"
            }
        });
        params[timestamp_field] = json!(1_752_689_600_123_u64);
        let event = parse_event(method, params);
        let item = match event {
            TurnStreamEvent::ItemStarted { item, .. }
            | TurnStreamEvent::ItemCompleted { item, .. } => item,
            _ => panic!("expected image item lifecycle"),
        };
        let ThreadItem::ImageGeneration(image) = item else {
            panic!("expected image generation item");
        };
        assert_eq!(image.status, "generating");
        assert_eq!(
            image.saved_path.as_deref(),
            Some(std::path::Path::new("generated/image-1.png"))
        );
    }
}
