use std::{
    net::{TcpListener, TcpStream},
    path::PathBuf,
    thread,
    time::Duration,
};

use beryl_backend::{
    BackendWebSocketEndpoint, CompatibilityError, CompatibilityProbe, CompatibilitySnapshot,
    ConfigReadOptions, ConfigReadResponse, DynamicToolCallResponse, DynamicToolFunctionSpec,
    DynamicToolNamespaceSpec, HardStopCapabilityProbe, HardStopTarget, HardStopTargetOutcome,
    InitializeResponse, ManagedBackendClientOptions, ManagedBackendError, ManagedBackendSession,
    ManagedWebSocketError, ModelListOptions, ModelListResponse, NonIdempotentRequestOutcome,
    NonSteerableTurnKind, REQUIRED_CODEX_APP_SERVER_VERSION, ThreadBranchCapabilityProbe,
    ThreadLoadOptions, ThreadStartOptions, ThreadStatus, TurnStartOptions, TurnStreamEvent,
    UserInput, active_turn_not_steerable_error,
};
use beryl_model::{CasThreadId, CasTurnId};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, accept_hdr};

fn cas_thread_id(value: &str) -> CasThreadId {
    CasThreadId::new(value).unwrap()
}

fn cas_turn_id(value: &str) -> CasTurnId {
    CasTurnId::new(value).unwrap()
}

fn exact_response<T: std::fmt::Debug>(outcome: NonIdempotentRequestOutcome<T>) -> T {
    match outcome {
        NonIdempotentRequestOutcome::ExactResponse { response } => response,
        other => panic!("expected exact backend response, got {other:?}"),
    }
}

#[test]
fn websocket_transport_error_display_includes_source_detail() {
    let error = ManagedBackendError::WebSocketTransport {
        method: "thread/read".to_string(),
        endpoint: "ws://127.0.0.1:49154".to_string(),
        source: ManagedWebSocketError::protocol("message too large"),
    };

    let display = error.to_string();

    assert!(display.contains("thread/read"));
    assert!(display.contains("message too large"));
}

#[test]
fn websocket_client_initializes_routes_responses_and_buffers_notifications() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("config/read"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "method": "thread/name/updated",
                    "params": {
                        "threadId": "thread_123",
                        "threadName": "Buffered title"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                    "config": {}
                }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .read_config(&PathBuf::from(r"C:\work\beryl"), Duration::from_secs(2))
        .unwrap();
    assert_eq!(response.config.model, None);

    let event = client
        .next_turn_stream_event(Duration::from_millis(10))
        .unwrap()
        .unwrap();
    assert_eq!(
        event,
        TurnStreamEvent::ThreadNameUpdated {
            thread_id: "thread_123".to_string(),
            thread_name: Some("Buffered title".to_string())
        }
    );

    server.join().unwrap();
}

#[test]
fn websocket_client_reads_large_single_frame_response() {
    const LARGE_PADDING_BYTES: usize = 17 * 1024 * 1024;

    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("model/list"));
        assert_eq!(request["params"], json!({ "limit": 1 }));

        let large_padding = "A".repeat(LARGE_PADDING_BYTES);
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "data": [],
                        "padding": large_padding
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let models = client
        .list_model_page(&ModelListOptions::page(1), Duration::from_secs(10))
        .unwrap();
    assert!(models.data.is_empty());

    server.join().unwrap();
}

#[test]
fn websocket_turn_start_serializes_ordered_user_input() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/start"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_1",
                "input": [
                    {
                        "type": "text",
                        "text": "First fragment"
                    },
                    {
                        "type": "text",
                        "text": "Second fragment"
                    }
                ],
                "model": "gpt-5.5",
                "effort": "high"
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "turn": {
                            "id": "turn_1",
                            "items": {
                                "poison": "turn/start response items must not be materialized"
                            },
                            "status": "inProgress"
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = exact_response(
        client.start_turn_with_user_input_options(
            &cas_thread_id("thread_1"),
            vec![
                UserInput::text("First fragment"),
                UserInput::text("Second fragment"),
            ],
            TurnStartOptions::default()
                .with_model("gpt-5.5")
                .with_reasoning_effort("high"),
            Duration::from_secs(2),
        ),
    );

    assert_eq!(response.turn_id().as_str(), "turn_1");
    server.join().unwrap();
}

#[test]
fn websocket_turn_start_serializes_hidden_developer_instructions_context() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/start"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_1",
                "input": [
                    {
                        "type": "text",
                        "text": "Follow up"
                    }
                ],
                "collaborationMode": {
                    "mode": "default",
                    "settings": {
                        "model": "gpt-5.5",
                        "reasoning_effort": "high",
                        "developer_instructions": "Use the operator's project rules."
                    }
                }
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "turn": {
                            "id": "turn_1",
                            "items": [],
                            "status": "inProgress"
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = exact_response(client.start_turn_with_user_input_options(
        &cas_thread_id("thread_1"),
        vec![UserInput::text("Follow up")],
        TurnStartOptions::default().with_developer_instructions_context(
            Some("Use the operator's project rules.".to_string()),
            "gpt-5.5",
            Some("high".to_string()),
        ),
        Duration::from_secs(2),
    ));

    assert_eq!(response.turn_id().as_str(), "turn_1");
    server.join().unwrap();
}

#[test]
fn websocket_turn_start_serializes_disabled_developer_instructions_as_hidden_reset() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/start"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_1",
                "input": [
                    {
                        "type": "text",
                        "text": "Follow up"
                    }
                ],
                "collaborationMode": {
                    "mode": "default",
                    "settings": {
                        "model": "gpt-5.5",
                        "developer_instructions": null
                    }
                }
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "turn": {
                            "id": "turn_1",
                            "items": [],
                            "status": "inProgress"
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = exact_response(client.start_turn_with_user_input_options(
        &cas_thread_id("thread_1"),
        vec![UserInput::text("Follow up")],
        TurnStartOptions::default().with_developer_instructions_context(None, "gpt-5.5", None),
        Duration::from_secs(2),
    ));

    assert_eq!(response.turn_id().as_str(), "turn_1");
    server.join().unwrap();
}

#[test]
fn websocket_thread_start_serializes_dynamic_tools_and_developer_instructions() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/start"));
        assert_eq!(
            request["params"],
            json!({
                "cwd": "C:\\work\\beryl",
                "ephemeral": false,
                "developerInstructions": "Use project-specific review instructions.",
                "model": "gpt-5.5",
                "modelProvider": "openai",
                "approvalPolicy": "on-request",
                "sandbox": "workspace-write",
                "dynamicTools": [
                    {
                        "type": "namespace",
                        "name": "beryl",
                        "description": "Beryl-owned tools.",
                        "tools": [
                            {
                                "type": "function",
                                "name": "inspect_runtime_state",
                                "description": "Inspect bounded runtime state.",
                                "inputSchema": {
                                    "type": "object",
                                    "required": ["ops"],
                                    "properties": {
                                        "ops": {
                                            "type": "array"
                                        }
                                    }
                                },
                                "deferLoading": true
                            }
                        ]
                    },
                    {
                        "type": "namespace",
                        "name": "beryl_diagnostic",
                        "description": "Beryl diagnostic-child tools.",
                        "tools": [
                            {
                                "type": "function",
                                "name": "status",
                                "description": "Read diagnostic child process lifecycle status.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {},
                                    "additionalProperties": false
                                },
                                "deferLoading": false
                            }
                        ]
                    }
                ]
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "thread": {
                            "cliVersion": "0.128.0",
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_1",
                            "modelProvider": "openai",
                            "preview": "",
                            "source": "appServer",
                            "status": {
                                "type": "idle"
                            },
                            "turns": [],
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .start_thread_with_options(
            &PathBuf::from(r"C:\work\beryl"),
            ThreadStartOptions::persistent()
                .with_developer_instructions("Use project-specific review instructions.")
                .with_model("gpt-5.5")
                .with_model_provider("openai")
                .with_approval_policy(beryl_backend::ThreadApprovalPolicy::OnRequest)
                .with_sandbox(beryl_backend::ThreadSandboxMode::WorkspaceWrite)
                .with_dynamic_tool(
                    DynamicToolNamespaceSpec::new(
                        "beryl",
                        "Beryl-owned tools.",
                        vec![
                            DynamicToolFunctionSpec::new(
                                "inspect_runtime_state",
                                "Inspect bounded runtime state.",
                                json!({
                                    "type": "object",
                                    "required": ["ops"],
                                    "properties": {
                                        "ops": {
                                            "type": "array"
                                        }
                                    }
                                }),
                            )
                            .with_defer_loading(true),
                        ],
                    )
                    .into(),
                )
                .with_dynamic_tool(
                    DynamicToolNamespaceSpec::new(
                        "beryl_diagnostic",
                        "Beryl diagnostic-child tools.",
                        vec![
                            DynamicToolFunctionSpec::new(
                                "status",
                                "Read diagnostic child process lifecycle status.",
                                json!({
                                    "type": "object",
                                    "properties": {},
                                    "additionalProperties": false
                                }),
                            )
                            .with_defer_loading(false),
                        ],
                    )
                    .into(),
                ),
            Duration::from_secs(2),
        )
        .unwrap();

    assert_eq!(response.thread_id().as_str(), "thread_1");
    server.join().unwrap();
}

#[test]
fn websocket_thread_start_omits_developer_instructions_when_unset() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/start"));
        assert_eq!(
            request["params"],
            json!({
                "cwd": "C:\\work\\beryl",
                "ephemeral": false
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "thread": {
                            "cliVersion": "0.128.0",
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_1",
                            "modelProvider": "openai",
                            "preview": "",
                            "source": "appServer",
                            "status": {
                                "type": "idle"
                            },
                            "turns": [],
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .start_thread(&PathBuf::from(r"C:\work\beryl"), Duration::from_secs(2))
        .unwrap();

    assert_eq!(response.thread_id().as_str(), "thread_1");
    server.join().unwrap();
}

#[test]
fn websocket_thread_fork_and_rollback_use_observed_branch_protocol() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/fork"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_source",
                "cwd": "C:\\work\\beryl",
                "excludeTurns": true,
                "ephemeral": false
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "model": "gpt-5.5",
                        "modelProvider": "openai",
                        "reasoningEffort": "high",
                        "thread": {
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_branch",
                            "modelProvider": "openai",
                            "preview": "First request",
                            "status": {
                                "type": "idle"
                            },
                            "turns": [
                                {
                                    "id": "turn_1",
                                    "items": [],
                                    "status": "completed"
                                },
                                {
                                    "id": "turn_2",
                                    "items": [],
                                    "status": "completed"
                                }
                            ],
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(3));
        assert_eq!(request["method"], json!("thread/rollback"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_branch",
                "numTurns": 1
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "result": {
                        "thread": {
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_branch",
                            "modelProvider": "openai",
                            "preview": "First request",
                            "status": {
                                "type": "idle"
                            },
                            "turns": [
                                {
                                    "id": "turn_1",
                                    "items": [],
                                    "status": "completed"
                                }
                            ],
                            "updatedAt": 3
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let source_thread_id = CasThreadId::new("thread_source").unwrap();
    let branch_thread_id = CasThreadId::new("thread_branch").unwrap();
    let load_options = ThreadLoadOptions::for_root(r"C:\work\beryl");
    let fork = client
        .fork_thread(&source_thread_id, &load_options, Duration::from_secs(2))
        .unwrap();
    assert_eq!(fork.thread_id().as_str(), "thread_branch");
    assert_eq!(fork.status(), &ThreadStatus::Idle);
    assert_eq!(fork.metadata().model.as_deref(), Some("gpt-5.5"));
    assert_eq!(fork.metadata().reasoning_effort.as_deref(), Some("high"));

    let rollback = client
        .rollback_thread(&branch_thread_id, 1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(rollback.thread_id().as_str(), "thread_branch");
    assert_eq!(rollback.status(), &ThreadStatus::Idle);
    server.join().unwrap();
}

#[test]
fn websocket_thread_fork_through_turn_sets_exact_lineage_boundary() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/fork"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_source",
                "cwd": "C:\\work\\beryl",
                "excludeTurns": true,
                "ephemeral": false,
                "lastTurnId": "turn_1"
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "model": "gpt-5.5",
                        "modelProvider": "openai",
                        "thread": {
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_branch",
                            "modelProvider": "openai",
                            "preview": "First request",
                            "status": {
                                "type": "idle"
                            },
                            "turns": [],
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let source_thread_id = CasThreadId::new("thread_source").unwrap();
    let last_turn_id = CasTurnId::new("turn_1").unwrap();
    let load_options = ThreadLoadOptions::for_root(r"C:\work\beryl");
    let fork = client
        .fork_thread_through_turn(
            &source_thread_id,
            &last_turn_id,
            &load_options,
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(fork.thread_id().as_str(), "thread_branch");
    server.join().unwrap();
}

#[test]
fn websocket_dynamic_tool_call_request_streams_and_response_serializes() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": "request_1",
                    "method": "item/tool/call",
                    "params": {
                        "threadId": "thread_1",
                        "turnId": "turn_1",
                        "callId": "call_1",
                        "namespace": "beryl",
                        "tool": "read_document_outline",
                        "arguments": {
                            "nodeId": "node_1"
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let response = read_json(&mut socket);
        assert_eq!(
            response,
            json!({
                "jsonrpc": "2.0",
                "id": "request_1",
                "result": {
                    "success": true,
                    "contentItems": [
                        {
                            "type": "inputText",
                            "text": "{\"ok\":true}"
                        }
                    ]
                }
            })
        );
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let event = client
        .next_turn_stream_event(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    let TurnStreamEvent::DynamicToolCallRequested(request) = event else {
        panic!("expected dynamic tool call request");
    };
    assert_eq!(request.thread_id(), "thread_1");
    assert_eq!(request.turn_id(), "turn_1");
    assert_eq!(request.call_id(), "call_1");
    assert_eq!(request.namespace(), Some("beryl"));
    assert_eq!(request.tool(), "read_document_outline");
    assert_eq!(request.arguments(), &json!({ "nodeId": "node_1" }));

    client
        .respond_dynamic_tool_call(
            &request,
            &DynamicToolCallResponse::success_text("{\"ok\":true}"),
        )
        .unwrap();

    server.join().unwrap();
}

#[test]
fn websocket_dynamic_tool_call_request_defers_while_waiting_for_response() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("config/read"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": "request_1",
                    "method": "item/tool/call",
                    "params": {
                        "threadId": "thread_1",
                        "turnId": "turn_1",
                        "callId": "call_1",
                        "tool": "read_runtime_summary",
                        "arguments": {}
                    }
                })
                .to_string(),
            ))
            .unwrap();
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                    "config": {}
                }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .read_config(&PathBuf::from(r"C:\work\beryl"), Duration::from_secs(2))
        .unwrap();
    assert_eq!(response.config.model, None);

    let event = client
        .next_turn_stream_event(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    let TurnStreamEvent::DynamicToolCallRequested(request) = event else {
        panic!("expected deferred dynamic tool call request");
    };
    assert_eq!(request.tool(), "read_runtime_summary");

    server.join().unwrap();
}

#[test]
fn websocket_notification_defers_while_waiting_for_response() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("config/read"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "method": "thread/status/changed",
                    "params": {
                        "threadId": "thread_1",
                        "status": { "type": "active", "activeFlags": [] }
                    }
                })
                .to_string(),
            ))
            .unwrap();
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                    "config": {}
                }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .read_config(&PathBuf::from(r"C:\work\beryl"), Duration::from_secs(2))
        .unwrap();
    assert_eq!(response.config.model, None);

    let event = client
        .next_turn_stream_event(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(
        event,
        TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_1".to_string(),
            status: ThreadStatus::Active {
                active_flags: Vec::new()
            }
        }
    );

    server.join().unwrap();
}

#[test]
fn websocket_turn_steer_serializes_expected_turn_and_ordered_user_input() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/steer"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_1",
                "expectedTurnId": "turn_1",
                "input": [
                    {
                        "type": "text",
                        "text": "First steering fragment"
                    },
                    {
                        "type": "text",
                        "text": "Second steering fragment"
                    }
                ]
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "turnId": "turn_1"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = exact_response(client.steer_turn_with_user_input(
        &cas_thread_id("thread_1"),
        &cas_turn_id("turn_1"),
        vec![
            UserInput::text("First steering fragment"),
            UserInput::text("Second steering fragment"),
        ],
        Duration::from_secs(2),
    ));

    assert_eq!(response.turn_id().as_str(), "turn_1");
    server.join().unwrap();
}

#[test]
fn websocket_hard_stop_requests_serialize_exact_backend_handles() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/interrupt"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_parent",
                "turnId": "turn_parent"
            })
        );
        socket
            .send(Message::text(
                json!({ "jsonrpc": "2.0", "id": 2, "result": {} }).to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(3));
        assert_eq!(request["method"], json!("command/exec/terminate"));
        assert_eq!(
            request["params"],
            json!({
                "processId": "proc_123"
            })
        );
        socket
            .send(Message::text(
                json!({ "jsonrpc": "2.0", "id": 3, "result": {} }).to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(4));
        assert_eq!(request["method"], json!("thread/backgroundTerminals/clean"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_parent"
            })
        );
        socket
            .send(Message::text(
                json!({ "jsonrpc": "2.0", "id": 4, "result": {} }).to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    client
        .interrupt_turn(
            &cas_thread_id("thread_parent"),
            &cas_turn_id("turn_parent"),
            Duration::from_secs(2),
        )
        .unwrap();
    client
        .terminate_command_execution("proc_123", Duration::from_secs(2))
        .unwrap();
    client
        .clean_thread_background_terminals(&cas_thread_id("thread_parent"), Duration::from_secs(2))
        .unwrap();
    server.join().unwrap();
}

#[test]
fn websocket_hard_stop_turn_target_interrupts_exact_child_turn() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("turn/interrupt"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_child",
                "turnId": "turn_child"
            })
        );
        socket
            .send(Message::text(
                json!({ "jsonrpc": "2.0", "id": 2, "result": {} }).to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let outcome = client.request_hard_stop_target(
        &HardStopTarget::turn(cas_thread_id("thread_child"), cas_turn_id("turn_child")),
        Duration::from_secs(2),
    );
    assert!(outcome.is_success());
    server.join().unwrap();
}

#[test]
fn websocket_hard_stop_target_outcome_preserves_failed_target() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("command/exec/terminate"));
        assert_eq!(request["params"], json!({ "processId": "proc_missing" }));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": {
                        "code": -32000,
                        "message": "command exec process not found"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let target = HardStopTarget::command_execution("proc_missing");
    let outcome = client.request_hard_stop_target(&target, Duration::from_secs(2));
    let HardStopTargetOutcome::Failed {
        target,
        method,
        message,
    } = outcome
    else {
        panic!("expected failed hard-stop target outcome");
    };

    assert_eq!(target, HardStopTarget::command_execution("proc_missing"));
    assert_eq!(method, "command/exec/terminate");
    assert!(message.contains("command exec process not found"));
    server.join().unwrap();
}

#[test]
fn websocket_hard_stop_capability_probe_reports_optional_method_support() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("command/exec/terminate"));
        assert_eq!(
            request["params"],
            json!({
                "processId": "beryl-hard-stop-probe"
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": {
                        "code": -32000,
                        "message": "command exec process not found"
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(3));
        assert_eq!(request["method"], json!("thread/backgroundTerminals/clean"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "error": {
                        "code": -32601,
                        "message": "method not found"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let report = client
        .probe_hard_stop_capabilities(Duration::from_secs(2))
        .unwrap();
    assert_eq!(report.probe_results().len(), 2);
    assert_eq!(
        report.probe_results()[0].probe(),
        HardStopCapabilityProbe::CommandExecTerminate
    );
    assert!(report.probe_results()[0].supported());
    assert_eq!(
        report.probe_results()[1].probe(),
        HardStopCapabilityProbe::ThreadBackgroundTerminalsClean
    );
    assert!(!report.probe_results()[1].supported());
    assert!(report.capabilities().command_exec_terminate());
    assert!(!report.capabilities().thread_background_terminals_clean());
    server.join().unwrap();
}

#[test]
fn websocket_thread_branch_capability_probe_reports_optional_method_support() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/fork"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "00000000-0000-0000-0000-000000000000",
                "cwd": "C:\\work\\beryl",
                "excludeTurns": true,
                "ephemeral": false
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": {
                        "code": -32600,
                        "message": "no rollout found for thread id"
                    }
                })
                .to_string(),
            ))
            .unwrap();

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(3));
        assert_eq!(request["method"], json!("thread/rollback"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "00000000-0000-0000-0000-000000000000",
                "numTurns": 1
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "error": {
                        "code": -32601,
                        "message": "method not found"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let report = client
        .probe_thread_branch_capabilities(&PathBuf::from(r"C:\work\beryl"), Duration::from_secs(2))
        .unwrap();
    assert_eq!(report.probe_results().len(), 2);
    assert_eq!(
        report.probe_results()[0].probe(),
        ThreadBranchCapabilityProbe::ThreadFork
    );
    assert!(report.probe_results()[0].supported());
    assert_eq!(
        report.probe_results()[1].probe(),
        ThreadBranchCapabilityProbe::ThreadRollback
    );
    assert!(!report.probe_results()[1].supported());
    assert!(report.capabilities().thread_fork());
    assert!(!report.capabilities().thread_rollback());
    assert!(!report.capabilities().thread_branching());
    server.join().unwrap();
}

#[test]
fn websocket_turn_steer_preserves_non_steerable_request_error() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("turn/steer"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "error": {
                        "code": -32000,
                        "message": "active turn cannot be steered",
                        "data": {
                            "codexErrorInfo": {
                                "activeTurnNotSteerable": {
                                    "turnKind": "review"
                                }
                            }
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });
    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let outcome = client.steer_turn_with_user_input(
        &cas_thread_id("thread_1"),
        &cas_turn_id("turn_1"),
        vec![UserInput::text("Steer this")],
        Duration::from_secs(2),
    );

    let NonIdempotentRequestOutcome::ExactRejection { error } = outcome else {
        panic!("expected exact turn/steer rejection");
    };
    assert_eq!(error.code, -32000);
    assert_eq!(error.message, "active turn cannot be steered");
    assert_eq!(
        active_turn_not_steerable_error(&error).unwrap().turn_kind,
        NonSteerableTurnKind::Review
    );
    server.join().unwrap();
}

#[test]
fn websocket_clients_initialize_independently_and_start_request_ids_at_one() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = accept_hdr(
                stream,
                |request: &tungstenite::handshake::server::Request, response| {
                    assert_eq!(
                        request
                            .headers()
                            .get("authorization")
                            .unwrap()
                            .to_str()
                            .unwrap(),
                        "Bearer test-token"
                    );
                    Ok(response)
                },
            )
            .unwrap();
            expect_initialize(&mut socket, 1);
            expect_initialized(&mut socket);
            let request = read_json(&mut socket);
            assert_eq!(request["id"], json!(2));
            assert_eq!(request["method"], json!("model/list"));
            socket
                .send(Message::text(
                    json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "result": {
                            "data": []
                        }
                    })
                    .to_string(),
                ))
                .unwrap();
        }
    });

    for _ in 0..2 {
        let mut client = ManagedBackendSession::connect_websocket(
            endpoint.clone(),
            "Bearer test-token".to_string(),
            Duration::from_secs(2),
        )
        .unwrap();
        let models = client
            .list_model_page(&ModelListOptions::page(1), Duration::from_secs(2))
            .unwrap();
        assert!(models.data.is_empty());
    }

    server.join().unwrap();
}

#[test]
fn websocket_request_only_client_initializes_with_notification_opt_outs() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        let request = read_json(&mut socket);
        assert_eq!(request["jsonrpc"], json!("2.0"));
        assert_eq!(request["id"], json!(1));
        assert_eq!(request["method"], json!("initialize"));
        assert_eq!(
            request["params"]["capabilities"]["experimentalApi"],
            json!(true)
        );

        let opt_out_methods = request["params"]["capabilities"]["optOutNotificationMethods"]
            .as_array()
            .unwrap();
        assert!(
            opt_out_methods
                .iter()
                .any(|method| method.as_str() == Some("thread/started"))
        );
        assert!(
            opt_out_methods
                .iter()
                .any(|method| method.as_str() == Some("item/completed"))
        );
        assert!(
            opt_out_methods
                .iter()
                .any(|method| method.as_str() == Some("item/plan/delta"))
        );
        assert!(
            opt_out_methods
                .iter()
                .any(|method| method.as_str() == Some("item/fileChange/patchUpdated"))
        );

        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "userAgent": app_server_user_agent(REQUIRED_CODEX_APP_SERVER_VERSION),
                        "codexHome": "C:/Users/example/.codex",
                        "platformFamily": "windows",
                        "platformOs": "windows"
                    }
                })
                .to_string(),
            ))
            .unwrap();
        expect_initialized(&mut socket);
    });

    let mut client = ManagedBackendSession::connect_websocket_with_options(
        endpoint,
        "Bearer test-token".to_string(),
        ManagedBackendClientOptions::request_only(),
        Duration::from_secs(2),
    )
    .unwrap();
    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn websocket_thread_read_metadata_uses_metadata_only_request_and_normalizes_nickname() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/read"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_child",
                "includeTurns": false
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "thread": {
                            "cliVersion": "0.128.0",
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_child",
                            "modelProvider": "openai",
                            "preview": "",
                            "source": {
                                "subAgent": {
                                    "thread_spawn": {
                                        "agent_nickname": "Curie"
                                    }
                                }
                            },
                            "status": {
                                "type": "notLoaded"
                            },
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });

    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let summary = client
        .read_thread_metadata("thread_child", Duration::from_secs(2))
        .unwrap();
    assert_eq!(summary.id, "thread_child");
    assert_eq!(summary.agent_nickname.as_deref(), Some("Curie"));

    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn websocket_thread_read_metadata_details_preserve_runtime_metadata_when_exposed() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/read"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_child",
                "includeTurns": false
            })
        );
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "model": "gpt-5.5",
                        "modelProvider": "openai",
                        "reasoningEffort": "xhigh",
                        "thread": {
                            "agentNickname": "Curie",
                            "cliVersion": "0.128.0",
                            "createdAt": 1,
                            "cwd": "C:/work/beryl",
                            "ephemeral": false,
                            "id": "thread_child",
                            "modelProvider": "openai",
                            "preview": "",
                            "source": "subAgent",
                            "status": {
                                "type": "notLoaded"
                            },
                            "updatedAt": 2
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });

    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let metadata = client
        .read_thread_metadata_details("thread_child", Duration::from_secs(2))
        .unwrap();
    assert_eq!(metadata.thread.id, "thread_child");
    assert_eq!(metadata.thread.agent_nickname.as_deref(), Some("Curie"));
    assert_eq!(metadata.session_metadata.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(
        metadata.session_metadata.reasoning_effort.as_deref(),
        Some("xhigh")
    );

    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn websocket_account_rate_limits_read_uses_null_params_and_deserializes_multi_bucket_view() {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", |mut socket| {
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("account/rateLimits/read"));
        assert_eq!(request["params"], Value::Null);
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "rateLimits": {
                            "primary": {
                                "usedPercent": 55,
                                "windowDurationMins": 10080
                            }
                        },
                        "rateLimitsByLimitId": {
                            "codex": {
                                "limitId": "codex",
                                "limitName": "Codex",
                                "primary": {
                                    "usedPercent": 15,
                                    "windowDurationMins": 1440
                                },
                                "secondary": {
                                    "usedPercent": 55,
                                    "windowDurationMins": 10080
                                }
                            }
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });

    let mut client = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap();

    let response = client
        .read_account_rate_limits(Duration::from_secs(2))
        .unwrap();
    assert_eq!(
        response.rate_limits.primary.unwrap().window_duration_mins,
        Some(10080)
    );
    let by_limit_id = response.rate_limits_by_limit_id.unwrap();
    let codex = by_limit_id.get("codex").unwrap();
    assert_eq!(codex.limit_id.as_deref(), Some("codex"));
    assert_eq!(codex.limit_name.as_deref(), Some("Codex"));
    assert_eq!(codex.primary.as_ref().unwrap().used_percent, 15);

    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn compatibility_probe_responses_deserialize_from_observed_shapes() {
    let initialize: InitializeResponse = serde_json::from_value(json!({
        "userAgent": app_server_user_agent("0.137.0"),
        "codexHome": "C:/Users/example/.codex",
        "platformFamily": "windows",
        "platformOs": "windows"
    }))
    .unwrap();

    let models: ModelListResponse = serde_json::from_value(json!({
        "data": [
            {
                "id": "gpt-5.5",
                "model": "gpt-5.5",
                "displayName": "GPT-5.5",
                "description": "Frontier model",
                "hidden": false,
                "supportedReasoningEfforts": [
                    {
                        "reasoningEffort": "low",
                        "description": "Fast responses with lighter reasoning"
                    },
                    {
                        "reasoningEffort": "medium",
                        "description": "Balances speed and reasoning depth"
                    },
                    {
                        "reasoningEffort": "high",
                        "description": "Greater reasoning depth"
                    },
                    {
                        "reasoningEffort": "xhigh",
                        "description": "Extra high reasoning depth"
                    }
                ],
                "defaultReasoningEffort": "medium",
                "inputModalities": ["text", "image"],
                "supportsPersonality": true,
                "additionalSpeedTiers": ["priority", "fast"],
                "isDefault": true
            }
        ],
        "nextCursor": "model_cursor"
    }))
    .unwrap();

    let config: ConfigReadResponse = serde_json::from_value(json!({
        "config": {
            "model": "gpt-5.5",
            "model_reasoning_effort": "xhigh"
        },
        "origins": {}
    }))
    .unwrap();

    assert_eq!(initialize.codex_home, "C:/Users/example/.codex");
    assert_eq!(models.data.len(), 1);
    assert_eq!(models.data[0].id, "gpt-5.5");
    assert_eq!(models.data[0].display_name, "GPT-5.5");
    assert_eq!(
        models.data[0].supported_reasoning_efforts,
        vec![
            "low".to_string(),
            "medium".to_string(),
            "high".to_string(),
            "xhigh".to_string(),
        ]
    );
    assert_eq!(
        models.data[0].default_reasoning_effort.as_deref(),
        Some("medium")
    );
    assert_eq!(models.data[0].input_modalities, vec!["text", "image"]);
    assert!(models.data[0].supports_personality);
    assert!(models.data[0].is_default);
    assert_eq!(models.next_cursor.as_deref(), Some("model_cursor"));
    assert_eq!(config.config.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(
        config.config.model_reasoning_effort.as_deref(),
        Some("xhigh")
    );
}

#[test]
fn model_list_deserializes_legacy_reasoning_effort_strings() {
    let models: ModelListResponse = serde_json::from_value(json!({
        "data": [
            {
                "id": "gpt-5.4-mini",
                "model": "gpt-5.4-mini",
                "displayName": "GPT-5.4 Mini",
                "supportedReasoningEfforts": ["low", "medium"]
            }
        ]
    }))
    .unwrap();

    assert_eq!(
        models.data[0].supported_reasoning_efforts,
        vec!["low".to_string(), "medium".to_string()]
    );
}

#[test]
fn model_list_deserializes_reasoning_effort_maps() {
    let models: ModelListResponse = serde_json::from_value(json!({
        "data": [
            {
                "id": "gpt-5.5",
                "model": "gpt-5.5",
                "displayName": "GPT-5.5",
                "supportedReasoningEfforts": {
                    "low": {
                        "description": "Fast responses with lighter reasoning"
                    },
                    "medium": {
                        "description": "Balances speed and reasoning depth"
                    },
                    "high": {
                        "description": "Greater reasoning depth"
                    }
                },
                "defaultReasoningEffort": {
                    "reasoningEffort": "medium",
                    "description": "Balances speed and reasoning depth"
                }
            }
        ]
    }))
    .unwrap();

    let mut efforts = models.data[0].supported_reasoning_efforts.clone();
    efforts.sort();
    assert_eq!(
        efforts,
        vec!["high".to_string(), "low".to_string(), "medium".to_string()]
    );
    assert_eq!(
        models.data[0].default_reasoning_effort.as_deref(),
        Some("medium")
    );
}

#[test]
fn model_list_options_serialize_page_and_hidden_controls() {
    let options = ModelListOptions::page(25)
        .with_cursor("model_cursor")
        .include_hidden();

    assert_eq!(
        serde_json::to_value(options).unwrap(),
        json!({
            "cursor": "model_cursor",
            "limit": 25,
            "includeHidden": true
        })
    );

    assert_eq!(
        serde_json::to_value(ModelListOptions::default()).unwrap(),
        json!({})
    );
}

#[test]
fn config_read_options_serialize_cwd_and_layer_controls() {
    assert_eq!(
        serde_json::to_value(ConfigReadOptions::for_cwd(PathBuf::from("C:/work/beryl"))).unwrap(),
        json!({
            "cwd": "C:/work/beryl"
        })
    );

    assert_eq!(
        serde_json::to_value(
            ConfigReadOptions::for_cwd(PathBuf::from("C:/work/beryl")).include_layers()
        )
        .unwrap(),
        json!({
            "cwd": "C:/work/beryl",
            "includeLayers": true
        })
    );

    assert_eq!(
        serde_json::to_value(ConfigReadOptions::default()).unwrap(),
        json!({})
    );
}

#[test]
fn compatibility_snapshot_exposes_required_probes_and_platform_facts() {
    let host_snapshot = CompatibilitySnapshot::from_initialize_response(&InitializeResponse {
        user_agent: app_server_user_agent(REQUIRED_CODEX_APP_SERVER_VERSION),
        codex_home: "C:/Users/example/.codex".to_string(),
        platform_family: "windows".to_string(),
        platform_os: "windows".to_string(),
    });

    assert_eq!(
        host_snapshot.required_method_probes(),
        &[
            CompatibilityProbe::ConfigRead,
            CompatibilityProbe::ModelList,
            CompatibilityProbe::ThreadCompactStart,
            CompatibilityProbe::ThreadFork,
            CompatibilityProbe::ThreadInjectItems,
            CompatibilityProbe::ThreadResume,
            CompatibilityProbe::ThreadRollback,
            CompatibilityProbe::ThreadUnsubscribe,
            CompatibilityProbe::TurnInterrupt,
            CompatibilityProbe::TurnStart,
            CompatibilityProbe::TurnSteer,
        ]
    );
    assert_eq!(
        host_snapshot
            .required_method_probes()
            .iter()
            .map(|probe| probe.method())
            .collect::<Vec<_>>(),
        vec![
            "config/read",
            "model/list",
            "thread/compact/start",
            "thread/fork",
            "thread/inject_items",
            "thread/resume",
            "thread/rollback",
            "thread/unsubscribe",
            "turn/interrupt",
            "turn/start",
            "turn/steer",
        ]
    );
    assert_eq!(host_snapshot.platform_family(), "windows");
    assert_eq!(host_snapshot.platform_os(), "windows");
}

#[test]
fn websocket_initialize_version_gate_accepts_exact_required_version_before_contract_probes() {
    let user_agent = app_server_user_agent(REQUIRED_CODEX_APP_SERVER_VERSION);
    if let Err(error) = run_websocket_initialize_version_gate(Some(user_agent), true) {
        panic!("expected initialize userAgent to pass version gate: {error}");
    }
}

#[test]
fn websocket_initialize_rejects_missing_malformed_legacy_and_nonmatching_app_server_versions() {
    let missing = run_websocket_initialize_version_gate(None, false);
    assert!(matches!(
        missing,
        Err(ManagedBackendError::Compatibility(
            CompatibilityError::AppServerVersionMissing {
                required_version: REQUIRED_CODEX_APP_SERVER_VERSION,
            }
        ))
    ));

    let malformed = run_websocket_initialize_version_gate(
        Some("beryl/0.137 (Windows 10.0.26200; aarch64)".to_string()),
        false,
    );
    assert!(matches!(
        malformed,
        Err(ManagedBackendError::Compatibility(
            CompatibilityError::AppServerVersionUnrecognized {
                required_version: REQUIRED_CODEX_APP_SERVER_VERSION,
                user_agent,
            }
        )) if user_agent == "beryl/0.137 (Windows 10.0.26200; aarch64)"
    ));

    let legacy_shape =
        run_websocket_initialize_version_gate(Some("codex-cli 0.137.0".to_string()), false);
    assert!(matches!(
        legacy_shape,
        Err(ManagedBackendError::Compatibility(
            CompatibilityError::AppServerVersionUnrecognized {
                required_version: REQUIRED_CODEX_APP_SERVER_VERSION,
                user_agent,
            }
        )) if user_agent == "codex-cli 0.137.0"
    ));

    let older =
        run_websocket_initialize_version_gate(Some(app_server_user_agent("0.128.0")), false);
    assert!(matches!(
        older,
        Err(ManagedBackendError::Compatibility(
            CompatibilityError::AppServerVersionMismatch {
                required_version: REQUIRED_CODEX_APP_SERVER_VERSION,
                actual_version,
                user_agent,
            }
        )) if actual_version == "0.128.0" && user_agent == app_server_user_agent("0.128.0")
    ));

    for version in ["0.138.0", "1.0.0"] {
        let newer =
            run_websocket_initialize_version_gate(Some(app_server_user_agent(version)), false);
        assert!(matches!(
            newer,
            Err(ManagedBackendError::Compatibility(
                CompatibilityError::AppServerVersionMismatch {
                    required_version: REQUIRED_CODEX_APP_SERVER_VERSION,
                    actual_version,
                    user_agent,
                }
            )) if actual_version == version && user_agent == app_server_user_agent(version)
        ));
    }
}

fn app_server_user_agent(version: &str) -> String {
    format!("beryl/{version} (Windows 10.0.26200; aarch64) WindowsTerminal (beryl; 0.1.0)")
}

fn run_websocket_initialize_version_gate(
    user_agent: Option<String>,
    expect_initialized_notification: bool,
) -> Result<(), ManagedBackendError> {
    let (endpoint, server) = spawn_fake_app_server("Bearer test-token", move |mut socket| {
        let request = read_json(&mut socket);
        assert_eq!(request["jsonrpc"], json!("2.0"));
        assert_eq!(request["id"], json!(1));
        assert_eq!(request["method"], json!("initialize"));
        assert_eq!(request["params"]["clientInfo"]["name"], json!("beryl"));
        assert_eq!(
            request["params"]["capabilities"]["experimentalApi"],
            json!(true)
        );
        assert_thread_started_not_opted_out(&request);

        let mut result = json!({
            "codexHome": "C:/Users/example/.codex",
            "platformFamily": "windows",
            "platformOs": "windows"
        });
        if let Some(user_agent) = user_agent {
            result["userAgent"] = json!(user_agent);
        }

        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": result
                })
                .to_string(),
            ))
            .unwrap();

        if expect_initialized_notification {
            expect_initialized(&mut socket);
        }
    });

    let result = ManagedBackendSession::connect_websocket(
        endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    );
    let result = result.map(|mut client| {
        client.shutdown().unwrap();
    });

    server.join().unwrap();
    result
}

fn spawn_fake_app_server<F>(
    expected_auth: &'static str,
    handler: F,
) -> (BackendWebSocketEndpoint, thread::JoinHandle<()>)
where
    F: FnOnce(WebSocket<TcpStream>) + Send + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let socket = accept_hdr(
            stream,
            |request: &tungstenite::handshake::server::Request, response| {
                assert_eq!(
                    request
                        .headers()
                        .get("authorization")
                        .unwrap()
                        .to_str()
                        .unwrap(),
                    expected_auth
                );
                Ok(response)
            },
        )
        .unwrap();
        handler(socket);
    });

    (endpoint, server)
}

fn expect_initialize(socket: &mut WebSocket<TcpStream>, request_id: u64) {
    let request = read_json(socket);
    assert_eq!(request["jsonrpc"], json!("2.0"));
    assert_eq!(request["id"], json!(request_id));
    assert_eq!(request["method"], json!("initialize"));
    assert_eq!(request["params"]["clientInfo"]["name"], json!("beryl"));
    assert_eq!(
        request["params"]["capabilities"]["experimentalApi"],
        json!(true)
    );
    assert_thread_started_not_opted_out(&request);
    socket
        .send(Message::text(
            json!({
                    "jsonrpc": "2.0",
            "id": request_id,
            "result": {
                "userAgent": app_server_user_agent(REQUIRED_CODEX_APP_SERVER_VERSION),
                "codexHome": "C:/Users/example/.codex",
                "platformFamily": "windows",
                "platformOs": "windows"
                    }
                })
            .to_string(),
        ))
        .unwrap();
}

fn assert_thread_started_not_opted_out(request: &Value) {
    let opt_out_methods = request["params"]["capabilities"]
        .get("optOutNotificationMethods")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(
        opt_out_methods
            .iter()
            .all(|method| method.as_str() != Some("thread/started")),
        "initialize must not opt out of thread/started notifications"
    );
}

fn expect_initialized(socket: &mut WebSocket<TcpStream>) {
    let notification = read_json(socket);
    assert_eq!(notification["jsonrpc"], json!("2.0"));
    assert_eq!(notification["method"], json!("initialized"));
    assert!(notification.get("id").is_none());
}

fn read_json(socket: &mut WebSocket<TcpStream>) -> Value {
    loop {
        match socket.read().unwrap() {
            Message::Text(text) => return serde_json::from_str(text.as_str()).unwrap(),
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Close(frame) => panic!("websocket closed before JSON message: {frame:?}"),
            other => panic!("expected websocket text JSON message, got {other:?}"),
        }
    }
}
