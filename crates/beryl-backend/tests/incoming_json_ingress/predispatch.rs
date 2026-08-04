use std::{
    net::{TcpListener, TcpStream},
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use beryl_backend::{
    BackendWebSocketEndpoint, ForegroundSessionConfig, ManagedBackendClientConnector,
    ManagedBackendError, ManagedBackendSession, NonIdempotentRequestOutcome,
    StreamedInputDescriptor, StreamedInputHeader, StreamedInputSequenceDigest, StreamedInputSource,
    StreamedInputSourceError, StreamedInputSourceIdentity, StreamedInputSourceRevision,
    StreamedTextPage, StreamedTextSourceId, ThreadInjectionOutcome, ThreadInjectionPreflight,
    ThreadInjectionSource, ThreadInjectionSourceError, ThreadInjectionSourceIdentity,
    ThreadInjectionSourcePage, ThreadInjectionSourceRevision,
    lifecycle_test_support::fresh_idle_thread,
};
use beryl_model::{CasThreadId, RecoveryItemSequenceDigest};
use tungstenite::{Message, WebSocket, accept_hdr};

#[derive(Clone, Default)]
struct SourceCalls(Arc<AtomicUsize>);

impl SourceCalls {
    fn note(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

struct CountingStreamedSource {
    calls: SourceCalls,
}

impl StreamedInputSource for CountingStreamedSource {
    fn header(&self) -> StreamedInputHeader {
        self.calls.note();
        streamed_header()
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        self.calls.note();
        Ok(streamed_header())
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

fn streamed_header() -> StreamedInputHeader {
    StreamedInputHeader::new(
        StreamedInputSourceIdentity::new([1; 32]),
        StreamedInputSourceRevision::new(1),
        0,
        StreamedInputSequenceDigest::new([2; 32]),
    )
}

struct CountingInjectionSource {
    calls: SourceCalls,
}

impl ThreadInjectionSource for CountingInjectionSource {
    fn next_page(
        &mut self,
        _max_utf8_bytes: usize,
    ) -> Result<Option<ThreadInjectionSourcePage>, ThreadInjectionSourceError> {
        self.calls.note();
        Err(ThreadInjectionSourceError::ReadFailed)
    }
}

#[test]
fn stdio_streamed_turn_is_rejected_before_source_or_session_state() {
    let mut session = ManagedBackendSession::stdio_streamed_input_gate_for_lifecycle_test()
        .expect("detached lifecycle session");
    session.poison_streamed_user_message_verifier_for_lifecycle_test();
    let before = session.predispatch_state_for_lifecycle_test();
    let calls = SourceCalls::default();
    let source = CountingStreamedSource {
        calls: calls.clone(),
    };
    let thread_id = CasThreadId::new("thread_streamed_predispatch").unwrap();

    let outcome = session.start_turn_with_streamed_input(
        &thread_id,
        Box::new(source),
        Duration::from_millis(10),
    );
    let NonIdempotentRequestOutcome::ProvenNotDispatched { error } = outcome else {
        panic!("unsupported stdio turn/start must be proven not dispatched");
    };
    assert!(matches!(
        *error,
        ManagedBackendError::StreamedInputTransportUnsupported {
            ref method,
            transport: "stdio",
        } if method == "turn/start"
    ));
    assert_eq!(calls.count(), 0);
    assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
}

#[test]
fn unsupported_stdio_injection_consumes_target_without_reading_source_or_mutating_session_state() {
    let mut session = ManagedBackendSession::stdio_streamed_input_gate_for_lifecycle_test()
        .expect("detached lifecycle session");
    let before = session.predispatch_state_for_lifecycle_test();
    let source_identity = ThreadInjectionSourceIdentity::new([3; 32]);
    let source_revision = ThreadInjectionSourceRevision::new(4);
    let preflight = ThreadInjectionPreflight::new(
        source_identity,
        source_revision,
        1,
        1,
        RecoveryItemSequenceDigest::from_bytes([5; 32]),
    )
    .unwrap();
    let calls = SourceCalls::default();
    let mut source = CountingInjectionSource {
        calls: calls.clone(),
    };
    let thread_id = CasThreadId::new("thread_injection_predispatch").unwrap();
    let target = fresh_idle_thread(thread_id.clone());

    let outcome =
        session.inject_thread_items(target, &preflight, &mut source, Duration::from_millis(10));
    let ThreadInjectionOutcome::ProvenNotDispatched {
        thread_id: actual,
        error,
    } = outcome
    else {
        panic!("unsupported stdio injection must be proven not dispatched");
    };
    assert_eq!(actual, thread_id);
    assert!(matches!(
        *error,
        ManagedBackendError::ThreadInjectionTransportUnsupported {
            ref method,
            transport: "stdio",
        } if method == "thread/inject_items"
    ));
    assert_eq!(calls.count(), 0);
    assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
}

#[test]
fn foreground_initialize_predispatch_write_failure_writes_no_json_and_preserves_exact_state() {
    let (endpoint, server) = spawn_no_json_server();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(), Duration::from_secs(2))
        .unwrap();
    let before = session.predispatch_state_for_lifecycle_test();
    session.fail_next_write_before_dispatch_for_lifecycle_test();

    let error = session
        .initialize_foreground(Duration::from_millis(10))
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::WriteRequest { method, .. }
            if method == "initialize"
    ));
    assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
    assert!(!session.has_full_turn_stream());
    assert!(!session.transport_is_closed_for_lifecycle_test());
    drop(session);
    server.join().unwrap();
}

const AUTHORIZATION: &str = "Bearer phase-28-test-token";

fn foreground_config() -> ForegroundSessionConfig {
    ForegroundSessionConfig::new(nonzero_usize(16))
}

fn nonzero_usize(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

fn spawn_no_json_server() -> (BackendWebSocketEndpoint, thread::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let endpoint = BackendWebSocketEndpoint::loopback(listener.local_addr().unwrap().port());
    let server = thread::spawn(move || {
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
        socket
            .get_mut()
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        assert_no_json_frame(&mut socket);
    });
    (endpoint, server)
}

fn assert_no_json_frame(socket: &mut WebSocket<TcpStream>) {
    match socket.read() {
        Ok(Message::Text(text)) => panic!("unexpected outbound JSON text: {text}"),
        Ok(Message::Binary(bytes)) => panic!("unexpected outbound binary frame: {bytes:?}"),
        Ok(Message::Close(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => {}
        Err(tungstenite::Error::Io(source))
            if matches!(
                source.kind(),
                std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::UnexpectedEof
            ) => {}
        Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {}
        Err(error) => panic!("unexpected test-server WebSocket failure: {error}"),
    }
}
