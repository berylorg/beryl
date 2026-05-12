use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    thread,
    time::Duration,
};

use beryl_backend::{
    BackendLaunchSpec, BackendWebSocketEndpoint, ManagedBackendError, ManagedBackendSession,
    ThreadItem, ThreadListOptions, ThreadReadOptions, ThreadStatus, ThreadTurnsListOptions,
    TurnStreamEvent,
};
use beryl_model::workspace::RuntimeMode;
use serde_json::{Value, json};
use tungstenite::{
    Message, WebSocket, accept_hdr, connect,
    handshake::{
        derive_accept_key,
        server::{ErrorResponse, Request, Response},
    },
    http::StatusCode,
};

#[test]
fn managed_websocket_clients_keep_stream_notifications_isolated() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let mut foreground = accept_authenticated(&listener, "Bearer test-token");
        expect_initialize(&mut foreground, 1);
        expect_initialized(&mut foreground);

        let mut background = accept_authenticated(&listener, "Bearer test-token");
        expect_initialize(&mut background, 1);
        expect_initialized(&mut background);

        send_notification(
            &mut foreground,
            "thread/status/changed",
            json!({
                "threadId": "foreground_thread",
                "status": { "type": "active", "activeFlags": [] }
            }),
        );
        send_notification(
            &mut background,
            "thread/name/updated",
            json!({
                "threadId": "maintenance_thread",
                "threadName": "Background Title"
            }),
        );
    });

    let mut foreground = connect_test_client(&endpoint);
    let mut background = connect_test_client(&endpoint);

    assert_eq!(
        foreground
            .next_turn_stream_event(Duration::from_secs(2))
            .unwrap()
            .unwrap(),
        TurnStreamEvent::ThreadStatusChanged {
            thread_id: "foreground_thread".to_string(),
            status: ThreadStatus::Active {
                active_flags: Vec::new()
            }
        }
    );
    assert_eq!(
        background
            .next_turn_stream_event(Duration::from_secs(2))
            .unwrap()
            .unwrap(),
        TurnStreamEvent::ThreadNameUpdated {
            thread_id: "maintenance_thread".to_string(),
            thread_name: Some("Background Title".to_string())
        }
    );

    server.join().unwrap();
}

#[test]
fn managed_websocket_auth_rejects_unauthenticated_and_allows_authorized_initialize() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server_endpoint = endpoint.clone();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        accept_hdr(stream, |request: &Request, _response| {
            assert!(request.headers().get("authorization").is_none());
            Err(unauthorized_response())
        })
        .unwrap_err();

        let mut authorized = accept_authenticated(&listener, "Bearer test-token");
        expect_initialize(&mut authorized, 1);
        expect_initialized(&mut authorized);
    });

    assert!(connect(server_endpoint.listen_url()).is_err());

    let client = connect_test_client(&endpoint);
    assert!(client.process_id().is_none());

    server.join().unwrap();
}

#[test]
fn managed_websocket_reads_fragmented_notification_with_interleaved_ping() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let mut socket = accept_authenticated(&listener, "Bearer test-token");
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let notification = json!({
            "jsonrpc": "2.0",
            "method": "thread/status/changed",
            "params": {
                "threadId": "fragmented_thread",
                "status": { "type": "active", "activeFlags": [] }
            }
        })
        .to_string();
        let split_at = notification.len() / 2;
        let (first, second) = notification.as_bytes().split_at(split_at);
        let stream = socket.get_mut();
        write_raw_frame(stream, false, 0x1, false, first);
        write_raw_frame(stream, true, 0x9, false, b"hi");
        write_raw_frame(stream, true, 0x0, false, second);

        let pong = read_raw_client_frame(stream);
        assert_eq!(pong.opcode, 0xA);
        assert!(pong.masked);
        assert_eq!(pong.payload, b"hi");
    });

    let mut client = connect_test_client(&endpoint);
    assert_eq!(
        client
            .next_turn_stream_event(Duration::from_secs(2))
            .unwrap()
            .unwrap(),
        TurnStreamEvent::ThreadStatusChanged {
            thread_id: "fragmented_thread".to_string(),
            status: ThreadStatus::Active {
                active_flags: Vec::new()
            }
        }
    );

    server.join().unwrap();
}

#[test]
fn managed_websocket_rejects_pending_notification_count_overflow() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let mut socket = accept_authenticated(&listener, "Bearer test-token");
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/list"));

        for index in 0..1025 {
            send_notification(
                &mut socket,
                "thread/name/updated",
                json!({
                    "threadId": format!("thread_{index}"),
                    "threadName": format!("Thread {index}")
                }),
            );
        }
        let _ = socket.send(Message::text(
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": {
                    "data": []
                }
            })
            .to_string(),
        ));
    });

    let mut client = connect_test_client(&endpoint);
    let error = client
        .list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(2))
        .unwrap_err();

    assert_bounded_resource(error, "pending message queue count", 1024);
    server.join().unwrap();
}

#[test]
fn managed_websocket_rejects_pending_notification_byte_overflow() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let mut socket = accept_authenticated(&listener, "Bearer test-token");
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/list"));

        send_notification(
            &mut socket,
            "thread/status/changed",
            json!({
                "threadId": "large_pending_thread",
                "status": { "type": "active", "activeFlags": [] },
                "padding": "x".repeat(17 * 1024 * 1024)
            }),
        );
        let _ = socket.send(Message::text(
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": {
                    "data": []
                }
            })
            .to_string(),
        ));
    });

    let mut client = connect_test_client(&endpoint);
    let error = client
        .list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(2))
        .unwrap_err();

    assert_bounded_resource(error, "pending message queue byte budget", 16 * 1024 * 1024);
    server.join().unwrap();
}

#[test]
fn managed_websocket_rejects_deferred_dynamic_tool_request_overflow() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let mut socket = accept_authenticated(&listener, "Bearer test-token");
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/list"));

        for index in 0..65 {
            socket
                .send(Message::text(
                    json!({
                        "jsonrpc": "2.0",
                        "id": format!("request_{index}"),
                        "method": "item/tool/call",
                        "params": {
                            "threadId": "thread_1",
                            "turnId": "turn_1",
                            "callId": format!("call_{index}"),
                            "namespace": "beryl",
                            "tool": "read_checklist",
                            "arguments": {}
                        }
                    })
                    .to_string(),
                ))
                .unwrap();
        }
        let _ = socket.send(Message::text(
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": {
                    "data": []
                }
            })
            .to_string(),
        ));
    });

    let mut client = connect_test_client(&endpoint);
    let error = client
        .list_thread_page(&ThreadListOptions::page(1), Duration::from_secs(2))
        .unwrap_err();

    assert_bounded_resource(error, "dynamic tool-call request queue count", 64);
    server.join().unwrap();
}

#[test]
fn managed_websocket_rejects_oversized_handshake_read_ahead() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server_endpoint = endpoint.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_http_upgrade_request(&mut stream);
        assert!(request.contains("Authorization: Bearer test-token"));
        let key = websocket_key(&request);
        let accept_key = derive_accept_key(key.as_bytes());
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
             Connection: Upgrade\r\n\
             Upgrade: websocket\r\n\
             Sec-WebSocket-Accept: {accept_key}\r\n\
             \r\n"
        );
        let mut bytes = response.into_bytes();
        bytes.extend_from_slice(&vec![0_u8; 8 * 1024]);
        stream.write_all(&bytes).unwrap();
        thread::sleep(Duration::from_millis(100));
    });

    let error = ManagedBackendSession::connect_websocket(
        websocket_test_launch(server_endpoint.clone()),
        server_endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap_err();

    let ManagedBackendError::ConnectWebSocket { source, .. } = error else {
        panic!("unexpected error: {error:?}");
    };
    assert!(
        source.to_string().contains("read-ahead"),
        "unexpected error text: {source}"
    );
    server.join().unwrap();
}

#[test]
fn managed_websocket_sanitizes_large_thread_turns_list_image_results() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let mut socket = accept_authenticated(&listener, "Bearer test-token");
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/turns/list"));
        assert_eq!(request["params"]["threadId"], json!("thread_images"));

        let large_result = large_generated_image_result();
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "data": [{
                            "id": "turn_1",
                            "status": "completed",
                            "items": [
                                {
                                    "type": "imageGeneration",
                                    "id": "image_saved",
                                    "status": "completed",
                                    "revisedPrompt": "happy sun",
                                    "result": large_result.clone(),
                                    "savedPath": "C:/work/beryl/sunny.png"
                                },
                                {
                                    "type": "imageGeneration",
                                    "id": "image_unsaved",
                                    "status": "completed",
                                    "result": large_result
                                }
                            ]
                        }],
                        "nextCursor": "next_page",
                        "backwardsCursor": "previous_page"
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });

    let mut client = connect_test_client(&endpoint);
    let response = client
        .list_thread_turns(
            "thread_images",
            &ThreadTurnsListOptions::page(1),
            Duration::from_secs(2),
        )
        .unwrap();

    assert_eq!(response.next_cursor.as_deref(), Some("next_page"));
    assert_eq!(response.backwards_cursor.as_deref(), Some("previous_page"));
    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].items.len(), 2);
    assert_generated_image_item(
        &response.data[0].items[0],
        "image_saved",
        Some("C:/work/beryl/sunny.png"),
        Some("happy sun"),
    );
    assert_generated_image_item(&response.data[0].items[1], "image_unsaved", None, None);

    server.join().unwrap();
}

#[test]
fn managed_websocket_sanitizes_thread_read_result_before_type_across_fragments() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let mut socket = accept_authenticated(&listener, "Bearer test-token");
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/read"));
        assert_eq!(
            request["params"],
            json!({
                "threadId": "thread_images",
                "includeTurns": true
            })
        );

        let result = serde_json::to_string(&large_generated_image_result()).unwrap();
        let response = format!(
            r#"{{"jsonrpc":"2.0","id":2,"result":{{"thread":{{"cliVersion":"0.128.0","createdAt":1,"cwd":"C:/work/beryl","ephemeral":false,"id":"thread_images","modelProvider":"openai","preview":"","status":{{"type":"idle"}},"updatedAt":2,"turns":[{{"id":"turn_1","status":"completed","items":[{{"id":"image_early","result":{result},"savedPath":"C:/work/beryl/early.png","type":"imageGeneration","status":"completed","revisedPrompt":"escaped payload"}}]}}]}}}}}}"#
        );
        let split_at = response.len() / 2;
        let (first, second) = response.as_bytes().split_at(split_at);
        let stream = socket.get_mut();
        write_raw_frame(stream, false, 0x1, false, first);
        write_raw_frame(stream, true, 0x9, false, b"json");
        write_raw_frame(stream, true, 0x0, false, second);

        let pong = read_raw_client_frame(stream);
        assert_eq!(pong.opcode, 0xA);
        assert!(pong.masked);
        assert_eq!(pong.payload, b"json");
    });

    let mut client = connect_test_client(&endpoint);
    let response = client
        .read_thread(
            "thread_images",
            ThreadReadOptions::include_turns(),
            Duration::from_secs(2),
        )
        .unwrap();

    assert_eq!(response.thread.turns.len(), 1);
    assert_eq!(response.thread.turns[0].items.len(), 1);
    assert_generated_image_item(
        &response.thread.turns[0].items[0],
        "image_early",
        Some("C:/work/beryl/early.png"),
        Some("escaped payload"),
    );

    server.join().unwrap();
}

#[test]
fn managed_websocket_rejects_unexpected_sanitized_history_shape() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let mut socket = accept_authenticated(&listener, "Bearer test-token");
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);

        let request = read_json(&mut socket);
        assert_eq!(request["id"], json!(2));
        assert_eq!(request["method"], json!("thread/turns/list"));
        socket
            .send(Message::text(
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "result": {
                        "data": {
                            "not": "an array"
                        }
                    }
                })
                .to_string(),
            ))
            .unwrap();
    });

    let mut client = connect_test_client(&endpoint);
    let error = client
        .list_thread_turns(
            "thread_images",
            &ThreadTurnsListOptions::default(),
            Duration::from_secs(2),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        ManagedBackendError::SanitizeResponse { ref method, .. }
            if method == "thread/turns/list"
    ));

    server.join().unwrap();
}

#[test]
fn managed_websocket_close_frame_reports_transport_closed() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
        let mut socket = accept_authenticated(&listener, "Bearer test-token");
        expect_initialize(&mut socket, 1);
        expect_initialized(&mut socket);
        write_raw_frame(socket.get_mut(), true, 0x8, false, &1000_u16.to_be_bytes());
    });

    let mut client = connect_test_client(&endpoint);
    let error = client
        .next_turn_stream_event(Duration::from_secs(2))
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::TransportClosed { ref method } if method == "turn stream"
    ));

    server.join().unwrap();
}

#[test]
fn managed_websocket_rejects_masked_server_frame() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server_endpoint = endpoint.clone();
    let server = thread::spawn(move || {
        let mut socket = accept_authenticated(&listener, "Bearer test-token");
        assert_initialize_request(&read_json(&mut socket), 1);
        write_raw_frame(
            socket.get_mut(),
            true,
            0x1,
            true,
            initialize_response(1).as_bytes(),
        );
    });

    let error = ManagedBackendSession::connect_websocket(
        websocket_test_launch(server_endpoint.clone()),
        server_endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ManagedBackendError::WebSocketTransport { ref method, .. } if method == "initialize"
    ));
    assert!(error.to_string().contains("masked"));

    server.join().unwrap();
}

#[test]
fn managed_websocket_rejects_reserved_bit_server_frame() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server_endpoint = endpoint.clone();
    let server = thread::spawn(move || {
        let mut socket = accept_authenticated(&listener, "Bearer test-token");
        assert_initialize_request(&read_json(&mut socket), 1);
        write_raw_frame_with_first_byte(
            socket.get_mut(),
            0xC1,
            false,
            initialize_response(1).as_bytes(),
        );
    });

    let error = ManagedBackendSession::connect_websocket(
        websocket_test_launch(server_endpoint.clone()),
        server_endpoint,
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        ManagedBackendError::WebSocketTransport { ref method, .. } if method == "initialize"
    ));
    assert!(error.to_string().contains("reserved bit"));

    server.join().unwrap();
}

fn connect_test_client(endpoint: &BackendWebSocketEndpoint) -> ManagedBackendSession {
    ManagedBackendSession::connect_websocket(
        websocket_test_launch(endpoint.clone()),
        endpoint.clone(),
        "Bearer test-token".to_string(),
        Duration::from_secs(2),
    )
    .unwrap()
}

fn assert_bounded_resource(error: ManagedBackendError, resource: &'static str, limit: usize) {
    match error {
        ManagedBackendError::BoundedResourceExceeded {
            resource: actual_resource,
            limit: actual_limit,
            ..
        } => {
            assert_eq!(actual_resource, resource);
            assert_eq!(actual_limit, limit);
        }
        other => panic!("expected bounded resource error, got {other:?}"),
    }
}

fn read_http_upgrade_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        stream.read_exact(&mut byte).unwrap();
        request.push(byte[0]);
        if request.ends_with(b"\r\n\r\n") {
            break;
        }
        assert!(
            request.len() < 16 * 1024,
            "test WebSocket request header exceeded sanity limit"
        );
    }
    String::from_utf8(request).unwrap()
}

fn websocket_key(request: &str) -> &str {
    request
        .lines()
        .find_map(|line| line.strip_prefix("Sec-WebSocket-Key: "))
        .expect("request should include Sec-WebSocket-Key")
        .trim()
}

fn websocket_test_launch(endpoint: BackendWebSocketEndpoint) -> BackendLaunchSpec {
    BackendLaunchSpec::managed_websocket(
        RuntimeMode::HostWindows,
        r"C:\work\beryl",
        endpoint,
        PathBuf::from(r"C:\tmp\beryl-token.txt"),
    )
}

fn accept_authenticated(
    listener: &TcpListener,
    expected_auth: &'static str,
) -> WebSocket<TcpStream> {
    let (stream, _) = listener.accept().unwrap();
    accept_hdr(stream, move |request: &Request, response| {
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
    })
    .unwrap()
}

fn unauthorized_response() -> ErrorResponse {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .body(Some("missing bearer token".to_string()))
        .unwrap()
}

fn expect_initialize(socket: &mut WebSocket<TcpStream>, request_id: u64) {
    let request = read_json(socket);
    assert_initialize_request(&request, request_id);
    socket
        .send(Message::text(initialize_response(request_id)))
        .unwrap();
}

fn assert_initialize_request(request: &Value, request_id: u64) {
    assert_eq!(request["jsonrpc"], json!("2.0"));
    assert_eq!(request["id"], json!(request_id));
    assert_eq!(request["method"], json!("initialize"));
    assert_eq!(request["params"]["clientInfo"]["name"], json!("beryl"));
    assert_eq!(
        request["params"]["capabilities"]["experimentalApi"],
        json!(true)
    );
    assert_thread_started_not_opted_out(request);
}

fn initialize_response(request_id: u64) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {
            "userAgent": "codex-cli 0.125.0",
            "codexHome": "C:/Users/example/.codex",
            "platformFamily": "windows",
            "platformOs": "windows"
        }
    })
    .to_string()
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

fn large_generated_image_result() -> String {
    let mut result = String::from("escaped quote: \\\" backslash: \\\\ unicode escape: \\u263A ");
    result.push_str(&"x".repeat(2 * 1024 * 1024));
    result
}

fn assert_generated_image_item(
    item: &ThreadItem,
    expected_id: &str,
    expected_saved_path: Option<&str>,
    expected_revised_prompt: Option<&str>,
) {
    let ThreadItem::ImageGeneration(item) = item else {
        panic!("expected imageGeneration item, got {}", item.item_type());
    };
    assert_eq!(item.id, expected_id);
    assert_eq!(item.result, None);
    assert_eq!(item.saved_path.as_deref(), expected_saved_path);
    assert_eq!(item.revised_prompt.as_deref(), expected_revised_prompt);
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

fn write_raw_frame(stream: &mut TcpStream, fin: bool, opcode: u8, masked: bool, payload: &[u8]) {
    let first_byte = if fin { 0x80 | opcode } else { opcode };
    write_raw_frame_with_first_byte(stream, first_byte, masked, payload);
}

fn write_raw_frame_with_first_byte(
    stream: &mut TcpStream,
    first_byte: u8,
    masked: bool,
    payload: &[u8],
) {
    let mut header = vec![first_byte];
    let mask_bit = if masked { 0x80 } else { 0 };
    if payload.len() < 126 {
        header.push(mask_bit | payload.len() as u8);
    } else if payload.len() <= u16::MAX as usize {
        header.push(mask_bit | 126);
        header.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        header.push(mask_bit | 127);
        header.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }

    let mut body = payload.to_vec();
    if masked {
        let mask = [1_u8, 2, 3, 4];
        header.extend_from_slice(&mask);
        for (byte, mask_byte) in body.iter_mut().zip(mask.iter().cycle()) {
            *byte ^= mask_byte;
        }
    }

    stream.write_all(&header).unwrap();
    stream.write_all(&body).unwrap();
    stream.flush().unwrap();
}

struct RawClientFrame {
    opcode: u8,
    masked: bool,
    payload: Vec<u8>,
}

fn read_raw_client_frame(stream: &mut TcpStream) -> RawClientFrame {
    let mut header = [0_u8; 2];
    stream.read_exact(&mut header).unwrap();
    let opcode = header[0] & 0x0F;
    let masked = header[1] & 0x80 != 0;
    let mut len = usize::from(header[1] & 0x7F);
    if len == 126 {
        let mut extended = [0_u8; 2];
        stream.read_exact(&mut extended).unwrap();
        len = usize::from(u16::from_be_bytes(extended));
    } else if len == 127 {
        let mut extended = [0_u8; 8];
        stream.read_exact(&mut extended).unwrap();
        len = u64::from_be_bytes(extended) as usize;
    }

    let mut mask = [0_u8; 4];
    if masked {
        stream.read_exact(&mut mask).unwrap();
    }
    let mut payload = vec![0_u8; len];
    stream.read_exact(&mut payload).unwrap();
    if masked {
        for (byte, mask_byte) in payload.iter_mut().zip(mask.iter().cycle()) {
            *byte ^= mask_byte;
        }
    }

    RawClientFrame {
        opcode,
        masked,
        payload,
    }
}
