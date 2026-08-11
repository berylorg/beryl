use std::{
    io,
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::Duration,
};

use beryl_backend::BackendWebSocketEndpoint;
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, accept_hdr};

pub const AUTHORIZATION: &str = "Bearer phase37-normal-terminal";
pub const CAS_THREAD_ID: &str = "phase37-cas-thread";
pub const CAS_TURN_ID: &str = "phase37-cas-turn";
pub const CAS_ITEM_ID: &str = "phase37-user-item";
pub const CAS_COMMAND_ITEM_ID: &str = "phase70-command-item";
pub const SUBMITTED_TEXT: &str = "phase37 ordinary terminal success";
pub const STARTED_AT_MS: u64 = 37_002;
pub const COMPLETED_AT_MS: u64 = 37_003;
pub const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServerEvent {
    AdmissionReady,
    ProjectionReady,
    TurnStartObserved,
    Closed,
}

enum ServerScenario {
    AdmissionOnly,
    AdmissionOnlyControlledClose,
    Terminal,
    RecoveryTerminal(Vec<Value>),
    ResumeTerminal(Box<str>),
    ResumeDelayedRejection(Box<str>),
    ConnectionLoss,
    SteeringCorrelationLoss,
}

enum ServerCommand {
    AssertQuietAndClose,
    ReleaseTurnStartRejection,
    SendSteeringCorrelationLoss(String),
}

#[derive(Clone)]
pub struct SteeringFailureTrigger {
    commands: SyncSender<ServerCommand>,
}

impl SteeringFailureTrigger {
    pub fn send(self, correlation: String) {
        self.commands
            .send(ServerCommand::SendSteeringCorrelationLoss(correlation))
            .unwrap();
    }
}

pub struct NormalTerminalServer {
    endpoint: BackendWebSocketEndpoint,
    events: Receiver<ServerEvent>,
    commands: SyncSender<ServerCommand>,
    handle: Option<thread::JoinHandle<()>>,
}

impl NormalTerminalServer {
    pub fn spawn() -> Self {
        Self::spawn_scenario(ServerScenario::Terminal)
    }

    pub fn spawn_recovery_terminal(expected_items: Vec<Value>) -> Self {
        Self::spawn_scenario(ServerScenario::RecoveryTerminal(expected_items))
    }

    pub fn spawn_resume_terminal(cas_thread_id: impl Into<Box<str>>) -> Self {
        Self::spawn_scenario(ServerScenario::ResumeTerminal(cas_thread_id.into()))
    }

    pub fn spawn_resume_delayed_rejection(cas_thread_id: impl Into<Box<str>>) -> Self {
        Self::spawn_scenario(ServerScenario::ResumeDelayedRejection(cas_thread_id.into()))
    }

    pub fn spawn_connection_loss() -> Self {
        Self::spawn_scenario(ServerScenario::ConnectionLoss)
    }

    pub fn spawn_steering_correlation_loss() -> Self {
        Self::spawn_scenario(ServerScenario::SteeringCorrelationLoss)
    }

    pub fn spawn_admission_only() -> Self {
        Self::spawn_scenario(ServerScenario::AdmissionOnly)
    }

    pub fn spawn_admission_only_controlled_close() -> Self {
        Self::spawn_scenario(ServerScenario::AdmissionOnlyControlledClose)
    }

    fn spawn_scenario(scenario: ServerScenario) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let (event_sender, events) = mpsc::sync_channel(1);
        let (commands, command_receiver) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("phase37-normal-terminal-server".to_owned())
            .spawn(move || run_server(listener, event_sender, command_receiver, scenario))
            .unwrap();
        Self {
            endpoint,
            events,
            commands,
            handle: Some(handle),
        }
    }

    pub fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }

    pub fn wait_for_projection(&self) {
        self.expect(ServerEvent::ProjectionReady);
    }

    pub fn wait_for_admission(&self) {
        self.expect(ServerEvent::AdmissionReady);
    }

    pub fn wait_for_turn_start(&self) {
        self.expect(ServerEvent::TurnStartObserved);
    }

    pub fn release_turn_start_rejection(&self) {
        self.commands
            .send(ServerCommand::ReleaseTurnStartRejection)
            .unwrap();
    }

    pub fn steering_failure_trigger(&self) -> SteeringFailureTrigger {
        SteeringFailureTrigger {
            commands: self.commands.clone(),
        }
    }

    pub fn assert_quiet_and_close(&self) {
        self.commands
            .send(ServerCommand::AssertQuietAndClose)
            .unwrap();
    }

    pub fn join(mut self) {
        self.expect(ServerEvent::Closed);
        self.handle.take().unwrap().join().unwrap();
    }

    fn expect(&self, expected: ServerEvent) {
        let actual = self
            .events
            .recv_timeout(TIMEOUT)
            .unwrap_or_else(|error| panic!("timed out waiting for {expected:?}: {error}"));
        assert_eq!(actual, expected);
    }
}

fn run_server(
    listener: TcpListener,
    events: SyncSender<ServerEvent>,
    commands: Receiver<ServerCommand>,
    scenario: ServerScenario,
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
    match scenario {
        ServerScenario::AdmissionOnly => {
            events.send(ServerEvent::AdmissionReady).unwrap();
            read_until_close(&mut socket).unwrap();
        }
        ServerScenario::AdmissionOnlyControlledClose => {
            events.send(ServerEvent::AdmissionReady).unwrap();
            assert!(matches!(
                commands.recv_timeout(TIMEOUT).unwrap(),
                ServerCommand::AssertQuietAndClose
            ));
            assert_quiet_then_close(&mut socket);
        }
        ServerScenario::Terminal => {
            complete_projection(&mut socket);
            events.send(ServerEvent::ProjectionReady).unwrap();
            complete_ordinary_turn(&mut socket, CAS_THREAD_ID);
            read_until_close(&mut socket).unwrap();
        }
        ServerScenario::RecoveryTerminal(expected_items) => {
            complete_projection(&mut socket);
            complete_recovery_injection(&mut socket, &expected_items);
            events.send(ServerEvent::ProjectionReady).unwrap();
            complete_ordinary_turn_and_report(&mut socket, &events, CAS_THREAD_ID);
            complete_unsubscribe(&mut socket, CAS_THREAD_ID);
            read_until_close(&mut socket).unwrap();
        }
        ServerScenario::ResumeTerminal(cas_thread_id) => {
            complete_resume_projection(&mut socket, &cas_thread_id);
            events.send(ServerEvent::ProjectionReady).unwrap();
            complete_ordinary_turn(&mut socket, &cas_thread_id);
            complete_unsubscribe(&mut socket, &cas_thread_id);
            read_until_close(&mut socket).unwrap();
        }
        ServerScenario::ResumeDelayedRejection(cas_thread_id) => {
            complete_resume_projection(&mut socket, &cas_thread_id);
            events.send(ServerEvent::ProjectionReady).unwrap();
            complete_delayed_turn_start_rejection(&mut socket, &events, &commands, &cas_thread_id);
            read_until_close(&mut socket).unwrap();
        }
        ServerScenario::ConnectionLoss => {
            complete_projection(&mut socket);
            events.send(ServerEvent::ProjectionReady).unwrap();
            complete_connection_loss(&mut socket, CAS_THREAD_ID);
        }
        ServerScenario::SteeringCorrelationLoss => {
            complete_projection(&mut socket);
            events.send(ServerEvent::ProjectionReady).unwrap();
            complete_steering_correlation_loss(&mut socket, &commands, CAS_THREAD_ID);
        }
    }
    events.send(ServerEvent::Closed).unwrap();
}

fn complete_admission(socket: &mut WebSocket<TcpStream>) {
    let initialize = read_json(socket).unwrap();
    assert_eq!(initialize["method"], "initialize");
    let initialize_id = initialize["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(
            r#"{{"id":{initialize_id},"result":{{"userAgent":"beryl/0.146.0","codexHome":"C:\\codex","platformFamily":"windows","platformOs":"windows"}}}}"#,
        ),
    );
    let initialized = read_json(socket).unwrap();
    assert_eq!(initialized["method"], "initialized");

    let request = read_json(socket).unwrap();
    assert_eq!(request["method"], "config/read");
    let id = request["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"config":{{"model":"gpt-5.6","model_reasoning_effort":"high","features":{{"multi_agent_v2":{{"enabled":true,"expose_spawn_agent_model_overrides":true}}}}}},"origins":{{"features.multi_agent_v2.enabled":{{"name":{{"type":"sessionFlags"}},"version":"0"}},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{{"name":{{"type":"sessionFlags"}},"version":"0"}}}}}}}}"#
        ),
    );
}

fn complete_projection(socket: &mut WebSocket<TcpStream>) {
    let request = read_json(socket).unwrap();
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/start");
    assert_eq!(request["params"]["ephemeral"], false);
    assert!(request["params"].get("input").is_none());
    let id = request["id"].as_u64().unwrap();
    send_thread_load_response(socket, id, CAS_THREAD_ID, false);
}

fn complete_resume_projection(socket: &mut WebSocket<TcpStream>, cas_thread_id: &str) {
    let request = read_json(socket).unwrap();
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/resume");
    assert_eq!(request["params"]["threadId"], cas_thread_id);
    let id = request["id"].as_u64().unwrap();
    send_thread_load_response(socket, id, cas_thread_id, true);
}

fn complete_recovery_injection(socket: &mut WebSocket<TcpStream>, expected_items: &[Value]) {
    let request = read_json(socket).expect("recovery injection request");
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/inject_items");
    assert_eq!(request["params"]["threadId"], CAS_THREAD_ID);
    assert_eq!(
        request["params"]["items"],
        Value::Array(expected_items.to_vec())
    );
    let id = request["id"].as_u64().unwrap();
    send_json(socket, &format!(r#"{{"id":{id},"result":{{}}}}"#));
}

fn send_thread_load_response(
    socket: &mut WebSocket<TcpStream>,
    id: u64,
    cas_thread_id: &str,
    resumed: bool,
) {
    let initial_turns_page = if resumed {
        r#","initialTurnsPage":null,"turnsBackwardsCursor":null,"itemsBackwardsCursor":null"#
    } else {
        ""
    };
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"thread":{{"id":"{cas_thread_id}","extra":null,"sessionId":"session-id","forkedFromId":null,"parentThreadId":null,"preview":"preview","ephemeral":false,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{{"type":"idle"}},"path":null,"cwd":"C:\\work\\beryl","cliVersion":"0.146.0","source":"appServer","threadSource":null,"agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[]}},"model":"gpt-5.6","modelProvider":"openai","serviceTier":null,"cwd":"C:\\work\\beryl","runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user","sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":"high","multiAgentMode":"explicitRequestOnly"{initial_turns_page}}}}}"#,
        ),
    );
}

fn complete_ordinary_turn(socket: &mut WebSocket<TcpStream>, cas_thread_id: &str) {
    let id = read_ordinary_turn_start(socket, cas_thread_id);
    finish_ordinary_turn(socket, cas_thread_id, id);
}

fn complete_ordinary_turn_and_report(
    socket: &mut WebSocket<TcpStream>,
    events: &SyncSender<ServerEvent>,
    cas_thread_id: &str,
) {
    let id = read_ordinary_turn_start(socket, cas_thread_id);
    events.send(ServerEvent::TurnStartObserved).unwrap();
    finish_ordinary_turn(socket, cas_thread_id, id);
}

fn finish_ordinary_turn(socket: &mut WebSocket<TcpStream>, cas_thread_id: &str, id: u64) {
    send_checked_user(
        socket,
        cas_thread_id,
        "item/started",
        "startedAtMs",
        STARTED_AT_MS,
    );
    send_checked_user(
        socket,
        cas_thread_id,
        "item/completed",
        "completedAtMs",
        COMPLETED_AT_MS,
    );
    send_turn_start_response(socket, id);
    send_json(socket, &terminal_wire_for(cas_thread_id));
}

fn complete_delayed_turn_start_rejection(
    socket: &mut WebSocket<TcpStream>,
    events: &SyncSender<ServerEvent>,
    commands: &Receiver<ServerCommand>,
    cas_thread_id: &str,
) {
    let id = read_ordinary_turn_start(socket, cas_thread_id);
    events.send(ServerEvent::TurnStartObserved).unwrap();
    assert!(matches!(
        commands.recv_timeout(TIMEOUT).unwrap(),
        ServerCommand::ReleaseTurnStartRejection
    ));
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"error":{{"code":-32602,"message":"phase63 exact turn/start rejection"}}}}"#
        ),
    );
}

fn complete_unsubscribe(socket: &mut WebSocket<TcpStream>, cas_thread_id: &str) {
    let request = read_json(socket).expect("scheduled projection unsubscribe");
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/unsubscribe");
    assert_eq!(request["params"]["threadId"], cas_thread_id);
    let id = request["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(r#"{{"id":{id},"result":{{"status":"unsubscribed"}}}}"#),
    );
}

fn complete_connection_loss(socket: &mut WebSocket<TcpStream>, cas_thread_id: &str) {
    let id = read_ordinary_turn_start(socket, cas_thread_id);
    send_checked_user(
        socket,
        cas_thread_id,
        "item/started",
        "startedAtMs",
        STARTED_AT_MS,
    );
    send_checked_user(
        socket,
        cas_thread_id,
        "item/completed",
        "completedAtMs",
        COMPLETED_AT_MS,
    );
    send_turn_start_response(socket, id);
    socket.close(None).unwrap();
}

fn complete_steering_correlation_loss(
    socket: &mut WebSocket<TcpStream>,
    commands: &Receiver<ServerCommand>,
    cas_thread_id: &str,
) {
    let id = read_ordinary_turn_start(socket, cas_thread_id);
    send_checked_user(
        socket,
        cas_thread_id,
        "item/started",
        "startedAtMs",
        STARTED_AT_MS,
    );
    send_checked_user(
        socket,
        cas_thread_id,
        "item/completed",
        "completedAtMs",
        COMPLETED_AT_MS,
    );
    send_turn_start_response(socket, id);
    let correlation = match commands
        .recv_timeout(TIMEOUT)
        .expect("steering-loss test must supply the accepted-input correlation")
    {
        ServerCommand::SendSteeringCorrelationLoss(correlation) => correlation,
        ServerCommand::AssertQuietAndClose | ServerCommand::ReleaseTurnStartRejection => {
            panic!("steering-loss server received a turn-start rejection release")
        }
    };
    let steering = read_json(socket).expect("steering delivery must send one turn/steer request");
    assert_eq!(steering["jsonrpc"], "2.0");
    assert_eq!(steering["method"], "turn/steer");
    assert_eq!(steering["params"]["threadId"], cas_thread_id);
    assert_eq!(steering["params"]["expectedTurnId"], CAS_TURN_ID);
    assert_eq!(
        steering["params"]["clientUserMessageId"],
        correlation.as_str()
    );
    send_json(
        socket,
        &format!(
            r#"{{"method":"item/started","params":{{"item":{{"type":"userMessage","id":"phase52-steering-item","clientId":"{correlation}","content":[{{"type":"text","text":"source-disagreeing steering input","text_elements":[]}}]}},"threadId":"{cas_thread_id}","turnId":"{CAS_TURN_ID}","startedAtMs":52002}}}}"#,
        ),
    );
    read_until_close(socket).unwrap();
}

fn send_command_execution(
    socket: &mut WebSocket<TcpStream>,
    cas_thread_id: &str,
    method: &str,
    status: &str,
    timestamp: u64,
) {
    let timestamp_field = if method == "item/started" {
        "startedAtMs"
    } else {
        "completedAtMs"
    };
    send_json(
        socket,
        &format!(
            r#"{{"method":"{method}","params":{{"item":{{"type":"commandExecution","id":"{CAS_COMMAND_ITEM_ID}","command":"phase70 command","cwd":"C:\\work\\beryl","status":"{status}","commandActions":[]}},"threadId":"{cas_thread_id}","turnId":"{CAS_TURN_ID}","{timestamp_field}":{timestamp}}}}}"#,
        ),
    );
}

fn read_ordinary_turn_start(socket: &mut WebSocket<TcpStream>, cas_thread_id: &str) -> u64 {
    let request = read_json(socket).unwrap();
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "turn/start");
    assert_eq!(request["params"]["threadId"], cas_thread_id);
    assert_eq!(
        request["params"]["input"],
        json!([{"type": "text", "text": SUBMITTED_TEXT}]),
    );
    request["id"].as_u64().unwrap()
}

fn send_turn_start_response(socket: &mut WebSocket<TcpStream>, id: u64) {
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"turn":{{"id":"{CAS_TURN_ID}","items":[],"status":"inProgress"}}}}}}"#,
        ),
    );
}

fn send_checked_user(
    socket: &mut WebSocket<TcpStream>,
    cas_thread_id: &str,
    method: &str,
    timestamp_field: &str,
    timestamp: u64,
) {
    send_json(
        socket,
        &format!(
            r#"{{"method":"{method}","params":{{"item":{{"type":"userMessage","id":"{CAS_ITEM_ID}","clientId":null,"content":[{{"type":"text","text":"{SUBMITTED_TEXT}","text_elements":[]}}]}},"threadId":"{cas_thread_id}","turnId":"{CAS_TURN_ID}","{timestamp_field}":{timestamp}}}}}"#,
        ),
    );
}

pub fn terminal_wire() -> String {
    terminal_wire_for(CAS_THREAD_ID)
}

fn terminal_wire_for(cas_thread_id: &str) -> String {
    format!(
        r#"{{"method":"turn/completed","params":{{"threadId":"{cas_thread_id}","turn":{{"id":"{CAS_TURN_ID}","items":[],"itemsView":"notLoaded","status":"completed","error":null,"startedAt":37010,"completedAt":37011,"durationMs":1}}}}}}"#,
    )
}

fn assert_quiet_then_close(socket: &mut WebSocket<TcpStream>) {
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(150)))
        .unwrap();
    match socket.read() {
        Err(tungstenite::Error::Io(error))
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) => {}
        Ok(message) => panic!("unexpected client frame during inert adoption: {message:?}"),
        Err(error) => panic!("unexpected client transport result during inert adoption: {error}"),
    }
    socket.close(None).unwrap();
}

fn read_json(socket: &mut WebSocket<TcpStream>) -> Option<Value> {
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => return Some(serde_json::from_str(&text).unwrap()),
            Ok(Message::Close(_)) => return None,
            Ok(Message::Ping(payload)) => socket.send(Message::Pong(payload)).unwrap(),
            Ok(Message::Pong(_) | Message::Frame(_)) => {}
            Ok(Message::Binary(bytes)) => panic!("unexpected binary frame: {bytes:?}"),
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return None;
            }
            Err(error) => panic!("phase37 server read failed: {error}"),
        }
    }
}

fn send_json(socket: &mut WebSocket<TcpStream>, value: &str) {
    socket.send(Message::Text(value.into())).unwrap();
}

fn read_until_close(socket: &mut WebSocket<TcpStream>) -> Result<(), tungstenite::Error> {
    loop {
        match socket.read() {
            Ok(Message::Close(_))
            | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(());
            }
            Ok(Message::Ping(payload)) => socket.send(Message::Pong(payload))?,
            Ok(Message::Pong(_) | Message::Frame(_)) => {}
            Ok(Message::Text(text)) => panic!("unexpected client text after terminal: {text}"),
            Ok(Message::Binary(bytes)) => {
                panic!("unexpected client binary after terminal: {bytes:?}")
            }
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
