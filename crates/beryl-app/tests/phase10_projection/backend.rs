#![allow(dead_code)]

use std::{
    net::{TcpListener, TcpStream},
    path::Path,
    sync::mpsc::{self, Sender},
    thread,
    time::Duration,
};

use beryl_app::{
    cas_projection::AdmittedProjectionSession, conversation_tools::ConversationToolRegistry,
};
use beryl_backend::{
    BackendWebSocketEndpoint, ManagedBackendSession, REQUIRED_CODEX_APP_SERVER_VERSION,
};
use beryl_model::{CasProcessGeneration, RuntimeId};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, accept_hdr};

#[path = "../phase13_ordinary_turn/backend_turn.rs"]
mod backend_turn;

pub use backend_turn::{TurnStartAction, TurnStartReply};

pub const EXECUTION_ROOT: &str = r"C:\work\beryl";
pub const TIMEOUT: Duration = Duration::from_secs(2);

const AUTHORIZATION: &str = "Bearer phase10-projection-test-token";

type ServerSocket = WebSocket<TcpStream>;

#[derive(Clone, Debug)]
pub enum ProjectionStep {
    Fresh {
        target: &'static str,
    },
    Resume {
        source: String,
    },
    RejectResume {
        source: String,
    },
    Fork {
        source: String,
        through_turn: Option<String>,
        target: &'static str,
    },
    RejectFork {
        source: String,
        through_turn: Option<String>,
    },
    Recover {
        target: &'static str,
        injection: InjectionReply,
    },
    Unsubscribe {
        target: &'static str,
        reply: UnsubscribeReply,
    },
    UnsubscribeAfterNotification {
        target: &'static str,
        method: &'static str,
        params: Value,
        reply: UnsubscribeReply,
    },
    UnsubscribeAfterApprovalRequest {
        target: &'static str,
        approval_thread: &'static str,
        approval_turn: &'static str,
        approval_item: &'static str,
    },
    TurnStart {
        target: &'static str,
        expected_input: &'static str,
        before_reply: Vec<TurnStartAction>,
        reply: TurnStartReply,
        after_reply: Vec<TurnStartAction>,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum InjectionReply {
    Success,
    Reject,
    Disconnect,
}

#[derive(Clone, Copy, Debug)]
pub enum UnsubscribeReply {
    Status(&'static str),
    Reject,
    Disconnect,
}

pub struct FakeAppServer {
    endpoint: BackendWebSocketEndpoint,
    control: Option<Sender<ServerCommand>>,
    handle: thread::JoinHandle<()>,
}

enum ServerCommand {
    Notification { method: String, params: Value },
    Disconnect,
    Stop,
}

impl FakeAppServer {
    pub fn spawn(steps: Vec<ProjectionStep>) -> Self {
        Self::spawn_inner(steps, true)
    }

    pub fn spawn_for_admission_rejection() -> Self {
        Self::spawn_inner(Vec::new(), false)
    }

    fn spawn_inner(steps: Vec<ProjectionStep>, expect_probe: bool) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let (control, commands) = mpsc::channel();
        let handle = thread::spawn(move || {
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
                        AUTHORIZATION
                    );
                    Ok(response)
                },
            )
            .unwrap();
            expect_initialize(&mut socket);
            if expect_probe {
                expect_compatibility_probe(&mut socket);
            }
            let disconnect_after_steps = steps.last().is_some_and(ProjectionStep::disconnects);
            let mut request_id = 13;
            for step in steps {
                request_id = handle_projection_step(&mut socket, request_id, step);
            }
            if !disconnect_after_steps {
                while let Ok(command) = commands.recv() {
                    match command {
                        ServerCommand::Notification { method, params } => {
                            send_notification(&mut socket, &method, params);
                        }
                        ServerCommand::Disconnect => break,
                        ServerCommand::Stop => break,
                    }
                }
            }
        });
        Self {
            endpoint,
            control: Some(control),
            handle,
        }
    }

    pub fn admit(
        &self,
        runtime_id: RuntimeId,
        process_generation: CasProcessGeneration,
    ) -> AdmittedProjectionSession {
        let backend = ManagedBackendSession::connect_websocket(
            self.endpoint.clone(),
            AUTHORIZATION.to_string(),
            TIMEOUT,
        )
        .unwrap();
        AdmittedProjectionSession::admit(
            backend,
            runtime_id,
            process_generation,
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap()
    }

    pub fn connect_request_only(&self) -> ManagedBackendSession {
        ManagedBackendSession::connect_websocket_with_options(
            self.endpoint.clone(),
            AUTHORIZATION.to_string(),
            beryl_backend::ManagedBackendClientOptions::request_only(),
            TIMEOUT,
        )
        .unwrap()
    }

    pub fn send_notification(&self, method: &str, params: Value) {
        self.control
            .as_ref()
            .unwrap()
            .send(ServerCommand::Notification {
                method: method.to_owned(),
                params,
            })
            .unwrap();
    }

    pub fn disconnect(&self) {
        self.control
            .as_ref()
            .unwrap()
            .send(ServerCommand::Disconnect)
            .unwrap();
    }

    pub fn join(mut self) {
        if let Some(control) = self.control.take() {
            let _ = control.send(ServerCommand::Stop);
        }
        self.handle.join().unwrap();
    }
}

impl ProjectionStep {
    fn disconnects(&self) -> bool {
        match self {
            Self::Recover {
                injection: InjectionReply::Disconnect,
                ..
            }
            | Self::Unsubscribe {
                reply: UnsubscribeReply::Disconnect,
                ..
            }
            | Self::UnsubscribeAfterNotification {
                reply: UnsubscribeReply::Disconnect,
                ..
            } => true,
            Self::TurnStart { reply, .. } => reply.disconnects(),
            _ => false,
        }
    }
}

fn handle_projection_step(socket: &mut ServerSocket, request_id: u64, step: ProjectionStep) -> u64 {
    match step {
        ProjectionStep::Fresh { target } => {
            expect_request(
                socket,
                request_id,
                "thread/start",
                persistent_start_params(),
            );
            send_result(socket, request_id, lineage_result(target));
            request_id + 1
        }
        ProjectionStep::Resume { source } => {
            expect_request(
                socket,
                request_id,
                "thread/resume",
                json!({
                    "threadId": source,
                    "cwd": EXECUTION_ROOT,
                    "excludeTurns": true
                }),
            );
            send_result(socket, request_id, lineage_result(&source));
            request_id + 1
        }
        ProjectionStep::RejectResume { source } => {
            expect_request(
                socket,
                request_id,
                "thread/resume",
                json!({
                    "threadId": source,
                    "cwd": EXECUTION_ROOT,
                    "excludeTurns": true
                }),
            );
            send_error(socket, request_id, -32600, "unclassified resume rejection");
            request_id + 1
        }
        ProjectionStep::Fork {
            source,
            through_turn,
            target,
        } => {
            let mut params = json!({
                "threadId": source,
                "cwd": EXECUTION_ROOT,
                "excludeTurns": true,
                "ephemeral": false
            });
            if let Some(turn) = through_turn {
                params["lastTurnId"] = json!(turn);
            }
            expect_request(socket, request_id, "thread/fork", params);
            send_result(socket, request_id, lineage_result(target));
            request_id + 1
        }
        ProjectionStep::RejectFork {
            source,
            through_turn,
        } => {
            let mut params = json!({
                "threadId": source,
                "cwd": EXECUTION_ROOT,
                "excludeTurns": true,
                "ephemeral": false
            });
            if let Some(turn) = through_turn {
                params["lastTurnId"] = json!(turn);
            }
            expect_request(socket, request_id, "thread/fork", params);
            send_error(socket, request_id, -32600, "unclassified fork rejection");
            request_id + 1
        }
        ProjectionStep::Recover { target, injection } => {
            expect_request(
                socket,
                request_id,
                "thread/start",
                persistent_start_params(),
            );
            send_result(socket, request_id, lineage_result(target));
            let injection_id = request_id + 1;
            let request = read_json(socket);
            assert_request_header(&request, injection_id, "thread/inject_items");
            assert_eq!(request["params"]["threadId"], json!(target));
            let items = request["params"]["items"].as_array().unwrap();
            assert_eq!(
                items,
                &[
                    json!({
                        "type": "message",
                        "role": "user",
                        "content": [{
                            "type": "input_text",
                            "text": "history user"
                        }]
                    }),
                    json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": "history assistant"
                        }]
                    })
                ],
                "recovery must inject the complete ordered parent prefix and exclude pending input"
            );
            match injection {
                InjectionReply::Success => {
                    send_result(socket, injection_id, json!({}));
                    injection_id + 1
                }
                InjectionReply::Reject => {
                    send_error(
                        socket,
                        injection_id,
                        -32001,
                        "injection rejected by fixture",
                    );
                    let unsubscribe_id = injection_id + 1;
                    expect_request(
                        socket,
                        unsubscribe_id,
                        "thread/unsubscribe",
                        json!({ "threadId": target }),
                    );
                    send_result(socket, unsubscribe_id, json!({ "status": "unsubscribed" }));
                    unsubscribe_id + 1
                }
                InjectionReply::Disconnect => injection_id + 1,
            }
        }
        ProjectionStep::Unsubscribe { target, reply } => {
            expect_request(
                socket,
                request_id,
                "thread/unsubscribe",
                json!({ "threadId": target }),
            );
            match reply {
                UnsubscribeReply::Status(status) => {
                    send_result(socket, request_id, json!({ "status": status }));
                }
                UnsubscribeReply::Reject => {
                    send_error(
                        socket,
                        request_id,
                        -32001,
                        "unsubscribe rejected by fixture",
                    );
                }
                UnsubscribeReply::Disconnect => {}
            }
            request_id + 1
        }
        ProjectionStep::UnsubscribeAfterNotification {
            target,
            method,
            params,
            reply,
        } => {
            expect_request(
                socket,
                request_id,
                "thread/unsubscribe",
                json!({ "threadId": target }),
            );
            send_notification(socket, method, params);
            match reply {
                UnsubscribeReply::Status(status) => {
                    send_result(socket, request_id, json!({ "status": status }));
                }
                UnsubscribeReply::Reject => {
                    send_error(
                        socket,
                        request_id,
                        -32001,
                        "unsubscribe rejected by fixture",
                    );
                }
                UnsubscribeReply::Disconnect => {}
            }
            request_id + 1
        }
        ProjectionStep::UnsubscribeAfterApprovalRequest {
            target,
            approval_thread,
            approval_turn,
            approval_item,
        } => {
            expect_request(
                socket,
                request_id,
                "thread/unsubscribe",
                json!({ "threadId": target }),
            );
            send_server_request(
                socket,
                700,
                "item/commandExecution/requestApproval",
                json!({
                    "threadId": approval_thread,
                    "turnId": approval_turn,
                    "itemId": approval_item,
                    "approvalId": null,
                    "command": "Remove-Item target",
                    "cwd": EXECUTION_ROOT,
                    "reason": "phase 12 ordering proof"
                }),
            );
            let denial = read_json(socket);
            assert_eq!(denial["id"], json!(700));
            assert_eq!(denial["result"], json!({ "decision": "cancel" }));
            send_result(socket, request_id, json!({ "status": "unsubscribed" }));
            request_id + 1
        }
        ProjectionStep::TurnStart {
            target,
            expected_input,
            before_reply,
            reply,
            after_reply,
        } => backend_turn::handle_turn_start(
            socket,
            request_id,
            target,
            expected_input,
            before_reply,
            reply,
            after_reply,
        ),
    }
}

fn expect_initialize(socket: &mut ServerSocket) {
    let request = read_json(socket);
    assert_request_header(&request, 1, "initialize");
    assert_eq!(
        request["params"]["capabilities"]["experimentalApi"],
        json!(true)
    );
    send_result(
        socket,
        1,
        json!({
            "userAgent": format!(
                "beryl/{REQUIRED_CODEX_APP_SERVER_VERSION} (Windows; aarch64)"
            ),
            "codexHome": "C:/Users/example/.codex",
            "platformFamily": "windows",
            "platformOs": "windows"
        }),
    );
    let initialized = read_json(socket);
    assert_eq!(initialized["method"], json!("initialized"));
    assert!(initialized.get("id").is_none());
}

fn expect_compatibility_probe(socket: &mut ServerSocket) {
    expect_request(socket, 2, "config/read", json!({ "cwd": EXECUTION_ROOT }));
    send_result(
        socket,
        2,
        json!({
            "config": {
                "model": "gpt-5.6",
                "modelReasoningEffort": "high"
            }
        }),
    );
    expect_request(socket, 3, "model/list", json!({ "limit": 100 }));
    send_result(socket, 3, json!({ "data": [], "nextCursor": null }));

    let probe_thread = "00000000-0000-0000-0000-000000000000";
    let probe_turn = "00000000-0000-0000-0000-000000000001";
    expect_request(
        socket,
        4,
        "thread/compact/start",
        json!({ "threadId": probe_thread }),
    );
    send_recognized_target_error(socket, 4);
    expect_request(
        socket,
        5,
        "thread/fork",
        json!({
            "threadId": probe_thread,
            "cwd": EXECUTION_ROOT,
            "excludeTurns": true,
            "ephemeral": false
        }),
    );
    send_recognized_target_error(socket, 5);
    expect_request(
        socket,
        6,
        "thread/inject_items",
        json!({
            "threadId": probe_thread,
            "items": [{
                "type": "message",
                "role": "user",
                "content": [{
                    "type": "input_text",
                    "text": "Beryl compatibility probe"
                }]
            }]
        }),
    );
    send_recognized_target_error(socket, 6);
    expect_request(
        socket,
        7,
        "thread/resume",
        json!({
            "threadId": probe_thread,
            "cwd": EXECUTION_ROOT,
            "excludeTurns": true
        }),
    );
    send_recognized_target_error(socket, 7);
    expect_request(
        socket,
        8,
        "thread/rollback",
        json!({ "threadId": probe_thread, "numTurns": 1 }),
    );
    send_recognized_target_error(socket, 8);
    expect_request(
        socket,
        9,
        "thread/unsubscribe",
        json!({ "threadId": probe_thread }),
    );
    send_result(socket, 9, json!({ "status": "notLoaded" }));
    expect_request(
        socket,
        10,
        "turn/interrupt",
        json!({ "threadId": probe_thread, "turnId": probe_turn }),
    );
    send_recognized_target_error(socket, 10);
    expect_request(
        socket,
        11,
        "turn/start",
        json!({
            "threadId": probe_thread,
            "input": [{ "type": "text", "text": "Beryl compatibility probe" }]
        }),
    );
    send_recognized_target_error(socket, 11);
    expect_request(
        socket,
        12,
        "turn/steer",
        json!({
            "threadId": probe_thread,
            "expectedTurnId": probe_turn,
            "input": [{ "type": "text", "text": "Beryl compatibility probe" }]
        }),
    );
    send_recognized_target_error(socket, 12);
}

fn lineage_result(thread_id: &str) -> Value {
    json!({
        "model": "gpt-5.6",
        "modelProvider": "openai",
        "reasoningEffort": "high",
        "thread": {
            "id": thread_id,
            "status": { "type": "idle" }
        }
    })
}

fn persistent_start_params() -> Value {
    json!({
        "cwd": EXECUTION_ROOT,
        "ephemeral": false,
        "dynamicTools": ConversationToolRegistry::canonical().specs()
    })
}

fn expect_request(socket: &mut ServerSocket, id: u64, method: &str, params: Value) {
    let request = read_json(socket);
    assert_request_header(&request, id, method);
    assert_eq!(request["params"], params);
}

fn assert_request_header(request: &Value, id: u64, method: &str) {
    assert_eq!(request["jsonrpc"], json!("2.0"));
    assert_eq!(request["id"], json!(id));
    assert_eq!(request["method"], json!(method));
}

fn send_recognized_target_error(socket: &mut ServerSocket, id: u64) {
    send_error(
        socket,
        id,
        -32600,
        "no loaded thread found for compatibility probe",
    );
}

fn send_error(socket: &mut ServerSocket, id: u64, code: i64, message: &str) {
    socket
        .send(Message::text(
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code, "message": message }
            })
            .to_string(),
        ))
        .unwrap();
}

fn send_result(socket: &mut ServerSocket, id: u64, result: Value) {
    socket
        .send(Message::text(
            json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string(),
        ))
        .unwrap();
}

fn send_notification(socket: &mut ServerSocket, method: &str, params: Value) {
    socket
        .send(Message::text(
            json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string(),
        ))
        .unwrap();
}

fn send_server_request(socket: &mut ServerSocket, id: u64, method: &str, params: Value) {
    socket
        .send(Message::text(
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string(),
        ))
        .unwrap();
}

fn read_json(socket: &mut ServerSocket) -> Value {
    loop {
        match socket.read().unwrap() {
            Message::Text(text) => return serde_json::from_str(&text).unwrap(),
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Close(frame) => panic!("WebSocket closed before JSON message: {frame:?}"),
            other => panic!("expected WebSocket text JSON, got {other:?}"),
        }
    }
}
