use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    path::Path,
    thread,
    time::Duration,
};

use beryl_backend::{
    BackendWebSocketEndpoint, ItemLifecycleTimestampMs, ManagedBackendError, ManagedBackendSession,
    REQUIRED_CODEX_APP_SERVER_VERSION, ThreadItem, TurnStreamEnvelope, TurnStreamEvent,
};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, accept_hdr};

const AUTHORIZATION: &str = "Bearer image-ingress-test";
const TIMESTAMP_MS: u64 = 1_752_689_600_123;

#[test]
fn image_result_is_absent_after_the_pinned_discriminators_for_every_supported_position() {
    let result_markers = [
        "RESULT_FIRST_MUST_DISAPPEAR",
        "RESULT_MIDDLE_MUST_DISAPPEAR",
        "RESULT_LAST_MUST_DISAPPEAR",
    ];
    let messages = vec![
        lifecycle_notification(
            "item/completed",
            &format!(
                r#"{{"type":"imageGeneration","result":"{}","savedPath":"C:\\runtime\\generated one.png","revisedPrompt":"line\n\"quoted\"","status":"completed","id":"image-first"}}"#,
                result_markers[0]
            ),
        ),
        lifecycle_notification(
            "item/completed",
            &format!(
                r#"{{"type":"imageGeneration","id":"image-middle","result":"{}","status":"completed","savedPath":""}}"#,
                result_markers[1]
            ),
        ),
        lifecycle_notification(
            "item/completed",
            &format!(
                r#"{{"type":"imageGeneration","id":"image-last","status":"completed","revisedPrompt":null,"result":"{}"}}"#,
                result_markers[2]
            ),
        ),
    ];
    let (mut session, server) = connect_with_messages(messages);

    let first = next_envelope(&mut session);
    let TurnStreamEvent::ItemCompleted {
        completed_at_ms,
        item: ThreadItem::ImageGeneration(first_image),
        ..
    } = first.event()
    else {
        panic!("expected first completed image item: {:?}", first.event());
    };
    assert_eq!(
        *completed_at_ms,
        ItemLifecycleTimestampMs::new(TIMESTAMP_MS)
    );
    assert_eq!(
        first_image.saved_path.as_deref(),
        Some(Path::new(r"C:\runtime\generated one.png"))
    );
    assert_eq!(
        first_image.revised_prompt.as_deref(),
        Some("line\n\"quoted\"")
    );

    let second = next_envelope(&mut session);
    let TurnStreamEvent::ItemCompleted {
        item: ThreadItem::ImageGeneration(second_image),
        ..
    } = second.event()
    else {
        panic!("expected second completed image item: {:?}", second.event());
    };
    assert_eq!(second_image.saved_path.as_deref(), Some(Path::new("")));

    let third = next_envelope(&mut session);
    let TurnStreamEvent::ItemCompleted {
        item: ThreadItem::ImageGeneration(third_image),
        ..
    } = third.event()
    else {
        panic!("expected third completed image item: {:?}", third.event());
    };
    assert!(third_image.saved_path.is_none());

    for (envelope, marker) in [
        (&first, result_markers[0]),
        (&second, result_markers[1]),
        (&third, result_markers[2]),
    ] {
        let debug = format!("{envelope:?}");
        assert!(!debug.contains(marker));
        assert!(!debug.contains("result"));
    }
    server.join().unwrap();
}

#[test]
#[cfg(feature = "lifecycle-test-support")]
fn huge_fragmented_result_is_discarded_before_retained_byte_accounting() {
    let small_marker = "SMALL_RESULT_SECRET";
    let huge_marker = "HUGE_RESULT_SECRET_".repeat(150_000);
    let small = lifecycle_notification(
        "item/started",
        &format!(
            r#"{{"type":"imageGeneration","id":"image-accounting","status":"inProgress","savedPath":"generated.png","result":"{small_marker}"}}"#
        ),
    );
    let huge = lifecycle_notification(
        "item/started",
        &format!(
            r#"{{"type":"imageGeneration","id":"image-accounting","status":"inProgress","savedPath":"generated.png","result":"{huge_marker}"}}"#
        ),
    );
    let (endpoint, server) = spawn_server(move |mut socket| {
        complete_initialize(&mut socket);
        socket.send(Message::text(small)).unwrap();
        let first_split = huge.len() / 3;
        let second_split = huge.len() * 2 / 3;
        write_raw_frame(
            socket.get_mut(),
            false,
            0x1,
            &huge.as_bytes()[..first_split],
        );
        write_raw_frame(
            socket.get_mut(),
            false,
            0x0,
            &huge.as_bytes()[first_split..second_split],
        );
        write_raw_frame(socket.get_mut(), true, 0x9, b"image-ingress-ping");
        write_raw_frame(
            socket.get_mut(),
            true,
            0x0,
            &huge.as_bytes()[second_split..],
        );
        loop {
            match socket.read().unwrap() {
                Message::Pong(payload) if payload.as_ref() == b"image-ingress-ping" => break,
                Message::Ping(_) | Message::Pong(_) => {}
                other => panic!("expected image-ingress pong, got {other:?}"),
            }
        }
    });
    let mut session = connect(endpoint);

    let small_envelope = next_envelope(&mut session);
    let small_metrics = session
        .last_websocket_ingress_test_metrics()
        .expect("small message ingress metrics");
    let huge_envelope = next_envelope(&mut session);
    let huge_metrics = session
        .last_websocket_ingress_test_metrics()
        .expect("huge message ingress metrics");
    assert_eq!(
        small_envelope.approximate_retained_bytes(),
        huge_envelope.approximate_retained_bytes()
    );
    assert!(huge_envelope.approximate_retained_bytes() < 1024);
    assert!(huge_metrics.0 > small_metrics.0.saturating_mul(1_000));
    assert!(small_metrics.1 <= 8 * 1024);
    assert!(huge_metrics.1 <= 8 * 1024);
    assert!(small_metrics.2 <= 8 * 1024);
    assert!(huge_metrics.2 <= 8 * 1024);
    assert!(huge_metrics.3 > 2_000_000);
    assert!(!small_metrics.4);
    assert!(!huge_metrics.4);
    let debug = format!("{huge_envelope:?}");
    assert!(!debug.contains("HUGE_RESULT_SECRET"));
    assert!(!debug.contains("result"));
    server.join().unwrap();
}

#[test]
fn unrelated_nested_tool_item_remains_byte_semantically_untouched() {
    let marker = "TOOL_ARGUMENT_RESULT_MUST_REMAIN";
    let request = format!(
        r#"{{"jsonrpc":"2.0","id":77,"method":"item/tool/call","params":{{"threadId":"thread-tool","turnId":"turn-tool","callId":"call-tool","tool":"inspect","arguments":{{"item":{{"id":"nested","type":"imageGeneration","status":"completed","result":"{marker}"}}}}}}}}"#
    );
    let (mut session, server) = connect_with_messages(vec![request]);
    let envelope = next_envelope(&mut session);
    let TurnStreamEvent::DynamicToolCallRequested(request) = envelope.event() else {
        panic!("expected dynamic tool request: {:?}", envelope.event());
    };
    assert_eq!(
        request.arguments()["item"]["result"],
        Value::String(marker.to_string())
    );
    server.join().unwrap();
}

#[test]
fn non_image_item_result_remains_exact_on_the_same_lifecycle_path() {
    let marker = "MCP_RESULT_MUST_REMAIN";
    let item = format!(
        r#"{{"type":"mcpToolCall","id":"mcp-result","server":"fixture","tool":"inspect","status":"completed","arguments":{{"input":true}},"appContext":null,"mcpAppResourceUri":null,"pluginId":null,"result":{{"content":[{{"type":"text","text":"{marker}"}}],"structuredContent":{{"marker":"{marker}"}},"_meta":null}},"error":null,"durationMs":12}}"#
    );
    let (mut session, server) =
        connect_with_messages(vec![lifecycle_notification("item/completed", &item)]);
    let envelope = next_envelope(&mut session);
    let TurnStreamEvent::ItemCompleted {
        item: ThreadItem::McpToolCall(item),
        ..
    } = envelope.event()
    else {
        panic!("expected completed MCP item: {:?}", envelope.event());
    };
    let result = item.result.as_ref().expect("MCP result must remain");
    assert_eq!(result.content[0]["text"], json!(marker));
    assert_eq!(
        result.structured_content.as_ref().unwrap()["marker"],
        marker
    );
    server.join().unwrap();
}

#[test]
fn malformed_or_ambiguous_image_shapes_fail_closed_without_diagnostic_leakage() {
    let marker = "MALFORMED_RESULT_SECRET";
    let valid_item = format!(
        r#"{{"type":"imageGeneration","id":"image","status":"completed","result":"{marker}"}}"#
    );
    let cases = vec![
        lifecycle_notification(
            "item/completed",
            r#"{"type":"imageGeneration","id":"image","status":"completed"}"#,
        ),
        lifecycle_notification(
            "item/completed",
            &format!(
                r#"{{"type":"imageGeneration","id":"image","status":"completed","result":["{marker}"]}}"#
            ),
        ),
        lifecycle_notification(
            "item/completed",
            &format!(
                r#"{{"result":"{marker}","type":"imageGeneration","id":"image","status":"completed"}}"#
            ),
        ),
        lifecycle_notification(
            "item/completed",
            &format!(
                r#"{{"type":"imageGeneration","id":"image","status":"completed","result":"{marker}","result":"second"}}"#
            ),
        ),
        lifecycle_notification(
            "item/completed",
            &format!(
                r#"{{"type":"imageGeneration","id":"image","type":"imageGeneration","status":"completed","result":"{marker}"}}"#
            ),
        ),
        format!(
            r#"{{"jsonrpc":"2.0","method":"item/completed","method":"future/method","params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image"}}}}"#
        ),
        format!(
            r#"{{"jsonrpc":"2.0","method":"item/completed","params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image","item":{valid_item}}}}}"#
        ),
        format!(
            r#"{{"jsonrpc":"2.0","id":9,"method":"item/completed","params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image"}}}}"#
        ),
        format!(
            r#"{{"jsonrpc":"2.0","method":"item/completed","params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image"}},"params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image"}}}}"#
        ),
        format!(
            r#"{{"jsonrpc":"2.0","params":{{"item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS},"threadId":"thread-image"}},"method":"item/completed"}}"#
        ),
        format!(
            r#"{{"jsonrpc":"2.0","method":"item/completed","params":{{"threadId":"thread-image","item":{valid_item},"turnId":"turn-image","completedAtMs":{TIMESTAMP_MS}}}}}"#
        ),
    ];

    for raw in cases {
        let (mut session, server) = connect_with_messages(vec![raw]);
        let error = session
            .poll_turn_stream_envelope(Duration::from_secs(2))
            .expect_err("malformed image ingress must fail closed");
        assert!(matches!(error, ManagedBackendError::InvalidJsonLine { .. }));
        let display = error.to_string();
        let debug = format!("{error:?}");
        for rendered in [&display, &debug] {
            assert!(!rendered.contains(marker));
            assert!(!rendered.contains("data:image"));
        }
        server.join().unwrap();
    }
}

fn lifecycle_notification(method: &str, item: &str) -> String {
    let timestamp = if method == "item/started" {
        format!(r#""startedAtMs":{TIMESTAMP_MS}"#)
    } else {
        format!(r#""completedAtMs":{TIMESTAMP_MS}"#)
    };
    format!(
        r#"{{"jsonrpc":"2.0","method":"{method}","params":{{"item":{item},"turnId":"turn-image",{timestamp},"threadId":"thread-image"}}}}"#
    )
}

fn next_envelope(session: &mut ManagedBackendSession) -> TurnStreamEnvelope {
    session
        .poll_turn_stream_envelope(Duration::from_secs(3))
        .unwrap()
        .expect("expected turn-stream envelope")
}

fn connect_with_messages(messages: Vec<String>) -> (ManagedBackendSession, thread::JoinHandle<()>) {
    let (endpoint, server) = spawn_server(move |mut socket| {
        complete_initialize(&mut socket);
        for message in messages {
            socket.send(Message::text(message)).unwrap();
        }
    });
    (connect(endpoint), server)
}

fn connect(endpoint: BackendWebSocketEndpoint) -> ManagedBackendSession {
    ManagedBackendSession::connect_websocket(
        endpoint,
        AUTHORIZATION.to_string(),
        Duration::from_secs(3),
    )
    .unwrap()
}

fn spawn_server(
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

fn complete_initialize(socket: &mut WebSocket<TcpStream>) {
    let request = read_json(socket);
    assert_eq!(request["method"], json!("initialize"));
    assert_eq!(request["id"], json!(1));
    socket
        .send(Message::text(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "userAgent": format!(
                        "beryl/{} (Windows 10.0.26200; aarch64) WindowsTerminal (beryl; 0.1.0)",
                        REQUIRED_CODEX_APP_SERVER_VERSION
                    ),
                    "codexHome": "C:/Users/example/.codex",
                    "platformFamily": "windows",
                    "platformOs": "windows"
                }
            })
            .to_string(),
        ))
        .unwrap();
    let initialized = read_json(socket);
    assert_eq!(initialized["method"], json!("initialized"));
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

fn write_raw_frame(stream: &mut TcpStream, fin: bool, opcode: u8, payload: &[u8]) {
    let mut header = vec![if fin { 0x80 | opcode } else { opcode }];
    if payload.len() < 126 {
        header.push(payload.len() as u8);
    } else if payload.len() <= u16::MAX as usize {
        header.push(126);
        header.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        header.push(127);
        header.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }
    stream.write_all(&header).unwrap();
    stream.write_all(payload).unwrap();
    stream.flush().unwrap();
}
