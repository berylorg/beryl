use std::path::Path;

use beryl_app::cas_projection::{
    AdmittedProjectionSession, CasProjectionCoordinator, CasProjectionRequest, LoadedCasProjection,
    ProjectionExecutionError,
};
use beryl_backend::{ManagedBackendClientConnector, ThreadStartOptions};
use beryl_home_store::CursorReadLimits;
use beryl_model::{CasProcessGeneration, SyndicDraftId, SyndicThreadId};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use syndic_storage::*;

use crate::{EXECUTION_ROOT, command, server::WireServer, syndic::Fixture};

pub fn point() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub fn create_discussion(fixture: &mut Fixture, passage: &str) -> SyndicThreadId {
    let child_seed = fixture.thread.as_bytes()[0] ^ 0x20;
    create_from_source(fixture, passage, passage, 0, child_seed)
}

pub fn create_discussion_from_completed(
    fixture: &mut Fixture,
    submitted: crate::syndic::SubmittedTurn,
    passage: &str,
    child_seed: u8,
) -> SyndicThreadId {
    create_from_completed(fixture, submitted, passage, 0, child_seed)
}

pub fn create_code_discussion(fixture: &mut Fixture, passage: &str) -> SyndicThreadId {
    let child_seed = fixture.thread.as_bytes()[0] ^ 0x20;
    create_from_source(
        fixture,
        &format!("```\n{passage}\n```"),
        passage,
        4,
        child_seed,
    )
}

fn create_from_source(
    fixture: &mut Fixture,
    source_text: &str,
    passage: &str,
    start: u64,
    child_seed: u8,
) -> SyndicThreadId {
    let submitted = fixture.submit_text("source question");
    fixture.complete_with_assistant(submitted, source_text);
    create_from_completed(fixture, submitted, passage, start, child_seed)
}

fn create_from_completed(
    fixture: &mut Fixture,
    submitted: crate::syndic::SubmittedTurn,
    passage: &str,
    start: u64,
    child_seed: u8,
) -> SyndicThreadId {
    let home = fixture.home();
    let parent = fixture
        .storage
        .thread(&home, fixture.thread, point())
        .unwrap()
        .unwrap();
    let head = fixture
        .storage
        .transcript_view_head(&home, fixture.thread, point())
        .unwrap()
        .unwrap();
    let entries = fixture
        .storage
        .transcript_entries(
            &home,
            fixture.thread,
            head.generation(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| {
            fixture
                .storage
                .canonical_item(&home, entry.item_id(), point())
                .unwrap()
                .is_some_and(|item| {
                    item.turn_id() == submitted.turn
                        && item.provider_kind() == ProviderItemKind::AgentMessage
                })
        })
        .unwrap();
    let source = fixture
        .storage
        .prepare_discussion_source(
            &home,
            DiscussionContextSource::new(
                fixture.thread,
                submitted.turn,
                entry.item_id(),
                entry.projection_id(),
                entry.projection_revision(),
                DiscussionContextRange::new(start, start + passage.len() as u64).unwrap(),
            ),
            parent.selected_path(),
            CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
            DiscussionContextText::new(passage).unwrap(),
        )
        .unwrap();
    let tail = fixture
        .storage
        .thread_tail(&home, fixture.thread, point())
        .unwrap()
        .unwrap();
    let child = SyndicThreadId::from_bytes([child_seed; 16]);
    let prepared = fixture
        .storage
        .prepare_discussion_creation(
            &home,
            source,
            CreateDiscussion::new(
                child,
                SyndicDraftId::from_bytes([child_seed.wrapping_add(1); 16]),
                tail.last_activity_at(),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        )
        .unwrap();
    command::execute_fixture_contribution(&home, prepared.contribution());
    drop(home);
    fixture.submit_text_on(child, " unchanged local input");
    child
}

pub fn source_thread(fixture: &Fixture, child: SyndicThreadId) -> String {
    fixture
        .native_source(child)
        .binding()
        .cas_thread_id()
        .as_str()
        .to_owned()
}

pub fn frame(fixture: &Fixture, child: SyndicThreadId) -> String {
    let home = fixture.home();
    let thread = fixture
        .storage
        .thread(&home, child, point())
        .unwrap()
        .unwrap();
    let record = fixture
        .storage
        .context_envelope(&home, thread.context_owner_id().unwrap(), point())
        .unwrap()
        .unwrap();
    let descriptor = record.envelope().descriptor();
    let source = descriptor.source();
    let hex = |value: &[u8]| {
        value
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let text = record.envelope().text().as_str();
    let hash: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    assert_eq!(descriptor.digest().as_bytes(), &hash);
    format!(
        "[Beryl discussion context v1]\nsource_thread={}\nsource_turn={}\nsource_item={}\nsource_projection={}\nprojection_revision={:016x}\nrange_start={:016x}\nrange_end={:016x}\nselected_utf8_bytes={:016x}\nselected_sha256={}\n\n{}\n[/Beryl discussion context v1]",
        hex(source.thread_id().as_bytes()),
        hex(source.turn_id().as_bytes()),
        hex(source.item_id().as_bytes()),
        hex(source.projection_id().as_bytes()),
        source.projection_revision().get(),
        source.range().start(),
        source.range().end(),
        text.len(),
        hex(&hash),
        text
    )
}

pub fn assistant(text: &str) -> Value {
    json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]})
}

pub fn user(text: &str) -> Value {
    json!({"type":"message","role":"user","content":[{"type":"input_text","text":text}]})
}

pub fn admit(fixture: &Fixture, server: &WireServer, process: u64) -> AdmittedProjectionSession {
    fixture
        .store
        .admit_runtime_lifecycle_test_candidate(
            &ManagedBackendClientConnector::for_lifecycle_test(
                server.endpoint(),
                crate::wire::AUTHORIZATION,
            ),
            crate::syndic::execution_binding(),
            CasProcessGeneration::new(process).unwrap(),
            Path::new(EXECUTION_ROOT),
            crate::wire::TIMEOUT,
        )
        .unwrap()
}

pub fn obtain(
    fixture: &Fixture,
    session: &mut AdmittedProjectionSession,
    thread: SyndicThreadId,
    model_window: u64,
) -> Result<LoadedCasProjection, ProjectionExecutionError> {
    let home = fixture.home();
    CasProjectionCoordinator::for_healthy_home(&home)
        .unwrap()
        .obtain_projection(
            &home,
            &fixture.storage,
            session,
            &request(fixture, thread, model_window),
            &fixture.cancellation,
        )
}

pub fn request(
    fixture: &Fixture,
    thread: SyndicThreadId,
    model_window: u64,
) -> CasProjectionRequest {
    CasProjectionRequest::new(
        thread,
        fixture.selected_path(thread),
        crate::syndic::execution_binding(),
        ThreadStartOptions::persistent(),
        Some(model_window),
        SyndicTimestamp::from_unix_millis(90_000),
        crate::wire::TIMEOUT,
    )
}

pub fn close(session: AdmittedProjectionSession, server: WireServer) {
    session.invalidate_connection();
    drop(session);
    server.join();
}

pub fn execute_same_discussion_continuation(
    fixture: &mut Fixture,
    session: &mut AdmittedProjectionSession,
    projection: Box<LoadedCasProjection>,
) -> Box<LoadedCasProjection> {
    use beryl_app::cas_projection::{
        OrdinaryDynamicToolHandlers, OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest,
    };
    use beryl_backend::TurnStartOptions;

    let thread = projection.syndic_thread_id();
    let original = projection.lineage_proof();
    let CasLineageProof::ContextAware(original_context) = original else {
        panic!("continuation requires the original established context")
    };
    let target = projection.cas_thread_id().clone();
    let prior_projection = projection;
    let submitted = fixture.submit_text_on(thread, " second unchanged discussion input");
    let projection = obtain(fixture, session, thread, 1_000_000).unwrap();
    assert_eq!(projection.cas_thread_id(), &target);
    assert_eq!(projection.lineage_proof(), original);
    let home = fixture.home();
    let outcome = CasProjectionCoordinator::for_healthy_home(&home)
        .unwrap()
        .execute_ordinary_turn(
            &home,
            &fixture.storage,
            &fixture.state.assets(),
            None,
            projection,
            &fixture.cancellation,
            &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), crate::wire::TIMEOUT),
            OrdinaryDynamicToolHandlers::new(&mut crate::UnusedLifecycle, &mut crate::UnusedBranch),
        )
        .unwrap();
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = outcome else {
        panic!("same-discussion continuation failed: {outcome:?}")
    };
    assert_eq!(status, TurnEndStatus::complete());
    let CasLineageProof::ContextAware(continued) = projection.lineage_proof() else {
        panic!("same-discussion continuation lost context")
    };
    assert_eq!(continued.injection(), original_context.injection());
    assert_eq!(continued.summary(), original_context.summary());
    assert_eq!(
        continued.projection_digest(),
        original_context.projection_digest()
    );
    assert_eq!(
        continued.loaded_generation(),
        original_context.loaded_generation()
    );
    assert_eq!(
        continued.ordering_floor(),
        original_context.ordering_floor()
    );
    let state = fixture
        .storage
        .turn_state(&home, submitted.turn, point())
        .unwrap()
        .unwrap();
    let TurnDispatchProvenance::Activated(anchor) = state.dispatch_provenance() else {
        panic!("continued input must retain its own exact activation")
    };
    let snapshot = fixture
        .storage
        .execution_snapshot(&home, anchor.snapshot_id(), point())
        .unwrap()
        .unwrap();
    assert!(snapshot.started_at() >= original_context.ordering_floor());
    let completed_at = fixture
        .storage
        .thread_tail(&home, thread, point())
        .unwrap()
        .unwrap()
        .last_activity_at();
    drop(home);
    fixture.advance_clock_to(completed_at.unix_millis() + 1);
    drop(prior_projection);
    projection
}
