use std::{collections::BTreeMap, path::PathBuf};

use beryl_backend::{
    CollabAgentState, CollabAgentStatus, CollabAgentTool, CollabAgentToolCallItem,
    CollabAgentToolCallStatus, CommandAction, CommandExecutionItem, CommandExecutionSource,
    CommandExecutionStatus, DynamicToolCallItem, DynamicToolCallOutputContentItem,
    DynamicToolCallStatus, FileChangeItem, FileUpdateChange, ItemLifecycleTimestampMs,
    McpToolCallAppContext, McpToolCallError, McpToolCallItem, McpToolCallResult, McpToolCallStatus,
    PatchApplyStatus, PatchChangeKind, ThreadItem, TurnStreamEvent, parse_turn_stream_event,
};
use beryl_model::{CasItemId, CasThreadId, CasTurnId};
use serde_json::{Value, json};

const THREAD_ID: &str = "thread-phase13";
const TURN_ID: &str = "turn-phase13";
const TIMESTAMP_MS: u64 = 1_752_689_600_123;

fn item_id(value: &str) -> CasItemId {
    CasItemId::new(value).expect("fixture item id")
}

fn parse_item(value: Value) -> ThreadItem {
    serde_json::from_value(value).expect("pinned item fixture must parse")
}

fn assert_paired_item(item: Value, expected: ThreadItem) {
    assert_eq!(parse_item(item.clone()), expected);
    for (method, timestamp_field) in [
        ("item/started", "startedAtMs"),
        ("item/completed", "completedAtMs"),
    ] {
        let mut params = json!({
            "threadId": THREAD_ID,
            "turnId": TURN_ID,
            "item": item.clone()
        });
        params[timestamp_field] = json!(TIMESTAMP_MS);
        let actual = parse_turn_stream_event(method, Some(params))
            .expect("lifecycle notification must parse")
            .expect("lifecycle notification must be recognized");
        let expected_event = match method {
            "item/started" => TurnStreamEvent::ItemStarted {
                thread_id: CasThreadId::new(THREAD_ID).unwrap(),
                turn_id: CasTurnId::new(TURN_ID).unwrap(),
                started_at_ms: ItemLifecycleTimestampMs::new(TIMESTAMP_MS),
                item: expected.clone(),
            },
            "item/completed" => TurnStreamEvent::ItemCompleted {
                thread_id: CasThreadId::new(THREAD_ID).unwrap(),
                turn_id: CasTurnId::new(TURN_ID).unwrap(),
                completed_at_ms: ItemLifecycleTimestampMs::new(TIMESTAMP_MS),
                item: expected.clone(),
            },
            _ => unreachable!(),
        };
        assert_eq!(actual, expected_event);
    }
}

#[test]
fn command_and_file_items_preserve_every_public_field_and_nested_branch() {
    assert_paired_item(
        json!({
            "id": "command-rich",
            "type": "commandExecution",
            "command": "cargo nextest run",
            "cwd": "C:/repo",
            "processId": "pty-42",
            "source": "unifiedExecInteraction",
            "status": "completed",
            "commandActions": [
                {"type": "read", "command": "type Cargo.toml", "name": "Cargo.toml", "path": "C:/repo/Cargo.toml"},
                {"type": "listFiles", "command": "rg --files", "path": "C:/repo/src"},
                {"type": "search", "command": "rg proof", "query": "proof", "path": "C:/repo"},
                {"type": "unknown", "command": "custom operation"}
            ],
            "aggregatedOutput": "all tests passed\n",
            "exitCode": -7,
            "durationMs": 1234
        }),
        ThreadItem::CommandExecution(CommandExecutionItem {
            id: item_id("command-rich"),
            command: "cargo nextest run".to_string(),
            cwd: "C:/repo".to_string(),
            process_id: Some("pty-42".to_string()),
            source: CommandExecutionSource::UnifiedExecInteraction,
            status: CommandExecutionStatus::Completed,
            command_actions: vec![
                CommandAction::Read {
                    command: "type Cargo.toml".to_string(),
                    name: "Cargo.toml".to_string(),
                    path: PathBuf::from("C:/repo/Cargo.toml"),
                },
                CommandAction::ListFiles {
                    command: "rg --files".to_string(),
                    path: Some("C:/repo/src".to_string()),
                },
                CommandAction::Search {
                    command: "rg proof".to_string(),
                    query: Some("proof".to_string()),
                    path: Some("C:/repo".to_string()),
                },
                CommandAction::Unknown {
                    command: "custom operation".to_string(),
                },
            ],
            aggregated_output: Some("all tests passed\n".to_string()),
            exit_code: Some(-7),
            duration_ms: Some(1234),
        }),
    );

    assert_paired_item(
        json!({
            "id": "file-rich",
            "type": "fileChange",
            "status": "declined",
            "changes": [
                {"path": "src/added.rs", "diff": "+added", "kind": {"type": "add"}},
                {"path": "src/deleted.rs", "diff": "-deleted", "kind": {"type": "delete"}},
                {"path": "src/old.rs", "diff": "-old\n+new", "kind": {"type": "update", "move_path": "src/new.rs"}},
                {"path": "src/stable.rs", "diff": "+line", "kind": {"type": "update", "move_path": null}}
            ]
        }),
        ThreadItem::FileChange(FileChangeItem {
            id: item_id("file-rich"),
            status: PatchApplyStatus::Declined,
            changes: vec![
                FileUpdateChange {
                    path: "src/added.rs".to_string(),
                    diff: "+added".to_string(),
                    kind: PatchChangeKind::Add,
                },
                FileUpdateChange {
                    path: "src/deleted.rs".to_string(),
                    diff: "-deleted".to_string(),
                    kind: PatchChangeKind::Delete,
                },
                FileUpdateChange {
                    path: "src/old.rs".to_string(),
                    diff: "-old\n+new".to_string(),
                    kind: PatchChangeKind::Update {
                        move_path: Some("src/new.rs".to_string()),
                    },
                },
                FileUpdateChange {
                    path: "src/stable.rs".to_string(),
                    diff: "+line".to_string(),
                    kind: PatchChangeKind::Update { move_path: None },
                },
            ],
        }),
    );
}

#[test]
fn mcp_and_dynamic_items_preserve_every_public_field_and_structured_leaf() {
    assert_paired_item(
        json!({
            "id": "mcp-rich",
            "type": "mcpToolCall",
            "server": "filesystem",
            "tool": "read_many",
            "status": "failed",
            "arguments": {"paths": ["a", "b"], "options": {"binary": false}},
            "appContext": {
                "connectorId": "connector-7",
                "linkId": "link-8",
                "resourceUri": "file:///resource/new",
                "appName": "Files",
                "templateId": "template-9",
                "actionName": "Read many"
            },
            "mcpAppResourceUri": "file:///resource/deprecated",
            "pluginId": "plugin-10",
            "result": {
                "content": [
                    {"type": "text", "text": "first"},
                    {"type": "resource", "resource": {"uri": "file:///a", "text": "body"}}
                ],
                "structuredContent": {"count": 2, "ok": true, "missing": null},
                "_meta": {"trace": [1, 2, 3]}
            },
            "error": {"message": "partial failure"},
            "durationMs": 9876
        }),
        ThreadItem::McpToolCall(McpToolCallItem {
            id: item_id("mcp-rich"),
            server: "filesystem".to_string(),
            tool: "read_many".to_string(),
            status: McpToolCallStatus::Failed,
            arguments: json!({"paths": ["a", "b"], "options": {"binary": false}}),
            app_context: Some(McpToolCallAppContext {
                connector_id: "connector-7".to_string(),
                link_id: Some("link-8".to_string()),
                resource_uri: Some("file:///resource/new".to_string()),
                app_name: Some("Files".to_string()),
                template_id: Some("template-9".to_string()),
                action_name: Some("Read many".to_string()),
            }),
            mcp_app_resource_uri: Some("file:///resource/deprecated".to_string()),
            plugin_id: Some("plugin-10".to_string()),
            result: Some(Box::new(McpToolCallResult {
                content: vec![
                    json!({"type": "text", "text": "first"}),
                    json!({"type": "resource", "resource": {"uri": "file:///a", "text": "body"}}),
                ],
                structured_content: Some(json!({"count": 2, "ok": true, "missing": null})),
                meta: Some(json!({"trace": [1, 2, 3]})),
            })),
            error: Some(McpToolCallError {
                message: "partial failure".to_string(),
            }),
            duration_ms: Some(9876),
        }),
    );

    assert_paired_item(
        json!({
            "id": "dynamic-rich",
            "type": "dynamicToolCall",
            "namespace": "beryl",
            "tool": "inspect",
            "arguments": {"depth": 4, "tags": ["a", "b"]},
            "status": "completed",
            "contentItems": [
                {"type": "inputText", "text": "structured answer"},
                {"type": "inputImage", "imageUrl": "data:image/png;base64,AQ=="}
            ],
            "success": false,
            "durationMs": 4567
        }),
        ThreadItem::DynamicToolCall(DynamicToolCallItem {
            id: item_id("dynamic-rich"),
            namespace: Some("beryl".to_string()),
            tool: "inspect".to_string(),
            arguments: json!({"depth": 4, "tags": ["a", "b"]}),
            status: DynamicToolCallStatus::Completed,
            content_items: Some(vec![
                DynamicToolCallOutputContentItem::InputText {
                    text: "structured answer".to_string(),
                },
                DynamicToolCallOutputContentItem::InputImage {
                    image_url: "data:image/png;base64,AQ==".to_string(),
                },
            ]),
            success: Some(false),
            duration_ms: Some(4567),
        }),
    );
}

#[test]
fn collaboration_item_preserves_every_public_field_and_agent_status_branch() {
    let agents_states = BTreeMap::from([
        (
            "agent-completed".to_string(),
            CollabAgentState {
                status: CollabAgentStatus::Completed,
                message: Some("done".to_string()),
            },
        ),
        (
            "agent-errored".to_string(),
            CollabAgentState {
                status: CollabAgentStatus::Errored,
                message: Some("boom".to_string()),
            },
        ),
        (
            "agent-interrupted".to_string(),
            CollabAgentState {
                status: CollabAgentStatus::Interrupted,
                message: None,
            },
        ),
        (
            "agent-not-found".to_string(),
            CollabAgentState {
                status: CollabAgentStatus::NotFound,
                message: None,
            },
        ),
        (
            "agent-pending".to_string(),
            CollabAgentState {
                status: CollabAgentStatus::PendingInit,
                message: None,
            },
        ),
        (
            "agent-running".to_string(),
            CollabAgentState {
                status: CollabAgentStatus::Running,
                message: None,
            },
        ),
        (
            "agent-shutdown".to_string(),
            CollabAgentState {
                status: CollabAgentStatus::Shutdown,
                message: None,
            },
        ),
    ]);
    assert_paired_item(
        json!({
            "id": "collab-rich",
            "type": "collabAgentToolCall",
            "tool": "spawnAgent",
            "status": "inProgress",
            "senderThreadId": "thread-parent",
            "receiverThreadIds": ["thread-child-a", "thread-child-b"],
            "prompt": "inspect the protocol",
            "model": "gpt-proof",
            "reasoningEffort": "high",
            "agentsStates": {
                "agent-pending": {"status": "pendingInit", "message": null},
                "agent-running": {"status": "running", "message": null},
                "agent-interrupted": {"status": "interrupted", "message": null},
                "agent-completed": {"status": "completed", "message": "done"},
                "agent-errored": {"status": "errored", "message": "boom"},
                "agent-shutdown": {"status": "shutdown", "message": null},
                "agent-not-found": {"status": "notFound", "message": null}
            }
        }),
        ThreadItem::CollabAgentToolCall(CollabAgentToolCallItem {
            id: item_id("collab-rich"),
            tool: CollabAgentTool::SpawnAgent,
            status: CollabAgentToolCallStatus::InProgress,
            sender_thread_id: CasThreadId::new("thread-parent").unwrap(),
            receiver_thread_ids: vec![
                CasThreadId::new("thread-child-a").unwrap(),
                CasThreadId::new("thread-child-b").unwrap(),
            ],
            prompt: Some("inspect the protocol".to_string()),
            model: Some("gpt-proof".to_string()),
            reasoning_effort: Some("high".to_string()),
            agents_states,
        }),
    );
}

fn minimal_command(source: &str, status: &str) -> Value {
    json!({
        "id": format!("command-{source}-{status}"),
        "type": "commandExecution",
        "command": "proof",
        "cwd": "C:/repo",
        "source": source,
        "status": status,
        "commandActions": []
    })
}

#[test]
fn every_operational_status_and_source_enum_branch_is_typed() {
    for (wire, expected) in [
        ("agent", CommandExecutionSource::Agent),
        ("userShell", CommandExecutionSource::UserShell),
        (
            "unifiedExecStartup",
            CommandExecutionSource::UnifiedExecStartup,
        ),
        (
            "unifiedExecInteraction",
            CommandExecutionSource::UnifiedExecInteraction,
        ),
    ] {
        let ThreadItem::CommandExecution(item) = parse_item(minimal_command(wire, "inProgress"))
        else {
            panic!("expected command item");
        };
        assert_eq!(item.source, expected);
    }

    for (wire, expected) in [
        ("inProgress", CommandExecutionStatus::InProgress),
        ("completed", CommandExecutionStatus::Completed),
        ("failed", CommandExecutionStatus::Failed),
        ("declined", CommandExecutionStatus::Declined),
    ] {
        let ThreadItem::CommandExecution(item) = parse_item(minimal_command("agent", wire)) else {
            panic!("expected command item");
        };
        assert_eq!(item.status, expected);
    }

    for (wire, expected) in [
        ("inProgress", PatchApplyStatus::InProgress),
        ("completed", PatchApplyStatus::Completed),
        ("failed", PatchApplyStatus::Failed),
        ("declined", PatchApplyStatus::Declined),
    ] {
        let ThreadItem::FileChange(item) = parse_item(json!({
            "id": format!("file-{wire}"),
            "type": "fileChange",
            "status": wire,
            "changes": []
        })) else {
            panic!("expected file-change item");
        };
        assert_eq!(item.status, expected);
    }

    for (wire, expected) in [
        ("inProgress", McpToolCallStatus::InProgress),
        ("completed", McpToolCallStatus::Completed),
        ("failed", McpToolCallStatus::Failed),
    ] {
        let ThreadItem::McpToolCall(item) = parse_item(json!({
            "id": format!("mcp-{wire}"),
            "type": "mcpToolCall",
            "server": "server",
            "tool": "tool",
            "status": wire,
            "arguments": {}
        })) else {
            panic!("expected MCP item");
        };
        assert_eq!(item.status, expected);
    }

    for (wire, expected) in [
        ("inProgress", DynamicToolCallStatus::InProgress),
        ("completed", DynamicToolCallStatus::Completed),
        ("failed", DynamicToolCallStatus::Failed),
    ] {
        let ThreadItem::DynamicToolCall(item) = parse_item(json!({
            "id": format!("dynamic-{wire}"),
            "type": "dynamicToolCall",
            "tool": "tool",
            "status": wire,
            "arguments": {}
        })) else {
            panic!("expected dynamic-tool item");
        };
        assert_eq!(item.status, expected);
    }
}

fn minimal_collaboration(tool: &str, status: &str) -> ThreadItem {
    parse_item(json!({
        "id": format!("collab-{tool}-{status}"),
        "type": "collabAgentToolCall",
        "tool": tool,
        "status": status,
        "senderThreadId": "thread-parent",
        "receiverThreadIds": [],
        "agentsStates": {}
    }))
}

#[test]
fn every_collaboration_tool_and_call_status_branch_is_typed() {
    for (wire, expected) in [
        ("spawnAgent", CollabAgentTool::SpawnAgent),
        ("sendInput", CollabAgentTool::SendInput),
        ("resumeAgent", CollabAgentTool::ResumeAgent),
        ("wait", CollabAgentTool::Wait),
        ("closeAgent", CollabAgentTool::CloseAgent),
    ] {
        let ThreadItem::CollabAgentToolCall(item) = minimal_collaboration(wire, "inProgress")
        else {
            panic!("expected collaboration item");
        };
        assert_eq!(item.tool, expected);
    }

    for (wire, expected) in [
        ("inProgress", CollabAgentToolCallStatus::InProgress),
        ("completed", CollabAgentToolCallStatus::Completed),
        ("failed", CollabAgentToolCallStatus::Failed),
    ] {
        let ThreadItem::CollabAgentToolCall(item) = minimal_collaboration("wait", wire) else {
            panic!("expected collaboration item");
        };
        assert_eq!(item.status, expected);
    }
}
