use std::{
    io,
    net::{Shutdown, TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::Duration,
};

use super::generator::{
    WireMessage, finish_observation, probe_while_blocked, wait_for_pong, write_missing_text,
    write_observation, write_prefix,
};

use beryl_backend::{BackendWebSocketEndpoint, CompatibilityProbe};
use serde_json::Value;
use tungstenite::{Message, WebSocket, accept_hdr};

pub const AUTHORIZATION: &str = "Bearer phase35-provider-residency";
pub const CAS_THREAD_ID: &str = "phase35-cas-thread";
pub const CAS_TURN_ID: &str = "phase35-cas-turn";
pub const TIMEOUT: Duration = Duration::from_secs(10);

pub use super::generator::SEMANTIC_PATTERN;

#[derive(Clone, Copy, Debug)]
pub struct ObservationSpec {
    pub sequence: u64,
    pub pattern_repetitions: u64,
}

impl ObservationSpec {
    pub const fn new(sequence: u64, pattern_repetitions: u64) -> Self {
        Self {
            sequence,
            pattern_repetitions,
        }
    }

    pub fn item_id(self) -> String {
        format!("phase35-item-{}", self.sequence)
    }

    pub fn semantic_bytes(self) -> u64 {
        self.pattern_repetitions
            .checked_mul(u64::try_from(SEMANTIC_PATTERN.len()).unwrap())
            .unwrap()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservationReport {
    pub sequence: u64,
    pub wire_bytes: u64,
    pub semantic_bytes: u64,
    pub frame_count: u64,
}

enum ServerCommand {
    Observation(ObservationSpec),
    BeginBackpressure(ObservationSpec),
    ProbeBackpressure,
    MissingText { sequence: u64 },
    Close,
}

#[derive(Debug)]
enum ServerEvent {
    ProjectionReady,
    Observation(ObservationReport),
    BackpressurePrefix,
    NoPongWhileBlocked,
    PongAfterRelease,
    SchemaSent,
    Closed,
}

pub struct ProviderServer {
    endpoint: BackendWebSocketEndpoint,
    commands: SyncSender<ServerCommand>,
    events: Receiver<ServerEvent>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ProviderServer {
    pub fn spawn() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let (commands, command_receiver) = mpsc::sync_channel(1);
        let (event_sender, events) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("phase35-provider-server".to_owned())
            .spawn(move || run_server(listener, command_receiver, event_sender))
            .unwrap();
        Self {
            endpoint,
            commands,
            events,
            handle: Some(handle),
        }
    }

    pub fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }

    pub fn wait_for_projection(&self) {
        self.expect_event(ServerEvent::ProjectionReady);
    }

    pub fn send_observation(&self, spec: ObservationSpec) -> ObservationReport {
        self.commands
            .send(ServerCommand::Observation(spec))
            .unwrap();
        self.expect_observation(spec.sequence)
    }

    pub fn begin_backpressure(&self, spec: ObservationSpec) {
        self.commands
            .send(ServerCommand::BeginBackpressure(spec))
            .unwrap();
        self.expect_event(ServerEvent::BackpressurePrefix);
    }

    pub fn probe_backpressure(&self) {
        self.commands
            .send(ServerCommand::ProbeBackpressure)
            .unwrap();
    }

    pub fn wait_for_no_pong(&self) {
        self.expect_event(ServerEvent::NoPongWhileBlocked);
    }

    pub fn finish_backpressure(&self, sequence: u64) -> ObservationReport {
        self.expect_event(ServerEvent::PongAfterRelease);
        self.expect_observation(sequence)
    }

    pub fn send_missing_text(&self, sequence: u64) {
        self.commands
            .send(ServerCommand::MissingText { sequence })
            .unwrap();
        self.expect_event(ServerEvent::SchemaSent);
    }

    pub fn request_close(&self) {
        self.commands.send(ServerCommand::Close).unwrap();
    }

    pub fn join(mut self) {
        self.expect_event(ServerEvent::Closed);
        self.handle.take().unwrap().join().unwrap();
    }

    fn expect_observation(&self, sequence: u64) -> ObservationReport {
        match self.events.recv_timeout(TIMEOUT).unwrap() {
            ServerEvent::Observation(report) if report.sequence == sequence => report,
            event => panic!("expected observation {sequence}, got {event:?}"),
        }
    }

    fn expect_event(&self, expected: ServerEvent) {
        let actual = self
            .events
            .recv_timeout(TIMEOUT)
            .unwrap_or_else(|error| panic!("timed out waiting for {expected:?}: {error}"));
        assert_eq!(
            std::mem::discriminant(&actual),
            std::mem::discriminant(&expected)
        );
    }
}

fn run_server(
    listener: TcpListener,
    commands: Receiver<ServerCommand>,
    events: SyncSender<ServerEvent>,
) {
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
    complete_admission(&mut socket);
    complete_projection(&mut socket);
    events.send(ServerEvent::ProjectionReady).unwrap();

    let mut pending = None;
    while let Ok(command) = commands.recv() {
        match command {
            ServerCommand::Observation(spec) => {
                assert!(pending.is_none());
                let report = write_observation(&mut socket, spec);
                events.send(ServerEvent::Observation(report)).unwrap();
            }
            ServerCommand::BeginBackpressure(spec) => {
                assert!(pending.is_none());
                let mut message = WireMessage::new(spec);
                write_prefix(&mut socket, &mut message);
                pending = Some(message);
                events.send(ServerEvent::BackpressurePrefix).unwrap();
            }
            ServerCommand::ProbeBackpressure => {
                let mut message = pending.take().expect("backpressure message is pending");
                probe_while_blocked(&mut socket, &mut message);
                events.send(ServerEvent::NoPongWhileBlocked).unwrap();
                wait_for_pong(&mut socket);
                events.send(ServerEvent::PongAfterRelease).unwrap();
                let report = finish_observation(&mut socket, message);
                events.send(ServerEvent::Observation(report)).unwrap();
            }
            ServerCommand::MissingText { sequence } => {
                assert!(pending.is_none());
                write_missing_text(&mut socket, sequence);
                events.send(ServerEvent::SchemaSent).unwrap();
            }
            ServerCommand::Close => {
                let _ = read_until_close(&mut socket);
                events.send(ServerEvent::Closed).unwrap();
                return;
            }
        }
    }
    let _ = socket.get_mut().shutdown(Shutdown::Both);
    let _ = events.send(ServerEvent::Closed);
}

fn complete_admission(socket: &mut WebSocket<TcpStream>) {
    let initialize = read_json(socket).unwrap();
    assert_eq!(initialize["method"], "initialize");
    send_json(
        socket,
        r#"{"id":1,"result":{"userAgent":"beryl/0.146.0","codexHome":"C:\\codex","platformFamily":"windows","platformOs":"windows"}}"#,
    );
    let initialized = read_json(socket).unwrap();
    assert_eq!(initialized["method"], "initialized");

    for probe in CompatibilityProbe::ALL {
        let request = read_json(socket).unwrap();
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
            _ => format!(r#"{{"error":{{"code":-32600,"message":"recognized"}},"id":{id}}}"#,),
        };
        send_json(socket, &response);
    }
}

fn complete_projection(socket: &mut WebSocket<TcpStream>) {
    let request = read_json(socket).unwrap();
    assert_eq!(request["method"], "thread/start");
    assert_eq!(request["params"]["ephemeral"], false);
    let id = request["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"thread":{{"id":"{CAS_THREAD_ID}","extra":null,"sessionId":"session-id","forkedFromId":null,"parentThreadId":null,"preview":"preview","ephemeral":false,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{{"type":"idle"}},"path":null,"cwd":"C:\\work\\beryl","cliVersion":"0.146.0","source":"appServer","threadSource":null,"agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[]}},"model":"gpt-5.6","modelProvider":"openai","serviceTier":null,"cwd":"C:\\work\\beryl","runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user","sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":"high","multiAgentMode":"explicitRequestOnly"}}}}"#,
        ),
    );
}

fn read_json(socket: &mut WebSocket<TcpStream>) -> Option<Value> {
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => return Some(serde_json::from_str(&text).unwrap()),
            Ok(Message::Close(_)) => return None,
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => {}
            Ok(Message::Binary(bytes)) => panic!("unexpected binary frame: {bytes:?}"),
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return None;
            }
            Err(error) => panic!("provider server read failed: {error}"),
        }
    }
}

fn send_json(socket: &mut WebSocket<TcpStream>, value: &str) {
    socket.send(Message::Text(value.into())).unwrap();
}

fn read_until_close(socket: &mut WebSocket<TcpStream>) -> Result<(), tungstenite::Error> {
    socket.get_mut().set_read_timeout(Some(TIMEOUT)).unwrap();
    loop {
        match socket.read() {
            Ok(Message::Close(_))
            | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(());
            }
            Ok(Message::Ping(payload)) => socket.send(Message::Pong(payload))?,
            Ok(Message::Pong(_) | Message::Frame(_)) => {}
            Ok(Message::Text(text)) => panic!("unexpected closing client text: {text}"),
            Ok(Message::Binary(bytes)) => panic!("unexpected closing client binary: {bytes:?}"),
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::UnexpectedEof
                ) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        }
    }
}
