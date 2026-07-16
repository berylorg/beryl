#![cfg(feature = "test-faults")]

#[path = "phase6_live_history/canonical.rs"]
mod canonical;
mod support;

use beryl_home_store::{CommandError, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{
    CasItemId, DraftRevision, InputGateRevision, SyndicItemId, SyndicThreadId, SyndicTurnId,
    ThreadRevision,
};
use syndic_storage::*;

use support::{
    TestHome, draft_id,
    exact_cas::{admit_event, establish_turn},
    id, open, stage_prepared_content, timestamp,
};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> Result<(), CommandError> {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).map(|_| ())
}

fn typed_error(error: &CommandError) -> &SyndicMutationError {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected Syndic validation rejection, got {error}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

fn seed_pending_turn(store: &HomeStore, storage: SyndicStorage) -> (SyndicThreadId, SyndicTurnId) {
    let thread = id(1);
    let draft = draft_id(2);
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(thread, draft, timestamp(1)),
        ),
    )
    .unwrap();

    let payload = ComposerPayload::new(vec![ComposerAtom::text("question").unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(store, storage, &content);
    let current = storage
        .current_draft(store, thread, limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(2)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    )
    .unwrap();

    let current = storage
        .current_draft(store, thread, limit())
        .unwrap()
        .unwrap();
    let submission = IdleSubmission::new(
        thread,
        ThreadRevision::new(1).unwrap(),
        draft,
        DraftRevision::new(2).unwrap(),
        current.draft().content(),
        InputGateRevision::new(1).unwrap(),
        draft_id(3),
        SyndicItemId::from_bytes([4; 16]),
        AdmissionMarkers::default(),
        timestamp(3),
    );
    let turn = submission.submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(storage.revision(store).unwrap(), submission),
    )
    .unwrap();
    (thread, turn)
}

fn next_event(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: &CasTurnSource,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) -> LiveSourceEvent {
    let state = storage.turn_state(store, turn, limit()).unwrap().unwrap();
    let gate = storage.input_gate(store, thread, limit()).unwrap().unwrap();
    LiveSourceEvent::new(
        thread,
        turn,
        state.record().revision(),
        gate.record().revision(),
        SourceEventSequence::new(state.record().source_event_count() + 1).unwrap(),
        Some(source.clone()),
        payload,
        observed_at,
    )
    .unwrap()
}

fn admit(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: &CasTurnSource,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) -> LiveSourceEvent {
    let event = next_event(
        store,
        storage,
        thread,
        turn,
        source,
        payload.clone(),
        observed_at,
    );
    admit_event(store, storage, thread, turn, source, payload, observed_at);
    event
}

fn correlate_submitted_user_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: &CasTurnSource,
    observed_at: SyndicTimestamp,
) {
    let index = turn_items(store, storage, turn)
        .into_iter()
        .next()
        .expect("submitted turn has its local user item");
    let item = storage
        .canonical_item(store, index.item_id(), limit())
        .unwrap()
        .unwrap();
    let descriptor = SourceItemDescriptor::new(
        index.item_id(),
        CasItemId::new(format!("correlated-user-{}", index.item_id())).unwrap(),
        ProviderItemKind::UserMessage,
        item.record().disposition(),
    )
    .unwrap();
    admit(
        store,
        storage,
        thread,
        turn,
        source,
        SourceEventPayload::ItemStarted {
            item: descriptor.clone(),
            assistant_phase: None,
        },
        observed_at,
    );
    admit(
        store,
        storage,
        thread,
        turn,
        source,
        SourceEventPayload::ItemCompleted {
            item: descriptor,
            assistant_phase: None,
        },
        observed_at,
    );
}

fn read_utf8(
    store: &HomeStore,
    storage: SyndicStorage,
    content: beryl_model::SyndicContentId,
) -> String {
    let mut bytes = Vec::new();
    let mut after = None;
    loop {
        let page = storage
            .content_chunks(
                store,
                content,
                after,
                CursorReadLimits::new(32, 2_000_000).unwrap(),
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

fn source_events(
    store: &HomeStore,
    storage: SyndicStorage,
    turn: SyndicTurnId,
) -> Vec<SourceEventRecord> {
    storage
        .source_events(
            store,
            turn,
            None,
            CursorReadLimits::new(64, 4_000_000).unwrap(),
        )
        .unwrap()
        .records()
        .to_vec()
}

fn turn_items(
    store: &HomeStore,
    storage: SyndicStorage,
    turn: SyndicTurnId,
) -> Vec<TurnItemIndexRecord> {
    storage
        .turn_items(
            store,
            turn,
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap()
        .records()
        .to_vec()
}

fn project_item(store: &HomeStore, storage: SyndicStorage, item_id: SyndicItemId) {
    let item = storage
        .canonical_item(store, item_id, limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
    execute(
        store,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item_id, item.record().revision(), generation),
        ),
    )
    .unwrap();
    loop {
        if storage
            .item_projection_set(store, item_id, generation, limit())
            .unwrap()
            .is_some()
        {
            break;
        }
        let build = storage
            .item_projection_build(store, item_id, generation, limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage.advance_item_projection_build(
                storage.revision(store).unwrap(),
                AdvanceItemProjectionBuild::new(item_id, generation, build.record().revision()),
            ),
        )
        .unwrap();
    }
}

fn complete_item_frontier(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    ordinal: TurnItemOrdinal,
    item_id: SyndicItemId,
    updated_at: SyndicTimestamp,
) {
    let item = storage
        .canonical_item(store, item_id, limit())
        .unwrap()
        .unwrap();
    let manifest = storage
        .content_manifest(
            store,
            item.record()
                .payload()
                .content()
                .expect("finalizable canonical item has content")
                .id(),
            limit(),
        )
        .unwrap()
        .unwrap();
    if manifest.record().lifecycle() == ContentLifecycle::Live {
        let state = storage.turn_state(store, turn, limit()).unwrap().unwrap();
        execute(
            store,
            storage.freeze_next_turn_item(
                storage.revision(store).unwrap(),
                FreezeNextTurnItem::new(
                    thread,
                    turn,
                    state.record().revision(),
                    ordinal,
                    item_id,
                    updated_at,
                ),
            ),
        )
        .unwrap();
    }
    let item = storage
        .canonical_item(store, item_id, limit())
        .unwrap()
        .unwrap();
    if matches!(
        item.record().kind(),
        CanonicalItemKind::UserInput | CanonicalItemKind::AssistantMessage(_)
    ) {
        project_item(store, storage, item_id);
    }
    let state = storage.turn_state(store, turn, limit()).unwrap().unwrap();
    execute(
        store,
        storage.finalize_next_turn_item(
            storage.revision(store).unwrap(),
            FinalizeNextTurnItem::new(
                thread,
                turn,
                state.record().revision(),
                ordinal,
                item_id,
                updated_at,
            ),
        ),
    )
    .unwrap();
}

#[test]
fn every_terminal_outcome_persists_its_exact_gate_semantics() {
    for (name, outcome, expected) in [
        (
            "complete",
            TurnTerminalOutcome::Complete,
            TurnLifecycle::Complete,
        ),
        (
            "interrupted",
            TurnTerminalOutcome::Interrupted,
            TurnLifecycle::Interrupted,
        ),
        ("failed", TurnTerminalOutcome::Failed, TurnLifecycle::Failed),
        (
            "incomplete",
            TurnTerminalOutcome::Incomplete,
            TurnLifecycle::Incomplete,
        ),
        (
            "unknown",
            TurnTerminalOutcome::UnknownTerminal,
            TurnLifecycle::UnknownTerminal,
        ),
    ] {
        let home = TestHome::new(&format!("phase6-terminal-{name}"));
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let (thread, turn) = seed_pending_turn(&store, storage);
        let source = establish_turn(&store, storage, thread, turn, timestamp(4));
        admit(
            &store,
            storage,
            thread,
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(4),
        );
        admit(
            &store,
            storage,
            thread,
            turn,
            &source,
            SourceEventPayload::TurnEnded(
                TurnEndStatus::new(outcome, Some(TurnIncompleteReason::ItemAuditFailed)).unwrap(),
            ),
            timestamp(5),
        );
        let state = storage.turn_state(&store, turn, limit()).unwrap().unwrap();
        assert_eq!(state.record().lifecycle(), expected);
        assert_eq!(state.record().terminal_outcome(), Some(outcome));
        assert_eq!(
            state.record().incomplete_reason(),
            Some(TurnIncompleteReason::ItemAuditFailed)
        );
        let gate = storage
            .input_gate(&store, thread, limit())
            .unwrap()
            .unwrap();
        if expected.is_proven_terminal() {
            assert_eq!(gate.record().state(), &InputGateState::Idle);
        } else {
            assert!(matches!(gate.record().state(), InputGateState::Stopping(_)));
        }
        store.validate_registered_domains().unwrap();
        store.close().unwrap();
    }
}

#[test]
fn unknown_terminal_accepts_late_data_but_phase_conflicts_change_nothing() {
    let home = TestHome::new("phase6-unknown-terminal-late-data");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn) = seed_pending_turn(&store, storage);
    let assistant = SyndicItemId::from_bytes([30; 16]);
    let cas_assistant = CasItemId::new("phase6-unknown-terminal-assistant").unwrap();
    let source = establish_turn(&store, storage, thread, turn, timestamp(4));
    let descriptor = SourceItemDescriptor::new(
        assistant,
        cas_assistant,
        ProviderItemKind::AgentMessage,
        ProviderItemDisposition::CanonicalText,
    )
    .unwrap();

    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::UnknownTerminal,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(5),
    );
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemStarted {
            item: descriptor.clone(),
            assistant_phase: Some(AssistantMessagePhase::Commentary),
        },
        timestamp(6),
    );

    let conflicting = next_event(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemCompleted {
            item: descriptor.clone(),
            assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
        },
        timestamp(7),
    );
    let error = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), conflicting),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::AssistantPhaseConflict
    ));
    assert_eq!(
        storage
            .turn_state(&store, turn, limit())
            .unwrap()
            .unwrap()
            .record()
            .source_event_count(),
        3
    );

    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemCompleted {
            item: descriptor,
            assistant_phase: Some(AssistantMessagePhase::Unknown),
        },
        timestamp(7),
    );
    assert_eq!(
        storage
            .canonical_item(&store, assistant, limit())
            .unwrap()
            .unwrap()
            .record()
            .kind(),
        CanonicalItemKind::AssistantMessage(AssistantMessagePhase::Commentary)
    );
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(8),
    );
    let state = storage.turn_state(&store, turn, limit()).unwrap().unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Interrupted);
    assert_eq!(state.record().source_event_count(), 5);
    assert_eq!(
        storage
            .input_gate(&store, thread, limit())
            .unwrap()
            .unwrap()
            .record()
            .state(),
        &InputGateState::Idle
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn a_live_event_cannot_mutate_another_threads_turn_or_gate() {
    let home = TestHome::new("phase6-cross-thread-rejection");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (first_thread, first_turn) = seed_pending_turn(&store, storage);
    let source = establish_turn(&store, storage, first_thread, first_turn, timestamp(4));
    let other_thread = id(40);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(other_thread, draft_id(41), timestamp(4)),
        ),
    )
    .unwrap();
    let state = storage
        .turn_state(&store, first_turn, limit())
        .unwrap()
        .unwrap();
    let other_gate = storage
        .input_gate(&store, other_thread, limit())
        .unwrap()
        .unwrap();
    let first_gate = storage
        .input_gate(&store, first_thread, limit())
        .unwrap()
        .unwrap();
    let mismatched = LiveSourceEvent::new(
        other_thread,
        first_turn,
        state.record().revision(),
        other_gate.record().revision(),
        SourceEventSequence::FIRST,
        Some(source),
        SourceEventPayload::TurnActivated,
        timestamp(5),
    )
    .unwrap();
    let error = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), mismatched),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::LiveTurnConflict
    ));
    assert_eq!(
        storage
            .turn_state(&store, first_turn, limit())
            .unwrap()
            .unwrap()
            .record(),
        state.record()
    );
    assert_eq!(
        storage
            .input_gate(&store, other_thread, limit())
            .unwrap()
            .unwrap()
            .record(),
        other_gate.record()
    );
    assert_eq!(
        storage
            .input_gate(&store, first_thread, limit())
            .unwrap()
            .unwrap()
            .record(),
        first_gate.record()
    );
    assert_eq!(
        storage
            .turn(&store, first_turn, limit())
            .unwrap()
            .unwrap()
            .record()
            .parent(),
        ConversationParent::Root
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
