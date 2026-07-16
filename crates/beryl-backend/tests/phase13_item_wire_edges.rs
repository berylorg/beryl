use beryl_backend::{
    CommandExecutionSource, ItemLifecycleTimestampMs, ThreadItem, TurnStreamEvent,
    parse_turn_stream_event,
};
use serde_json::{Map, Value, json};

fn parse_item(value: Value) -> Result<ThreadItem, serde_json::Error> {
    serde_json::from_value(value)
}

fn parent_object_mut<'a>(value: &'a mut Value, pointer: &str) -> &'a mut Map<String, Value> {
    if pointer.is_empty() {
        value.as_object_mut().expect("fixture root object")
    } else {
        value
            .pointer_mut(pointer)
            .expect("fixture pointer")
            .as_object_mut()
            .expect("fixture nested object")
    }
}

fn assert_missing_fails(base: &Value, pointer: &str, field: &str) {
    let mut missing = base.clone();
    let removed = parent_object_mut(&mut missing, pointer).remove(field);
    assert!(removed.is_some(), "fixture must contain {pointer}/{field}");
    assert!(
        parse_item(missing).is_err(),
        "missing required field must fail: {pointer}/{field}"
    );
}

#[test]
fn every_item_variant_rejects_each_missing_required_top_level_field() {
    let cases: Vec<(Value, &[&str])> = vec![
        (
            json!({"type": "userMessage", "id": "user", "content": []}),
            &["type", "id", "content"],
        ),
        (
            json!({"type": "hookPrompt", "id": "hook", "fragments": []}),
            &["type", "id", "fragments"],
        ),
        (
            json!({"type": "agentMessage", "id": "agent", "text": "answer"}),
            &["type", "id", "text"],
        ),
        (
            json!({"type": "plan", "id": "plan", "text": "plan"}),
            &["type", "id", "text"],
        ),
        (
            json!({"type": "reasoning", "id": "reasoning"}),
            &["type", "id"],
        ),
        (
            json!({
                "type": "commandExecution",
                "id": "command",
                "command": "proof",
                "cwd": "C:/repo",
                "status": "inProgress",
                "commandActions": []
            }),
            &["type", "id", "command", "cwd", "status", "commandActions"],
        ),
        (
            json!({"type": "fileChange", "id": "file", "status": "inProgress", "changes": []}),
            &["type", "id", "status", "changes"],
        ),
        (
            json!({
                "type": "mcpToolCall",
                "id": "mcp",
                "server": "server",
                "tool": "tool",
                "status": "inProgress",
                "arguments": {}
            }),
            &["type", "id", "server", "tool", "status", "arguments"],
        ),
        (
            json!({
                "type": "dynamicToolCall",
                "id": "dynamic",
                "tool": "tool",
                "arguments": {},
                "status": "inProgress"
            }),
            &["type", "id", "tool", "arguments", "status"],
        ),
        (
            json!({
                "type": "collabAgentToolCall",
                "id": "collab",
                "tool": "wait",
                "status": "inProgress",
                "senderThreadId": "thread-parent",
                "receiverThreadIds": [],
                "agentsStates": {}
            }),
            &[
                "type",
                "id",
                "tool",
                "status",
                "senderThreadId",
                "receiverThreadIds",
                "agentsStates",
            ],
        ),
        (
            json!({
                "type": "subAgentActivity",
                "id": "subagent",
                "kind": "started",
                "agentThreadId": "thread-child",
                "agentPath": "agents/thread-child"
            }),
            &["type", "id", "kind", "agentThreadId", "agentPath"],
        ),
        (
            json!({"type": "webSearch", "id": "web", "query": "query"}),
            &["type", "id", "query"],
        ),
        (
            json!({"type": "imageView", "id": "view", "path": "/runtime/view.png"}),
            &["type", "id", "path"],
        ),
        (
            json!({"type": "sleep", "id": "sleep", "durationMs": 1}),
            &["type", "id", "durationMs"],
        ),
        (
            json!({"type": "imageGeneration", "id": "image", "status": "completed", "result": "base64"}),
            &["type", "id", "status"],
        ),
        (
            json!({"type": "enteredReviewMode", "id": "enter", "review": "review"}),
            &["type", "id", "review"],
        ),
        (
            json!({"type": "exitedReviewMode", "id": "exit", "review": "review"}),
            &["type", "id", "review"],
        ),
        (
            json!({"type": "contextCompaction", "id": "compact"}),
            &["type", "id"],
        ),
    ];

    assert_eq!(cases.len(), 18);
    for (base, fields) in cases {
        for field in fields {
            assert_missing_fails(&base, "", field);
        }
    }
}

#[test]
fn nested_public_records_reject_each_missing_required_field() {
    let cases: Vec<(Value, &str, &[&str])> = vec![
        (
            json!({
                "type": "userMessage", "id": "user", "content": [
                    {"type": "text", "text": "x", "text_elements": [
                        {"byteRange": {"start": 0, "end": 1}, "placeholder": "x"}
                    ]}
                ]
            }),
            "/content/0",
            &["type", "text"],
        ),
        (
            json!({"type": "userMessage", "id": "user", "content": [{"type": "image", "url": "data:image/png;base64,AA=="}]}),
            "/content/0",
            &["type", "url"],
        ),
        (
            json!({"type": "userMessage", "id": "user", "content": [{"type": "localImage", "path": "/runtime/a.png"}]}),
            "/content/0",
            &["type", "path"],
        ),
        (
            json!({"type": "userMessage", "id": "user", "content": [{"type": "skill", "name": "proof", "path": "/runtime/SKILL.md"}]}),
            "/content/0",
            &["type", "name", "path"],
        ),
        (
            json!({"type": "userMessage", "id": "user", "content": [{"type": "mention", "name": "app", "path": "app://one"}]}),
            "/content/0",
            &["type", "name", "path"],
        ),
        (
            json!({"type": "userMessage", "id": "user", "content": [{"type": "text", "text": "x", "text_elements": [{"byteRange": {"start": 0, "end": 1}}]}]}),
            "/content/0/text_elements/0",
            &["byteRange"],
        ),
        (
            json!({"type": "userMessage", "id": "user", "content": [{"type": "text", "text": "x", "text_elements": [{"byteRange": {"start": 0, "end": 1}}]}]}),
            "/content/0/text_elements/0/byteRange",
            &["start", "end"],
        ),
        (
            json!({"type": "hookPrompt", "id": "hook", "fragments": [{"text": "x", "hookRunId": "run"}]}),
            "/fragments/0",
            &["text", "hookRunId"],
        ),
        (
            json!({
                "type": "agentMessage", "id": "agent", "text": "x",
                "memoryCitation": {"entries": [{"path": "a", "lineStart": 1, "lineEnd": 2, "note": "n"}], "threadIds": []}
            }),
            "/memoryCitation",
            &["entries", "threadIds"],
        ),
        (
            json!({
                "type": "agentMessage", "id": "agent", "text": "x",
                "memoryCitation": {"entries": [{"path": "a", "lineStart": 1, "lineEnd": 2, "note": "n"}], "threadIds": []}
            }),
            "/memoryCitation/entries/0",
            &["path", "lineStart", "lineEnd", "note"],
        ),
        (
            json!({
                "type": "commandExecution", "id": "command", "command": "x", "cwd": "C:/repo", "status": "inProgress",
                "commandActions": [{"type": "read", "command": "read", "name": "file", "path": "C:/repo/file"}]
            }),
            "/commandActions/0",
            &["type", "command", "name", "path"],
        ),
        (
            json!({
                "type": "commandExecution", "id": "command", "command": "x", "cwd": "C:/repo", "status": "inProgress",
                "commandActions": [{"type": "listFiles", "command": "list", "path": "C:/repo"}]
            }),
            "/commandActions/0",
            &["type", "command"],
        ),
        (
            json!({
                "type": "commandExecution", "id": "command", "command": "x", "cwd": "C:/repo", "status": "inProgress",
                "commandActions": [{"type": "search", "command": "search", "query": "q", "path": "C:/repo"}]
            }),
            "/commandActions/0",
            &["type", "command"],
        ),
        (
            json!({
                "type": "commandExecution", "id": "command", "command": "x", "cwd": "C:/repo", "status": "inProgress",
                "commandActions": [{"type": "unknown", "command": "custom"}]
            }),
            "/commandActions/0",
            &["type", "command"],
        ),
        (
            json!({
                "type": "fileChange", "id": "file", "status": "inProgress",
                "changes": [{"path": "a", "diff": "+x", "kind": {"type": "update", "move_path": "b"}}]
            }),
            "/changes/0",
            &["path", "diff", "kind"],
        ),
        (
            json!({
                "type": "fileChange", "id": "file", "status": "inProgress",
                "changes": [{"path": "a", "diff": "+x", "kind": {"type": "update", "move_path": "b"}}]
            }),
            "/changes/0/kind",
            &["type"],
        ),
    ];

    for (base, pointer, fields) in cases {
        for field in fields {
            assert_missing_fails(&base, pointer, field);
        }
    }
}

#[test]
fn nested_tool_and_activity_records_reject_missing_required_fields() {
    let cases: Vec<(Value, &str, &[&str])> = vec![
        (
            json!({
                "type": "mcpToolCall", "id": "mcp", "server": "server", "tool": "tool", "status": "completed", "arguments": {},
                "appContext": {"connectorId": "connector", "linkId": null, "resourceUri": null, "appName": null, "templateId": null, "actionName": null}
            }),
            "/appContext",
            &["connectorId"],
        ),
        (
            json!({
                "type": "mcpToolCall", "id": "mcp", "server": "server", "tool": "tool", "status": "completed", "arguments": {},
                "result": {"content": [], "structuredContent": null, "_meta": null}
            }),
            "/result",
            &["content"],
        ),
        (
            json!({
                "type": "mcpToolCall", "id": "mcp", "server": "server", "tool": "tool", "status": "failed", "arguments": {},
                "error": {"message": "failure"}
            }),
            "/error",
            &["message"],
        ),
        (
            json!({
                "type": "dynamicToolCall", "id": "dynamic", "tool": "tool", "arguments": {}, "status": "completed",
                "contentItems": [{"type": "inputText", "text": "answer"}]
            }),
            "/contentItems/0",
            &["type", "text"],
        ),
        (
            json!({
                "type": "dynamicToolCall", "id": "dynamic", "tool": "tool", "arguments": {}, "status": "completed",
                "contentItems": [{"type": "inputImage", "imageUrl": "data:image/png;base64,AA=="}]
            }),
            "/contentItems/0",
            &["type", "imageUrl"],
        ),
        (
            json!({
                "type": "collabAgentToolCall", "id": "collab", "tool": "wait", "status": "completed",
                "senderThreadId": "thread-parent", "receiverThreadIds": [],
                "agentsStates": {"thread-child": {"status": "completed", "message": null}}
            }),
            "/agentsStates/thread-child",
            &["status"],
        ),
        (
            json!({
                "type": "webSearch", "id": "web", "query": "query",
                "action": {"type": "findInPage", "url": null, "pattern": null}
            }),
            "/action",
            &["type"],
        ),
    ];

    for (base, pointer, fields) in cases {
        for field in fields {
            assert_missing_fails(&base, pointer, field);
        }
    }
}

#[test]
fn malformed_types_and_unknown_closed_enums_fail() {
    let malformed = [
        json!({"type": "futureItem", "id": "future"}),
        json!({"type": "agentMessage", "id": "agent", "text": [], "phase": "commentary"}),
        json!({"type": "agentMessage", "id": "agent", "text": "x", "phase": "future"}),
        json!({"type": "userMessage", "id": "user", "content": [{"type": "image", "detail": "huge", "url": "data:image/png;base64,AA=="}]}),
        json!({"type": "userMessage", "id": "user", "content": [{"type": "text", "text": "x", "text_elements": [{"byteRange": {"start": -1, "end": 1}}]}]}),
        json!({"type": "commandExecution", "id": "command", "command": "x", "cwd": "C:/repo", "status": "future", "commandActions": []}),
        json!({"type": "fileChange", "id": "file", "status": "completed", "changes": [{"path": "a", "diff": "x", "kind": {"type": "future"}}]}),
        json!({"type": "mcpToolCall", "id": "mcp", "server": "s", "tool": "t", "status": "future", "arguments": {}}),
        json!({"type": "dynamicToolCall", "id": "dynamic", "tool": "t", "status": "completed", "arguments": {}, "contentItems": [{"type": "future", "text": "x"}]}),
        json!({"type": "collabAgentToolCall", "id": "collab", "tool": "future", "status": "completed", "senderThreadId": "parent", "receiverThreadIds": [], "agentsStates": {}}),
        json!({"type": "subAgentActivity", "id": "sub", "kind": "future", "agentThreadId": "child", "agentPath": "agents/child"}),
        json!({"type": "sleep", "id": "sleep", "durationMs": -1}),
    ];

    for value in malformed {
        assert!(parse_item(value).is_err());
    }
}

#[test]
fn lifecycle_wire_rejects_missing_required_identity_and_item_fields() {
    for (method, timestamp_field) in [
        ("item/started", "startedAtMs"),
        ("item/completed", "completedAtMs"),
    ] {
        let mut base = json!({
            "threadId": "thread-phase13",
            "turnId": "turn-phase13",
            "item": {"type": "contextCompaction", "id": "compact"}
        });
        base[timestamp_field] = json!(1_752_689_600_123_i64);
        for field in ["threadId", "turnId", "item"] {
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
fn lifecycle_timestamps_are_required_exact_and_nonnegative() {
    let item = json!({"type": "contextCompaction", "id": "compact"});
    for (method, timestamp_field, wrong_timestamp_field) in [
        ("item/started", "startedAtMs", "completedAtMs"),
        ("item/completed", "completedAtMs", "startedAtMs"),
    ] {
        let base = json!({
            "threadId": "thread-phase13",
            "turnId": "turn-phase13",
            "item": item.clone()
        });
        assert!(parse_turn_stream_event(method, Some(base.clone())).is_err());

        let mut wrong = base.clone();
        wrong[wrong_timestamp_field] = json!(7_u64);
        assert!(parse_turn_stream_event(method, Some(wrong)).is_err());

        for malformed in [json!(null), json!(-1), json!(1.5), json!("7")] {
            let mut params = base.clone();
            params[timestamp_field] = malformed;
            assert!(parse_turn_stream_event(method, Some(params)).is_err());
        }

        for timestamp in [0_u64, 1_752_689_600_123_u64, u64::MAX] {
            let mut params = base.clone();
            params[timestamp_field] = json!(timestamp);
            let event = parse_turn_stream_event(method, Some(params))
                .unwrap()
                .unwrap();
            let actual = match event {
                TurnStreamEvent::ItemStarted { started_at_ms, .. } => started_at_ms,
                TurnStreamEvent::ItemCompleted {
                    completed_at_ms, ..
                } => completed_at_ms,
                other => panic!("unexpected lifecycle event: {other:?}"),
            };
            assert_eq!(actual, ItemLifecycleTimestampMs::new(timestamp));
            assert_eq!(actual.get(), timestamp);
        }
    }
}

#[test]
fn missing_command_source_uses_the_pinned_agent_default_but_null_is_malformed() {
    let base = json!({
        "type": "commandExecution",
        "id": "command-default-source",
        "command": "proof",
        "cwd": "C:/repo",
        "status": "inProgress",
        "commandActions": []
    });
    let ThreadItem::CommandExecution(command) = parse_item(base.clone()).unwrap() else {
        panic!("expected command item");
    };
    assert_eq!(command.source, CommandExecutionSource::Agent);

    let mut explicit_null = base;
    explicit_null["source"] = Value::Null;
    assert!(parse_item(explicit_null).is_err());
}
