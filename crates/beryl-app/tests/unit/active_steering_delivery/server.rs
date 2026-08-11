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

pub(super) const AUTHORIZATION: &str = "Bearer phase54-active-steering";
pub(super) const CAS_THREAD_ID: &str = "phase54-cas-thread";
pub(super) const CAS_TURN_ID: &str = "phase54-cas-turn";
const CAS_INITIAL_ITEM_ID: &str = "phase54-initial-item";
const CAS_ITEM_ID: &str = "phase54-steering-item";
pub(super) const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SteeringServerScenario {
    NoSteeringRequest,
    OrderedPairSuccess,
    SecondOnlySuccess,
    StructuredRejection,
    StructuredRejectionThenSuccess,
    UnconfirmedRejection,
    CompletionUnknown,
    SuccessResponseBeforeLifecycle,
    LifecycleBeforeSuccessResponse,
    RepeatedImageSuccess,
    SuccessThenTargetLoss,
}

#[derive(Clone, Copy)]
enum ExpectedInput {
    Text(&'static str),
    RepeatedImage,
}

#[derive(Debug, Eq, PartialEq)]
enum ServerEvent {
    ProjectionReady,
    ExactResponseSent,
    Closed,
}

enum ServerCommand {
    ReleaseLifecycle,
}

pub(super) struct SteeringServer {
    endpoint: BackendWebSocketEndpoint,
    events: Receiver<ServerEvent>,
    commands: SyncSender<ServerCommand>,
    handle: Option<thread::JoinHandle<()>>,
}

impl SteeringServer {
    pub(super) fn spawn(scenario: SteeringServerScenario) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let (event_sender, events) = mpsc::sync_channel(4);
        let (commands, command_receiver) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("phase54-active-steering-server".to_owned())
            .spawn(move || run_server(listener, event_sender, command_receiver, scenario))
            .unwrap();
        Self {
            endpoint,
            events,
            commands,
            handle: Some(handle),
        }
    }

    pub(super) fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }

    pub(super) fn wait_for_projection(&self) {
        self.expect(ServerEvent::ProjectionReady);
    }

    pub(super) fn wait_for_exact_response(&self) {
        self.expect(ServerEvent::ExactResponseSent);
    }

    pub(super) fn release_lifecycle(&self) {
        self.commands.send(ServerCommand::ReleaseLifecycle).unwrap();
    }

    pub(super) fn join(mut self) {
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
    scenario: SteeringServerScenario,
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
    complete_active_turn(&mut socket);
    run_scenario(&mut socket, &events, &commands, scenario);
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
    send_json(socket, &format!(r#"{{"id":{id},"result":{{"config":{{"model":"gpt-5.6","model_reasoning_effort":"high","features":{{"multi_agent_v2":{{"enabled":true,"expose_spawn_agent_model_overrides":true}}}}}},"origins":{{"features.multi_agent_v2.enabled":{{"name":{{"type":"sessionFlags"}},"version":"0"}},"features.multi_agent_v2.expose_spawn_agent_model_overrides":{{"name":{{"type":"sessionFlags"}},"version":"0"}}}}}}}}"#));
}

fn complete_projection(socket: &mut WebSocket<TcpStream>) {
    let request = read_json(socket).unwrap();
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/start");
    assert_eq!(request["params"]["ephemeral"], false);
    assert!(request["params"].get("input").is_none());
    let id = request["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"thread":{{"id":"{CAS_THREAD_ID}","extra":null,"sessionId":"phase54-session","forkedFromId":null,"parentThreadId":null,"preview":"preview","ephemeral":false,"historyMode":"legacy","modelProvider":"openai","createdAt":1,"updatedAt":2,"recencyAt":null,"status":{{"type":"idle"}},"path":null,"cwd":"C:\\work\\beryl","cliVersion":"0.146.0","source":"appServer","threadSource":null,"agentNickname":null,"agentRole":null,"gitInfo":null,"name":null,"turns":[]}},"model":"gpt-5.6","modelProvider":"openai","serviceTier":null,"cwd":"C:\\work\\beryl","runtimeWorkspaceRoots":[],"instructionSources":[],"approvalPolicy":"never","approvalsReviewer":"user","sandbox":{{}},"activePermissionProfile":null,"reasoningEffort":"high","multiAgentMode":"explicitRequestOnly"}}}}"#,
        ),
    );
}

fn complete_active_turn(socket: &mut WebSocket<TcpStream>) {
    let request = read_json(socket).expect("fixture must start the pending ordinary turn");
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "turn/start");
    assert_eq!(request["params"]["threadId"], CAS_THREAD_ID);
    assert_eq!(
        request["params"]["input"],
        json!([{"type": "text", "text": super::support::SUBMITTED_TEXT}]),
    );
    let id = request["id"].as_u64().unwrap();
    send_initial_user_lifecycle(socket, "item/started", "startedAtMs", 5);
    send_initial_user_lifecycle(socket, "item/completed", "completedAtMs", 6);
    send_json(
        socket,
        &format!(
            r#"{{"id":{id},"result":{{"turn":{{"id":"{CAS_TURN_ID}","items":[],"status":"inProgress"}}}}}}"#,
        ),
    );
}

fn send_initial_user_lifecycle(
    socket: &mut WebSocket<TcpStream>,
    method: &str,
    timestamp_field: &str,
    timestamp: u64,
) {
    send_json(
        socket,
        &format!(
            r#"{{"method":"{method}","params":{{"item":{{"type":"userMessage","id":"{CAS_INITIAL_ITEM_ID}","clientId":null,"content":[{{"type":"text","text":"{}","text_elements":[]}}]}},"threadId":"{CAS_THREAD_ID}","turnId":"{CAS_TURN_ID}","{timestamp_field}":{timestamp}}}}}"#,
            super::support::SUBMITTED_TEXT,
        ),
    );
}

fn run_scenario(
    socket: &mut WebSocket<TcpStream>,
    events: &SyncSender<ServerEvent>,
    commands: &Receiver<ServerCommand>,
    scenario: SteeringServerScenario,
) {
    match scenario {
        SteeringServerScenario::NoSteeringRequest => read_until_close(socket).unwrap(),
        SteeringServerScenario::OrderedPairSuccess => {
            complete_success(
                socket,
                ExpectedInput::Text(super::support::STEERING_TEXT),
                "phase54-steering-item-1",
                54_031,
            );
            complete_success(
                socket,
                ExpectedInput::Text(super::support::SECOND_STEERING_TEXT),
                "phase54-steering-item-2",
                54_041,
            );
            read_until_close(socket).unwrap();
        }
        SteeringServerScenario::SecondOnlySuccess => {
            complete_success(
                socket,
                ExpectedInput::Text(super::support::SECOND_STEERING_TEXT),
                "phase54-steering-item-second-only",
                54_046,
            );
            read_until_close(socket).unwrap();
        }
        SteeringServerScenario::StructuredRejection => {
            complete_structured_rejection(socket);
            read_until_close(socket).unwrap();
        }
        SteeringServerScenario::StructuredRejectionThenSuccess => {
            complete_structured_rejection(socket);
            complete_success(
                socket,
                ExpectedInput::Text(super::support::SECOND_STEERING_TEXT),
                "phase54-steering-item-after-rejection",
                54_051,
            );
            read_until_close(socket).unwrap();
        }
        SteeringServerScenario::UnconfirmedRejection => {
            let request =
                read_steering_request(socket, ExpectedInput::Text(super::support::STEERING_TEXT));
            let id = request["id"].as_u64().unwrap();
            send_json(
                socket,
                &format!(
                    r#"{{"error":{{"code":-32600,"message":"active turn mismatch"}},"id":{id}}}"#,
                ),
            );
            read_until_close(socket).unwrap();
        }
        SteeringServerScenario::CompletionUnknown => {
            let request =
                read_steering_request(socket, ExpectedInput::Text(super::support::STEERING_TEXT));
            let id = request["id"].as_u64().unwrap();
            send_json(
                socket,
                &format!(r#"{{"id":{id},"result":{{"turnId":"phase54-wrong-turn"}}}}"#),
            );
            read_until_close(socket).unwrap();
        }
        SteeringServerScenario::SuccessResponseBeforeLifecycle => {
            complete_delayed_success(
                socket,
                events,
                commands,
                ExpectedInput::Text(super::support::STEERING_TEXT),
            );
            read_until_close(socket).unwrap();
        }
        SteeringServerScenario::LifecycleBeforeSuccessResponse => {
            complete_lifecycle_before_success(socket);
            read_until_close(socket).unwrap();
        }
        SteeringServerScenario::RepeatedImageSuccess => {
            complete_success(socket, ExpectedInput::RepeatedImage, CAS_ITEM_ID, 54_011);
            read_until_close(socket).unwrap();
        }
        SteeringServerScenario::SuccessThenTargetLoss => {
            complete_response_then_loss(socket, events, commands);
        }
    }
}

fn complete_structured_rejection(socket: &mut WebSocket<TcpStream>) {
    let request = read_steering_request(socket, ExpectedInput::Text(super::support::STEERING_TEXT));
    let id = request["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(
            r#"{{"error":{{"code":-32600,"data":{{"message":"steering denied","codexErrorInfo":{{"activeTurnNotSteerable":{{"turnKind":"review"}}}},"additionalDetails":null}},"message":"outer steering diagnostic"}},"id":{id}}}"#,
        ),
    );
}

fn complete_delayed_success(
    socket: &mut WebSocket<TcpStream>,
    events: &SyncSender<ServerEvent>,
    commands: &Receiver<ServerCommand>,
    expected_input: ExpectedInput,
) {
    let request = read_steering_request(socket, expected_input);
    let id = request["id"].as_u64().unwrap();
    let correlation = request["params"]["clientUserMessageId"]
        .as_str()
        .unwrap()
        .to_owned();
    let content = request["params"]["input"].clone();
    send_json(
        socket,
        &format!(r#"{{"id":{id},"result":{{"turnId":"{CAS_TURN_ID}"}}}}"#),
    );
    events.send(ServerEvent::ExactResponseSent).unwrap();
    assert!(matches!(
        commands.recv_timeout(TIMEOUT),
        Ok(ServerCommand::ReleaseLifecycle)
    ));
    send_lifecycle(
        socket,
        "item/started",
        "startedAtMs",
        54_001,
        CAS_ITEM_ID,
        &correlation,
        &content,
    );
    send_lifecycle(
        socket,
        "item/completed",
        "completedAtMs",
        54_002,
        CAS_ITEM_ID,
        &correlation,
        &content,
    );
}

fn complete_success(
    socket: &mut WebSocket<TcpStream>,
    expected_input: ExpectedInput,
    item_id: &str,
    lifecycle_timestamp: u64,
) {
    let request = read_steering_request(socket, expected_input);
    let id = request["id"].as_u64().unwrap();
    let correlation = request["params"]["clientUserMessageId"].as_str().unwrap();
    let content = request["params"]["input"].clone();
    send_json(
        socket,
        &format!(r#"{{"id":{id},"result":{{"turnId":"{CAS_TURN_ID}"}}}}"#),
    );
    send_lifecycle(
        socket,
        "item/started",
        "startedAtMs",
        lifecycle_timestamp,
        item_id,
        correlation,
        &content,
    );
    send_lifecycle(
        socket,
        "item/completed",
        "completedAtMs",
        lifecycle_timestamp + 1,
        item_id,
        correlation,
        &content,
    );
}

fn complete_lifecycle_before_success(socket: &mut WebSocket<TcpStream>) {
    let request = read_steering_request(socket, ExpectedInput::Text(super::support::STEERING_TEXT));
    let id = request["id"].as_u64().unwrap();
    let correlation = request["params"]["clientUserMessageId"].as_str().unwrap();
    let content = request["params"]["input"].clone();
    send_lifecycle(
        socket,
        "item/started",
        "startedAtMs",
        54_021,
        CAS_ITEM_ID,
        correlation,
        &content,
    );
    send_lifecycle(
        socket,
        "item/completed",
        "completedAtMs",
        54_022,
        CAS_ITEM_ID,
        correlation,
        &content,
    );
    send_json(
        socket,
        &format!(r#"{{"id":{id},"result":{{"turnId":"{CAS_TURN_ID}"}}}}"#),
    );
}

fn complete_response_then_loss(
    socket: &mut WebSocket<TcpStream>,
    events: &SyncSender<ServerEvent>,
    commands: &Receiver<ServerCommand>,
) {
    let request = read_steering_request(socket, ExpectedInput::Text(super::support::STEERING_TEXT));
    let id = request["id"].as_u64().unwrap();
    send_json(
        socket,
        &format!(r#"{{"id":{id},"result":{{"turnId":"{CAS_TURN_ID}"}}}}"#),
    );
    events.send(ServerEvent::ExactResponseSent).unwrap();
    assert!(matches!(
        commands.recv_timeout(TIMEOUT),
        Ok(ServerCommand::ReleaseLifecycle)
    ));
    socket.close(None).unwrap();
}

fn read_steering_request(
    socket: &mut WebSocket<TcpStream>,
    expected_input: ExpectedInput,
) -> Value {
    let request = read_json(socket).expect("delivery must send one turn/steer request");
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "turn/steer");
    assert_eq!(request["params"]["threadId"], CAS_THREAD_ID);
    assert_eq!(request["params"]["expectedTurnId"], CAS_TURN_ID);
    match expected_input {
        ExpectedInput::Text(expected) => assert_eq!(
            request["params"]["input"],
            json!([{"type": "text", "text": expected}]),
        ),
        ExpectedInput::RepeatedImage => {
            let input = request["params"]["input"]
                .as_array()
                .expect("image-bearing replay emits an input sequence");
            assert_eq!(input.len(), 3);
            assert_eq!(
                input[0],
                json!({
                    "type": "text",
                    "text": format!("{}Image A:", super::support::IMAGE_LEADING_TEXT),
                })
            );
            assert_eq!(input[1]["type"], "localImage");
            assert!(
                input[1]["path"]
                    .as_str()
                    .is_some_and(|path| !path.is_empty())
            );
            assert_eq!(
                input[2],
                json!({"type": "text", "text": " between [Image A] after"})
            );
        }
    }
    let correlation = request["params"]["clientUserMessageId"]
        .as_str()
        .expect("turn/steer carries its canonical accepted-input correlation");
    assert!(correlation.starts_with("beryl.accepted-input.v1:"));
    assert_eq!(correlation.len(), "beryl.accepted-input.v1:".len() + 32);
    request
}

fn send_lifecycle(
    socket: &mut WebSocket<TcpStream>,
    method: &str,
    timestamp_field: &str,
    timestamp: u64,
    item_id: &str,
    correlation: &str,
    content: &Value,
) {
    let mut echoed_content = content.clone();
    for item in echoed_content
        .as_array_mut()
        .expect("steering input must be an item sequence")
    {
        let object = item
            .as_object_mut()
            .expect("steering input items must be objects");
        match object.get("type").and_then(Value::as_str) {
            Some("text") => {
                object.insert("text_elements".to_owned(), json!([]));
            }
            Some("localImage") => {
                object.insert("detail".to_owned(), Value::Null);
            }
            _ => {}
        }
    }
    let mut message = json!({
        "method": method,
        "params": {
            "item": {
                "type": "userMessage",
                "id": item_id,
                "clientId": correlation,
                "content": echoed_content,
            },
            "threadId": CAS_THREAD_ID,
            "turnId": CAS_TURN_ID,
        },
    });
    message["params"][timestamp_field] = json!(timestamp);
    socket
        .send(Message::Text(message.to_string().into()))
        .unwrap();
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
            Err(error) => panic!("phase54 server read failed: {error}"),
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
            Ok(Message::Text(text)) => panic!("unexpected client text after delivery: {text}"),
            Ok(Message::Binary(bytes)) => {
                panic!("unexpected client binary after delivery: {bytes:?}")
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
