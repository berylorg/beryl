use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use beryl_backend::{
    ForegroundSessionConfig, ManagedBackendClientConnector, NonIdempotentRequestOutcome,
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamRejection,
    OrderedTurnStreamSink, OrderedTurnStreamSubmitCause, OrderedTurnStreamSubmitError,
    StreamedInputDescriptor, StreamedInputDescriptorKind, StreamedInputHeader,
    StreamedInputSequenceDigestAccumulator, StreamedInputSource, StreamedInputSourceError,
    StreamedInputSourceIdentity, StreamedInputSourceRevision, StreamedTextDescriptor,
    StreamedTextPage, StreamedTextSourceId, TextSourceProof, TurnStatus, UserMessageEchoLifecycle,
};
use beryl_model::CasThreadId;

#[allow(dead_code)]
#[path = "../phase31_bounded_dispatch/support.rs"]
mod dispatch_support;

use dispatch_support::{
    AUTHORIZATION, TIMEOUT, assert_initialize, assert_initialized, expect_close, read_json,
    send_initialize_response, send_json, spawn_server,
};

#[derive(Clone, Default)]
struct SourceCalls(Arc<Mutex<usize>>);

impl SourceCalls {
    fn note(&self) {
        let mut count = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        *count = count.checked_add(1).unwrap();
    }

    fn count(&self) -> usize {
        *self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

const SUBMITTED_TEXT: &str = "phase-36";

struct TextReplaySource {
    calls: SourceCalls,
    pass_open: bool,
    descriptor_emitted: bool,
}

impl StreamedInputSource for TextReplaySource {
    fn header(&self) -> StreamedInputHeader {
        self.calls.note();
        text_header()
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        self.calls.note();
        self.pass_open = true;
        self.descriptor_emitted = false;
        Ok(text_header())
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        self.calls.note();
        if !self.pass_open {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        if self.descriptor_emitted {
            self.pass_open = false;
            return Ok(None);
        }
        self.descriptor_emitted = true;
        Ok(Some(StreamedInputDescriptor::new(
            source_identity(),
            source_revision(),
            1,
            StreamedInputDescriptorKind::Text(StreamedTextDescriptor::new(
                text_source_id(),
                text_proof(),
                u64::try_from(SUBMITTED_TEXT.len()).unwrap(),
            )),
        )))
    }

    fn read_text_page(
        &mut self,
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        self.calls.note();
        if source_id != text_source_id() || start != 0 || max_utf8_bytes < SUBMITTED_TEXT.len() {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        Ok(StreamedTextPage::new(
            source_identity(),
            source_revision(),
            text_source_id(),
            text_proof(),
            0,
            SUBMITTED_TEXT,
            None,
        ))
    }
}

fn text_header() -> StreamedInputHeader {
    let mut digest = StreamedInputSequenceDigestAccumulator::new(1);
    digest
        .push_text(
            1,
            text_proof(),
            u64::try_from(SUBMITTED_TEXT.len()).unwrap(),
        )
        .unwrap();
    StreamedInputHeader::new(
        source_identity(),
        source_revision(),
        1,
        digest.finish().unwrap(),
    )
}

fn source_identity() -> StreamedInputSourceIdentity {
    StreamedInputSourceIdentity::new([31; 32])
}

fn source_revision() -> StreamedInputSourceRevision {
    StreamedInputSourceRevision::new(7)
}

fn text_source_id() -> StreamedTextSourceId {
    StreamedTextSourceId::new([47; 32])
}

fn text_proof() -> TextSourceProof {
    TextSourceProof::new([63; 32])
}

#[derive(Debug, Eq, PartialEq)]
struct CheckedTrace {
    lifecycle: UserMessageEchoLifecycle,
    thread_id: String,
    turn_id: String,
    item_id: String,
    checked_input_items: u64,
    timestamp_ms: u64,
}

struct BlockingCheckedSink {
    trace: mpsc::SyncSender<CheckedTrace>,
    release_completed: mpsc::Receiver<()>,
}

impl OrderedTurnStreamSink for BlockingCheckedSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        let OrderedTurnStreamOperation::CheckedUserMessage(message) = operation else {
            return Err(OrderedTurnStreamSubmitError::new(
                operation,
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::SchemaMismatch),
            ));
        };
        let lifecycle = message.lifecycle();
        let trace = CheckedTrace {
            lifecycle,
            thread_id: message.thread_id().as_str().to_string(),
            turn_id: message.turn_id().as_str().to_string(),
            item_id: message.correlation().item_id().as_str().to_string(),
            checked_input_items: message.correlation().checked_input_items(),
            timestamp_ms: message.timestamp().get(),
        };
        if self.trace.send(trace).is_err() {
            return Err(OrderedTurnStreamSubmitError::new(
                OrderedTurnStreamOperation::CheckedUserMessage(message),
                OrderedTurnStreamSubmitCause::ReceiverLost,
            ));
        }
        if lifecycle == UserMessageEchoLifecycle::Completed
            && self.release_completed.recv_timeout(TIMEOUT).is_err()
        {
            return Err(OrderedTurnStreamSubmitError::new(
                OrderedTurnStreamOperation::CheckedUserMessage(message),
                OrderedTurnStreamSubmitCause::Timeout,
            ));
        }
        Ok(OrderedTurnStreamCompletion::Applied)
    }
}

#[test]
fn production_websocket_streamed_turn_waits_for_both_checked_acknowledgements() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let start = read_json(socket).unwrap();
        assert_eq!(start["jsonrpc"], "2.0");
        assert_eq!(start["id"], 2);
        assert_eq!(start["method"], "turn/start");
        assert_eq!(start["params"]["threadId"], "thread-streamed");
        assert_eq!(
            start["params"]["input"],
            serde_json::json!([{"type": "text", "text": SUBMITTED_TEXT}])
        );
        send_checked_lifecycle(socket, "item/started", "startedAtMs");
        send_checked_lifecycle(socket, "item/completed", "completedAtMs");
        send_json(
            socket,
            r#"{"id":2,"result":{"turn":{"id":"turn-streamed","items":[],"status":"inProgress"}}}"#,
        );
        expect_close(socket);
    });

    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    let mut session = connector
        .connect_foreground_candidate(
            ForegroundSessionConfig::new(NonZeroUsize::new(16).unwrap()),
            TIMEOUT,
        )
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();

    let (trace_sender, trace_receiver) = mpsc::sync_channel(2);
    let (release_sender, release_receiver) = mpsc::sync_channel(0);
    session
        .bind_ordered_turn_stream_sink(Box::new(BlockingCheckedSink {
            trace: trace_sender,
            release_completed: release_receiver,
        }))
        .unwrap();

    let calls = SourceCalls::default();
    let worker_calls = calls.clone();
    let (result_sender, result_receiver) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let thread_id = CasThreadId::new("thread-streamed").unwrap();
        let outcome = session.start_turn_with_streamed_input(
            &thread_id,
            Box::new(TextReplaySource {
                calls: worker_calls,
                pass_open: false,
                descriptor_emitted: false,
            }),
            TIMEOUT,
        );
        let NonIdempotentRequestOutcome::ExactResponse { response } = outcome else {
            panic!("production streamed turn/start did not return an exact response");
        };
        let state = session.predispatch_state_for_lifecycle_test();
        let closed = session.transport_is_closed_for_lifecycle_test();
        session.shutdown().unwrap();
        result_sender
            .send((
                response.thread_id().as_str().to_string(),
                response.turn_id().as_str().to_string(),
                *response.status(),
                state,
                closed,
            ))
            .unwrap();
    });

    assert_eq!(
        trace_receiver.recv_timeout(TIMEOUT).unwrap(),
        CheckedTrace {
            lifecycle: UserMessageEchoLifecycle::Started,
            thread_id: "thread-streamed".to_string(),
            turn_id: "turn-streamed".to_string(),
            item_id: "item-streamed".to_string(),
            checked_input_items: 1,
            timestamp_ms: 123,
        }
    );
    assert_eq!(
        trace_receiver.recv_timeout(TIMEOUT).unwrap(),
        CheckedTrace {
            lifecycle: UserMessageEchoLifecycle::Completed,
            thread_id: "thread-streamed".to_string(),
            turn_id: "turn-streamed".to_string(),
            item_id: "item-streamed".to_string(),
            checked_input_items: 1,
            timestamp_ms: 123,
        }
    );
    assert!(matches!(
        result_receiver.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    release_sender.send(()).unwrap();
    let (thread_id, turn_id, status, state, closed) =
        result_receiver.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(thread_id, "thread-streamed");
    assert_eq!(turn_id, "turn-streamed");
    assert_eq!(status, TurnStatus::InProgress);
    assert_eq!(state, (3, true, false));
    assert!(!closed);
    assert_eq!(calls.count(), 13);

    worker.join().unwrap();
    server.join().unwrap();
}

fn send_checked_lifecycle(
    socket: &mut tungstenite::WebSocket<std::net::TcpStream>,
    method: &str,
    timestamp: &str,
) {
    send_json(
        socket,
        &format!(
            r#"{{"method":"{method}","params":{{"item":{{"type":"userMessage","id":"item-streamed","clientId":null,"content":[{{"type":"text","text":"{SUBMITTED_TEXT}","text_elements":[]}}]}},"threadId":"thread-streamed","turnId":"turn-streamed","{timestamp}":123}}}}"#,
        ),
    );
}
