use std::{
    net::TcpStream,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use beryl_backend::{
    BackendWebSocketEndpoint, ForegroundSessionConfig, ManagedBackendClientConnector,
    ManagedBackendError, ManagedBackendSession, NonIdempotentRequestOutcome,
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamRejection,
    OrderedTurnStreamSink, OrderedTurnStreamSubmitCause, OrderedTurnStreamSubmitError,
    StreamedInputDescriptor, StreamedInputHeader, StreamedInputSequenceDigestAccumulator,
    StreamedInputSource, StreamedInputSourceError, StreamedInputSourceIdentity,
    StreamedInputSourceRevision, StreamedTextPage, StreamedTextSourceId,
    StreamedUserMessageCorrelationError,
};
use beryl_model::CasThreadId;
use serde_json::Value;
use tungstenite::WebSocket;

#[allow(dead_code)]
#[path = "../phase31_bounded_dispatch/support.rs"]
mod dispatch_support;

use dispatch_support::{
    AUTHORIZATION, TIMEOUT, assert_initialize, assert_initialized, expect_close, read_json,
    send_initialize_response, send_json, spawn_server,
};

#[derive(Clone, Default)]
struct Calls(Arc<AtomicUsize>);

impl Calls {
    fn note(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

struct EmptyReplaySource {
    calls: Calls,
    drops: Calls,
}

impl Drop for EmptyReplaySource {
    fn drop(&mut self) {
        self.drops.note();
    }
}

impl StreamedInputSource for EmptyReplaySource {
    fn header(&self) -> StreamedInputHeader {
        self.calls.note();
        empty_header()
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        self.calls.note();
        Ok(empty_header())
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        self.calls.note();
        Ok(None)
    }

    fn read_text_page(
        &mut self,
        _source_id: StreamedTextSourceId,
        _start: u64,
        _max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        self.calls.note();
        Err(StreamedInputSourceError::ReadFailed)
    }
}

fn empty_header() -> StreamedInputHeader {
    StreamedInputHeader::new(
        StreamedInputSourceIdentity::new([36; 32]),
        StreamedInputSourceRevision::new(1),
        0,
        StreamedInputSequenceDigestAccumulator::new(0)
            .finish()
            .unwrap(),
    )
}

struct CheckedSink {
    calls: Calls,
    failure: Option<OrderedTurnStreamSubmitCause>,
}

impl OrderedTurnStreamSink for CheckedSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        self.calls.note();
        if !matches!(
            &operation,
            OrderedTurnStreamOperation::CheckedUserMessage(_)
        ) {
            return Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::SchemaMismatch),
            ));
        }
        if let Some(cause) = self.failure {
            return Err(OrderedTurnStreamSubmitError::new(operation, cause));
        }
        Ok(OrderedTurnStreamCompletion::Applied)
    }
}

fn initialized_session(
    endpoint: BackendWebSocketEndpoint,
    sink_calls: Calls,
    sink_failure: Option<OrderedTurnStreamSubmitCause>,
) -> ManagedBackendSession {
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    let mut session = connector
        .connect_foreground_candidate(
            ForegroundSessionConfig::new(NonZeroUsize::new(16).unwrap()),
            TIMEOUT,
        )
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    session
        .bind_ordered_turn_stream_sink(Box::new(CheckedSink {
            calls: sink_calls,
            failure: sink_failure,
        }))
        .unwrap();
    session
}

fn initialize_server(socket: &mut WebSocket<TcpStream>) {
    assert_initialize(&read_json(socket).unwrap(), false);
    send_initialize_response(socket, 1);
    assert_initialized(&read_json(socket).unwrap());
}

fn assert_empty_turn_start(request: &Value) {
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["id"], 2);
    assert_eq!(request["method"], "turn/start");
    assert_eq!(request["params"]["threadId"], "thread-failure");
    assert_eq!(request["params"]["input"], serde_json::json!([]));
}

#[test]
fn response_before_both_checked_echoes_is_completion_unknown_and_retires_authority() {
    let (endpoint, server) = spawn_server(|socket| {
        initialize_server(socket);
        assert_empty_turn_start(&read_json(socket).unwrap());
        send_json(
            socket,
            r#"{"id":2,"result":{"turn":{"id":"turn-failure","items":[],"status":"inProgress"}}}"#,
        );
        expect_close(socket);
    });

    let sink_calls = Calls::default();
    let mut session = initialized_session(endpoint, sink_calls.clone(), None);
    let source_calls = Calls::default();
    let source_drops = Calls::default();
    let thread_id = CasThreadId::new("thread-failure").unwrap();
    let outcome = session.start_turn_with_streamed_input(
        &thread_id,
        Box::new(EmptyReplaySource {
            calls: source_calls.clone(),
            drops: source_drops.clone(),
        }),
        TIMEOUT,
    );

    let NonIdempotentRequestOutcome::CompletionUnknown { error } = outcome else {
        panic!("an early successful response must be completion-unknown");
    };
    assert!(matches!(
        *error,
        ManagedBackendError::StreamedUserMessageCorrelation {
            ref method,
            source: StreamedUserMessageCorrelationError::SuccessfulResponseBeforeBothEchoes,
            transport_bytes_written: true,
        } if method == "turn/start"
    ));
    assert_eq!(sink_calls.count(), 0);
    assert!(source_calls.count() > 0);
    assert_eq!(source_drops.count(), 1);
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        (3, true, false)
    );
    assert!(session.transport_is_closed_for_lifecycle_test());

    server.join().unwrap();
}

#[test]
fn rejection_before_any_echo_is_exact_and_releases_request_authority() {
    let (endpoint, server) = spawn_server(|socket| {
        initialize_server(socket);
        assert_empty_turn_start(&read_json(socket).unwrap());
        send_json(
            socket,
            r#"{"error":{"code":-32036,"message":"turn rejected"},"id":2}"#,
        );
        expect_close(socket);
    });

    let sink_calls = Calls::default();
    let mut session = initialized_session(endpoint, sink_calls.clone(), None);
    let source_calls = Calls::default();
    let source_drops = Calls::default();
    let thread_id = CasThreadId::new("thread-failure").unwrap();
    let outcome = session.start_turn_with_streamed_input(
        &thread_id,
        Box::new(EmptyReplaySource {
            calls: source_calls.clone(),
            drops: source_drops.clone(),
        }),
        TIMEOUT,
    );

    let NonIdempotentRequestOutcome::ExactRejection { error } = outcome else {
        panic!("a rejection before any echo must remain exact");
    };
    assert_eq!(error.code(), -32036);
    assert_eq!(error.message(), "turn rejected");
    assert!(!error.message_was_truncated());
    assert!(!error.data_was_present());
    assert_eq!(sink_calls.count(), 0);
    assert!(source_calls.count() > 0);
    assert_eq!(source_drops.count(), 1);
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        (3, true, false)
    );
    assert!(!session.transport_is_closed_for_lifecycle_test());

    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn checked_echo_sink_failure_after_dispatch_is_completion_unknown_and_retires_authority() {
    let (endpoint, server) = spawn_server(|socket| {
        initialize_server(socket);
        assert_empty_turn_start(&read_json(socket).unwrap());
        send_checked_started(socket);
        expect_close(socket);
    });

    let sink_calls = Calls::default();
    let mut session = initialized_session(
        endpoint,
        sink_calls.clone(),
        Some(OrderedTurnStreamSubmitCause::ReceiverLost),
    );
    let source_calls = Calls::default();
    let source_drops = Calls::default();
    let thread_id = CasThreadId::new("thread-failure").unwrap();
    let outcome = session.start_turn_with_streamed_input(
        &thread_id,
        Box::new(EmptyReplaySource {
            calls: source_calls.clone(),
            drops: source_drops.clone(),
        }),
        TIMEOUT,
    );

    let NonIdempotentRequestOutcome::CompletionUnknown { error } = outcome else {
        panic!("an ordered sink failure after dispatch must be completion-unknown");
    };
    match *error {
        ManagedBackendError::OrderedTurnStream { method, source } => {
            assert_eq!(method, "turn/start");
            assert_eq!(source.cause(), OrderedTurnStreamSubmitCause::ReceiverLost);
            assert!(matches!(
                source.into_operation(),
                OrderedTurnStreamOperation::CheckedUserMessage(_)
            ));
        }
        error => panic!("unexpected ordered sink failure: {error:?}"),
    }
    assert_eq!(sink_calls.count(), 1);
    assert!(source_calls.count() > 0);
    assert_eq!(source_drops.count(), 1);
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        (3, false, false)
    );
    assert!(session.transport_is_closed_for_lifecycle_test());

    server.join().unwrap();
}

#[test]
fn proven_predispatch_write_failure_removes_expectation_verifier_and_source() {
    let (endpoint, server) = spawn_server(|socket| {
        initialize_server(socket);
        expect_close(socket);
    });

    let sink_calls = Calls::default();
    let mut session = initialized_session(endpoint, sink_calls.clone(), None);
    let before = session.predispatch_state_for_lifecycle_test();
    session.fail_next_write_before_dispatch_for_lifecycle_test();
    let source_calls = Calls::default();
    let source_drops = Calls::default();
    let thread_id = CasThreadId::new("thread-failure").unwrap();
    let outcome = session.start_turn_with_streamed_input(
        &thread_id,
        Box::new(EmptyReplaySource {
            calls: source_calls.clone(),
            drops: source_drops.clone(),
        }),
        TIMEOUT,
    );

    let NonIdempotentRequestOutcome::ProvenNotDispatched { error } = outcome else {
        panic!("a forced predispatch write failure must retain exact dispatch proof");
    };
    assert!(matches!(
        *error,
        ManagedBackendError::WriteRequest { ref method, .. } if method == "turn/start"
    ));
    assert_eq!(source_calls.count(), 1);
    assert_eq!(source_drops.count(), 1);
    assert_eq!(sink_calls.count(), 0);
    assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
    assert!(!session.transport_is_closed_for_lifecycle_test());

    session.shutdown().unwrap();
    server.join().unwrap();
}

fn send_checked_started(socket: &mut WebSocket<TcpStream>) {
    send_json(
        socket,
        r#"{"method":"item/started","params":{"item":{"type":"userMessage","id":"item-failure","clientId":null,"content":[]},"threadId":"thread-failure","turnId":"turn-failure","startedAtMs":123}}"#,
    );
}
