use super::*;
use beryl_backend::{SteeringUserMessageCaptureMode, UnverifiedSteeringUserMessage};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct PassiveSink {
    failed: Arc<AtomicBool>,
    selections: usize,
    unverified: Vec<UnverifiedSteeringUserMessage>,
    abandoned: Vec<SteeringUserMessageAbandonReason>,
    reads: Arc<AtomicUsize>,
    source_dropped: Arc<AtomicBool>,
    cancel: bool,
    restore_mode_after_passive: bool,
    mode_queries: usize,
    fail_after_query: Option<usize>,
    cancel_after_query: Option<usize>,
    fail_on_descriptor: bool,
    sealed: Arc<AtomicUsize>,
}

impl PassiveSink {
    fn new(failed: bool) -> Self {
        Self {
            failed: Arc::new(AtomicBool::new(failed)),
            selections: 0,
            unverified: Vec::new(),
            abandoned: Vec::new(),
            reads: Arc::new(AtomicUsize::new(0)),
            source_dropped: Arc::new(AtomicBool::new(false)),
            cancel: false,
            restore_mode_after_passive: false,
            mode_queries: 0,
            fail_after_query: None,
            cancel_after_query: None,
            fail_on_descriptor: false,
            sealed: Arc::new(AtomicUsize::new(0)),
        }
    }
}

struct FailingReplay {
    source: TextReplaySource,
    failed: Arc<AtomicBool>,
    reads: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
    fail_on_descriptor: bool,
}

impl StreamedInputSource for FailingReplay {
    fn header(&self) -> StreamedInputHeader {
        self.source.header()
    }
    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        self.source.begin_pass()
    }
    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        let result = self.source.next_descriptor();
        if self.fail_on_descriptor {
            self.failed.store(true, Ordering::SeqCst);
        }
        result
    }
    fn read_text_page(
        &mut self,
        _: StreamedTextSourceId,
        _: u64,
        _: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        assert_eq!(
            self.reads.fetch_add(1, Ordering::SeqCst),
            0,
            "failed replay cannot be retried"
        );
        self.failed.store(true, Ordering::SeqCst);
        Err(StreamedInputSourceError::InvalidSource)
    }
}

impl Drop for FailingReplay {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

impl OrderedTurnStreamSink for PassiveSink {
    fn submit(
        &mut self,
        _: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        panic!("steering echoes have a distinct completion")
    }
    fn steering_user_message_capture_mode(
        &mut self,
    ) -> Result<SteeringUserMessageCaptureMode, OrderedTurnStreamSubmitCause> {
        if self.cancel {
            return Err(OrderedTurnStreamSubmitCause::Cancelled);
        }
        let failed = self.failed.load(Ordering::SeqCst);
        self.mode_queries += 1;
        if self.cancel_after_query == Some(self.mode_queries) {
            self.cancel = true;
        }
        if self.fail_after_query == Some(self.mode_queries) {
            self.failed.store(true, Ordering::SeqCst);
        }
        if failed && self.restore_mode_after_passive {
            self.failed.store(false, Ordering::SeqCst);
        }
        Ok(if failed {
            SteeringUserMessageCaptureMode::Passive
        } else {
            SteeringUserMessageCaptureMode::Verify
        })
    }
    fn select_steering_user_message(
        &mut self,
        _: SteeringUserMessageSelection,
    ) -> Result<SteeringUserMessageSource, SteeringUserMessageSelectionError> {
        self.selections += 1;
        Ok(SteeringUserMessageSource::new(
            CasThreadId::new("thread-steer").unwrap(),
            CasTurnId::new("turn-steer").unwrap(),
            Box::new(FailingReplay {
                source: TextReplaySource::new(),
                failed: self.failed.clone(),
                reads: self.reads.clone(),
                dropped: self.source_dropped.clone(),
                fail_on_descriptor: self.fail_on_descriptor,
            }),
        ))
    }
    fn submit_checked_steering_user_message(
        &mut self,
        _: CheckedSteeringUserMessage,
    ) -> Result<(), CheckedSteeringUserMessageSubmitError> {
        panic!("unverified input cannot construct a checked completion")
    }
    fn submit_unverified_steering_user_message(
        &mut self,
        message: UnverifiedSteeringUserMessage,
    ) -> Result<(), OrderedTurnStreamSubmitCause> {
        self.unverified.push(message);
        self.sealed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn abandon_steering_user_message(
        &mut self,
        reason: SteeringUserMessageAbandonReason,
    ) -> Result<(), OrderedTurnStreamSubmitCause> {
        self.abandoned.push(reason);
        Ok(())
    }
}

#[test]
fn passive_echo_discards_large_text_without_replay_then_accepts_later_echo() {
    let mut sink = PassiveSink::new(true);
    let large = "λ".repeat(100_000);
    for (method, text) in [
        ("item/started", large.as_str()),
        ("item/completed", "later"),
    ] {
        let input = lifecycle_json(method, text, "turn-steer", "correlation-1");
        decode_provider_json_for_test(input.as_bytes(), 17, &mut sink).unwrap();
    }
    assert_eq!(sink.selections, 0);
    assert_eq!(sink.reads.load(Ordering::SeqCst), 0);
    assert_eq!(sink.unverified.len(), 2);
    let message = &sink.unverified[0];
    assert_eq!(message.thread_id().as_str(), "thread-steer");
    assert_eq!(message.turn_id().as_str(), "turn-steer");
    assert_eq!(message.item_id().as_str(), "item-steer");
    assert_eq!(message.client_user_message_id().as_str(), "correlation-1");
    assert_eq!(message.timestamp().get(), 123);
    assert_eq!(message.lifecycle(), UserMessageEchoLifecycle::Started);
    assert!(sink.abandoned.is_empty());
}

#[test]
fn replay_failure_switches_only_on_reported_service_failure_and_releases_source() {
    let mut sink = PassiveSink::new(false);
    let input = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1");
    decode_provider_json_for_test(input.as_bytes(), 1, &mut sink).unwrap();
    assert_eq!(sink.selections, 1);
    assert_eq!(sink.reads.load(Ordering::SeqCst), 1);
    assert!(sink.source_dropped.load(Ordering::SeqCst));
    assert_eq!(sink.unverified.len(), 1);
    assert!(sink.abandoned.is_empty());
}

#[test]
fn passive_observation_never_returns_to_verification() {
    let mut sink = PassiveSink::new(true);
    sink.restore_mode_after_passive = true;
    let input = lifecycle_json(
        "item/completed",
        "not the stored text",
        "turn-steer",
        "correlation-1",
    );
    decode_provider_json_for_test(input.as_bytes(), 1, &mut sink).unwrap();
    assert_eq!(sink.selections, 0);
    assert_eq!(sink.unverified.len(), 1);
}

#[test]
fn passive_echo_still_rejects_closed_schema_and_route_violations() {
    let base = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1");
    let cases = [
        base.replace(
            "\"text_elements\":[]",
            "\"text_elements\":[],\"extra\":true",
        ),
        base.replace("\"type\":\"text\"", "\"type\":\"audio\""),
        base.replace("\"text_elements\":[]", "\"text_elements\":[1]"),
        base.replace("\"turnId\":\"turn-steer\"", "\"turnId\":\"\""),
        base.replace("\"text\":\"steered text\"", "\"text\":\"bad\\uD800\""),
        base.replace(
            "\"text_elements\":[]",
            "\"text_elements\":[],\"text\":\"duplicate\"",
        ),
    ];
    for input in cases {
        let mut sink = PassiveSink::new(true);
        assert!(decode_provider_json_for_test(input.as_bytes(), 3, &mut sink).is_err());
        assert!(sink.unverified.is_empty());
        assert_eq!(sink.abandoned.len(), 1);
    }
}

#[test]
fn passive_cancellation_never_publishes_a_completion() {
    let mut sink = PassiveSink::new(true);
    sink.cancel = true;
    let input = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1");
    assert!(decode_provider_json_for_test(input.as_bytes(), 3, &mut sink).is_err());
    assert!(sink.unverified.is_empty());
    assert_eq!(
        sink.abandoned,
        vec![SteeringUserMessageAbandonReason::Cancelled]
    );
}

#[test]
fn cancellation_during_passive_text_consumption_abandons_without_completion() {
    let mut sink = PassiveSink::new(true);
    sink.cancel_after_query = Some(4);
    let input = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1");
    assert!(decode_provider_json_for_test(input.as_bytes(), 1, &mut sink).is_err());
    assert_eq!(sink.mode_queries, 4);
    assert_eq!(sink.selections, 0);
    assert!(sink.unverified.is_empty());
    assert_eq!(
        sink.abandoned,
        vec![SteeringUserMessageAbandonReason::Cancelled]
    );
}

#[test]
fn passive_local_image_validates_detail_and_discards_path() {
    let base = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1");
    let input = base.replace(
        r#"{"type":"text","text":"steered text","text_elements":[]}"#,
        r#"{"type":"localImage","path":"C:\\images\\one.png","detail":"original"}"#,
    );
    let mut sink = PassiveSink::new(true);
    decode_provider_json_for_test(input.as_bytes(), 1, &mut sink).unwrap();
    assert_eq!(sink.unverified.len(), 1);
    let mut sink = PassiveSink::new(true);
    assert!(
        decode_provider_json_for_test(
            input.replace("original", "invalid").as_bytes(),
            2,
            &mut sink
        )
        .is_err()
    );
    assert!(sink.unverified.is_empty());
}

#[test]
fn count_disagreement_rechecks_failure_after_the_initial_mode_read() {
    let mut sink = PassiveSink::new(false);
    sink.fail_after_query = Some(3);
    let input = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1").replace(
        r#"[{"type":"text","text":"steered text","text_elements":[]}]"#,
        "[]",
    );
    decode_provider_json_for_test(input.as_bytes(), 3, &mut sink).unwrap();
    assert_eq!(sink.selections, 1);
    assert_eq!(sink.reads.load(Ordering::SeqCst), 0);
    assert!(sink.source_dropped.load(Ordering::SeqCst));
    assert_eq!(sink.unverified.len(), 1);
}

#[test]
fn variant_disagreement_rechecks_failure_after_replay_selects_the_descriptor() {
    let mut sink = PassiveSink::new(false);
    sink.fail_on_descriptor = true;
    let input = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1").replace(
        r#"{"type":"text","text":"steered text","text_elements":[]}"#,
        r#"{"type":"localImage","path":"C:\\one.png","detail":null}"#,
    );
    decode_provider_json_for_test(input.as_bytes(), 3, &mut sink).unwrap();
    assert_eq!(sink.selections, 1);
    assert_eq!(sink.reads.load(Ordering::SeqCst), 0);
    assert!(sink.source_dropped.load(Ordering::SeqCst));
    assert_eq!(sink.unverified.len(), 1);
}

#[test]
fn passive_transport_loss_abandons_without_metadata_completion() {
    let mut sink = PassiveSink::new(true);
    let prefix = r#"{"method":"item/started","params":{"item":{"type":"userMessage","id":"item-steer","clientId":"correlation-1","content":[{"type":"text","text":"unfinished"#;
    assert!(decode_provider_transport_loss_for_test(prefix.as_bytes(), 3, &mut sink).is_err());
    assert_eq!(
        sink.abandoned,
        vec![SteeringUserMessageAbandonReason::TransportLost]
    );
    assert!(sink.unverified.is_empty());
}

#[test]
fn healthy_wrong_turn_preserves_the_exact_error_variant() {
    let trace = Arc::new(Mutex::new(SinkTrace::default()));
    let mut sink = SteeringSink::new(trace).with_expected_turn("other-turn");
    let input = lifecycle_json("item/started", TEXT, "turn-steer", "correlation-1");
    let error = decode_provider_json_for_test(input.as_bytes(), 3, &mut sink).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::SteeringUserMessage {
            source: beryl_backend::SteeringUserMessageError::TurnMismatch,
            ..
        }
    ));
}

#[test]
fn live_request_keeps_exact_response_and_later_passive_echoes_keep_streaming() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["method"], "turn/steer");
        assert_eq!(request["id"], 2);
        send_json(socket, r#"{"id":2,"result":{"turnId":"turn-steer"}}"#);
        for method in ["item/started", "item/completed"] {
            send_json(
                socket,
                &lifecycle_json(method, "unverified text", "turn-steer", "correlation-1"),
            );
        }
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
    let sink = PassiveSink::new(true);
    let sealed = sink.sealed.clone();
    session
        .bind_ordered_turn_stream_sink(Box::new(sink))
        .unwrap();
    let outcome = session.steer_turn_with_streamed_input(
        &CasThreadId::new("thread-steer").unwrap(),
        &CasTurnId::new("turn-steer").unwrap(),
        &ClientUserMessageId::try_new("correlation-1").unwrap(),
        Box::new(TextReplaySource::new()),
        TIMEOUT,
    );
    let NonIdempotentRequestOutcome::ExactResponse { response } = outcome else {
        panic!("passive receive must not change the matching response outcome")
    };
    assert_eq!(response.turn_id().as_str(), "turn-steer");
    for count in 1..=2 {
        assert_eq!(
            session.poll_ordered_turn_stream_progress(TIMEOUT).unwrap(),
            OrderedTurnStreamProgress::Progress
        );
        assert_eq!(sealed.load(Ordering::SeqCst), count);
    }
    session.shutdown().unwrap();
    server.join().unwrap();
}
