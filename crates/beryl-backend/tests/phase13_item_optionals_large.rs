use std::path::PathBuf;

use beryl_backend::{
    AgentMessageItem, DynamicToolCallItem, DynamicToolCallOutputContentItem, DynamicToolCallStatus,
    FileChangeItem, FileUpdateChange, ImageGenerationItem, McpToolCallItem, McpToolCallResult,
    McpToolCallStatus, PatchApplyStatus, PatchChangeKind, ThreadItem, TurnStreamEvent,
    parse_turn_stream_event,
};
use beryl_model::{CasItemId, CasThreadId, CasTurnId};
use serde_json::{Map, Value, json};

fn parse_item(value: Value) -> ThreadItem {
    serde_json::from_value(value).expect("pinned item fixture must parse")
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

fn assert_missing_and_null_are_equal(base: &Value, pointer: &str, field: &str) {
    let mut missing = base.clone();
    let removed = parent_object_mut(&mut missing, pointer).remove(field);
    assert!(removed.is_some(), "fixture must contain {pointer}/{field}");
    let mut explicit_null = base.clone();
    parent_object_mut(&mut explicit_null, pointer).insert(field.to_string(), Value::Null);
    assert_eq!(parse_item(missing), parse_item(explicit_null));
}

#[test]
fn operational_optional_fields_treat_missing_and_null_as_the_same_absence() {
    let command = json!({
        "type": "commandExecution", "id": "command-optionals", "command": "proof", "cwd": "C:/repo",
        "processId": "pty-1", "source": "agent", "status": "completed",
        "commandActions": [
            {"type": "listFiles", "command": "list", "path": "C:/repo"},
            {"type": "search", "command": "search", "query": "q", "path": "C:/repo"}
        ],
        "aggregatedOutput": "output", "exitCode": 0, "durationMs": 2
    });
    for field in ["processId", "aggregatedOutput", "exitCode", "durationMs"] {
        assert_missing_and_null_are_equal(&command, "", field);
    }
    assert_missing_and_null_are_equal(&command, "/commandActions/0", "path");
    assert_missing_and_null_are_equal(&command, "/commandActions/1", "query");
    assert_missing_and_null_are_equal(&command, "/commandActions/1", "path");

    let file = json!({
        "type": "fileChange", "id": "file-optionals", "status": "completed",
        "changes": [{"path": "a", "diff": "+x", "kind": {"type": "update", "move_path": "b"}}]
    });
    assert_missing_and_null_are_equal(&file, "/changes/0/kind", "move_path");

    let mcp = json!({
        "type": "mcpToolCall", "id": "mcp-optionals", "server": "server", "tool": "tool",
        "status": "completed", "arguments": {},
        "appContext": {
            "connectorId": "connector", "linkId": "link", "resourceUri": "file:///a",
            "appName": "app", "templateId": "template", "actionName": "action"
        },
        "mcpAppResourceUri": "file:///deprecated", "pluginId": "plugin",
        "result": {"content": [], "structuredContent": {}, "_meta": {}},
        "error": {"message": "failure"}, "durationMs": 3
    });
    for field in [
        "appContext",
        "mcpAppResourceUri",
        "pluginId",
        "result",
        "error",
        "durationMs",
    ] {
        assert_missing_and_null_are_equal(&mcp, "", field);
    }
    for field in [
        "linkId",
        "resourceUri",
        "appName",
        "templateId",
        "actionName",
    ] {
        assert_missing_and_null_are_equal(&mcp, "/appContext", field);
    }
    for field in ["structuredContent", "_meta"] {
        assert_missing_and_null_are_equal(&mcp, "/result", field);
    }

    let dynamic = json!({
        "type": "dynamicToolCall", "id": "dynamic-optionals", "namespace": "beryl", "tool": "tool",
        "arguments": {}, "status": "completed",
        "contentItems": [{"type": "inputText", "text": "answer"}], "success": true, "durationMs": 4
    });
    for field in ["namespace", "contentItems", "success", "durationMs"] {
        assert_missing_and_null_are_equal(&dynamic, "", field);
    }

    let collab = json!({
        "type": "collabAgentToolCall", "id": "collab-optionals", "tool": "spawnAgent", "status": "completed",
        "senderThreadId": "parent", "receiverThreadIds": ["child"],
        "prompt": "prompt", "model": "model", "reasoningEffort": "high",
        "agentsStates": {"child": {"status": "completed", "message": "done"}}
    });
    for field in ["prompt", "model", "reasoningEffort"] {
        assert_missing_and_null_are_equal(&collab, "", field);
    }
    assert_missing_and_null_are_equal(&collab, "/agentsStates/child", "message");
}

#[test]
fn auxiliary_optional_fields_treat_missing_and_null_as_the_same_absence() {
    let web_search = json!({
        "type": "webSearch", "id": "web-optionals", "query": "query",
        "action": {"type": "search", "query": "one", "queries": ["one", "two"]}
    });
    assert_missing_and_null_are_equal(&web_search, "", "action");
    assert_missing_and_null_are_equal(&web_search, "/action", "query");
    assert_missing_and_null_are_equal(&web_search, "/action", "queries");

    let open_page = json!({
        "type": "webSearch", "id": "web-open-optional", "query": "query",
        "action": {"type": "openPage", "url": "https://example.invalid"}
    });
    assert_missing_and_null_are_equal(&open_page, "/action", "url");

    let find_in_page = json!({
        "type": "webSearch", "id": "web-find-optionals", "query": "query",
        "action": {"type": "findInPage", "url": "https://example.invalid", "pattern": "proof"}
    });
    assert_missing_and_null_are_equal(&find_in_page, "/action", "url");
    assert_missing_and_null_are_equal(&find_in_page, "/action", "pattern");

    let image = json!({
        "type": "imageGeneration", "id": "image-optionals", "status": "completed",
        "revisedPrompt": "revised", "result": "data:image/png;base64,AA==",
        "savedPath": "/runtime/generated.png"
    });
    assert_missing_and_null_are_equal(&image, "", "revisedPrompt");
    assert_missing_and_null_are_equal(&image, "", "savedPath");
}

fn completed_item(item: Value) -> ThreadItem {
    let event = parse_turn_stream_event(
        "item/completed",
        Some(json!({
            "threadId": "thread-large",
            "turnId": "turn-large",
            "completedAtMs": 1_752_689_600_123_i64,
            "item": item
        })),
    )
    .expect("large lifecycle event must parse")
    .expect("large lifecycle event must be recognized");
    let TurnStreamEvent::ItemCompleted {
        thread_id,
        turn_id,
        item,
        ..
    } = event
    else {
        panic!("expected completed item");
    };
    assert_eq!(thread_id, CasThreadId::new("thread-large").unwrap());
    assert_eq!(turn_id, CasTurnId::new("turn-large").unwrap());
    item
}

#[test]
fn large_public_payloads_survive_codec_and_event_normalization_without_truncation() {
    let large_text = "phase13-public-payload-".repeat(100_000);
    assert!(large_text.len() > 2_000_000);

    let dynamic = json!({
        "type": "dynamicToolCall",
        "id": "dynamic-large",
        "namespace": "beryl",
        "tool": "large",
        "arguments": {"nested": [large_text.clone(), {"leaf": large_text.clone()}]},
        "status": "completed",
        "contentItems": [{"type": "inputText", "text": large_text.clone()}],
        "success": true,
        "durationMs": 55
    });
    let expected_dynamic = ThreadItem::DynamicToolCall(DynamicToolCallItem {
        id: CasItemId::new("dynamic-large").unwrap(),
        namespace: Some("beryl".to_string()),
        tool: "large".to_string(),
        arguments: json!({"nested": [large_text.clone(), {"leaf": large_text.clone()}]}),
        status: DynamicToolCallStatus::Completed,
        content_items: Some(vec![DynamicToolCallOutputContentItem::InputText {
            text: large_text.clone(),
        }]),
        success: Some(true),
        duration_ms: Some(55),
    });
    assert_eq!(parse_item(dynamic.clone()), expected_dynamic);
    assert_eq!(completed_item(dynamic), expected_dynamic);

    let mcp = json!({
        "type": "mcpToolCall",
        "id": "mcp-large",
        "server": "large-server",
        "tool": "large-tool",
        "status": "completed",
        "arguments": {"leaf": large_text.clone()},
        "result": {
            "content": [{"type": "text", "text": large_text.clone()}],
            "structuredContent": {"leaf": large_text.clone()},
            "_meta": {"leaf": large_text.clone()}
        }
    });
    let expected_mcp = ThreadItem::McpToolCall(McpToolCallItem {
        id: CasItemId::new("mcp-large").unwrap(),
        server: "large-server".to_string(),
        tool: "large-tool".to_string(),
        status: McpToolCallStatus::Completed,
        arguments: json!({"leaf": large_text.clone()}),
        app_context: None,
        mcp_app_resource_uri: None,
        plugin_id: None,
        result: Some(Box::new(McpToolCallResult {
            content: vec![json!({"type": "text", "text": large_text.clone()})],
            structured_content: Some(json!({"leaf": large_text.clone()})),
            meta: Some(json!({"leaf": large_text.clone()})),
        })),
        error: None,
        duration_ms: None,
    });
    assert_eq!(parse_item(mcp.clone()), expected_mcp);
    assert_eq!(completed_item(mcp), expected_mcp);

    let generated_result = format!("data:image/png;base64,{large_text}");
    let image = json!({
        "type": "imageGeneration",
        "id": "image-large",
        "status": "completed",
        "revisedPrompt": large_text.clone(),
        "result": generated_result.clone(),
        "savedPath": "/runtime/generated/large.png"
    });
    let expected_image = ThreadItem::ImageGeneration(ImageGenerationItem {
        id: CasItemId::new("image-large").unwrap(),
        status: "completed".to_string(),
        revised_prompt: Some(large_text.clone()),
        saved_path: Some(PathBuf::from("/runtime/generated/large.png")),
    });
    assert_eq!(parse_item(image.clone()), expected_image);
    assert_eq!(completed_item(image), expected_image);

    let agent = json!({
        "type": "agentMessage", "id": "agent-large", "text": large_text.clone()
    });
    let expected_agent = ThreadItem::AgentMessage(AgentMessageItem {
        id: CasItemId::new("agent-large").unwrap(),
        text: large_text.clone(),
        phase: None,
        memory_citation: None,
    });
    assert_eq!(parse_item(agent.clone()), expected_agent);
    assert_eq!(completed_item(agent), expected_agent);

    let file = json!({
        "type": "fileChange", "id": "file-large", "status": "completed",
        "changes": [{"path": "large.txt", "diff": large_text.clone(), "kind": {"type": "update", "move_path": null}}]
    });
    let expected_file = ThreadItem::FileChange(FileChangeItem {
        id: CasItemId::new("file-large").unwrap(),
        status: PatchApplyStatus::Completed,
        changes: vec![FileUpdateChange {
            path: "large.txt".to_string(),
            diff: large_text,
            kind: PatchChangeKind::Update { move_path: None },
        }],
    });
    assert_eq!(parse_item(file.clone()), expected_file);
    assert_eq!(completed_item(file), expected_file);
}
