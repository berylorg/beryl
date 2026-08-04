#![cfg(feature = "lifecycle-test-support")]

#[path = "phase31_bounded_dispatch/support.rs"]
mod websocket;

use std::sync::{Arc, Mutex};

use beryl_backend::{
    CallerNoSuccessorFence, CompactThreadDisposition, CompactionAttemptCorrelation,
    ExactForegroundThread, LoadedThreadStatus, ManagedBackendClientConnector, ManagedBackendError,
    ManagedBackendSession, NormalTurnTerminalStatus, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamSink, OrderedTurnStreamSubmitError,
};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
    RuntimeId,
};

use websocket::{
    AUTHORIZATION, TIMEOUT, assert_initialize, assert_initialized, expect_close, foreground_config,
    read_json, read_text, send_initialize_response, send_json, spawn_server,
};

const THREAD: &str = "thread-compaction";

#[derive(Clone, Debug, Eq, PartialEq)]
enum Observed {
    Status(LoadedThreadStatus),
    Started(String),
    Terminal(NormalTurnTerminalStatus),
}

struct RecordingSink {
    observed: Arc<Mutex<Vec<Observed>>>,
}

impl OrderedTurnStreamSink for RecordingSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        let observed = match operation {
            OrderedTurnStreamOperation::ThreadStatusChanged(status) => {
                assert_eq!(status.thread_id().as_str(), THREAD);
                Observed::Status(status.status())
            }
            OrderedTurnStreamOperation::TurnStarted(turn) => {
                assert_eq!(turn.thread_id().as_str(), THREAD);
                Observed::Started(turn.turn_id().as_str().to_string())
            }
            OrderedTurnStreamOperation::NormalTurnTerminal(terminal) => {
                assert_eq!(terminal.thread_id().as_str(), THREAD);
                Observed::Terminal(terminal.status())
            }
            other => panic!("unexpected ordered compaction operation: {other:?}"),
        };
        self.observed.lock().unwrap().push(observed);
        Ok(OrderedTurnStreamCompletion::Applied)
    }
}

fn target() -> ExactForegroundThread {
    ExactForegroundThread::new(
        RuntimeId::from_bytes([3; 16]),
        CasLoadedSessionGeneration::new(
            CasProcessGeneration::new(7).unwrap(),
            CasLoadedThreadGeneration::new(11).unwrap(),
        ),
        CasThreadId::new(THREAD).unwrap(),
    )
}

fn connect_initialized(
    endpoint: beryl_backend::BackendWebSocketEndpoint,
    observed: Arc<Mutex<Vec<Observed>>>,
) -> ManagedBackendSession {
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(8), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    session
        .bind_ordered_turn_stream_sink(Box::new(RecordingSink { observed }))
        .unwrap();
    session.bind_exact_foreground_thread(target()).unwrap();
    session
}

fn authorize(
    session: &mut ManagedBackendSession,
) -> beryl_backend::ExactForegroundThreadAuthorization {
    session
        .authorize_exact_foreground_thread(
            target(),
            CompactionAttemptCorrelation::from_bytes([0xA5; 16]),
            CallerNoSuccessorFence::issue(),
        )
        .unwrap()
}

#[test]
fn exact_wire_keeps_attempt_local_and_routes_lifecycle_before_acknowledgement() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let wire = read_text(socket).unwrap();
        assert_eq!(
            wire,
            r#"{"method":"thread/compact/start","id":2,"params":{"threadId":"thread-compaction"}}"#
        );
        assert!(!wire.contains("a5a5"));

        send_json(
            socket,
            r#"{"method":"thread/status/changed","params":{"threadId":"thread-compaction","status":{"type":"active","activeFlags":[]}}}"#,
        );
        send_json(
            socket,
            r#"{"method":"turn/started","params":{"threadId":"thread-compaction","turn":{"id":"turn-compact","items":[],"itemsView":"notLoaded","status":"inProgress","error":null,"startedAt":1,"completedAt":null,"durationMs":null}}}"#,
        );
        send_json(
            socket,
            r#"{"method":"thread/status/changed","params":{"threadId":"thread-compaction","status":{"type":"idle"}}}"#,
        );
        send_json(
            socket,
            r#"{"method":"turn/completed","params":{"threadId":"thread-compaction","turn":{"id":"turn-compact","items":[],"itemsView":"notLoaded","status":"completed","error":null,"startedAt":1,"completedAt":2,"durationMs":1}}}"#,
        );
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut session = connect_initialized(endpoint, observed.clone());
    let authorization = authorize(&mut session);
    let outcome = session.compact_exact_foreground_thread(authorization, TIMEOUT);

    assert!(matches!(
        outcome.disposition(),
        CompactThreadDisposition::RequestAccepted
    ));
    assert_eq!(outcome.request().target(), &target());
    assert_eq!(
        outcome.request().attempt_correlation().as_bytes(),
        &[0xA5; 16]
    );
    assert!(outcome.request().had_no_successor_fence());
    assert_eq!(
        *observed.lock().unwrap(),
        [
            Observed::Status(LoadedThreadStatus::active(
                beryl_backend::ThreadActiveFlags::empty()
            )),
            Observed::Started("turn-compact".to_string()),
            Observed::Status(LoadedThreadStatus::Idle),
            Observed::Terminal(NormalTurnTerminalStatus::Completed),
        ]
    );
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn exact_rejection_is_distinct_from_completion_unknown() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        assert_eq!(read_json(socket).unwrap()["method"], "thread/compact/start");
        send_json(
            socket,
            r#"{"error":{"code":-32600,"message":"thread not found"},"id":2}"#,
        );
        expect_close(socket);
    });
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut session = connect_initialized(endpoint, observed);
    let authorization = authorize(&mut session);
    let outcome = session.compact_exact_foreground_thread(authorization, TIMEOUT);
    assert!(matches!(
        outcome.disposition(),
        CompactThreadDisposition::ExactRejection { error }
            if error.code() == -32600
    ));
    assert!(!session.transport_is_closed_for_lifecycle_test());
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn proven_nondispatch_preserves_session_but_response_loss_retires_it() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        assert_eq!(read_json(socket).unwrap()["id"], 2);
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut session = connect_initialized(endpoint, observed);
    session.fail_next_write_before_dispatch_for_lifecycle_test();
    let first_authorization = authorize(&mut session);
    let first = session.compact_exact_foreground_thread(first_authorization, TIMEOUT);
    assert!(matches!(
        first.disposition(),
        CompactThreadDisposition::ProvenNotDispatched { .. }
    ));
    let second_authorization = authorize(&mut session);
    let second = session.compact_exact_foreground_thread(second_authorization, TIMEOUT);
    assert!(matches!(
        second.disposition(),
        CompactThreadDisposition::RequestAccepted
    ));
    session.shutdown().unwrap();
    server.join().unwrap();

    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        assert_eq!(read_json(socket).unwrap()["method"], "thread/compact/start");
    });
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut session = connect_initialized(endpoint, observed);
    let authorization = authorize(&mut session);
    let outcome = session.compact_exact_foreground_thread(authorization, TIMEOUT);
    assert!(matches!(
        outcome.disposition(),
        CompactThreadDisposition::CompletionUnknown { .. }
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert!(matches!(
        session.authorize_exact_foreground_thread(
            target(),
            CompactionAttemptCorrelation::from_bytes([1; 16]),
            CallerNoSuccessorFence::issue(),
        ),
        Err(ManagedBackendError::TransportClosed { .. })
    ));
    server.join().unwrap();
}

#[test]
fn ordered_status_accepts_only_active_idle_and_system_error() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut sink = RecordingSink {
        observed: observed.clone(),
    };
    for json in [
        r#"{"method":"thread/status/changed","params":{"threadId":"thread-compaction","status":{"type":"active","activeFlags":["waitingOnApproval","waitingOnUserInput","futureFlag"]}}}"#,
        r#"{"method":"thread/status/changed","params":{"threadId":"thread-compaction","status":{"type":"idle"}}}"#,
        r#"{"method":"thread/status/changed","params":{"threadId":"thread-compaction","status":{"type":"systemError"}}}"#,
    ] {
        beryl_backend::lifecycle_test_support::decode_provider_json_for_test(
            json.as_bytes(),
            3,
            &mut sink,
        )
        .unwrap();
    }
    assert_eq!(observed.lock().unwrap().len(), 3);
    let active = observed.lock().unwrap()[0].clone();
    assert!(matches!(
        active,
        Observed::Status(LoadedThreadStatus::Active { active_flags })
            if active_flags.waiting_on_approval() && active_flags.waiting_on_user_input()
    ));

    let rejected = beryl_backend::lifecycle_test_support::decode_provider_json_for_test(
        br#"{"method":"thread/status/changed","params":{"threadId":"thread-compaction","status":{"type":"notLoaded"}}}"#,
        2,
        &mut sink,
    );
    assert!(matches!(
        rejected,
        Err(ManagedBackendError::ForegroundIngress {
            source: beryl_backend::ForegroundIngressError::MalformedThreadStatusChanged,
            ..
        })
    ));
}
