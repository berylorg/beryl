//! Raw-wire CAS server for streamed recovery injection requests.

use std::{
    net::{TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::Duration,
};

use beryl_backend::{BackendWebSocketEndpoint, CompatibilityProbe};
use serde_json::Value;
use tungstenite::{Message, WebSocket, accept_hdr, protocol::Role};

use crate::{
    history::{ExpectedInjection, HistorySpec},
    raw_wire::{
        RequestAbortReason, RequestCutoff, RequestOutcome, await_masked_client_close,
        verify_masked_text_message, write_unmasked_text_message,
    },
};

pub const AUTHORIZATION: &str = "Bearer phase44-recovery-history-stress";
pub const TIMEOUT: Duration = Duration::from_secs(60);

const TARGET_THREAD_PREFIX: &str = "phase44-recovery-target-";

pub struct RecoveryServer {
    endpoint: BackendWebSocketEndpoint,
    handle: Option<thread::JoinHandle<()>>,
}

enum ServerScenario {
    Success(Arc<Vec<HistorySpec>>),
    NoDispatch,
    PredispatchCancellation,
    PostFirstPageCancellation(Arc<HistorySpec>),
}

impl RecoveryServer {
    #[must_use]
    pub fn spawn_success(histories: Arc<Vec<HistorySpec>>) -> Self {
        Self::spawn(ServerScenario::Success(histories))
    }

    #[must_use]
    pub fn spawn_no_dispatch() -> Self {
        Self::spawn(ServerScenario::NoDispatch)
    }

    #[must_use]
    pub fn spawn_predispatch_cancellation() -> Self {
        Self::spawn(ServerScenario::PredispatchCancellation)
    }

    #[must_use]
    pub fn spawn_post_first_page_cancellation(history: Arc<HistorySpec>) -> Self {
        Self::spawn(ServerScenario::PostFirstPageCancellation(history))
    }

    fn spawn(scenario: ServerScenario) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let handle = thread::Builder::new()
            .name("phase44-recovery-cas".to_string())
            .spawn(move || run(listener, scenario))
            .unwrap();
        Self {
            endpoint,
            handle: Some(handle),
        }
    }

    #[must_use]
    pub fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }

    pub fn join(mut self) {
        self.handle.take().unwrap().join().unwrap();
    }
}

#[must_use]
pub fn target_thread(ordinal: usize) -> String {
    format!("{TARGET_THREAD_PREFIX}{ordinal}")
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
    match scenario {
        ServerScenario::Success(histories) => complete_successes(socket, &histories),
        ServerScenario::NoDispatch => await_close(&mut socket),
        ServerScenario::PredispatchCancellation => {
            let target = target_thread(1);
            complete_fresh_target(&mut socket, &target);
            complete_unsubscribe(&mut socket, &target);
            await_close(&mut socket);
        }
        ServerScenario::PostFirstPageCancellation(history) => {
            let target = target_thread(1);
            let injection_id = complete_fresh_target(&mut socket, &target);
            socket.flush().unwrap();
            let mut stream = socket.into_inner();
            let expected = ExpectedInjection::new(injection_id, &target, &history);
            let expected_bytes = expected.logical_bytes();
            let outcome = verify_masked_text_message(
                &mut stream,
                injection_id,
                expected,
                RequestCutoff::None,
            )
            .unwrap();
            let RequestOutcome::Aborted(observation) = outcome else {
                panic!("post-first-page cancellation unexpectedly completed its request")
            };
            assert_eq!(observation.request_id(), injection_id);
            assert!(observation.compared_bytes() > 0);
            assert!(observation.compared_bytes() < expected_bytes);
            assert!(observation.frame_count() >= 1);
            assert_eq!(observation.maximum_frame_payload_bytes(), 65_536);
            assert!(matches!(
                observation.reason(),
                RequestAbortReason::PeerClose | RequestAbortReason::TransportEof
            ));
        }
    }
}

fn complete_successes(mut socket: WebSocket<TcpStream>, histories: &[HistorySpec]) {
    assert!(!histories.is_empty());
    for (index, history) in histories.iter().enumerate() {
        let target = target_thread(index + 1);
        let injection_id = complete_fresh_target(&mut socket, &target);
        socket.flush().unwrap();
        let mut stream = socket.into_inner();
        complete_injection(&mut stream, injection_id, &target, history);
        if index + 1 == histories.len() {
            await_masked_client_close(&mut stream).unwrap();
            return;
        }
        socket = WebSocket::from_raw_socket(stream, Role::Server, None);
    }
}

fn complete_injection(
    stream: &mut TcpStream,
    request_id: u64,
    target: &str,
    history: &HistorySpec,
) {
    let expected = ExpectedInjection::new(request_id, target, history);
    let expected_bytes = expected.logical_bytes();
    let outcome =
        verify_masked_text_message(stream, request_id, expected, RequestCutoff::None).unwrap();
    let RequestOutcome::Complete(observation) = outcome else {
        panic!("phase44 success request was aborted: {outcome:?}")
    };
    assert_eq!(observation.request_id(), request_id);
    assert_eq!(observation.logical_bytes(), expected_bytes);
    assert!(observation.frame_count() >= 5);
    assert_eq!(observation.maximum_frame_payload_bytes(), 65_536);

    let request_id_text = request_id.to_string();
    let response = [r#"{"id":"#, request_id_text.as_str(), r#","result":{}}"#];
    write_unmasked_text_message(stream, response.into_iter().flat_map(str::bytes)).unwrap();
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

fn complete_fresh_target(socket: &mut WebSocket<TcpStream>, target: &str) -> u64 {
    let request = read_json(socket).expect("recovery thread/start request");
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/start");
    assert_eq!(request["params"]["ephemeral"], false);
    assert!(request["params"].get("input").is_none());
    let id = request["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"thread":{{"id":"{target}","extra":null,"sessionId":"session-id","forkedFromId":null,"parentThreadId":null,"preview":"preview","ephemeral":false,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{{"type":"idle"}},"path":null,"cwd":"C:\\work\\beryl","cliVersion":"0.146.0","source":"appServer","threadSource":null,"agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[]}},"model":"gpt-5.6","modelProvider":"openai","serviceTier":null,"cwd":"C:\\work\\beryl","runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user","sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":"high","multiAgentMode":"explicitRequestOnly"}}}}"#,
        ),
    );
    id.checked_add(1)
        .expect("injection request id follows start")
}

fn complete_unsubscribe(socket: &mut WebSocket<TcpStream>, target: &str) {
    let request = read_json(socket).expect("abandoned recovery target unsubscribe");
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/unsubscribe");
    assert_eq!(request["params"]["threadId"], target);
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
            Err(error) => panic!("phase44 server read failed: {error}"),
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
            Err(error) => panic!("phase44 server close wait failed: {error}"),
        }
    }
}
