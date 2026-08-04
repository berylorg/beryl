use std::{
    num::NonZeroUsize,
    sync::mpsc,
    sync::{Arc, Mutex},
    time::Duration,
};

use beryl_backend::{
    CheckedSteeringUserMessage, CheckedSteeringUserMessageSubmitError, ClientUserMessageId,
    ForegroundSessionConfig, ManagedBackendClientConnector, ManagedBackendError,
    ManagedBackendSession, NonIdempotentRequestOutcome, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamProgress, OrderedTurnStreamRejection,
    OrderedTurnStreamSink, OrderedTurnStreamSubmitCause, OrderedTurnStreamSubmitError,
    SteeringUserMessageAbandonReason, SteeringUserMessageSelection,
    SteeringUserMessageSelectionError, SteeringUserMessageSource, StreamedInputDescriptor,
    StreamedInputDescriptorKind, StreamedInputHeader, StreamedInputSequenceDigestAccumulator,
    StreamedInputSource, StreamedInputSourceError, StreamedInputSourceIdentity,
    StreamedInputSourceRevision, StreamedTextDescriptor, StreamedTextPage, StreamedTextSourceId,
    TextSourceProof, TurnSteerOutcome, UserMessageEchoLifecycle,
    lifecycle_test_support::{
        decode_provider_json_for_test, decode_provider_transport_loss_for_test,
    },
};
use beryl_model::{CasThreadId, CasTurnId};

#[allow(dead_code)]
#[path = "../phase31_bounded_dispatch/support.rs"]
mod dispatch_support;

use dispatch_support::{
    AUTHORIZATION, TIMEOUT, assert_initialize, assert_initialized, expect_close, read_json,
    send_initialize_response, send_json, spawn_server,
};

const TEXT: &str = "steered text";

fn source_identity() -> StreamedInputSourceIdentity {
    StreamedInputSourceIdentity::new([71; 32])
}

fn source_revision() -> StreamedInputSourceRevision {
    StreamedInputSourceRevision::new(4)
}

fn text_source_id() -> StreamedTextSourceId {
    StreamedTextSourceId::new([72; 32])
}

fn text_proof() -> TextSourceProof {
    TextSourceProof::new([73; 32])
}

fn text_header() -> StreamedInputHeader {
    let mut digest = StreamedInputSequenceDigestAccumulator::new(1);
    digest
        .push_text(1, text_proof(), u64::try_from(TEXT.len()).unwrap())
        .unwrap();
    StreamedInputHeader::new(
        source_identity(),
        source_revision(),
        1,
        digest.finish().unwrap(),
    )
}

struct TextReplaySource {
    descriptor_emitted: bool,
}

impl TextReplaySource {
    const fn new() -> Self {
        Self {
            descriptor_emitted: false,
        }
    }
}

impl StreamedInputSource for TextReplaySource {
    fn header(&self) -> StreamedInputHeader {
        text_header()
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        self.descriptor_emitted = false;
        Ok(text_header())
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        if self.descriptor_emitted {
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
                u64::try_from(TEXT.len()).unwrap(),
            )),
        )))
    }

    fn read_text_page(
        &mut self,
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        if source_id != text_source_id() || start != 0 || max_utf8_bytes < TEXT.len() {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        Ok(StreamedTextPage::new(
            source_identity(),
            source_revision(),
            text_source_id(),
            text_proof(),
            0,
            TEXT,
            None,
        ))
    }
}

struct UnreadSource;

impl StreamedInputSource for UnreadSource {
    fn header(&self) -> StreamedInputHeader {
        panic!("pre-dispatch gate read the steering source header")
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        panic!("pre-dispatch gate began steering source replay")
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        panic!("pre-dispatch gate read a steering descriptor")
    }

    fn read_text_page(
        &mut self,
        _source_id: StreamedTextSourceId,
        _start: u64,
        _max_utf8_bytes: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        panic!("pre-dispatch gate read steering text")
    }
}

fn steer_with_unread_source(session: &mut ManagedBackendSession) -> TurnSteerOutcome {
    session.steer_turn_with_streamed_input(
        &CasThreadId::new("thread-steer").unwrap(),
        &CasTurnId::new("turn-steer").unwrap(),
        &ClientUserMessageId::try_new("correlation-1").unwrap(),
        Box::new(UnreadSource),
        Duration::from_millis(20),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SelectionTrace {
    lifecycle: UserMessageEchoLifecycle,
    item_id: String,
    client_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CheckedTrace {
    lifecycle: UserMessageEchoLifecycle,
    thread_id: String,
    turn_id: String,
    item_id: String,
    client_id: String,
    timestamp_ms: u64,
    checked_input_items: u64,
}

#[derive(Default)]
struct SinkTrace {
    selections: Vec<SelectionTrace>,
    checked: Vec<CheckedTrace>,
    abandoned: Vec<SteeringUserMessageAbandonReason>,
}

struct SteeringSink {
    trace: Arc<Mutex<SinkTrace>>,
    expected_thread_id: CasThreadId,
    expected_turn_id: CasTurnId,
    reject_duplicate: bool,
}

impl SteeringSink {
    fn new(trace: Arc<Mutex<SinkTrace>>) -> Self {
        Self {
            trace,
            expected_thread_id: CasThreadId::new("thread-steer").unwrap(),
            expected_turn_id: CasTurnId::new("turn-steer").unwrap(),
            reject_duplicate: false,
        }
    }

    fn with_expected_turn(mut self, turn_id: &str) -> Self {
        self.expected_turn_id = CasTurnId::new(turn_id).unwrap();
        self
    }

    fn rejecting_duplicates(mut self) -> Self {
        self.reject_duplicate = true;
        self
    }
}

impl OrderedTurnStreamSink for SteeringSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        Err(OrderedTurnStreamSubmitError::new(
            operation,
            OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::SchemaMismatch),
        ))
    }

    fn select_steering_user_message(
        &mut self,
        selection: SteeringUserMessageSelection,
    ) -> Result<SteeringUserMessageSource, SteeringUserMessageSelectionError> {
        let mut trace = self.trace.lock().unwrap();
        if self.reject_duplicate
            && trace.selections.iter().any(|prior| {
                prior.client_id == selection.client_user_message_id().as_str()
                    && prior.lifecycle == selection.lifecycle()
            })
        {
            drop(trace);
            return Err(SteeringUserMessageSelectionError::new(
                selection,
                OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::StagingConflict),
            ));
        }
        trace.selections.push(SelectionTrace {
            lifecycle: selection.lifecycle(),
            item_id: selection.item_id().as_str().to_string(),
            client_id: selection.client_user_message_id().as_str().to_string(),
        });
        drop(trace);
        Ok(SteeringUserMessageSource::new(
            self.expected_thread_id.clone(),
            self.expected_turn_id.clone(),
            Box::new(TextReplaySource::new()),
        ))
    }

    fn submit_checked_steering_user_message(
        &mut self,
        message: CheckedSteeringUserMessage,
    ) -> Result<(), CheckedSteeringUserMessageSubmitError> {
        self.trace.lock().unwrap().checked.push(CheckedTrace {
            lifecycle: message.lifecycle(),
            thread_id: message.thread_id().as_str().to_string(),
            turn_id: message.turn_id().as_str().to_string(),
            item_id: message.item_id().as_str().to_string(),
            client_id: message.client_user_message_id().as_str().to_string(),
            timestamp_ms: message.timestamp().get(),
            checked_input_items: message.checked_input_items(),
        });
        Ok(())
    }

    fn abandon_steering_user_message(
        &mut self,
        reason: SteeringUserMessageAbandonReason,
    ) -> Result<(), OrderedTurnStreamSubmitCause> {
        self.trace.lock().unwrap().abandoned.push(reason);
        Ok(())
    }
}

fn lifecycle_json(method: &str, text: &str, turn_id: &str, client_id: &str) -> String {
    let timestamp = if method == "item/started" {
        "startedAtMs"
    } else {
        "completedAtMs"
    };
    format!(
        r#"{{"method":"{method}","params":{{"item":{{"type":"userMessage","id":"item-steer","clientId":"{client_id}","content":[{{"type":"text","text":"{text}","text_elements":[]}}]}},"threadId":"thread-steer","turnId":"{turn_id}","{timestamp}":123}}}}"#,
    )
}

#[test]
fn delayed_correlation_selects_fresh_replay_and_commits_exact_checked_lifecycle() {
    for fragment_bytes in [1, 3, 17, 257] {
        let trace = Arc::new(Mutex::new(SinkTrace::default()));
        let mut sink = SteeringSink::new(trace.clone());
        let input = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1");

        assert_eq!(
            decode_provider_json_for_test(input.as_bytes(), fragment_bytes, &mut sink).unwrap(),
            OrderedTurnStreamProgress::Progress
        );
        let trace = trace.lock().unwrap();
        assert_eq!(
            trace.selections,
            vec![SelectionTrace {
                lifecycle: UserMessageEchoLifecycle::Started,
                item_id: "item-steer".to_string(),
                client_id: "correlation-1".to_string(),
            }]
        );
        assert_eq!(
            trace.checked,
            vec![CheckedTrace {
                lifecycle: UserMessageEchoLifecycle::Started,
                thread_id: "thread-steer".to_string(),
                turn_id: "turn-steer".to_string(),
                item_id: "item-steer".to_string(),
                client_id: "correlation-1".to_string(),
                timestamp_ms: 123,
                checked_input_items: 1,
            }]
        );
        assert!(trace.abandoned.is_empty());
    }
}

#[test]
fn delayed_completed_lifecycle_is_checked_from_its_own_fresh_pass() {
    let trace = Arc::new(Mutex::new(SinkTrace::default()));
    let mut sink = SteeringSink::new(trace.clone());
    let input = lifecycle_json("item/completed", TEXT, "turn-steer", "correlation-1");

    decode_provider_json_for_test(input.as_bytes(), 2, &mut sink).unwrap();

    let trace = trace.lock().unwrap();
    assert_eq!(trace.checked.len(), 1);
    assert_eq!(
        trace.checked[0].lifecycle,
        UserMessageEchoLifecycle::Completed
    );
    assert!(trace.abandoned.is_empty());
}

#[test]
fn missing_or_late_correlation_fails_before_unbounded_content() {
    let cases = [
        r#"{"method":"item/started","params":{"item":{"type":"userMessage","id":"item-steer","clientId":null,"content":[]},"threadId":"thread-steer","turnId":"turn-steer","startedAtMs":123}}"#,
        r#"{"method":"item/started","params":{"item":{"type":"userMessage","id":"item-steer","content":[],"clientId":"correlation-1"},"threadId":"thread-steer","turnId":"turn-steer","startedAtMs":123}}"#,
    ];
    for input in cases {
        let trace = Arc::new(Mutex::new(SinkTrace::default()));
        let mut sink = SteeringSink::new(trace.clone());
        let error = decode_provider_json_for_test(input.as_bytes(), 3, &mut sink).unwrap_err();
        assert!(matches!(
            error,
            ManagedBackendError::SteeringUserMessage { .. }
        ));
        let trace = trace.lock().unwrap();
        assert!(trace.selections.is_empty());
        assert!(trace.checked.is_empty());
    }
}

#[test]
fn content_or_exact_turn_disagreement_abandons_selected_correlation() {
    for (text, expected_turn) in [
        (String::from("wrong"), "turn-steer"),
        (TEXT.into(), "turn-other"),
    ] {
        let trace = Arc::new(Mutex::new(SinkTrace::default()));
        let mut sink = SteeringSink::new(trace.clone()).with_expected_turn(expected_turn);
        let input = lifecycle_json("item/started", &text, "turn-steer", "correlation-1");

        let error = decode_provider_json_for_test(input.as_bytes(), 2, &mut sink).unwrap_err();
        assert!(
            matches!(error, ManagedBackendError::SteeringUserMessage { .. }),
            "unexpected delayed-correlation error: {error:?}"
        );
        let trace = trace.lock().unwrap();
        assert_eq!(trace.selections.len(), 1);
        assert!(trace.checked.is_empty());
        assert_eq!(
            trace.abandoned,
            vec![SteeringUserMessageAbandonReason::CorrelationFailure]
        );
    }
}

#[test]
fn cross_thread_or_post_selection_schema_failure_abandons_selected_correlation() {
    let cross_thread = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1").replace(
        r#""threadId":"thread-steer""#,
        r#""threadId":"thread-other""#,
    );
    let malformed = r#"{"method":"item/started","params":{"item":{"type":"userMessage","id":"item-steer","clientId":"correlation-1","content":{}},"threadId":"thread-steer","turnId":"turn-steer","startedAtMs":123}}"#;

    for (input, expected_reason) in [
        (
            cross_thread.as_str(),
            SteeringUserMessageAbandonReason::CorrelationFailure,
        ),
        (malformed, SteeringUserMessageAbandonReason::SchemaFailure),
    ] {
        let trace = Arc::new(Mutex::new(SinkTrace::default()));
        let mut sink = SteeringSink::new(trace.clone());

        let error = decode_provider_json_for_test(input.as_bytes(), 2, &mut sink).unwrap_err();

        match expected_reason {
            SteeringUserMessageAbandonReason::CorrelationFailure => assert!(matches!(
                error,
                ManagedBackendError::SteeringUserMessage { .. }
            )),
            SteeringUserMessageAbandonReason::SchemaFailure => assert!(matches!(
                error,
                ManagedBackendError::ProviderObservation { .. }
            )),
            _ => unreachable!("test covers only correlation and schema failures"),
        }
        let trace = trace.lock().unwrap();
        assert_eq!(trace.selections.len(), 1);
        assert!(trace.checked.is_empty());
        assert_eq!(trace.abandoned, vec![expected_reason]);
    }
}

#[test]
fn transport_loss_after_selection_abandons_the_delayed_correlation() {
    let input = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1");
    let text_start = input.find(TEXT).unwrap();
    let prefix = &input.as_bytes()[..text_start + 4];
    let trace = Arc::new(Mutex::new(SinkTrace::default()));
    let mut sink = SteeringSink::new(trace.clone());

    decode_provider_transport_loss_for_test(prefix, 3, &mut sink).unwrap_err();

    let trace = trace.lock().unwrap();
    assert_eq!(trace.selections.len(), 1);
    assert!(trace.checked.is_empty());
    assert_eq!(
        trace.abandoned,
        vec![SteeringUserMessageAbandonReason::TransportLost]
    );
}

#[test]
fn duplicate_delayed_lifecycle_is_rejected_by_ordered_correlation_authority() {
    let trace = Arc::new(Mutex::new(SinkTrace::default()));
    let mut sink = SteeringSink::new(trace.clone()).rejecting_duplicates();
    let input = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1");

    decode_provider_json_for_test(input.as_bytes(), 5, &mut sink).unwrap();
    let error = decode_provider_json_for_test(input.as_bytes(), 5, &mut sink).unwrap_err();

    assert!(matches!(
        error,
        ManagedBackendError::SteeringUserMessage { .. }
    ));
    let trace = trace.lock().unwrap();
    assert_eq!(trace.selections.len(), 1);
    assert_eq!(trace.checked.len(), 1);
}

#[test]
fn stdio_and_request_only_steering_fail_before_source_replay_or_session_mutation() {
    let mut stdio = ManagedBackendSession::stdio_streamed_input_gate_for_lifecycle_test().unwrap();
    let before = stdio.predispatch_state_for_lifecycle_test();
    let outcome = steer_with_unread_source(&mut stdio);
    assert!(matches!(
        outcome,
        NonIdempotentRequestOutcome::ProvenNotDispatched { error }
            if matches!(
                *error,
                ManagedBackendError::StreamedInputTransportUnsupported {
                    ref method,
                    transport: "stdio",
                } if method == "turn/steer"
            )
    ));
    assert_eq!(stdio.predispatch_state_for_lifecycle_test(), before);

    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    let mut request_only = connector.connect_request_client(TIMEOUT).unwrap();
    let before = request_only.predispatch_state_for_lifecycle_test();

    let outcome = steer_with_unread_source(&mut request_only);

    assert!(matches!(
        outcome,
        NonIdempotentRequestOutcome::ProvenNotDispatched { error }
            if matches!(
                *error,
                ManagedBackendError::StreamedInputTransportUnsupported {
                    ref method,
                    transport: "request-only websocket",
                } if method == "turn/steer"
            )
    ));
    assert_eq!(request_only.predispatch_state_for_lifecycle_test(), before);
    request_only.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn uninitialized_missing_profile_and_missing_sink_steering_gates_are_predispatch() {
    let (uninitialized_endpoint, uninitialized_server) = spawn_server(expect_close);
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(uninitialized_endpoint, AUTHORIZATION);
    let mut uninitialized = connector
        .connect_foreground_candidate(
            ForegroundSessionConfig::new(NonZeroUsize::new(8).unwrap()),
            TIMEOUT,
        )
        .unwrap();
    let before = uninitialized.predispatch_state_for_lifecycle_test();

    let outcome = steer_with_unread_source(&mut uninitialized);

    assert!(matches!(
        outcome,
        NonIdempotentRequestOutcome::ProvenNotDispatched { error }
            if matches!(*error, ManagedBackendError::ClientNotInitialized)
    ));
    assert_eq!(uninitialized.predispatch_state_for_lifecycle_test(), before);
    uninitialized.shutdown().unwrap();
    uninitialized_server.join().unwrap();

    let (initialized_endpoint, initialized_server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(initialized_endpoint, AUTHORIZATION);
    let mut initialized = connector
        .connect_foreground_candidate(
            ForegroundSessionConfig::new(NonZeroUsize::new(8).unwrap()),
            TIMEOUT,
        )
        .unwrap();
    initialized.initialize_foreground(TIMEOUT).unwrap();
    let before = initialized.predispatch_state_for_lifecycle_test();

    let outcome = steer_with_unread_source(&mut initialized);

    assert!(matches!(
        outcome,
        NonIdempotentRequestOutcome::ProvenNotDispatched { error }
            if matches!(*error, ManagedBackendError::OrderedTurnStreamSinkUnbound)
    ));
    assert_eq!(initialized.predispatch_state_for_lifecycle_test(), before);

    initialized.opt_out_full_turn_stream_for_lifecycle_test();
    let before = initialized.predispatch_state_for_lifecycle_test();
    let outcome = steer_with_unread_source(&mut initialized);
    assert!(matches!(
        outcome,
        NonIdempotentRequestOutcome::ProvenNotDispatched { error }
            if matches!(
                *error,
                ManagedBackendError::RequestProfileMismatch {
                    method: "turn/steer",
                    required_profile: "full turn stream",
                }
            )
    ));
    assert_eq!(initialized.predispatch_state_for_lifecycle_test(), before);
    initialized.shutdown().unwrap();
    initialized_server.join().unwrap();
}

#[test]
fn production_streamed_steer_returns_before_and_later_checks_correlated_lifecycle() {
    let (release_sender, release_receiver) = mpsc::sync_channel(0);
    let (endpoint, server) = spawn_server(move |socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let request = read_json(socket).unwrap();
        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 2);
        assert_eq!(request["method"], "turn/steer");
        let params = request["params"].as_object().unwrap();
        assert_eq!(params.len(), 4);
        assert_eq!(params["threadId"], "thread-steer");
        assert_eq!(params["clientUserMessageId"], "correlation-1");
        assert_eq!(params["expectedTurnId"], "turn-steer");
        assert_eq!(
            params["input"],
            serde_json::json!([{"type": "text", "text": TEXT}])
        );
        send_json(socket, r#"{"id":2,"result":{"turnId":"turn-steer"}}"#);
        release_receiver.recv_timeout(TIMEOUT).unwrap();
        send_json(
            socket,
            &lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1"),
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
    let trace = Arc::new(Mutex::new(SinkTrace::default()));
    session
        .bind_ordered_turn_stream_sink(Box::new(SteeringSink::new(trace.clone())))
        .unwrap();

    let thread_id = CasThreadId::new("thread-steer").unwrap();
    let turn_id = CasTurnId::new("turn-steer").unwrap();
    let client_id = ClientUserMessageId::try_new("correlation-1").unwrap();
    let outcome = session.steer_turn_with_streamed_input(
        &thread_id,
        &turn_id,
        &client_id,
        Box::new(TextReplaySource::new()),
        TIMEOUT,
    );
    let NonIdempotentRequestOutcome::ExactResponse { response } = outcome else {
        panic!("streamed turn/steer did not return exact success");
    };
    assert_eq!(response.turn_id(), &turn_id);
    assert!(trace.lock().unwrap().selections.is_empty());

    release_sender.send(()).unwrap();
    assert_eq!(
        session.poll_ordered_turn_stream_progress(TIMEOUT).unwrap(),
        OrderedTurnStreamProgress::Progress
    );
    let trace = trace.lock().unwrap();
    assert_eq!(trace.selections.len(), 1);
    assert_eq!(trace.checked.len(), 1);
    drop(trace);

    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn exact_rejection_keeps_steering_session_reusable_without_diagnostic_inference() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let rejected = read_json(socket).unwrap();
        assert_eq!(rejected["id"], 2);
        assert_eq!(rejected["method"], "turn/steer");
        send_json(
            socket,
            r#"{"error":{"code":-32600,"message":"active turn mismatch"},"id":2}"#,
        );

        let retry = read_json(socket).unwrap();
        assert_eq!(retry["id"], 3);
        assert_eq!(retry["method"], "turn/steer");
        send_json(socket, r#"{"id":3,"result":{"turnId":"turn-steer"}}"#);
        expect_close(socket);
    });
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    let mut session = connector
        .connect_foreground_candidate(
            ForegroundSessionConfig::new(NonZeroUsize::new(8).unwrap()),
            TIMEOUT,
        )
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    session
        .bind_ordered_turn_stream_sink(Box::new(SteeringSink::new(Arc::new(Mutex::new(
            SinkTrace::default(),
        )))))
        .unwrap();

    let thread_id = CasThreadId::new("thread-steer").unwrap();
    let turn_id = CasTurnId::new("turn-steer").unwrap();
    let client_id = ClientUserMessageId::try_new("correlation-1").unwrap();
    let rejected = session.steer_turn_with_streamed_input(
        &thread_id,
        &turn_id,
        &client_id,
        Box::new(TextReplaySource::new()),
        TIMEOUT,
    );
    let NonIdempotentRequestOutcome::ExactRejection { error } = rejected else {
        panic!("matching steering rejection was not exact");
    };
    assert_eq!(error.code(), -32_600);
    assert_eq!(error.message(), "active turn mismatch");
    assert_eq!(error.verdict(), None);
    assert!(!session.transport_is_closed_for_lifecycle_test());

    let retry = session.steer_turn_with_streamed_input(
        &thread_id,
        &turn_id,
        &client_id,
        Box::new(TextReplaySource::new()),
        TIMEOUT,
    );
    assert!(matches!(
        retry,
        NonIdempotentRequestOutcome::ExactResponse { response }
            if response.turn_id() == &turn_id
    ));

    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn mismatched_success_turn_is_completion_unknown_and_retires_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["method"], "turn/steer");
        send_json(socket, r#"{"id":2,"result":{"turnId":"turn-other"}}"#);
        expect_close(socket);
    });
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    let mut session = connector
        .connect_foreground_candidate(
            ForegroundSessionConfig::new(NonZeroUsize::new(8).unwrap()),
            TIMEOUT,
        )
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    session
        .bind_ordered_turn_stream_sink(Box::new(SteeringSink::new(Arc::new(Mutex::new(
            SinkTrace::default(),
        )))))
        .unwrap();

    let expected = CasTurnId::new("turn-steer").unwrap();
    let outcome = session.steer_turn_with_streamed_input(
        &CasThreadId::new("thread-steer").unwrap(),
        &expected,
        &ClientUserMessageId::try_new("correlation-1").unwrap(),
        Box::new(TextReplaySource::new()),
        TIMEOUT,
    );

    let NonIdempotentRequestOutcome::CompletionUnknown { error } = outcome else {
        panic!("wrong turn identity must be completion unknown");
    };
    assert!(matches!(
        *error,
        ManagedBackendError::TurnResponseIdentityMismatch {
            ref method,
            ref expected,
            ref actual,
        } if method == "turn/steer"
            && expected.as_str() == "turn-steer"
            && actual.as_str() == "turn-other"
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn malformed_or_missing_response_is_completion_unknown_and_retires_connection() {
    for malformed in [true, false] {
        let (endpoint, server) = spawn_server(move |socket| {
            assert_initialize(&read_json(socket).unwrap(), false);
            send_initialize_response(socket, 1);
            assert_initialized(&read_json(socket).unwrap());
            let request = read_json(socket).unwrap();
            assert_eq!(request["method"], "turn/steer");
            if malformed {
                send_json(socket, r#"{"id":2,"result":{}}"#);
            }
            expect_close(socket);
        });
        let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
        let mut session = connector
            .connect_foreground_candidate(
                ForegroundSessionConfig::new(NonZeroUsize::new(8).unwrap()),
                TIMEOUT,
            )
            .unwrap();
        session.initialize_foreground(TIMEOUT).unwrap();
        session
            .bind_ordered_turn_stream_sink(Box::new(SteeringSink::new(Arc::new(Mutex::new(
                SinkTrace::default(),
            )))))
            .unwrap();

        let outcome = session.steer_turn_with_streamed_input(
            &CasThreadId::new("thread-steer").unwrap(),
            &CasTurnId::new("turn-steer").unwrap(),
            &ClientUserMessageId::try_new("correlation-1").unwrap(),
            Box::new(TextReplaySource::new()),
            Duration::from_millis(40),
        );

        let NonIdempotentRequestOutcome::CompletionUnknown { error } = outcome else {
            panic!("post-dispatch steering response failure must be completion unknown");
        };
        if malformed {
            assert!(matches!(
                *error,
                ManagedBackendError::ForegroundIngress { .. }
            ));
        } else {
            assert!(matches!(
                *error,
                ManagedBackendError::RequestTimeout {
                    ref method,
                    timeout,
                } if method == "turn/steer" && timeout == Duration::from_millis(40)
            ));
        }
        assert!(session.transport_is_closed_for_lifecycle_test());
        server.join().unwrap();
    }
}

#[test]
fn proven_predispatch_steer_failure_releases_expectation_and_keeps_session_reusable() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    let mut session = connector
        .connect_foreground_candidate(
            ForegroundSessionConfig::new(NonZeroUsize::new(8).unwrap()),
            TIMEOUT,
        )
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    session
        .bind_ordered_turn_stream_sink(Box::new(SteeringSink::new(Arc::new(Mutex::new(
            SinkTrace::default(),
        )))))
        .unwrap();
    let before = session.predispatch_state_for_lifecycle_test();
    session.fail_next_write_before_dispatch_for_lifecycle_test();

    let outcome = session.steer_turn_with_streamed_input(
        &CasThreadId::new("thread-steer").unwrap(),
        &CasTurnId::new("turn-steer").unwrap(),
        &ClientUserMessageId::try_new("correlation-1").unwrap(),
        Box::new(TextReplaySource::new()),
        Duration::from_secs(1),
    );

    assert!(matches!(
        outcome,
        NonIdempotentRequestOutcome::ProvenNotDispatched { .. }
    ));
    assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
    assert!(!session.transport_is_closed_for_lifecycle_test());
    session.shutdown().unwrap();
    server.join().unwrap();
}
