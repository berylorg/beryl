#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{CasItemId, SyndicDraftId, SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::*;

use support::{
    TestHome, draft_id,
    exact_cas::{admit_event, correlate_user_item, establish_turn},
    id, open, stage_prepared_content, timestamp,
};

const PAGE_BYTES: usize = 4_096;

#[derive(Clone, Copy)]
struct TerminalTurn {
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    user_item: SyndicItemId,
    assistant_item: SyndicItemId,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

#[allow(clippy::too_many_arguments)]
fn submit_text(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    text: &str,
    next_draft: SyndicDraftId,
    user_item: SyndicItemId,
    updated_at: SyndicTimestamp,
    admitted_at: SyndicTimestamp,
) -> SyndicTurnId {
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
        user_item,
        AdmissionMarkers::default(),
        admitted_at,
    );
    let turn = submission.submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(storage.revision(store).unwrap(), submission),
    );
    turn
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

fn project_item(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId) {
    let canonical = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
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
            return;
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

fn finalize_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    ordinal: TurnItemOrdinal,
    item: SyndicItemId,
    updated_at: SyndicTimestamp,
) {
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.finalize_next_turn_item(
            storage.revision(store).unwrap(),
            FinalizeNextTurnItem::new(
                thread,
                turn,
                state.record().revision(),
                ordinal,
                item,
                updated_at,
            ),
        ),
    );
}

fn freeze_assistant(
    store: &HomeStore,
    storage: SyndicStorage,
    fixture: TerminalTurn,
    updated_at: SyndicTimestamp,
) {
    let state = storage
        .turn_state(store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.freeze_next_turn_item(
            storage.revision(store).unwrap(),
            FreezeNextTurnItem::new(
                fixture.thread,
                fixture.turn,
                state.record().revision(),
                TurnItemOrdinal::new(2).unwrap(),
                fixture.assistant_item,
                updated_at,
            ),
        ),
    );
}

fn publish_transcript(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> TranscriptViewHeadRecord {
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

    for _ in 0..1_024 {
        let build = storage
            .transcript_build(store, thread, generation, point_limit())
            .unwrap()
            .unwrap();
        if build.record().phase() == TranscriptBuildPhase::Complete {
            return storage
                .transcript_view_head(store, thread, point_limit())
                .unwrap()
                .unwrap()
                .record()
                .clone();
        }
        execute(
            store,
            storage.advance_transcript_build(
                storage.revision(store).unwrap(),
                AdvanceTranscriptBuild::new(thread, generation, build.record().revision()),
            ),
        );
    }
    panic!("bounded transcript construction did not finish");
}

fn transcript_entry(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    generation: TranscriptGeneration,
    item: SyndicItemId,
) -> TranscriptViewEntryRecord {
    let page = storage
        .transcript_entries(
            store,
            thread,
            generation,
            None,
            CursorReadLimits::new(16, PAGE_BYTES).unwrap(),
        )
        .unwrap();
    assert!(!page.has_more());
    page.records()
        .iter()
        .find(|entry| entry.item_id() == item)
        .cloned()
        .expect("selected user item must have a transcript entry")
}

fn seed_terminal_turn_with_open_assistant(
    store: &HomeStore,
    storage: SyndicStorage,
) -> TerminalTurn {
    let thread = id(1);
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(thread, draft_id(2), timestamp(1)),
        ),
    );
    let user_item = SyndicItemId::from_bytes([4; 16]);
    let turn = submit_text(
        store,
        storage,
        thread,
        "original question",
        draft_id(3),
        user_item,
        timestamp(2),
        timestamp(3),
    );
    let assistant_item = SyndicItemId::from_bytes([5; 16]);
    let cas_assistant = CasItemId::new("phase7-selected-path-assistant").unwrap();
    let descriptor = SourceItemDescriptor::new(
        assistant_item,
        cas_assistant.clone(),
        ProviderItemKind::AgentMessage,
        ProviderItemDisposition::CanonicalText,
    )
    .unwrap();
    let source = establish_turn(store, storage, thread, turn, timestamp(4));
    admit(
        store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    admit(
        store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemStarted {
            item: descriptor,
            assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
        },
        timestamp(5),
    );
    admit(
        store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemDelta {
            item_id: assistant_item,
            cas_item_id: cas_assistant,
            expected_kind: ProviderItemKind::AgentMessage,
            text: SourceEventText::new("retained answer").unwrap(),
        },
        timestamp(6),
    );
    admit(
        store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Complete,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(7),
    );

    let fixture = TerminalTurn {
        thread,
        turn,
        user_item,
        assistant_item,
    };
    project_item(store, storage, user_item);
    finalize_item(
        store,
        storage,
        thread,
        turn,
        TurnItemOrdinal::FIRST,
        user_item,
        timestamp(8),
    );
    let head = publish_transcript(store, storage, thread);
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(head.entry_count(), 1);
    fixture
}

fn replace_with_completed_root(
    store: &HomeStore,
    storage: SyndicStorage,
    old: TerminalTurn,
) -> (SyndicTurnId, SyndicItemId) {
    let thread = storage
        .thread(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let draft = storage
        .current_draft(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let entry = transcript_entry(
        store,
        storage,
        old.thread,
        head.record().generation(),
        old.user_item,
    );
    let selected_path = SelectedPathProof::new(
        thread.record().committed_tail(),
        thread.record().revision(),
        thread.record().selected_path_digest(),
    );
    execute(
        store,
        storage.start_replacement_edit(
            storage.revision(store).unwrap(),
            StartReplacementEdit::new(
                old.thread,
                thread.record().revision(),
                draft.draft().id(),
                draft.draft().revision(),
                gate.record().revision(),
                old.turn,
                old.user_item,
                selected_path,
                CurrentTranscriptEntryProof::new(head.record().generation(), entry.position()),
                AdmissionMarkers::default(),
                timestamp(9),
            ),
        ),
    );

    let editing = storage
        .current_draft(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let thread = storage
        .thread(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, old.thread, point_limit())
        .unwrap()
        .unwrap();
    let replacement_item = SyndicItemId::from_bytes([7; 16]);
    let submission = IdleSubmission::new(
        old.thread,
        thread.record().revision(),
        editing.draft().id(),
        editing.draft().revision(),
        editing.draft().content(),
        gate.record().revision(),
        draft_id(6),
        replacement_item,
        AdmissionMarkers::default(),
        timestamp(10),
    );
    let replacement_turn = submission.submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(storage.revision(store).unwrap(), submission),
    );

    let source = establish_turn(store, storage, old.thread, replacement_turn, timestamp(11));
    admit(
        store,
        storage,
        old.thread,
        replacement_turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(11),
    );
    correlate_user_item(
        store,
        storage,
        old.thread,
        replacement_turn,
        replacement_item,
        &source,
        timestamp(11),
    );
    admit(
        store,
        storage,
        old.thread,
        replacement_turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(12),
    );
    project_item(store, storage, replacement_item);
    finalize_item(
        store,
        storage,
        old.thread,
        replacement_turn,
        TurnItemOrdinal::FIRST,
        replacement_item,
        timestamp(13),
    );
    let head = publish_transcript(store, storage, old.thread);
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(head.committed_tail(), Some(replacement_turn));
    assert_eq!(head.entry_count(), 1);

    let replacement = storage
        .turn(store, replacement_turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(replacement.record().parent(), ConversationParent::Root);
    (replacement_turn, replacement_item)
}

fn head(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> TranscriptViewHeadRecord {
    storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone()
}

fn summary(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> HistorySummaryRecord {
    storage
        .history_summary(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone()
}

fn assistant_content_lifecycle(
    store: &HomeStore,
    storage: SyndicStorage,
    fixture: TerminalTurn,
) -> ContentLifecycle {
    let item = storage
        .canonical_item(store, fixture.assistant_item, point_limit())
        .unwrap()
        .unwrap();
    storage
        .content_manifest(
            store,
            item.record()
                .payload()
                .content()
                .expect("assistant item has canonical content")
                .id(),
            point_limit(),
        )
        .unwrap()
        .unwrap()
        .record()
        .lifecycle()
}

#[test]
fn off_path_finalization_preserves_selected_transcript_and_history_summary() {
    let home = TestHome::new("phase7-off-path-finalization");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let old = seed_terminal_turn_with_open_assistant(&store, storage);
    assert_eq!(
        assistant_content_lifecycle(&store, storage, old),
        ContentLifecycle::Live
    );

    let (replacement_turn, _) = replace_with_completed_root(&store, storage, old);
    let selected_head = head(&store, storage, old.thread);
    let selected_summary = summary(&store, storage, old.thread);
    assert_eq!(selected_head.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(selected_head.committed_tail(), Some(replacement_turn));
    assert!(selected_summary.complete());
    assert_eq!(selected_summary.committed_tail(), Some(replacement_turn));
    assert_ne!(replacement_turn, old.turn);

    freeze_assistant(&store, storage, old, timestamp(20));
    assert_eq!(
        assistant_content_lifecycle(&store, storage, old),
        ContentLifecycle::Finalized
    );
    assert_eq!(head(&store, storage, old.thread), selected_head);
    assert_eq!(summary(&store, storage, old.thread), selected_summary);

    project_item(&store, storage, old.assistant_item);
    assert_eq!(head(&store, storage, old.thread), selected_head);
    assert_eq!(summary(&store, storage, old.thread), selected_summary);
    finalize_item(
        &store,
        storage,
        old.thread,
        old.turn,
        TurnItemOrdinal::new(2).unwrap(),
        old.assistant_item,
        timestamp(21),
    );

    let old_state = storage
        .turn_state(&store, old.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(old_state.record().item_count(), 2);
    assert_eq!(old_state.record().finalized_item_count(), 2);
    assert_eq!(head(&store, storage, old.thread), selected_head);
    assert_eq!(summary(&store, storage, old.thread), selected_summary);
    store.validate_registered_domains().unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    assert_eq!(head(&reopened, storage, old.thread), selected_head);
    assert_eq!(summary(&reopened, storage, old.thread), selected_summary);
    assert_eq!(
        storage
            .turn_state(&reopened, old.turn, point_limit())
            .unwrap()
            .unwrap()
            .record()
            .finalized_item_count(),
        2
    );
    reopened.close().unwrap();
}

#[test]
fn selected_path_finalization_stales_transcript_and_updates_history_summary() {
    let home = TestHome::new("phase7-selected-path-finalization");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let selected = seed_terminal_turn_with_open_assistant(&store, storage);
    let before_head = head(&store, storage, selected.thread);
    let before_summary = summary(&store, storage, selected.thread);
    assert_eq!(before_head.lifecycle(), ProjectionLifecycle::Current);
    assert!(!before_summary.complete());

    freeze_assistant(&store, storage, selected, timestamp(9));
    let frozen_head = head(&store, storage, selected.thread);
    let frozen_summary = summary(&store, storage, selected.thread);
    assert_ne!(frozen_head, before_head);
    assert_eq!(frozen_head.lifecycle(), ProjectionLifecycle::Stale);
    assert_eq!(frozen_head.entry_count(), 0);
    assert_eq!(frozen_head.committed_tail(), before_head.committed_tail());
    assert_eq!(
        frozen_head.selected_path_digest(),
        before_head.selected_path_digest()
    );
    assert_eq!(frozen_summary.last_activity_at(), timestamp(9));
    assert!(!frozen_summary.complete());
    assert_ne!(frozen_summary, before_summary);

    project_item(&store, storage, selected.assistant_item);
    finalize_item(
        &store,
        storage,
        selected.thread,
        selected.turn,
        TurnItemOrdinal::new(2).unwrap(),
        selected.assistant_item,
        timestamp(10),
    );
    let finalized_head = head(&store, storage, selected.thread);
    let finalized_summary = summary(&store, storage, selected.thread);
    assert_eq!(finalized_head.lifecycle(), ProjectionLifecycle::Stale);
    assert_eq!(finalized_summary.last_activity_at(), timestamp(10));
    assert!(!finalized_summary.complete());
    assert_eq!(
        storage
            .turn_state(&store, selected.turn, point_limit())
            .unwrap()
            .unwrap()
            .record()
            .finalized_item_count(),
        2
    );

    let rebuilt = publish_transcript(&store, storage, selected.thread);
    let rebuilt_summary = summary(&store, storage, selected.thread);
    assert_eq!(rebuilt.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(rebuilt.entry_count(), 2);
    assert!(!rebuilt_summary.complete());
    assert_eq!(rebuilt_summary.last_activity_at(), timestamp(10));
    store.validate_registered_domains().unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}
