#![allow(dead_code)]

include!("../normal_terminal/server.rs");

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Debug)]
enum NativeLineageServerEvent {
    FirstResumeObserved,
    InitialRetriesExhausted,
    CommandRetriesExhausted,
    RecoveryInjected(Vec<Value>),
    TurnStartObserved,
    Closed,
}

#[derive(Debug)]
enum NativeLineageServerCommand {
    ReleaseFirstResume,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NativeLineageServerScenario {
    RetrySucceeds,
    RetryFailsThenRecoverySucceeds,
    RemainParked,
    CompleteNormally,
    RecoverNormally,
}

pub struct NativeLineageServer {
    endpoint: BackendWebSocketEndpoint,
    events: Receiver<NativeLineageServerEvent>,
    commands: SyncSender<NativeLineageServerCommand>,
    resume_requests: Arc<AtomicUsize>,
    handle: Option<thread::JoinHandle<()>>,
}

impl NativeLineageServer {
    pub fn spawn_retry_success(cas_thread_id: impl Into<Box<str>>) -> Self {
        Self::spawn(
            Some(cas_thread_id.into()),
            NativeLineageServerScenario::RetrySucceeds,
        )
    }

    pub fn spawn_retry_failure_then_recovery(cas_thread_id: impl Into<Box<str>>) -> Self {
        Self::spawn(
            Some(cas_thread_id.into()),
            NativeLineageServerScenario::RetryFailsThenRecoverySucceeds,
        )
    }

    pub fn spawn_parked(cas_thread_id: impl Into<Box<str>>) -> Self {
        Self::spawn(
            Some(cas_thread_id.into()),
            NativeLineageServerScenario::RemainParked,
        )
    }

    pub fn spawn_retry_success_any() -> Self {
        Self::spawn(None, NativeLineageServerScenario::RetrySucceeds)
    }

    pub fn spawn_normal_success_any() -> Self {
        Self::spawn(None, NativeLineageServerScenario::CompleteNormally)
    }

    pub fn spawn_recovery_success_any() -> Self {
        Self::spawn(None, NativeLineageServerScenario::RecoverNormally)
    }

    fn spawn(cas_thread_id: Option<Box<str>>, scenario: NativeLineageServerScenario) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
        let (event_sender, events) = mpsc::sync_channel(4);
        let (commands, command_receiver) = mpsc::sync_channel(1);
        let resume_requests = Arc::new(AtomicUsize::new(0));
        let server_resume_requests = Arc::clone(&resume_requests);
        let handle = thread::Builder::new()
            .name("native-lineage-server".to_owned())
            .spawn(move || {
                run_native_lineage_server(
                    listener,
                    event_sender,
                    command_receiver,
                    cas_thread_id.as_deref(),
                    scenario,
                    &server_resume_requests,
                )
            })
            .unwrap();
        Self {
            endpoint,
            events,
            commands,
            resume_requests,
            handle: Some(handle),
        }
    }

    pub fn endpoint(&self) -> BackendWebSocketEndpoint {
        self.endpoint.clone()
    }

    pub fn wait_for_first_resume(&self) {
        self.expect(NativeLineageServerEvent::FirstResumeObserved);
    }

    pub fn release_first_resume(&self) {
        self.commands
            .send(NativeLineageServerCommand::ReleaseFirstResume)
            .unwrap();
    }

    pub fn wait_for_initial_retries(&self) {
        self.expect(NativeLineageServerEvent::InitialRetriesExhausted);
    }

    pub fn wait_for_command_retries(&self) {
        self.expect(NativeLineageServerEvent::CommandRetriesExhausted);
    }

    pub fn wait_for_recovery_injection(&self) -> Vec<Value> {
        match self.recv() {
            NativeLineageServerEvent::RecoveryInjected(items) => items,
            actual => panic!("expected recovery injection, got {actual:?}"),
        }
    }

    pub fn wait_for_turn_start(&self) {
        self.expect(NativeLineageServerEvent::TurnStartObserved);
    }

    pub fn resume_request_count(&self) -> usize {
        self.resume_requests.load(Ordering::SeqCst)
    }

    pub fn join(mut self) {
        self.expect(NativeLineageServerEvent::Closed);
        self.handle.take().unwrap().join().unwrap();
    }

    fn expect(&self, expected: NativeLineageServerEvent) {
        let actual = self.recv();
        assert_eq!(
            std::mem::discriminant(&actual),
            std::mem::discriminant(&expected),
            "expected {expected:?}, got {actual:?}"
        );
    }

    fn recv(&self) -> NativeLineageServerEvent {
        self.events
            .recv_timeout(TIMEOUT)
            .unwrap_or_else(|error| panic!("timed out waiting for native-lineage server: {error}"))
    }
}

fn run_native_lineage_server(
    listener: TcpListener,
    events: SyncSender<NativeLineageServerEvent>,
    commands: Receiver<NativeLineageServerCommand>,
    expected_cas_thread_id: Option<&str>,
    scenario: NativeLineageServerScenario,
    resume_requests: &AtomicUsize,
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
    let expected_input = expected_cas_thread_id.map(|_| SUBMITTED_TEXT);
    if scenario == NativeLineageServerScenario::RecoverNormally {
        complete_projection(&mut socket);
        complete_recovery_injection_and_return(&mut socket);
        complete_turn_and_report(&mut socket, &events, CAS_THREAD_ID, None);
        complete_unsubscribe(&mut socket, CAS_THREAD_ID);
        read_until_close(&mut socket).unwrap();
        events.send(NativeLineageServerEvent::Closed).unwrap();
        return;
    }
    if scenario == NativeLineageServerScenario::CompleteNormally {
        let cas_thread_id =
            complete_any_resume_projection(&mut socket, expected_cas_thread_id, resume_requests);
        complete_turn_and_report(&mut socket, &events, &cas_thread_id, expected_input);
        complete_unsubscribe(&mut socket, &cas_thread_id);
        read_until_close(&mut socket).unwrap();
        events.send(NativeLineageServerEvent::Closed).unwrap();
        return;
    }
    let cas_thread_id = reject_resume_batch(
        &mut socket,
        expected_cas_thread_id,
        Some((&events, &commands)),
        resume_requests,
    );
    events
        .send(NativeLineageServerEvent::InitialRetriesExhausted)
        .unwrap();

    match scenario {
        NativeLineageServerScenario::RetrySucceeds => {
            complete_counted_resume_projection(&mut socket, &cas_thread_id, resume_requests);
            complete_turn_and_report(&mut socket, &events, &cas_thread_id, expected_input);
            complete_unsubscribe(&mut socket, &cas_thread_id);
            read_until_close(&mut socket).unwrap();
        }
        NativeLineageServerScenario::RetryFailsThenRecoverySucceeds => {
            reject_resume_batch(&mut socket, Some(&cas_thread_id), None, resume_requests);
            events
                .send(NativeLineageServerEvent::CommandRetriesExhausted)
                .unwrap();
            complete_projection(&mut socket);
            let items = complete_recovery_injection_and_return(&mut socket);
            events
                .send(NativeLineageServerEvent::RecoveryInjected(items))
                .unwrap();
            complete_turn_and_report(&mut socket, &events, CAS_THREAD_ID, expected_input);
            read_until_close(&mut socket).unwrap();
        }
        NativeLineageServerScenario::RemainParked => {
            read_until_close(&mut socket).unwrap();
        }
        NativeLineageServerScenario::CompleteNormally
        | NativeLineageServerScenario::RecoverNormally => unreachable!(),
    }
    events.send(NativeLineageServerEvent::Closed).unwrap();
}

fn reject_resume_batch(
    socket: &mut WebSocket<TcpStream>,
    expected_cas_thread_id: Option<&str>,
    first_resume_pause: Option<(
        &SyncSender<NativeLineageServerEvent>,
        &Receiver<NativeLineageServerCommand>,
    )>,
    resume_requests: &AtomicUsize,
) -> Box<str> {
    let mut cas_thread_id = None;
    for attempt in 0..3 {
        let request = read_json(socket).expect("native-lineage resume request");
        resume_requests.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["method"], "thread/resume");
        let observed = request["params"]["threadId"].as_str().unwrap();
        if let Some(expected) = expected_cas_thread_id {
            assert_eq!(observed, expected);
        }
        if let Some(first) = cas_thread_id.as_deref() {
            assert_eq!(observed, first);
        } else {
            cas_thread_id = Some(Box::<str>::from(observed));
        }
        if attempt == 0 {
            if let Some((events, commands)) = first_resume_pause {
                events
                    .send(NativeLineageServerEvent::FirstResumeObserved)
                    .unwrap();
                assert!(matches!(
                    commands.recv_timeout(TIMEOUT).unwrap(),
                    NativeLineageServerCommand::ReleaseFirstResume
                ));
            }
        }
        let id = request["id"].as_u64().unwrap();
        send_json(
            socket,
            &format!(
                r#"{{"error":{{"code":-32600,"message":"native source unavailable"}},"id":{id}}}"#
            ),
        );
    }
    cas_thread_id.unwrap()
}

fn complete_any_resume_projection(
    socket: &mut WebSocket<TcpStream>,
    expected_cas_thread_id: Option<&str>,
    resume_requests: &AtomicUsize,
) -> Box<str> {
    let request = read_json(socket).unwrap();
    resume_requests.fetch_add(1, Ordering::SeqCst);
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/resume");
    let cas_thread_id = request["params"]["threadId"].as_str().unwrap();
    if let Some(expected) = expected_cas_thread_id {
        assert_eq!(cas_thread_id, expected);
    }
    let id = request["id"].as_u64().unwrap();
    send_thread_load_response(socket, id, cas_thread_id, true);
    Box::from(cas_thread_id)
}

fn complete_counted_resume_projection(
    socket: &mut WebSocket<TcpStream>,
    cas_thread_id: &str,
    resume_requests: &AtomicUsize,
) {
    let request = read_json(socket).unwrap();
    resume_requests.fetch_add(1, Ordering::SeqCst);
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/resume");
    assert_eq!(request["params"]["threadId"], cas_thread_id);
    let id = request["id"].as_u64().unwrap();
    send_thread_load_response(socket, id, cas_thread_id, true);
}

fn complete_recovery_injection_and_return(socket: &mut WebSocket<TcpStream>) -> Vec<Value> {
    let request = read_json(socket).expect("recovery injection request");
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "thread/inject_items");
    assert_eq!(request["params"]["threadId"], CAS_THREAD_ID);
    let items = request["params"]["items"]
        .as_array()
        .expect("recovery items are an array")
        .clone();
    let id = request["id"].as_u64().unwrap();
    send_json(socket, &format!(r#"{{"id":{id},"result":{{}}}}"#));
    items
}

fn complete_turn_and_report(
    socket: &mut WebSocket<TcpStream>,
    events: &SyncSender<NativeLineageServerEvent>,
    cas_thread_id: &str,
    expected_input: Option<&str>,
) {
    let (id, observed_input) = if expected_input.is_some() {
        (read_ordinary_turn_start(socket, cas_thread_id), None)
    } else {
        let (id, input) = read_any_ordinary_turn_start(socket, cas_thread_id);
        (id, Some(input))
    };
    events
        .send(NativeLineageServerEvent::TurnStartObserved)
        .unwrap();
    if let Some(input) = observed_input {
        finish_ordinary_turn_with_text(socket, cas_thread_id, id, &input);
    } else {
        finish_ordinary_turn(socket, cas_thread_id, id);
    }
}

fn read_any_ordinary_turn_start(
    socket: &mut WebSocket<TcpStream>,
    cas_thread_id: &str,
) -> (u64, Box<str>) {
    let request = read_json(socket).unwrap();
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["method"], "turn/start");
    assert_eq!(request["params"]["threadId"], cas_thread_id);
    let input = request["params"]["input"].as_array().unwrap();
    assert_eq!(input.len(), 1);
    assert_eq!(input[0]["type"], "text");
    let text = input[0]["text"].as_str().unwrap();
    assert!(!text.is_empty());
    (request["id"].as_u64().unwrap(), Box::from(text))
}

fn finish_ordinary_turn_with_text(
    socket: &mut WebSocket<TcpStream>,
    cas_thread_id: &str,
    id: u64,
    input: &str,
) {
    send_checked_user_with_text(
        socket,
        cas_thread_id,
        "item/started",
        "startedAtMs",
        STARTED_AT_MS,
        input,
    );
    send_checked_user_with_text(
        socket,
        cas_thread_id,
        "item/completed",
        "completedAtMs",
        COMPLETED_AT_MS,
        input,
    );
    send_turn_start_response(socket, id);
    send_json(socket, &terminal_wire_for(cas_thread_id));
}

fn send_checked_user_with_text(
    socket: &mut WebSocket<TcpStream>,
    cas_thread_id: &str,
    method: &str,
    timestamp_field: &str,
    timestamp: u64,
    input: &str,
) {
    let mut params = json!({
        "item": {
            "type": "userMessage",
            "id": CAS_ITEM_ID,
            "clientId": null,
            "content": [{"type": "text", "text": input, "text_elements": []}],
        },
        "threadId": cas_thread_id,
        "turnId": CAS_TURN_ID,
    });
    params[timestamp_field] = json!(timestamp);
    send_json(
        socket,
        &json!({"method": method, "params": params}).to_string(),
    );
}
