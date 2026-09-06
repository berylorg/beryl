use std::{
    collections::VecDeque,
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use beryl_backend::{
    BackendWebSocketEndpoint, ManagedBackendSession, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamSink, OrderedTurnStreamSubmitError,
    ThreadInjectionPreflight, ThreadInjectionRole, ThreadInjectionSource,
    ThreadInjectionSourceError, ThreadInjectionSourceIdentity, ThreadInjectionSourcePage,
    ThreadInjectionSourceRevision,
};
use beryl_model::{
    RecoveryItemSequenceAccumulator, RecoveryItemSequenceDigest, RecoveryItemSequenceRole,
};
use serde_json::Value;
use tungstenite::WebSocket;

use super::recovery_page::{lease_filled, lease_with_bytes};
use super::support::{
    TIMEOUT, assert_initialize, assert_initialized, connector, foreground_config, read_json,
    send_initialize_response,
};

pub const USER_TEXT: &str = "restored user text";
pub const ASSISTANT_TEXT: &str = "restored assistant text";

pub struct InjectionFixture {
    pub preflight: ThreadInjectionPreflight,
    pub source: ScriptedInjectionSource,
}

pub struct ScriptedInjectionSource {
    events: VecDeque<Result<Option<ThreadInjectionSourcePage>, ThreadInjectionSourceError>>,
    calls: Arc<AtomicUsize>,
}

impl ScriptedInjectionSource {
    #[must_use]
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ThreadInjectionSource for ScriptedInjectionSource {
    fn next_page(
        &mut self,
        max_utf8_bytes: usize,
    ) -> Result<Option<ThreadInjectionSourcePage>, ThreadInjectionSourceError> {
        assert!(max_utf8_bytes > 0);
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.events
            .pop_front()
            .expect("scripted injection source was read past its terminal event")
    }
}

pub fn one_item_fixture(seed: u8, role: ThreadInjectionRole, text: &str) -> InjectionFixture {
    let identity = ThreadInjectionSourceIdentity::new([seed; 32]);
    let revision = ThreadInjectionSourceRevision::new(u64::from(seed));
    let text_bytes = u64::try_from(text.len()).unwrap();
    let sequence_role = match role {
        ThreadInjectionRole::UserInputText => RecoveryItemSequenceRole::UserInputText,
        ThreadInjectionRole::AssistantOutputText => RecoveryItemSequenceRole::AssistantOutputText,
    };
    let mut digest = RecoveryItemSequenceAccumulator::new(1, text_bytes);
    digest.begin_item(1, sequence_role, text_bytes).unwrap();
    digest.update_text(text.as_bytes()).unwrap();
    digest.finish_item().unwrap();
    let digest = digest.finish().unwrap();
    let preflight =
        ThreadInjectionPreflight::new(identity, revision, 1, text_bytes, digest).unwrap();
    let page = ThreadInjectionSourcePage::new(
        identity,
        revision,
        1,
        role,
        text_bytes,
        0,
        lease_with_bytes(text.as_bytes()),
        true,
        true,
    )
    .unwrap();
    InjectionFixture {
        preflight,
        source: ScriptedInjectionSource {
            events: VecDeque::from([Ok(Some(page)), Ok(None)]),
            calls: Arc::new(AtomicUsize::new(0)),
        },
    }
}

pub fn immediate_source_failure(seed: u8) -> InjectionFixture {
    let mut fixture = one_item_fixture(seed, ThreadInjectionRole::UserInputText, "x");
    fixture.source.events = VecDeque::from([Err(ThreadInjectionSourceError::ReadFailed)]);
    fixture
}

pub fn source_failure_after_full_page(seed: u8) -> InjectionFixture {
    let identity = ThreadInjectionSourceIdentity::new([seed; 32]);
    let revision = ThreadInjectionSourceRevision::new(u64::from(seed));
    let declared = 65_537;
    let preflight = ThreadInjectionPreflight::new(
        identity,
        revision,
        1,
        declared,
        RecoveryItemSequenceDigest::from_bytes([seed.wrapping_add(1); 32]),
    )
    .unwrap();
    let page = ThreadInjectionSourcePage::new(
        identity,
        revision,
        1,
        ThreadInjectionRole::UserInputText,
        declared,
        0,
        lease_filled(b'x', 65_536),
        false,
        false,
    )
    .unwrap();
    InjectionFixture {
        preflight,
        source: ScriptedInjectionSource {
            events: VecDeque::from([Ok(Some(page)), Err(ThreadInjectionSourceError::ReadFailed)]),
            calls: Arc::new(AtomicUsize::new(0)),
        },
    }
}

pub fn connect_initialized_foreground(endpoint: BackendWebSocketEndpoint) -> ManagedBackendSession {
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    session
        .bind_ordered_turn_stream_sink(Box::new(UnexpectedOrderedSink))
        .unwrap();
    session
}

pub fn initialize_server(socket: &mut WebSocket<TcpStream>) {
    assert_initialize(&read_json(socket).unwrap(), false);
    send_initialize_response(socket, 1);
    assert_initialized(&read_json(socket).unwrap());
}

pub fn assert_injection_request(
    request: &Value,
    id: u64,
    thread_id: &str,
    role: ThreadInjectionRole,
    text: &str,
) {
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["id"], id);
    assert_eq!(request["method"], "thread/inject_items");
    assert_eq!(request["params"]["threadId"], thread_id);
    let item = &request["params"]["items"][0];
    assert_eq!(item["type"], "message");
    let (wire_role, content_type) = match role {
        ThreadInjectionRole::UserInputText => ("user", "input_text"),
        ThreadInjectionRole::AssistantOutputText => ("assistant", "output_text"),
    };
    assert_eq!(item["role"], wire_role);
    assert_eq!(item["content"][0]["type"], content_type);
    assert_eq!(item["content"][0]["text"], text);
    assert_eq!(request["params"]["items"].as_array().unwrap().len(), 1);
}

struct UnexpectedOrderedSink;

impl OrderedTurnStreamSink for UnexpectedOrderedSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        panic!("unexpected ordered operation during injection: {operation:?}")
    }
}
