use std::{
    net::{TcpListener, TcpStream},
    num::NonZeroUsize,
    thread,
    time::Duration,
};

use beryl_backend::{
    BackendWebSocketEndpoint, ForegroundSessionConfig, ManagedBackendClientConnector,
};
use serde_json::Value;
use tungstenite::{Message, WebSocket, accept_hdr};

pub const AUTHORIZATION: &str = "Bearer phase31-bounded-dispatch";
pub const TIMEOUT: Duration = Duration::from_secs(2);
pub const CONFIG_CWD: &str = r"C:\work\beryl";

pub fn foreground_config(pre_bind_control_capacity: usize) -> ForegroundSessionConfig {
    ForegroundSessionConfig::new(
        NonZeroUsize::new(pre_bind_control_capacity)
            .expect("the fixture pre-bind control capacity must be nonzero"),
    )
}

pub fn connector(endpoint: BackendWebSocketEndpoint) -> ManagedBackendClientConnector {
    ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION)
}

pub fn spawn_server<F, T>(script: F) -> (BackendWebSocketEndpoint, thread::JoinHandle<T>)
where
    F: FnOnce(&mut WebSocket<TcpStream>) -> T + Send + 'static,
    T: Send + 'static,
{
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
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
                    AUTHORIZATION,
                );
                Ok(response)
            },
        )
        .unwrap();
        socket.get_mut().set_read_timeout(Some(TIMEOUT)).unwrap();
        script(&mut socket)
    });
    (endpoint, server)
}

pub fn read_json(socket: &mut WebSocket<TcpStream>) -> Option<Value> {
    read_text(socket).map(|text| serde_json::from_str(&text).unwrap())
}

pub fn read_text(socket: &mut WebSocket<TcpStream>) -> Option<String> {
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => return Some(text.to_string()),
            Ok(Message::Close(_)) => return None,
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => {}
            Ok(Message::Binary(bytes)) => panic!("unexpected binary message: {bytes:?}"),
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return None;
            }
            Err(error) => panic!("unexpected server read failure: {error}"),
        }
    }
}

pub fn send_json(socket: &mut WebSocket<TcpStream>, json: &str) {
    socket.send(Message::Text(json.into())).unwrap();
}

pub fn send_initialize_response(socket: &mut WebSocket<TcpStream>, id: u64) {
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"userAgent":"beryl/0.146.0","codexHome":"C:\\codex","platformFamily":"windows","platformOs":"windows"}}}}"#,
        ),
    );
}

pub fn send_empty_model_page(socket: &mut WebSocket<TcpStream>, id: u64) {
    send_json(
        socket,
        &format!(r#"{{"id":{id},"result":{{"data":[],"nextCursor":null}}}}"#),
    );
}

pub fn send_config_response(socket: &mut WebSocket<TcpStream>, id: u64) {
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"config":{{"model":"gpt-5.6","model_reasoning_effort":"high","features":{{"multi_agent_v2":{{"enabled":true,"expose_spawn_agent_model_overrides":true}}}}}},"origins":{{"features.multi_agent_v2.enabled":{{"name":{{"type":"sessionFlags"}},"version":"0"}},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{{"name":{{"type":"sessionFlags"}},"version":"0"}}}}}}}}"#,
        ),
    );
}

pub fn send_recognized_rejection(socket: &mut WebSocket<TcpStream>, id: u64) {
    send_json(
        socket,
        &format!(r#"{{"error":{{"code":-32600,"message":"recognized"}},"id":{id}}}"#),
    );
}

pub fn assert_initialize(request: &Value, request_only: bool) {
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["id"], 1);
    assert_eq!(request["method"], "initialize");
    assert_eq!(request["params"]["clientInfo"]["name"], "beryl");
    assert_eq!(
        request["params"]["clientInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(request["params"]["capabilities"]["experimentalApi"], true);
    let opt_outs = request["params"]["capabilities"].get("optOutNotificationMethods");
    if request_only {
        assert_eq!(
            opt_outs.and_then(Value::as_array).unwrap(),
            &REQUEST_ONLY_NOTIFICATION_OPT_OUTS
                .iter()
                .map(|method| Value::String((*method).to_string()))
                .collect::<Vec<_>>()
        );
    } else {
        assert!(opt_outs.is_none());
    }
}

pub fn assert_initialized(request: &Value) {
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "initialized");
    assert!(request.get("id").is_none());
    assert!(request.get("params").is_none());
}

pub fn expect_close(socket: &mut WebSocket<TcpStream>) {
    assert!(read_json(socket).is_none());
}

const REQUEST_ONLY_NOTIFICATION_OPT_OUTS: [&str; 21] = [
    "thread/started",
    "thread/status/changed",
    "thread/closed",
    "thread/name/updated",
    "thread/tokenUsage/updated",
    "account/rateLimits/updated",
    "turn/started",
    "turn/completed",
    "turn/diff/updated",
    "item/started",
    "item/completed",
    "item/agentMessage/delta",
    "item/plan/delta",
    "item/reasoning/summaryPartAdded",
    "item/reasoning/summaryTextDelta",
    "item/reasoning/textDelta",
    "item/commandExecution/outputDelta",
    "item/fileChange/outputDelta",
    "item/fileChange/patchUpdated",
    "item/mcpToolCall/progress",
    "codex/event/collab_agent_spawn_end",
];
