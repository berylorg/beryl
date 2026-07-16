use std::{
    net::{TcpListener, TcpStream},
    path::Path,
    thread,
    time::Duration,
};

use beryl_backend::{
    BackendWebSocketEndpoint, ManagedBackendSession, REQUIRED_CODEX_APP_SERVER_VERSION,
    ThreadInjectionBatch, ThreadInjectionItem, ThreadInjectionOutcome, ThreadSessionMetadata,
};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, accept_hdr};

pub(super) const AUTHORIZATION: &str = "Bearer cas-lineage-test-token";
pub(super) const EXECUTION_ROOT: &str = r"C:\work\beryl";
pub(super) const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) type ServerSocket = WebSocket<TcpStream>;

pub(super) fn run_injection_case<F>(
    batch: ThreadInjectionBatch,
    timeout: Duration,
    handle_injection: F,
) -> ThreadInjectionOutcome
where
    F: FnOnce(&mut ServerSocket, &str, &Value) + Send + 'static,
{
    let (endpoint, server) = spawn_fake_app_server(move |mut socket| {
        expect_initialize(&mut socket);

        let start = read_json(&mut socket);
        assert_eq!(start["id"], json!(2));
        assert_eq!(start["method"], json!("thread/start"));
        assert_eq!(
            start["params"],
            json!({
                "cwd": EXECUTION_ROOT,
                "ephemeral": false
            })
        );
        send_result(
            &mut socket,
            2,
            lineage_result("thread_fresh", json!({ "type": "idle" })),
        );

        let raw = read_text(&mut socket);
        let request: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(request["jsonrpc"], json!("2.0"));
        assert_eq!(request["id"], json!(3));
        assert_eq!(request["method"], json!("thread/inject_items"));
        handle_injection(&mut socket, &raw, &request);
    });

    let mut client = connect(endpoint);
    let target = client
        .start_thread(Path::new(EXECUTION_ROOT), REQUEST_TIMEOUT)
        .unwrap()
        .into_idle()
        .unwrap();
    let outcome = client.inject_thread_items(target, &batch, timeout);

    client.shutdown().unwrap();
    server.join().unwrap();
    outcome
}

pub(super) fn single_item_batch() -> ThreadInjectionBatch {
    ThreadInjectionBatch::new(vec![
        ThreadInjectionItem::user_input_text("exact recovery item").unwrap(),
    ])
    .unwrap()
}

pub(super) fn connect(endpoint: BackendWebSocketEndpoint) -> ManagedBackendSession {
    ManagedBackendSession::connect_websocket(endpoint, AUTHORIZATION.to_string(), REQUEST_TIMEOUT)
        .unwrap()
}

pub(super) fn spawn_fake_app_server<F>(
    handler: F,
) -> (BackendWebSocketEndpoint, thread::JoinHandle<()>)
where
    F: FnOnce(ServerSocket) + Send + 'static,
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

pub(super) fn expect_initialize(socket: &mut ServerSocket) {
    let request = read_json(socket);
    assert_eq!(request["jsonrpc"], json!("2.0"));
    assert_eq!(request["id"], json!(1));
    assert_eq!(request["method"], json!("initialize"));
    assert_eq!(request["params"]["clientInfo"]["name"], json!("beryl"));
    assert_eq!(
        request["params"]["capabilities"]["experimentalApi"],
        json!(true)
    );
    let opted_out = request["params"]["capabilities"]
        .get("optOutNotificationMethods")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(
        opted_out
            .iter()
            .all(|method| method.as_str() != Some("thread/started"))
    );
    send_result(
        socket,
        1,
        json!({
            "userAgent": app_server_user_agent(),
            "codexHome": "C:/Users/example/.codex",
            "platformFamily": "windows",
            "platformOs": "windows"
        }),
    );

    let initialized = read_json(socket);
    assert_eq!(initialized["jsonrpc"], json!("2.0"));
    assert_eq!(initialized["method"], json!("initialized"));
    assert!(initialized.get("id").is_none());
}

pub(super) fn lineage_result(thread_id: &str, status: Value) -> Value {
    json!({
        "model": "gpt-5.5",
        "modelProvider": "openai",
        "reasoningEffort": "high",
        "thread": {
            "cliVersion": REQUIRED_CODEX_APP_SERVER_VERSION,
            "createdAt": 1,
            "cwd": "C:/work/beryl",
            "ephemeral": false,
            "id": thread_id,
            "modelProvider": "openai",
            "preview": "must not become normalized history",
            "source": "appServer",
            "status": status,
            "turns": [{
                "id": { "poison": "turn bodies must not be deserialized" },
                "items": "not materialized",
                "status": false
            }],
            "updatedAt": 2
        }
    })
}

pub(super) fn assert_lineage_metadata(metadata: &ThreadSessionMetadata) {
    assert_eq!(metadata.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(metadata.model_provider.as_deref(), Some("openai"));
    assert_eq!(metadata.reasoning_effort.as_deref(), Some("high"));
}

pub(super) fn send_result(socket: &mut ServerSocket, id: u64, result: Value) {
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

pub(super) fn send_error(
    socket: &mut ServerSocket,
    id: u64,
    code: i64,
    message: &str,
    data: Option<Value>,
) {
    socket
        .send(Message::text(
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": code,
                    "message": message,
                    "data": data
                }
            })
            .to_string(),
        ))
        .unwrap();
}

pub(super) fn read_json(socket: &mut ServerSocket) -> Value {
    serde_json::from_str(&read_text(socket)).unwrap()
}

pub(super) fn read_text(socket: &mut ServerSocket) -> String {
    loop {
        match socket.read().unwrap() {
            Message::Text(text) => return text.to_string(),
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Close(frame) => panic!("websocket closed before JSON message: {frame:?}"),
            other => panic!("expected websocket text JSON message, got {other:?}"),
        }
    }
}

fn app_server_user_agent() -> String {
    format!(
        "beryl/{REQUIRED_CODEX_APP_SERVER_VERSION} (Windows 10.0.26200; aarch64) WindowsTerminal (beryl; 0.1.0)"
    )
}
