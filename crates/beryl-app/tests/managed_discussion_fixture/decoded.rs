use super::*;
use beryl_backend::{
    BackendWebSocketEndpoint, DynamicToolArgumentScalarKind, DynamicToolCall,
    DynamicToolCallResponse, ForegroundSessionConfig, ManagedBackendClientConnector,
    ManagedBackendSession, NonIdempotentRequestOutcome, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamSink, OrderedTurnStreamSubmitError, ProviderField,
    ProviderValueContext, StreamedInputDescriptor, StreamedInputDescriptorKind,
    StreamedInputHeader, StreamedInputSequenceDigestAccumulator, StreamedInputSource,
    StreamedInputSourceError, StreamedInputSourceIdentity, StreamedInputSourceRevision,
    StreamedTextDescriptor, StreamedTextPage, StreamedTextSourceId, TextSourceProof,
    UserMessageEchoLifecycle,
};
use beryl_stream::PagePool;
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

const TIMEOUT: Duration = Duration::from_secs(5);
const QUESTION: &str = "Exact source question café 🌸";
const ANSWER: &str = "Exact assistant café 🌸 passage";
const RESOLUTION: &str = "Exact resolution café 🌸";

struct TextSource {
    text: &'static str,
    emitted: bool,
}

impl TextSource {
    fn new(text: &'static str) -> Box<Self> {
        Box::new(Self {
            text,
            emitted: false,
        })
    }
}

impl StreamedInputSource for TextSource {
    fn header(&self) -> StreamedInputHeader {
        let mut digest = StreamedInputSequenceDigestAccumulator::new(1);
        digest
            .push_text(1, TextSourceProof::new([3; 32]), self.text.len() as u64)
            .unwrap();
        StreamedInputHeader::new(
            StreamedInputSourceIdentity::new([1; 32]),
            StreamedInputSourceRevision::new(1),
            1,
            digest.finish().unwrap(),
        )
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        self.emitted = false;
        Ok(self.header())
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        if std::mem::replace(&mut self.emitted, true) {
            return Ok(None);
        }
        Ok(Some(StreamedInputDescriptor::new(
            StreamedInputSourceIdentity::new([1; 32]),
            StreamedInputSourceRevision::new(1),
            1,
            StreamedInputDescriptorKind::Text(StreamedTextDescriptor::new(
                StreamedTextSourceId::new([2; 32]),
                TextSourceProof::new([3; 32]),
                self.text.len() as u64,
            )),
        )))
    }

    fn read_text_page(
        &mut self,
        source: StreamedTextSourceId,
        start: u64,
        max: usize,
    ) -> Result<StreamedTextPage, StreamedInputSourceError> {
        if source != StreamedTextSourceId::new([2; 32]) || start != 0 || max < self.text.len() {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        Ok(StreamedTextPage::new(
            StreamedInputSourceIdentity::new([1; 32]),
            StreamedInputSourceRevision::new(1),
            source,
            TextSourceProof::new([3; 32]),
            0,
            self.text,
            None,
        ))
    }
}

#[derive(Default)]
struct Trace {
    checked: Vec<(UserMessageEchoLifecycle, String, String, u64)>,
    call: Option<DynamicToolCall>,
    arguments: Vec<u8>,
    dynamic_seals: usize,
    assistant: Vec<u8>,
    provider_seals: usize,
    provider_routes: Vec<(String, String)>,
    terminal: Option<(String, String)>,
}

struct DecoderSink {
    trace: Arc<Mutex<Trace>>,
    pool: PagePool,
}

impl OrderedTurnStreamSink for DecoderSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        match operation {
            OrderedTurnStreamOperation::CheckedUserMessage(message) => {
                self.trace.lock().unwrap().checked.push((
                    message.lifecycle(),
                    message.thread_id().as_str().to_owned(),
                    message.turn_id().as_str().to_owned(),
                    message.correlation().checked_input_items(),
                ));
            }
            OrderedTurnStreamOperation::TurnStarted(_) => {}
            OrderedTurnStreamOperation::DynamicBegin(call) => {
                assert!(self.trace.lock().unwrap().call.replace(call).is_none());
            }
            OrderedTurnStreamOperation::DynamicArgumentControl(_) => {}
            OrderedTurnStreamOperation::DynamicAcquirePage
            | OrderedTurnStreamOperation::ProviderAcquirePage => {
                return Ok(OrderedTurnStreamCompletion::PageLease(
                    self.pool.try_lease().unwrap(),
                ));
            }
            OrderedTurnStreamOperation::DynamicArgumentFragment(fragment) => {
                if fragment.kind() == DynamicToolArgumentScalarKind::String {
                    self.trace
                        .lock()
                        .unwrap()
                        .arguments
                        .extend_from_slice(fragment.bytes());
                }
                let mut lease = fragment.into_lease();
                lease.clear();
                return Ok(OrderedTurnStreamCompletion::PageLease(lease));
            }
            OrderedTurnStreamOperation::DynamicSeal => {
                self.trace.lock().unwrap().dynamic_seals += 1
            }
            OrderedTurnStreamOperation::ProviderBegin(_)
            | OrderedTurnStreamOperation::ProviderControl(_) => {}
            OrderedTurnStreamOperation::ProviderFragment(fragment) => {
                if fragment.context()
                    == ProviderValueContext::Field(ProviderField::AgentMessageText)
                {
                    self.trace
                        .lock()
                        .unwrap()
                        .assistant
                        .extend_from_slice(fragment.bytes());
                }
                let mut lease = fragment.into_lease();
                lease.clear();
                return Ok(OrderedTurnStreamCompletion::PageLease(lease));
            }
            OrderedTurnStreamOperation::ProviderSeal(route) => {
                let mut trace = self.trace.lock().unwrap();
                trace.provider_seals += 1;
                trace.provider_routes.push((
                    route.thread_id().as_str().to_owned(),
                    route.turn_id().as_str().to_owned(),
                ));
            }
            OrderedTurnStreamOperation::NormalTurnTerminal(terminal) => {
                assert_eq!(
                    terminal.status(),
                    beryl_backend::NormalTurnTerminalStatus::Completed
                );
                self.trace.lock().unwrap().terminal = Some((
                    terminal.thread_id().as_str().to_owned(),
                    terminal.turn_id().as_str().to_owned(),
                ));
            }
            unexpected => panic!("unexpected fixture decoder operation: {unexpected:?}"),
        }
        Ok(OrderedTurnStreamCompletion::Applied)
    }
}

fn poll_until(session: &mut ManagedBackendSession, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "managed fixture decoder progress timed out"
        );
        session
            .poll_ordered_turn_stream_progress(Duration::from_millis(20))
            .unwrap();
    }
}

#[test]
fn controlled_discussion_fixture_passes_real_streamed_echo_tool_and_terminal_decoders() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let (mut child, address) = start_fixture(root);
    let mut warmup = connect(address, root, false);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        BackendWebSocketEndpoint::loopback(address.port()),
        "Bearer discussion-fixture-token",
    );
    let mut session = connector
        .connect_foreground_candidate(
            ForegroundSessionConfig::new(NonZeroUsize::new(16).unwrap()),
            TIMEOUT,
        )
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    assert!(session.has_full_turn_stream());
    session.read_config(root, TIMEOUT).unwrap();
    let trace = Arc::new(Mutex::new(Trace::default()));
    let pool = PagePool::new(
        NonZeroUsize::new(4096).unwrap(),
        NonZeroUsize::new(1).unwrap(),
    )
    .unwrap();
    session
        .bind_ordered_turn_stream_sink(Box::new(DecoderSink {
            trace: Arc::clone(&trace),
            pool: pool.clone(),
        }))
        .unwrap();
    let loaded = session.start_thread(root, TIMEOUT).unwrap().into_loaded();
    let thread = loaded.thread_id();
    let started =
        session.start_turn_with_streamed_input(thread, TextSource::new(QUESTION), TIMEOUT);
    let NonIdempotentRequestOutcome::ExactResponse { response } = started else {
        panic!("fixture start did not pass the real streamed echo verifier: {started:?}");
    };
    let turn = response.turn_id().as_str();
    assert_eq!(response.thread_id(), thread);
    {
        let trace = trace.lock().unwrap();
        assert_eq!(trace.checked.len(), 2);
        assert_eq!(
            trace.checked[0],
            (
                UserMessageEchoLifecycle::Started,
                thread.as_str().to_owned(),
                turn.to_owned(),
                1
            )
        );
        assert_eq!(
            trace.checked[1],
            (
                UserMessageEchoLifecycle::Completed,
                thread.as_str().to_owned(),
                turn.to_owned(),
                1
            )
        );
    }
    publish(
        root,
        &format!("discussion-resolve-{turn}.json"),
        &serde_json::to_vec(&json!({"resolution":RESOLUTION})).unwrap(),
    );
    poll_until(&mut session, || trace.lock().unwrap().dynamic_seals == 1);
    let call = trace.lock().unwrap().call.take().unwrap();
    assert!(call.is_sealed());
    assert_eq!(call.namespace(), Some("beryl"));
    assert_eq!(call.tool().as_str(), "resolve_branch_discussion");
    assert_eq!(call.thread_id(), thread);
    assert_eq!(call.turn_id(), response.turn_id());
    assert_eq!(trace.lock().unwrap().arguments, RESOLUTION.as_bytes());
    session
        .respond_dynamic_tool_call(
            &call,
            &DynamicToolCallResponse::success_text("decoder accepted"),
        )
        .unwrap();
    let receipt = root.join(format!("discussion-response-{turn}.json"));
    let deadline = Instant::now() + TIMEOUT;
    while !receipt.exists() {
        assert!(
            Instant::now() < deadline,
            "fixture did not receive exact tool response"
        );
        thread::sleep(Duration::from_millis(2));
    }
    let receipt: Value = serde_json::from_slice(&fs::read(receipt).unwrap()).unwrap();
    assert_eq!(receipt["result"]["success"], true);
    publish(
        root,
        &format!("discussion-assistant-{turn}.json"),
        &serde_json::to_vec(&json!({"text":ANSWER})).unwrap(),
    );
    publish(root, &format!("discussion-complete-{turn}"), b"");
    poll_until(&mut session, || trace.lock().unwrap().terminal.is_some());
    {
        let trace = trace.lock().unwrap();
        assert_eq!(
            trace.terminal,
            Some((thread.as_str().to_owned(), turn.to_owned()))
        );
        assert_eq!(trace.provider_seals, 2);
        assert_eq!(
            trace.provider_routes,
            vec![(thread.as_str().to_owned(), turn.to_owned()); 2]
        );
        assert_eq!(trace.assistant, ANSWER.repeat(2).as_bytes());
    }
    assert_eq!(pool.diagnostics().leased, 0);
    publish(root, "discussion-reject-generated-once", b"");
    let rejected = session.start_turn_with_streamed_input(
        thread,
        TextSource::new("Discussion resolution:\n\nexact rejected input"),
        TIMEOUT,
    );
    let NonIdempotentRequestOutcome::ExactRejection { error } = rejected else {
        panic!("fixture rejection was not decoded exactly: {rejected:?}");
    };
    assert_eq!(error.code(), -32602);
    assert_eq!(trace.lock().unwrap().checked.len(), 2);
    assert_eq!(pool.diagnostics().leased, 0);
    drop(call);
    session.unsubscribe_thread(thread, TIMEOUT).unwrap();
    session.shutdown().unwrap();
    warmup.close(None).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while child.0.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "fixture did not join decoder connections"
        );
        thread::sleep(Duration::from_millis(2));
    }
    assert!(child.0.try_wait().unwrap().unwrap().success());
}
