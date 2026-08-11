#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CommandError, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    CasItemId, SyndicDraftId, SyndicItemId, SyndicProjectionId, SyndicResourceId, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::{
    AdvanceItemProjectionBuild, AdvanceTranscriptBuild, CasTurnSource, ComposerAtom,
    ComposerPayload, CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    FinalizeNextTurnItem, FreezeNextTurnItem, IdleSubmission, ItemProjectionGeneration,
    MARKDOWN_CODE_INLINE_MAX_BYTES, MARKDOWN_SPAN_MAX_BYTES, PreparedContent, ProjectionLifecycle,
    ProviderFrameOrdinalV1, ProviderItemFrameV1, ProviderItemObservationV1, ProviderItemV1,
    ProviderLifecycleTimestampMsV1, ProviderSubmittedContentV1, ProviderUserMessageV1,
    SourceEventPayload, StartItemProjectionBuild, StartTranscriptBuild, SyndicMutationError,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, TranscriptBuildPhase,
    TranscriptBuildRecord, TranscriptGeneration, TranscriptPosition, TurnDepth, TurnEndStatus,
    TurnItemOrdinal,
};

use support::{
    TestHome, converge_and_release_terminal_history, draft_id,
    exact_cas::{admit_event, admit_item_frame, correlate_user_item, establish_turn},
    id, open, stage_prepared_content, timestamp,
};

const PAGE_BYTES: usize = 4_096;

#[derive(Clone, Copy)]
struct SubmittedTurn {
    turn: SyndicTurnId,
    item: SyndicItemId,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean transcript-construction fixture command, got {outcome:?}"),
    }
}

fn typed_error(error: &CommandError) -> &SyndicMutationError {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected Syndic validation rejection, got {error}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

fn create_thread(store: &HomeStore, storage: SyndicStorage) -> SyndicThreadId {
    let thread = id(1);
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft_id(2),
                support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    );
    thread
}

#[allow(clippy::too_many_arguments)]
fn submit_text(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    text: &str,
    next_draft: SyndicDraftId,
    item: SyndicItemId,
    updated_at: SyndicTimestamp,
    admitted_at: SyndicTimestamp,
) -> SubmittedTurn {
    let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(store, storage, &content);

    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, updated_at).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => panic!("fixture draft must change"),
    };
    execute(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    );

    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let submission = IdleSubmission::new(
        thread,
        thread_record.revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        next_draft,
        item,
        None,
        admitted_at,
    );
    let turn = submission.submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(storage.revision(store).unwrap(), submission),
    );
    SubmittedTurn { turn, item }
}

fn admit(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: &CasTurnSource,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) {
    admit_event(store, storage, thread, turn, source, payload, observed_at);
}

fn project_item(
    store: &HomeStore,
    storage: SyndicStorage,
    item: SyndicItemId,
) -> ItemProjectionGeneration {
    let canonical = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = storage
        .item_projection_head(store, item, point_limit())
        .unwrap()
        .map_or(ItemProjectionGeneration::FIRST, |head| {
            head.generation().checked_next().unwrap()
        });
    execute(
        store,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item, canonical.revision(), generation),
        ),
    );
    for _ in 0..1_024 {
        if storage
            .item_projection_set(store, item, generation, point_limit())
            .unwrap()
            .is_some()
        {
            return generation;
        }
        let build = storage
            .item_projection_build(store, item, generation, point_limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage.advance_item_projection_build(
                storage.revision(store).unwrap(),
                AdvanceItemProjectionBuild::new(item, generation, build.revision()),
            ),
        );
    }
    panic!("bounded item projection did not finish");
}

fn complete_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    submitted: SubmittedTurn,
    activated_at: SyndicTimestamp,
    completed_at: SyndicTimestamp,
    finalized_at: SyndicTimestamp,
) {
    let source = establish_turn(store, storage, thread, submitted.turn, activated_at);
    admit(
        store,
        storage,
        thread,
        submitted.turn,
        &source,
        SourceEventPayload::TurnActivated,
        activated_at,
    );
    correlate_user_item(
        store,
        storage,
        thread,
        submitted.turn,
        submitted.item,
        &source,
        activated_at,
    );
    admit(
        store,
        storage,
        thread,
        submitted.turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        completed_at,
    );
    let state = storage
        .turn_state(store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.freeze_next_turn_item(
            storage.revision(store).unwrap(),
            FreezeNextTurnItem::new(
                thread,
                submitted.turn,
                state.revision(),
                TurnItemOrdinal::FIRST,
                submitted.item,
                finalized_at,
            ),
        ),
    );
    project_item(store, storage, submitted.item);
    let state = storage
        .turn_state(store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.finalize_next_turn_item(
            storage.revision(store).unwrap(),
            FinalizeNextTurnItem::new(
                thread,
                submitted.turn,
                state.revision(),
                TurnItemOrdinal::FIRST,
                submitted.item,
                finalized_at,
            ),
        ),
    );
}

fn item_projection_ids(
    store: &HomeStore,
    storage: SyndicStorage,
    item: SyndicItemId,
) -> Vec<SyndicProjectionId> {
    let head = storage
        .item_projection_head(store, item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Current);
    let mut ids = Vec::new();
    let mut after = None;
    loop {
        let page = storage
            .item_projections(
                store,
                item,
                head.generation(),
                after,
                CursorReadLimits::new(11, PAGE_BYTES).unwrap(),
            )
            .unwrap();
        assert!(page.records().len() <= 11);
        assert!(page.stored_bytes() <= PAGE_BYTES);
        for index in page.records() {
            ids.push(index.projection_id());
            after = Some(index.ordinal());
        }
        if !page.has_more() {
            return ids;
        }
        assert!(!page.records().is_empty());
    }
}

fn projection_resource_ids(
    store: &HomeStore,
    storage: SyndicStorage,
    projections: &[SyndicProjectionId],
) -> Vec<SyndicResourceId> {
    projections
        .iter()
        .filter_map(|projection| {
            storage
                .projection(store, *projection, point_limit())
                .unwrap()
                .unwrap()
                .payload()
                .resource_id()
        })
        .collect()
}

fn start_transcript_build(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> (TranscriptGeneration, TranscriptBuildRecord) {
    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Stale);
    let generation = head.generation();
    execute(
        store,
        storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(thread, thread_record.revision(), head.revision()),
        ),
    );
    let build = storage
        .transcript_build(store, thread, generation, point_limit())
        .unwrap()
        .unwrap()
        .to_owned();
    (generation, build)
}

fn advance_transcript_build(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    generation: TranscriptGeneration,
) -> TranscriptBuildRecord {
    let current = storage
        .transcript_build(store, thread, generation, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.advance_transcript_build(
            storage.revision(store).unwrap(),
            AdvanceTranscriptBuild::new(thread, generation, current.revision()),
        ),
    );
    storage
        .transcript_build(store, thread, generation, point_limit())
        .unwrap()
        .unwrap()
}

fn finish_transcript_build(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    generation: TranscriptGeneration,
) -> TranscriptBuildRecord {
    for _ in 0..1_024 {
        let build = storage
            .transcript_build(store, thread, generation, point_limit())
            .unwrap()
            .unwrap();
        if build.phase() == TranscriptBuildPhase::Complete {
            return build;
        }
        advance_transcript_build(store, storage, thread, generation);
    }
    panic!("bounded transcript construction did not finish");
}

fn path_turns(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    generation: TranscriptGeneration,
) -> Vec<syndic_storage::TranscriptPathTurnRecord> {
    let mut records = Vec::new();
    let mut after = None;
    loop {
        let page = storage
            .transcript_path_turns(
                store,
                thread,
                generation,
                after,
                CursorReadLimits::new(1, PAGE_BYTES).unwrap(),
            )
            .unwrap();
        assert!(page.records().len() <= 1);
        assert!(page.stored_bytes() <= PAGE_BYTES);
        for record in page.records() {
            records.push(*record);
            after = Some(record.depth());
        }
        if !page.has_more() {
            return records;
        }
        assert_eq!(page.records().len(), 1);
    }
}

fn transcript_entries(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    generation: TranscriptGeneration,
) -> Vec<syndic_storage::TranscriptViewEntryRecord> {
    let mut records = Vec::new();
    let mut after = None;
    loop {
        let page = storage
            .transcript_entries(
                store,
                thread,
                generation,
                after,
                CursorReadLimits::new(7, PAGE_BYTES).unwrap(),
            )
            .unwrap();
        assert!(page.records().len() <= 7);
        assert!(page.stored_bytes() <= PAGE_BYTES);
        for record in page.records() {
            after = Some(record.position());
            records.push(record.clone());
        }
        if !page.has_more() {
            return records;
        }
        assert!(!page.records().is_empty());
    }
}

fn assert_unpublished_head(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    generation: TranscriptGeneration,
) {
    let head = storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.generation(), generation);
    assert_eq!(head.entry_count(), 0);
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Stale);
}

#[path = "phase7_transcript_construction/multi_batch.rs"]
mod multi_batch;
#[path = "phase7_transcript_construction/pending_tail.rs"]
mod pending_tail;
#[path = "phase7_transcript_construction/superseded_build.rs"]
mod superseded_build;
