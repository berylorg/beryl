use std::{
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::mpsc,
    thread,
    time::Duration,
};

use beryl_backend::{
    ApprovalResponseDisposition, BackendWebSocketEndpoint, ManagedBackendClientOptions,
    ManagedBackendError, ManagedBackendSession, REQUIRED_CODEX_APP_SERVER_VERSION, ThreadStatus,
    TurnStreamEvent,
};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, accept_hdr};

const AUTHORIZATION: &str = "Bearer test-token";

#[test]
fn full_turn_stream_proof_requires_an_initialized_profile_without_opt_outs() {
    let foreground =
        connect_with_profile(ManagedBackendClientOptions::foreground(), |initialize| {
            assert!(opt_out_methods(initialize).is_empty());
        });
    assert!(foreground.has_full_turn_stream());

    let request_only =
        connect_with_profile(ManagedBackendClientOptions::request_only(), |initialize| {
            let methods = opt_out_methods(initialize);
            assert!(methods.contains(&"thread/started"));
            assert!(methods.contains(&"item/completed"));
        });
    assert!(!request_only.has_full_turn_stream());

    let custom_opt_out = connect_with_profile(
        ManagedBackendClientOptions::foreground()
            .with_opt_out_notification_methods(["thread/name/updated"]),
        |initialize| {
            assert_eq!(opt_out_methods(initialize), ["thread/name/updated"]);
        },
    );
    assert!(!custom_opt_out.has_full_turn_stream());
}

fn connect_with_profile(
    options: ManagedBackendClientOptions,
    assert_initialize: impl FnOnce(&Value) + Send + 'static,
) -> ManagedBackendSession {
    let (endpoint, server) = spawn_app_server(move |mut socket| {
        complete_initialize(&mut socket, assert_initialize);
    });
    let session = ManagedBackendSession::connect_websocket_with_options(
        endpoint,
        AUTHORIZATION.to_string(),
        options,
        Duration::from_secs(2),
    )
    .unwrap();
    server.join().unwrap();
    session
}

fn opt_out_methods(initialize: &Value) -> Vec<&str> {
    initialize["params"]["capabilities"]
        .get("optOutNotificationMethods")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

#[test]
fn buffered_drain_skips_unsupported_messages_and_zero_timeout_preserves_fifo() {
    let (endpoint, server) = spawn_app_server(|mut socket| {
        complete_initialize(&mut socket, |_| {});

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("config/read"));
        send_notification(&mut socket, "unsupported/event", json!({ "ignored": true }));
        send_notification(
            &mut socket,
            "thread/status/changed",
            json!({
                "threadId": "thread_first",
                "status": { "type": "active", "activeFlags": [] }
            }),
        );
        send_notification(
            &mut socket,
            "thread/name/updated",
            json!({
                "threadId": "thread_second",
                "threadName": "Second"
            }),
        );
        send_response(
            &mut socket,
            request["id"].as_u64().unwrap(),
            json!({ "config": {} }),
        );
    });
    let mut session = connect_foreground(endpoint);

    session
        .read_config(&PathBuf::from(r"C:\work\beryl"), Duration::from_secs(2))
        .unwrap();

    let first = session
        .drain_buffered_turn_stream_envelope()
        .unwrap()
        .unwrap();
    assert!(first.approximate_retained_bytes() > 0);
    assert_eq!(
        first.event(),
        &TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_first".to_string(),
            status: ThreadStatus::Active {
                active_flags: Vec::new(),
            },
        }
    );

    let second = session
        .poll_turn_stream_envelope(Duration::ZERO)
        .unwrap()
        .unwrap();
    assert!(second.approximate_retained_bytes() > 0);
    assert_eq!(
        second.into_event(),
        TurnStreamEvent::ThreadNameUpdated {
            thread_id: "thread_second".to_string(),
            thread_name: Some("Second".to_string()),
        }
    );
    assert!(
        session
            .drain_buffered_turn_stream_envelope()
            .unwrap()
            .is_none()
    );

    server.join().unwrap();
}

#[test]
fn interleaved_approval_is_denied_and_retained_before_the_matching_response() {
    let (endpoint, server) = spawn_app_server(|mut socket| {
        complete_initialize(&mut socket, |_| {});

        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("config/read"));
        send_server_request(
            &mut socket,
            77,
            "item/commandExecution/requestApproval",
            json!({
                "threadId": "thread_approval",
                "turnId": "turn_approval",
                "itemId": "item_approval",
                "approvalId": null,
                "command": "Remove-Item target",
                "cwd": "C:/work/beryl",
                "reason": "requires manual approval"
            }),
        );
        let denial = read_json(&mut socket);
        assert_eq!(denial["id"], json!(77));
        assert_eq!(denial["result"], json!({ "decision": "cancel" }));
        send_response(
            &mut socket,
            request["id"].as_u64().unwrap(),
            json!({ "config": {} }),
        );
    });
    let mut session = connect_foreground(endpoint);

    session
        .read_config(&PathBuf::from(r"C:\work\beryl"), Duration::from_secs(2))
        .unwrap();
    let envelope = session
        .drain_buffered_turn_stream_envelope()
        .unwrap()
        .unwrap();
    match envelope.into_event() {
        TurnStreamEvent::ApprovalRequested(request) => {
            assert_eq!(request.thread_id(), Some("thread_approval"));
            assert_eq!(request.turn_id(), Some("turn_approval"));
            assert_eq!(request.item_id(), Some("item_approval"));
            assert_eq!(
                request.response_disposition(),
                ApprovalResponseDisposition::AutoDenied
            );
            assert!(matches!(
                session.deny_approval_request(&request),
                Err(ManagedBackendError::ApprovalResponseAlreadySent { .. })
            ));
        }
        other => panic!("expected retained approval request, got {other:?}"),
    }

    server.join().unwrap();
}

#[test]
fn idle_approval_remains_response_required_until_the_caller_denies_it() {
    let (endpoint, server) = spawn_app_server(|mut socket| {
        complete_initialize(&mut socket, |_| {});
        send_server_request(
            &mut socket,
            78,
            "item/commandExecution/requestApproval",
            json!({
                "threadId": "thread_idle_approval",
                "turnId": "turn_idle_approval",
                "itemId": "item_idle_approval"
            }),
        );
        let denial = read_json(&mut socket);
        assert_eq!(denial["id"], json!(78));
        assert_eq!(denial["result"], json!({ "decision": "cancel" }));
    });
    let mut session = connect_foreground(endpoint);

    let envelope = session
        .poll_turn_stream_envelope(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    match envelope.into_event() {
        TurnStreamEvent::ApprovalRequested(request) => {
            assert_eq!(
                request.response_disposition(),
                ApprovalResponseDisposition::ResponseRequired
            );
            let clone = request.clone();
            let mut foreign =
                connect_with_profile(ManagedBackendClientOptions::foreground(), |_| {});
            assert!(matches!(
                foreign.deny_approval_request(&request),
                Err(ManagedBackendError::ApprovalResponseAuthorityMismatch { .. })
            ));
            session.deny_approval_request(&request).unwrap();
            assert_eq!(
                request.response_disposition(),
                ApprovalResponseDisposition::Denied
            );
            assert_eq!(
                clone.response_disposition(),
                ApprovalResponseDisposition::Denied
            );
            assert!(matches!(
                session.deny_approval_request(&clone),
                Err(ManagedBackendError::ApprovalResponseAlreadySent { .. })
            ));
        }
        other => panic!("expected idle approval request, got {other:?}"),
    }

    server.join().unwrap();
}

#[test]
fn invalid_buffered_notification_remains_a_fatal_normalization_error() {
    let (endpoint, server) = spawn_app_server(|mut socket| {
        complete_initialize(&mut socket, |_| {});

        let request = read_json(&mut socket);
        send_notification(
            &mut socket,
            "thread/status/changed",
            json!({
                "threadId": "thread_invalid",
                "status": { "type": "futureStatus" }
            }),
        );
        send_response(
            &mut socket,
            request["id"].as_u64().unwrap(),
            json!({ "config": {} }),
        );
    });
    let mut session = connect_foreground(endpoint);

    session
        .read_config(&PathBuf::from(r"C:\work\beryl"), Duration::from_secs(2))
        .unwrap();
    let error = session.drain_buffered_turn_stream_envelope().unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::DeserializeNotification { ref method, .. }
            if method == "thread/status/changed"
    ));

    server.join().unwrap();
}

#[test]
fn equivalent_buffered_and_live_events_keep_the_same_retained_byte_count() {
    let (release_send, release_receive) = mpsc::channel();
    let (endpoint, server) = spawn_app_server(move |mut socket| {
        complete_initialize(&mut socket, |_| {});

        let request = read_json(&mut socket);
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "thread/status/changed",
            "params": {
                "threadId": "thread_same",
                "status": { "type": "active", "activeFlags": [] }
            }
        })
        .to_string();
        socket.send(Message::text(notification.clone())).unwrap();
        send_response(
            &mut socket,
            request["id"].as_u64().unwrap(),
            json!({ "config": {} }),
        );
        release_receive.recv().unwrap();
        socket.send(Message::text(notification)).unwrap();
    });
    let mut session = connect_foreground(endpoint);

    session
        .read_config(&PathBuf::from(r"C:\work\beryl"), Duration::from_secs(2))
        .unwrap();
    let buffered = session
        .drain_buffered_turn_stream_envelope()
        .unwrap()
        .unwrap();
    release_send.send(()).unwrap();
    let live = session
        .poll_turn_stream_envelope(Duration::from_secs(2))
        .unwrap()
        .unwrap();

    assert!(buffered.approximate_retained_bytes() > 0);
    assert_eq!(
        buffered.approximate_retained_bytes(),
        live.approximate_retained_bytes()
    );
    assert_eq!(buffered.event(), live.event());

    server.join().unwrap();
}

#[test]
fn quiet_poll_does_not_end_the_stream_before_a_later_event() {
    let (release_send, release_receive) = mpsc::channel();
    let (endpoint, server) = spawn_app_server(move |mut socket| {
        complete_initialize(&mut socket, |_| {});
        release_receive.recv().unwrap();
        send_notification(
            &mut socket,
            "thread/status/changed",
            json!({
                "threadId": "thread_later",
                "status": { "type": "active", "activeFlags": [] }
            }),
        );
    });
    let mut session = connect_foreground(endpoint);

    assert!(
        session
            .poll_turn_stream_envelope(Duration::from_millis(20))
            .unwrap()
            .is_none()
    );
    release_send.send(()).unwrap();
    let later = session
        .poll_turn_stream_envelope(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    assert_eq!(
        later.into_event(),
        TurnStreamEvent::ThreadStatusChanged {
            thread_id: "thread_later".to_string(),
            status: ThreadStatus::Active {
                active_flags: Vec::new(),
            },
        }
    );

    server.join().unwrap();
}

fn connect_foreground(endpoint: BackendWebSocketEndpoint) -> ManagedBackendSession {
    ManagedBackendSession::connect_websocket(
        endpoint,
        AUTHORIZATION.to_string(),
        Duration::from_secs(2),
    )
    .unwrap()
}

fn spawn_app_server(
    handler: impl FnOnce(WebSocket<TcpStream>) + Send + 'static,
) -> (BackendWebSocketEndpoint, thread::JoinHandle<()>) {
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
                    AUTHORIZATION
                );
                Ok(response)
            },
        )
        .unwrap();
        handler(socket);
    });
    (endpoint, server)
}

fn complete_initialize(socket: &mut WebSocket<TcpStream>, assert_initialize: impl FnOnce(&Value)) {
    let request = read_json(socket);
    assert_eq!(request["jsonrpc"], json!("2.0"));
    assert_eq!(request["id"], json!(1));
    assert_eq!(request["method"], json!("initialize"));
    assert_eq!(request["params"]["clientInfo"]["name"], json!("beryl"));
    assert_initialize(&request);
    send_response(
        socket,
        1,
        json!({
            "userAgent": format!(
                "beryl/{} (Windows 10.0.26200; aarch64) WindowsTerminal (beryl; 0.1.0)",
                REQUIRED_CODEX_APP_SERVER_VERSION
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

fn send_notification(socket: &mut WebSocket<TcpStream>, method: &str, params: Value) {
    socket
        .send(Message::text(
            json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params
            })
            .to_string(),
        ))
        .unwrap();
}

fn send_server_request(socket: &mut WebSocket<TcpStream>, id: u64, method: &str, params: Value) {
    socket
        .send(Message::text(
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params
            })
            .to_string(),
        ))
        .unwrap();
}

fn send_response(socket: &mut WebSocket<TcpStream>, id: u64, result: Value) {
    socket
        .send(Message::text(
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            })
            .to_string(),
        ))
        .unwrap();
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
