use std::{thread, time::Instant};

use beryl_app::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    OrdinaryDynamicToolContext, OrdinaryDynamicToolHandler, OrdinaryTurnExecutionRequest,
    ProjectionCancellationToken,
};
use beryl_backend::{
    DynamicToolCallRequest, DynamicToolCallResponse, ThreadStartOptions, TurnStartOptions,
};
use beryl_home_store::CursorReadLimits;
use beryl_model::{CasProcessGeneration, SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    CanonicalItemKind, SourceEventRecord, SyndicTimestamp, TurnItemIndexRecord, TurnLifecycle,
};

use crate::{
    backend::TIMEOUT,
    syndic::{Fixture, execution_binding, point_limit},
};

pub fn process(value: u64) -> CasProcessGeneration {
    CasProcessGeneration::new(value).unwrap()
}

pub fn projection_request(fixture: &Fixture, thread: SyndicThreadId) -> CasProjectionRequest {
    CasProjectionRequest::new(
        thread,
        fixture.selected_path(thread),
        execution_binding(),
        ThreadStartOptions::persistent(),
        Some(2_000_000),
        SyndicTimestamp::from_unix_millis(10_000),
        TIMEOUT,
    )
}

pub fn obtain(
    fixture: &Fixture,
    coordinator: &CasProjectionCoordinator,
    session: &mut AdmittedProjectionSession,
    thread: SyndicThreadId,
) -> LoadedCasProjection {
    coordinator
        .obtain_projection(
            &fixture.store,
            fixture.storage,
            session,
            &projection_request(fixture, thread),
            &ProjectionCancellationToken::new(),
        )
        .unwrap()
}

pub fn execution_request() -> OrdinaryTurnExecutionRequest {
    OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT)
}

#[derive(Default)]
pub struct NoTools;

impl OrdinaryDynamicToolHandler for NoTools {
    fn respond(
        &mut self,
        _context: OrdinaryDynamicToolContext,
        request: &DynamicToolCallRequest,
    ) -> DynamicToolCallResponse {
        panic!("unexpected dynamic-tool request: {}", request.summary())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct RecordedToolCall {
    pub thread: SyndicThreadId,
    pub turn: SyndicTurnId,
    pub cas_thread: String,
    pub cas_turn: String,
    pub call: String,
    pub namespace: Option<String>,
    pub tool: String,
}

#[derive(Default)]
pub struct RecordingTools {
    pub calls: Vec<RecordedToolCall>,
}

impl OrdinaryDynamicToolHandler for RecordingTools {
    fn respond(
        &mut self,
        context: OrdinaryDynamicToolContext,
        request: &DynamicToolCallRequest,
    ) -> DynamicToolCallResponse {
        self.calls.push(RecordedToolCall {
            thread: context.thread_id(),
            turn: context.turn_id(),
            cas_thread: request.thread_id().to_owned(),
            cas_turn: request.turn_id().to_owned(),
            call: request.call_id().to_owned(),
            namespace: request.namespace().map(str::to_owned),
            tool: request.tool().to_owned(),
        });
        DynamicToolCallResponse::success_text("phase13 tool response")
    }
}

pub fn wait_for_lifecycle(fixture: &Fixture, turn: SyndicTurnId, expected: TurnLifecycle) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let lifecycle = fixture
            .storage
            .turn_state(&fixture.store, turn, point_limit())
            .unwrap()
            .unwrap()
            .record()
            .lifecycle();
        if lifecycle == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for turn {turn} to become {expected:?}; current={lifecycle:?}"
        );
        thread::sleep(std::time::Duration::from_millis(5));
    }
}

pub fn source_events(fixture: &Fixture, turn: SyndicTurnId) -> Vec<SourceEventRecord> {
    fixture
        .storage
        .source_events(
            &fixture.store,
            turn,
            None,
            CursorReadLimits::new(64, 4_000_000).unwrap(),
        )
        .unwrap()
        .records()
        .to_vec()
}

pub fn turn_items(fixture: &Fixture, turn: SyndicTurnId) -> Vec<TurnItemIndexRecord> {
    fixture
        .storage
        .turn_items(
            &fixture.store,
            turn,
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap()
        .records()
        .to_vec()
}

pub fn item_by_kind(
    fixture: &Fixture,
    turn: SyndicTurnId,
    expected_kind: CanonicalItemKind,
) -> SyndicItemId {
    turn_items(fixture, turn)
        .into_iter()
        .find_map(|index| {
            let item = fixture
                .storage
                .canonical_item(&fixture.store, index.item_id(), point_limit())
                .unwrap()
                .unwrap();
            (item.record().kind() == expected_kind).then_some(index.item_id())
        })
        .unwrap_or_else(|| panic!("turn {turn} lacks canonical item kind {expected_kind:?}"))
}

pub fn item_text(fixture: &Fixture, item_id: SyndicItemId) -> String {
    let item = fixture
        .storage
        .canonical_item(&fixture.store, item_id, point_limit())
        .unwrap()
        .unwrap();
    let content = item
        .record()
        .payload()
        .content()
        .expect("text fixture item must own content")
        .id();
    let mut bytes = Vec::new();
    let mut after = None;
    loop {
        let page = fixture
            .storage
            .content_chunks(
                &fixture.store,
                content,
                after,
                CursorReadLimits::new(64, 4_000_000).unwrap(),
            )
            .unwrap();
        for chunk in page.records() {
            bytes.extend_from_slice(chunk.bytes());
            after = Some(chunk.ordinal());
        }
        if !page.has_more() {
            break;
        }
    }
    String::from_utf8(bytes).unwrap()
}
