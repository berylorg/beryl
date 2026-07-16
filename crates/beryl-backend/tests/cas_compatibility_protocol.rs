use std::{
    net::{TcpListener, TcpStream},
    path::Path,
    thread,
    time::Duration,
};

use beryl_backend::{
    BackendWebSocketEndpoint, CompatibilityProbe, ManagedBackendSession,
    REQUIRED_CODEX_APP_SERVER_VERSION,
};
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, accept_hdr};

const AUTHORIZATION: &str = "Bearer cas-compatibility-test-token";
const EXECUTION_ROOT: &str = r"C:\work\beryl";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

type ServerSocket = WebSocket<TcpStream>;

#[test]
fn exact_0144_1_compatibility_probe_sends_every_required_typed_shape() {
    let (endpoint, server) = spawn_fake_app_server(|mut socket| {
        expect_initialize(&mut socket);

        expect_request(
            &mut socket,
            2,
            "config/read",
            json!({ "cwd": EXECUTION_ROOT }),
        );
        send_result(
            &mut socket,
            2,
            json!({
                "config": {
                    "model": "gpt-5.5",
                    "modelReasoningEffort": "high"
                }
            }),
        );

        expect_request(&mut socket, 3, "model/list", json!({ "limit": 100 }));
        send_result(&mut socket, 3, json!({ "data": [], "nextCursor": null }));

        let probe_thread_id = "00000000-0000-0000-0000-000000000000";
        let probe_turn_id = "00000000-0000-0000-0000-000000000001";

        expect_request(
            &mut socket,
            4,
            "thread/compact/start",
            json!({ "threadId": probe_thread_id }),
        );
        send_recognized_target_error(&mut socket, 4);

        expect_request(
            &mut socket,
            5,
            "thread/fork",
            json!({
                "threadId": probe_thread_id,
                "cwd": EXECUTION_ROOT,
                "excludeTurns": true,
                "ephemeral": false
            }),
        );
        send_recognized_target_error(&mut socket, 5);

        expect_request(
            &mut socket,
            6,
            "thread/inject_items",
            json!({
                "threadId": probe_thread_id,
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
        send_recognized_target_error(&mut socket, 6);

        expect_request(
            &mut socket,
            7,
            "thread/resume",
            json!({
                "threadId": probe_thread_id,
                "cwd": EXECUTION_ROOT,
                "excludeTurns": true
            }),
        );
        send_recognized_target_error(&mut socket, 7);

        expect_request(
            &mut socket,
            8,
            "thread/rollback",
            json!({ "threadId": probe_thread_id, "numTurns": 1 }),
        );
        send_recognized_target_error(&mut socket, 8);

        expect_request(
            &mut socket,
            9,
            "thread/unsubscribe",
            json!({ "threadId": probe_thread_id }),
        );
        send_result(&mut socket, 9, json!({ "status": "notLoaded" }));

        expect_request(
            &mut socket,
            10,
            "turn/interrupt",
            json!({ "threadId": probe_thread_id, "turnId": probe_turn_id }),
        );
        send_recognized_target_error(&mut socket, 10);

        expect_request(
            &mut socket,
            11,
            "turn/start",
            json!({
                "threadId": probe_thread_id,
                "input": [{
                    "type": "text",
                    "text": "Beryl compatibility probe"
                }]
            }),
        );
        send_recognized_target_error(&mut socket, 11);

        expect_request(
            &mut socket,
            12,
            "turn/steer",
            json!({
                "threadId": probe_thread_id,
                "expectedTurnId": probe_turn_id,
                "input": [{
                    "type": "text",
                    "text": "Beryl compatibility probe"
                }]
            }),
        );
        send_recognized_target_error(&mut socket, 12);
    });

    let mut client = connect(endpoint);
    let report = client
        .probe_compatibility(Path::new(EXECUTION_ROOT), REQUEST_TIMEOUT)
        .unwrap();

    let probes = report
        .method_successes()
        .iter()
        .map(|success| success.probe())
        .collect::<Vec<_>>();
    assert_eq!(probes, expected_probes());
    assert!(report.thread_branch_capabilities().thread_branching());
    assert_eq!(report.config_defaults().model.as_deref(), Some("gpt-5.5"));
    assert_eq!(
        report.config_defaults().model_reasoning_effort.as_deref(),
        Some("high")
    );
    assert!(report.model_list().is_empty());

    client.shutdown().unwrap();
    server.join().unwrap();
}

fn expected_probes() -> Vec<CompatibilityProbe> {
    vec![
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
}

fn connect(endpoint: BackendWebSocketEndpoint) -> ManagedBackendSession {
    ManagedBackendSession::connect_websocket(endpoint, AUTHORIZATION.to_string(), REQUEST_TIMEOUT)
        .unwrap()
}

fn spawn_fake_app_server<F>(handler: F) -> (BackendWebSocketEndpoint, thread::JoinHandle<()>)
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

fn expect_initialize(socket: &mut ServerSocket) {
    let request = read_json(socket);
    assert_eq!(request["id"], json!(1));
    assert_eq!(request["method"], json!("initialize"));
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
}

fn expect_request(socket: &mut ServerSocket, id: u64, method: &str, params: Value) {
    let request = read_json(socket);
    assert_eq!(request["jsonrpc"], json!("2.0"));
    assert_eq!(request["id"], json!(id));
    assert_eq!(request["method"], json!(method));
    assert_eq!(request["params"], params);
}

fn send_recognized_target_error(socket: &mut ServerSocket, id: u64) {
    socket
        .send(Message::text(
            json!({
                "id": id,
                "error": {
                    "code": -32600,
                    "message": "no loaded thread found for compatibility probe"
                }
            })
            .to_string(),
        ))
        .unwrap();
}

fn send_result(socket: &mut ServerSocket, id: u64, result: Value) {
    socket
        .send(Message::text(
            json!({ "id": id, "result": result }).to_string(),
        ))
        .unwrap();
}

fn read_json(socket: &mut ServerSocket) -> Value {
    loop {
        match socket.read().unwrap() {
            Message::Text(text) => return serde_json::from_str(&text).unwrap(),
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Close(frame) => panic!("websocket closed before JSON message: {frame:?}"),
            other => panic!("expected websocket text JSON message, got {other:?}"),
        }
    }
}
