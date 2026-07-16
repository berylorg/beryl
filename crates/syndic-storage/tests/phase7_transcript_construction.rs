#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CommandError, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    CasItemId, SyndicDraftId, SyndicItemId, SyndicProjectionId, SyndicResourceId, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::{
    AdmissionMarkers, AdvanceItemProjectionBuild, AdvanceTranscriptBuild, CasTurnSource,
    ComposerAtom, ComposerPayload, CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    FinalizeNextTurnItem, IdleSubmission, ItemProjectionGeneration, MARKDOWN_CODE_INLINE_MAX_BYTES,
    MARKDOWN_SPAN_MAX_BYTES, PreparedContent, ProjectionLifecycle, ProviderItemKind,
    SourceEventPayload, SourceItemDescriptor, StartItemProjectionBuild, StartTranscriptBuild,
    SyndicMutationError, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    TranscriptBuildPhase, TranscriptBuildRecord, TranscriptGeneration, TranscriptPosition,
    TurnDepth, TurnEndStatus, TurnItemOrdinal,
};

use support::{
    TestHome, draft_id,
    exact_cas::{admit_event, correlate_user_item, establish_turn},
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
    store.execute(command).unwrap();
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
            CreateThread::ordinary(thread, draft_id(2), timestamp(1)),
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
        thread_record.record().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.record().revision(),
        next_draft,
        item,
        AdmissionMarkers::default(),
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
            head.record().generation().checked_next().unwrap()
        });
    execute(
        store,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item, canonical.record().revision(), generation),
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
                AdvanceItemProjectionBuild::new(item, generation, build.record().revision()),
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
                state.record().revision(),
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
    assert_eq!(head.record().lifecycle(), ProjectionLifecycle::Current);
    let mut ids = Vec::new();
    let mut after = None;
    loop {
        let page = storage
            .item_projections(
                store,
                item,
                head.record().generation(),
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
                .record()
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
    assert_eq!(head.record().lifecycle(), ProjectionLifecycle::Stale);
    let generation = head.record().generation();
    execute(
        store,
        storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(
                thread,
                thread_record.record().revision(),
                head.record().revision(),
            ),
        ),
    );
    let build = storage
        .transcript_build(store, thread, generation, point_limit())
        .unwrap()
        .unwrap()
        .record()
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
            AdvanceTranscriptBuild::new(thread, generation, current.record().revision()),
        ),
    );
    *storage
        .transcript_build(store, thread, generation, point_limit())
        .unwrap()
        .unwrap()
        .record()
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
        if build.record().phase() == TranscriptBuildPhase::Complete {
            return *build.record();
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
    assert_eq!(head.record().generation(), generation);
    assert_eq!(head.record().entry_count(), 0);
    assert_eq!(head.record().lifecycle(), ProjectionLifecycle::Stale);
}

#[test]
fn multi_batch_publication_resumes_and_orders_root_to_tail() {
    let home = TestHome::new("phase7-transcript-multi-batch");
    let mut store = open(home.path());
    let mut storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(&store, storage);

    let root = submit_text(
        &store,
        storage,
        thread,
        "root",
        draft_id(3),
        SyndicItemId::from_bytes([20; 16]),
        timestamp(2),
        timestamp(3),
    );
    complete_turn(
        &store,
        storage,
        thread,
        root,
        timestamp(4),
        timestamp(5),
        timestamp(6),
    );
    let middle = submit_text(
        &store,
        storage,
        thread,
        "middle",
        draft_id(4),
        SyndicItemId::from_bytes([21; 16]),
        timestamp(7),
        timestamp(8),
    );
    complete_turn(
        &store,
        storage,
        thread,
        middle,
        timestamp(9),
        timestamp(10),
        timestamp(11),
    );
    let large_tail = "x".repeat(MARKDOWN_SPAN_MAX_BYTES * 65);
    let tail = submit_text(
        &store,
        storage,
        thread,
        &large_tail,
        draft_id(5),
        SyndicItemId::from_bytes([22; 16]),
        timestamp(12),
        timestamp(13),
    );
    complete_turn(
        &store,
        storage,
        thread,
        tail,
        timestamp(14),
        timestamp(15),
        timestamp(16),
    );

    let root_projections = item_projection_ids(&store, storage, root.item);
    let middle_projections = item_projection_ids(&store, storage, middle.item);
    let tail_projections = item_projection_ids(&store, storage, tail.item);
    assert_eq!(root_projections.len(), 1);
    assert_eq!(middle_projections.len(), 1);
    assert_eq!(tail_projections.len(), 65);

    let (generation, started) = start_transcript_build(&store, storage, thread);
    assert_eq!(
        started.phase(),
        TranscriptBuildPhase::Collecting {
            next_turn: Some(tail.turn),
        }
    );
    let interrupted_collecting = advance_transcript_build(&store, storage, thread, generation);
    assert_eq!(interrupted_collecting.path_turn_count(), 1);
    assert_eq!(
        interrupted_collecting.phase(),
        TranscriptBuildPhase::Collecting {
            next_turn: Some(middle.turn),
        }
    );

    store.close().unwrap();
    store = open(home.path());
    storage = SyndicStorage::register(&mut store).unwrap();
    store.validate_registered_domains().unwrap();
    assert_eq!(
        *storage
            .transcript_build(&store, thread, generation, point_limit())
            .unwrap()
            .unwrap()
            .record(),
        interrupted_collecting
    );

    let collecting_middle = advance_transcript_build(&store, storage, thread, generation);
    assert_eq!(collecting_middle.path_turn_count(), 2);
    assert_eq!(
        collecting_middle.phase(),
        TranscriptBuildPhase::Collecting {
            next_turn: Some(root.turn),
        }
    );
    let ready_to_publish = advance_transcript_build(&store, storage, thread, generation);
    assert_eq!(ready_to_publish.path_turn_count(), 3);
    assert_eq!(
        ready_to_publish.phase(),
        TranscriptBuildPhase::Publishing {
            next_depth: TurnDepth::FIRST,
            next_item: TurnItemOrdinal::FIRST,
            next_projection: syndic_storage::ProjectionOrdinal::FIRST,
        }
    );

    let path = path_turns(&store, storage, thread, generation);
    assert_eq!(path.len(), 3);
    assert_eq!(
        path.iter()
            .map(|record| (record.depth().get(), record.turn_id()))
            .collect::<Vec<_>>(),
        [(1, root.turn), (2, middle.turn), (3, tail.turn)]
    );
    assert_unpublished_head(&store, storage, thread, generation);

    let after_root = advance_transcript_build(&store, storage, thread, generation);
    assert_eq!(after_root.entry_count(), 1);
    let after_middle = advance_transcript_build(&store, storage, thread, generation);
    assert_eq!(after_middle.entry_count(), 2);
    let interrupted_publishing = advance_transcript_build(&store, storage, thread, generation);
    assert_eq!(interrupted_publishing.entry_count(), 66);
    assert_eq!(
        interrupted_publishing.phase(),
        TranscriptBuildPhase::Publishing {
            next_depth: TurnDepth::new(3).unwrap(),
            next_item: TurnItemOrdinal::FIRST,
            next_projection: syndic_storage::ProjectionOrdinal::new(65).unwrap(),
        }
    );
    assert_unpublished_head(&store, storage, thread, generation);

    store.close().unwrap();
    store = open(home.path());
    storage = SyndicStorage::register(&mut store).unwrap();
    store.validate_registered_domains().unwrap();
    assert_eq!(
        *storage
            .transcript_build(&store, thread, generation, point_limit())
            .unwrap()
            .unwrap()
            .record(),
        interrupted_publishing
    );

    let completed = advance_transcript_build(&store, storage, thread, generation);
    assert_eq!(completed.phase(), TranscriptBuildPhase::Complete);
    assert_eq!(completed.entry_count(), 67);
    let head = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.record().generation(), generation);
    assert_eq!(head.record().entry_count(), 67);
    assert_eq!(head.record().lifecycle(), ProjectionLifecycle::Current);
    assert!(
        storage
            .history_summary(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .record()
            .complete()
    );

    let mut expected = Vec::new();
    expected.extend(
        root_projections
            .iter()
            .copied()
            .map(|projection| (root.item, projection)),
    );
    expected.extend(
        middle_projections
            .iter()
            .copied()
            .map(|projection| (middle.item, projection)),
    );
    expected.extend(
        tail_projections
            .iter()
            .copied()
            .map(|projection| (tail.item, projection)),
    );
    let entries = transcript_entries(&store, storage, thread, generation);
    assert_eq!(entries.len(), expected.len());
    for (index, (entry, (item, projection))) in entries.iter().zip(expected.into_iter()).enumerate()
    {
        assert_eq!(
            entry.position(),
            TranscriptPosition::new(index as u64 + 1).unwrap()
        );
        assert_eq!(entry.item_id(), item);
        assert_eq!(entry.projection_id(), projection);
    }

    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn selected_tail_advance_supersedes_a_collecting_build() {
    let home = TestHome::new("phase7-transcript-superseded");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(&store, storage);
    let root = submit_text(
        &store,
        storage,
        thread,
        "root before supersession",
        draft_id(3),
        SyndicItemId::from_bytes([30; 16]),
        timestamp(2),
        timestamp(3),
    );
    complete_turn(
        &store,
        storage,
        thread,
        root,
        timestamp(4),
        timestamp(5),
        timestamp(6),
    );

    let (old_generation, collecting) = start_transcript_build(&store, storage, thread);
    assert!(matches!(
        collecting.phase(),
        TranscriptBuildPhase::Collecting { .. }
    ));
    assert!(path_turns(&store, storage, thread, old_generation).is_empty());

    let new_tail = submit_text(
        &store,
        storage,
        thread,
        "new selected tail",
        draft_id(4),
        SyndicItemId::from_bytes([31; 16]),
        timestamp(7),
        timestamp(8),
    );
    let superseded = storage
        .transcript_build(&store, thread, old_generation, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        superseded.record().phase(),
        TranscriptBuildPhase::Superseded
    );
    assert_eq!(superseded.record().path_turn_count(), 0);
    assert_eq!(superseded.record().entry_count(), 0);

    let head = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        head.record().generation(),
        old_generation.checked_next().unwrap()
    );
    assert_eq!(head.record().committed_tail(), Some(new_tail.turn));
    assert_eq!(head.record().entry_count(), 0);
    assert_eq!(head.record().lifecycle(), ProjectionLifecycle::Stale);
    assert!(transcript_entries(&store, storage, thread, old_generation).is_empty());

    store.validate_registered_domains().unwrap();
    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    assert_eq!(
        storage
            .transcript_build(&reopened, thread, old_generation, point_limit())
            .unwrap()
            .unwrap()
            .record()
            .phase(),
        TranscriptBuildPhase::Superseded
    );
    reopened.close().unwrap();
}

#[test]
fn pending_tail_stays_out_of_public_entries_until_its_frontier_is_finalized() {
    let home = TestHome::new("phase7-transcript-complete-tail-gate");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(&store, storage);
    let authored = format!(
        "```text\n{}\n```\n",
        "x".repeat(MARKDOWN_CODE_INLINE_MAX_BYTES + 1)
    );
    let pending = submit_text(
        &store,
        storage,
        thread,
        &authored,
        draft_id(3),
        SyndicItemId::from_bytes([40; 16]),
        timestamp(2),
        timestamp(3),
    );
    let initial_item_generation = project_item(&store, storage, pending.item);
    let initial_item_set = storage
        .item_projection_set(&store, pending.item, initial_item_generation, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone();
    let initial_projection_ids = item_projection_ids(&store, storage, pending.item);
    let initial_resource_ids = projection_resource_ids(&store, storage, &initial_projection_ids);
    assert_eq!(initial_item_set.resource_count(), 1);
    assert_eq!(initial_resource_ids.len(), 1);

    let (pending_generation, _) = start_transcript_build(&store, storage, thread);
    let pending_build = finish_transcript_build(&store, storage, thread, pending_generation);
    assert_eq!(pending_build.entry_count(), 0);
    assert!(!pending_build.history_complete());
    let pending_path = path_turns(&store, storage, thread, pending_generation);
    assert_eq!(pending_path.len(), 1);
    assert_eq!(pending_path[0].finalized_item_count(), 0);
    assert!(transcript_entries(&store, storage, thread, pending_generation).is_empty());
    let pending_head = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(pending_head.record().entry_count(), 0);
    assert_eq!(
        pending_head.record().lifecycle(),
        ProjectionLifecycle::Current
    );
    assert!(
        !storage
            .history_summary(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .record()
            .complete()
    );

    let source = establish_turn(&store, storage, thread, pending.turn, timestamp(4));
    admit(
        &store,
        storage,
        thread,
        pending.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    let local_item = storage
        .canonical_item(&store, pending.item, point_limit())
        .unwrap()
        .unwrap();
    let descriptor = SourceItemDescriptor::new(
        pending.item,
        CasItemId::new("phase13-correlated-user").unwrap(),
        ProviderItemKind::UserMessage,
        local_item.record().disposition(),
    )
    .unwrap();
    admit(
        &store,
        storage,
        thread,
        pending.turn,
        &source,
        SourceEventPayload::ItemStarted {
            item: descriptor.clone(),
            assistant_phase: None,
        },
        timestamp(4),
    );
    let started_item = storage
        .canonical_item(&store, pending.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        started_item.record().revision(),
        initial_item_set
            .source_item_revision()
            .checked_next()
            .unwrap()
    );
    let stale_item_head = storage
        .item_projection_head(&store, pending.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        stale_item_head.record().lifecycle(),
        ProjectionLifecycle::Stale
    );
    assert_eq!(
        stale_item_head.record().generation(),
        initial_item_generation
    );
    let stale_transcript_head = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        stale_transcript_head.record().lifecycle(),
        ProjectionLifecycle::Stale
    );

    let started_item_generation = project_item(&store, storage, pending.item);
    let started_item_set = storage
        .item_projection_set(&store, pending.item, started_item_generation, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone();
    assert_ne!(started_item_generation, initial_item_generation);
    assert_eq!(
        started_item_set.source_content(),
        initial_item_set.source_content()
    );
    assert_eq!(
        started_item_set.stable_digest(),
        initial_item_set.stable_digest()
    );
    assert_eq!(started_item_set.digest(), initial_item_set.digest());
    assert_eq!(
        started_item_set.resume_checkpoint(),
        initial_item_set.resume_checkpoint()
    );
    assert_eq!(
        item_projection_ids(&store, storage, pending.item),
        initial_projection_ids
    );
    assert_eq!(
        projection_resource_ids(&store, storage, &initial_projection_ids),
        initial_resource_ids
    );

    let (started_transcript_generation, _) = start_transcript_build(&store, storage, thread);
    assert_ne!(started_transcript_generation, pending_generation);
    let started_transcript =
        finish_transcript_build(&store, storage, thread, started_transcript_generation);
    assert_eq!(started_transcript.entry_count(), 0);
    admit(
        &store,
        storage,
        thread,
        pending.turn,
        &source,
        SourceEventPayload::ItemCompleted {
            item: descriptor,
            assistant_phase: None,
        },
        timestamp(4),
    );
    let completed_item = storage
        .canonical_item(&store, pending.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        completed_item.record().revision(),
        started_item.record().revision().checked_next().unwrap()
    );
    let stale_item_head = storage
        .item_projection_head(&store, pending.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        stale_item_head.record().lifecycle(),
        ProjectionLifecycle::Stale
    );
    assert_eq!(
        stale_item_head.record().generation(),
        started_item_generation
    );
    let stale_transcript_head = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        stale_transcript_head.record().lifecycle(),
        ProjectionLifecycle::Stale
    );
    assert_ne!(
        stale_transcript_head.record().generation(),
        started_transcript_generation
    );
    admit(
        &store,
        storage,
        thread,
        pending.turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(5),
    );
    let state = storage
        .turn_state(&store, pending.turn, point_limit())
        .unwrap()
        .unwrap();
    let mut rejected = HomeCommand::new(store.home_revision().unwrap());
    rejected
        .add(storage.finalize_next_turn_item(
            storage.revision(&store).unwrap(),
            FinalizeNextTurnItem::new(
                thread,
                pending.turn,
                state.record().revision(),
                TurnItemOrdinal::FIRST,
                pending.item,
                timestamp(6),
            ),
        ))
        .unwrap();
    let error = store.execute(rejected).unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::CanonicalFinalizationConflict
    ));

    let completed_item_generation = project_item(&store, storage, pending.item);
    let completed_item_set = storage
        .item_projection_set(
            &store,
            pending.item,
            completed_item_generation,
            point_limit(),
        )
        .unwrap()
        .unwrap()
        .record()
        .clone();
    assert_ne!(completed_item_generation, started_item_generation);
    assert_eq!(
        completed_item_set.source_item_revision(),
        completed_item.record().revision()
    );
    assert_eq!(
        completed_item_set.source_content(),
        initial_item_set.source_content()
    );
    assert_eq!(
        completed_item_set.stable_digest(),
        initial_item_set.stable_digest()
    );
    assert_eq!(completed_item_set.digest(), initial_item_set.digest());
    assert_eq!(
        completed_item_set.resume_checkpoint(),
        initial_item_set.resume_checkpoint()
    );
    assert_eq!(
        item_projection_ids(&store, storage, pending.item),
        initial_projection_ids
    );
    assert_eq!(
        projection_resource_ids(&store, storage, &initial_projection_ids),
        initial_resource_ids
    );

    let state = storage
        .turn_state(&store, pending.turn, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.finalize_next_turn_item(
            storage.revision(&store).unwrap(),
            FinalizeNextTurnItem::new(
                thread,
                pending.turn,
                state.record().revision(),
                TurnItemOrdinal::FIRST,
                pending.item,
                timestamp(6),
            ),
        ),
    );

    let (final_generation, _) = start_transcript_build(&store, storage, thread);
    assert_ne!(final_generation, pending_generation);
    let completed = finish_transcript_build(&store, storage, thread, final_generation);
    assert_eq!(completed.entry_count(), 1);
    assert!(completed.history_complete());
    let entries = transcript_entries(&store, storage, thread, final_generation);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].position(), TranscriptPosition::FIRST);
    assert_eq!(entries[0].item_id(), pending.item);
    assert!(
        storage
            .history_summary(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .record()
            .complete()
    );

    store.validate_registered_domains().unwrap();
    store.close().unwrap();
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    let reopened_item = reopened_storage
        .canonical_item(&reopened, pending.item, point_limit())
        .unwrap()
        .unwrap();
    let reopened_head = reopened_storage
        .item_projection_head(&reopened, pending.item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        reopened_head.record().lifecycle(),
        ProjectionLifecycle::Current
    );
    assert_eq!(
        reopened_head.record().source_item_revision(),
        reopened_item.record().revision()
    );
    reopened.close().unwrap();
}
