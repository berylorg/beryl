use super::*;

#[derive(Clone, Copy)]
pub(super) struct TerminalTurn {
    pub(super) thread: SyndicThreadId,
    pub(super) turn: SyndicTurnId,
    pub(super) user_item: SyndicItemId,
    pub(super) assistant_item: SyndicItemId,
}

pub(super) fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub(super) fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean selected-path fixture command, got {outcome:?}"),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn submit_text(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    text: &str,
    next_draft: SyndicDraftId,
    user_item: SyndicItemId,
    admitted_at: SyndicTimestamp,
) -> SyndicTurnId {
    submit_current_draft(
        store,
        storage,
        thread,
        next_draft,
        user_item,
        text,
        admitted_at,
    )
}

pub(super) fn admit(
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

pub(super) fn agent_value(text: &str) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text),
        phase: Some(ProviderMessagePhaseV1::FinalAnswer),
        memory_citation: None,
    })
}

pub(super) fn project_item(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId) {
    let canonical = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
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
                AdvanceItemProjectionBuild::new(item, generation, build.revision()),
            ),
        );
    }
    panic!("bounded item projection did not finish");
}

pub(super) fn finalize_item(
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
            FinalizeNextTurnItem::new(thread, turn, state.revision(), ordinal, item, updated_at),
        ),
    );
}

pub(super) fn freeze_assistant(
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
                state.revision(),
                TurnItemOrdinal::new(2).unwrap(),
                fixture.assistant_item,
                updated_at,
            ),
        ),
    );
}

pub(super) fn publish_transcript(
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
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Stale);
    let generation = head.generation();
    execute(
        store,
        storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(thread, thread_record.revision(), head.revision()),
        ),
    );

    for _ in 0..1_024 {
        let build = storage
            .transcript_build(store, thread, generation, point_limit())
            .unwrap()
            .unwrap();
        if build.phase() == TranscriptBuildPhase::Complete {
            return storage
                .transcript_view_head(store, thread, point_limit())
                .unwrap()
                .unwrap()
                .clone();
        }
        execute(
            store,
            storage.advance_transcript_build(
                storage.revision(store).unwrap(),
                AdvanceTranscriptBuild::new(thread, generation, build.revision()),
            ),
        );
    }
    panic!("bounded transcript construction did not finish");
}

pub(super) fn transcript_entry(
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
