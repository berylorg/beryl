use std::{
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

use beryl_backend::{BackendWebSocketEndpoint, CompatibilityProbe};
use serde_json::Value;
use tungstenite::{Message, WebSocket, accept_hdr};

use crate::raw_wire::{
    RequestCutoff, RequestOutcome, await_masked_client_close, verify_masked_text_message,
    write_unmasked_text_message,
};

pub const AUTHORIZATION: &str = "Bearer phase40-recovery-page-residency";
pub const TARGET_THREAD: &str = "phase40-recovery-target";
pub const TIMEOUT: Duration = Duration::from_secs(30);

pub struct RecoveryServer {
    endpoint: BackendWebSocketEndpoint,
    handle: Option<thread::JoinHandle<()>>,
}

enum ServerScenario {
    Complete {
        expected_user: &'static str,
        expected_assistant: &'static str,
    },
    Cancellation,
}

impl RecoveryServer {
    #[must_use]
    pub fn spawn(expected_user: &'static str, expected_assistant: &'static str) -> Self {
        Self::spawn_scenario(ServerScenario::Complete {
            expected_user,
            expected_assistant,
        })
    }

    #[must_use]
    pub fn spawn_cancellation() -> Self {
        Self::spawn_scenario(ServerScenario::Cancellation)
    }

    fn spawn_scenario(scenario: ServerScenario) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let handle = thread::Builder::new()
            .name("phase40-recovery-cas".to_string())
            .spawn(move || run(listener, scenario))
            .unwrap();
        Self {
            endpoint,
            handle: Some(handle),
        }
    }

    pub fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }

    pub fn join(mut self) {
        self.handle.take().unwrap().join().unwrap();
    }
}

fn run(listener: TcpListener, scenario: ServerScenario) {
    let (stream, _) = listener.accept().unwrap();
    let mut socket = accept_hdr(
        stream,
        |request: &tungstenite::handshake::server::Request, response| {
            assert_eq!(
                request
                    .headers()
                    .get("authorization")
                    .expect("client supplies authorization")
                    .to_str()
                    .unwrap(),
                AUTHORIZATION,
            );
            Ok(response)
        },
    )
    .unwrap();
    socket.get_mut().set_read_timeout(Some(TIMEOUT)).unwrap();
    socket.get_mut().set_write_timeout(Some(TIMEOUT)).unwrap();

    complete_admission(&mut socket);
    let request_id = complete_fresh_target(&mut socket);
    match scenario {
        ServerScenario::Complete {
            expected_user,
            expected_assistant,
        } => {
            socket.flush().unwrap();
            let mut stream = socket.into_inner();
            complete_injection(&mut stream, request_id, expected_user, expected_assistant);
            await_masked_client_close(&mut stream).unwrap();
        }
        ServerScenario::Cancellation => {
            complete_unsubscribe(&mut socket);
            await_close(&mut socket);
        }
    }
}

fn complete_admission(socket: &mut WebSocket<TcpStream>) {
    let initialize = read_json(socket).expect("initialize request");
    assert_eq!(initialize["method"], "initialize");
    let initialize_id = initialize["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(
            r#"{{"id":{initialize_id},"result":{{"userAgent":"beryl/0.146.0","codexHome":"C:\\codex","platformFamily":"windows","platformOs":"windows"}}}}"#,
        ),
    );
    let initialized = read_json(socket).expect("initialized notification");
    assert_eq!(initialized["method"], "initialized");

    for probe in CompatibilityProbe::ALL {
        let request = read_json(socket).expect("compatibility probe request");
        assert_eq!(request["method"], probe.method());
        let id = request["id"].as_u64().unwrap();
        let response = match probe {
            CompatibilityProbe::ConfigRead => format!(
                r#"{{"id":{id},"result":{{"config":{{"model":"gpt-5.6","model_reasoning_effort":"high","features":{{"multi_agent_v2":{{"enabled":true,"expose_spawn_agent_model_overrides":true}}}}}},"origins":{{"features.multi_agent_v2.enabled":{{"name":{{"type":"sessionFlags"}},"version":"0"}},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{{"name":{{"type":"sessionFlags"}},"version":"0"}}}}}}}}"#,
            ),
            CompatibilityProbe::ModelList => {
                format!(r#"{{"id":{id},"result":{{"data":[],"nextCursor":null}}}}"#)
            }
            CompatibilityProbe::ThreadUnsubscribe => {
                format!(r#"{{"id":{id},"result":{{"status":"notLoaded"}}}}"#)
            }
            _ => format!(r#"{{"error":{{"code":-32600,"message":"recognized"}},"id":{id}}}"#),
        };
        send_json(socket, &response);
    }
}

fn complete_fresh_target(socket: &mut WebSocket<TcpStream>) -> u64 {
    let request = read_json(socket).expect("recovery thread/start request");
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/start");
    assert_eq!(request["params"]["ephemeral"], false);
    assert!(request["params"].get("input").is_none());
    let id = request["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"thread":{{"id":"{TARGET_THREAD}","extra":null,"sessionId":"session-id","forkedFromId":null,"parentThreadId":null,"preview":"preview","ephemeral":false,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{{"type":"idle"}},"path":null,"cwd":"C:\\work\\beryl","cliVersion":"0.146.0","source":"appServer","threadSource":null,"agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[]}},"model":"gpt-5.6","modelProvider":"openai","serviceTier":null,"cwd":"C:\\work\\beryl","runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user","sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":"high","multiAgentMode":"explicitRequestOnly"}}}}"#,
        ),
    );
    id.checked_add(1)
        .expect("injection request id follows start")
}

fn complete_injection(
    stream: &mut TcpStream,
    request_id: u64,
    expected_user: &str,
    expected_assistant: &str,
) {
    let request_id_text = request_id.to_string();
    let pieces = [
        r#"{"jsonrpc":"2.0","id":"#,
        request_id_text.as_str(),
        r#","method":"thread/inject_items","params":{"threadId":""#,
        TARGET_THREAD,
        r#"","items":[{"type":"message","role":"user","content":[{"type":"input_text","text":""#,
        expected_user,
        r#""}]},{"type":"message","role":"assistant","content":[{"type":"output_text","text":""#,
        expected_assistant,
        r#""}]}]}}"#,
    ];
    let outcome = verify_masked_text_message(
        stream,
        request_id,
        pieces.into_iter().flat_map(|piece| piece.bytes()),
        RequestCutoff::None,
    )
    .unwrap();
    assert!(matches!(outcome, RequestOutcome::Complete(_)));

    let response = [r#"{"id":"#, request_id_text.as_str(), r#","result":{}}"#];
    write_unmasked_text_message(stream, response.into_iter().flat_map(|piece| piece.bytes()))
        .unwrap();
}

fn complete_unsubscribe(socket: &mut WebSocket<TcpStream>) {
    let request = read_json(socket).expect("abandoned recovery target unsubscribe");
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/unsubscribe");
    assert_eq!(request["params"]["threadId"], TARGET_THREAD);
    let id = request["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(r#"{{"id":{id},"result":{{"status":"unsubscribed"}}}}"#),
    );
}

fn read_json(socket: &mut WebSocket<TcpStream>) -> Option<Value> {
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => return Some(serde_json::from_str(&text).unwrap()),
            Ok(Message::Close(_)) => return None,
            Ok(Message::Ping(payload)) => socket.send(Message::Pong(payload)).unwrap(),
            Ok(Message::Pong(_) | Message::Frame(_)) => {}
            Ok(Message::Binary(bytes)) => panic!("unexpected binary message: {bytes:?}"),
            Err(error) => panic!("phase40 server read failed: {error}"),
        }
    }
}

fn send_json(socket: &mut WebSocket<TcpStream>, value: &str) {
    socket.send(Message::Text(value.into())).unwrap();
}

fn await_close(socket: &mut WebSocket<TcpStream>) {
    loop {
        match socket.read() {
            Ok(Message::Close(_)) => return,
            Ok(Message::Ping(payload)) => socket.send(Message::Pong(payload)).unwrap(),
            Ok(Message::Pong(_) | Message::Frame(_)) => {}
            Ok(message) => panic!("unexpected post-recovery message: {message:?}"),
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => return,
            Err(error) => panic!("phase40 server close wait failed: {error}"),
        }
    }
}
