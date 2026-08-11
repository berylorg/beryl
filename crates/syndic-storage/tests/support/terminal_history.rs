use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use syndic_storage::{
    AdvanceItemProjectionBuild, AdvanceTranscriptBuild, CanonicalItemPresentation,
    CompleteTerminalHistory, ContentLifecycle, FinalizeNextTurnItem, FreezeNextTurnItem,
    GeneratedMediaResourceDisposition, ItemProjectionGeneration, ProjectionLifecycle,
    ProviderItemLifecycle, ResourceBacking, StartItemProjectionBuild, StartTranscriptBuild,
    SyndicPointReadLimit, SyndicStorage, TranscriptBuildPhase, TurnItemOrdinal,
};

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean terminal-history fixture command, got {outcome:?}"),
    }
}

pub fn converge_and_release_terminal_history(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
) {
    converge_items(store, storage, thread_id, turn_id);
    converge_transcript(store, storage, thread_id);
    let gate = storage
        .input_gate(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let state = storage
        .turn_state(store, turn_id, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.complete_terminal_history(
            storage.revision(store).unwrap(),
            CompleteTerminalHistory::new(
                thread_id,
                turn_id,
                gate,
                state.revision(),
                head.generation(),
                head.revision(),
            ),
        ),
    );
}

fn converge_items(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
) {
    loop {
        let state = storage
            .turn_state(store, turn_id, point_limit())
            .unwrap()
            .unwrap();
        if state.finalized_item_count() == state.item_count() {
            return;
        }
        let ordinal =
            TurnItemOrdinal::new(state.finalized_item_count().checked_add(1).unwrap()).unwrap();
        let after = ordinal
            .get()
            .checked_sub(1)
            .and_then(|value| TurnItemOrdinal::new(value).ok());
        let page = storage
            .turn_items(
                store,
                turn_id,
                after,
                CursorReadLimits::new(1, 1_000_000).unwrap(),
            )
            .unwrap();
        let index = page.records().first().expect("next terminal item exists");
        assert_eq!(index.ordinal(), ordinal);
        let item = storage
            .canonical_item(store, index.item_id(), point_limit())
            .unwrap()
            .unwrap();
        if item.provider_lifecycle() != ProviderItemLifecycle::Completed {
            return;
        }
        if let Some(content) = item.provider_content() {
            let manifest = storage
                .content_manifest(store, content.id(), point_limit())
                .unwrap()
                .unwrap();
            if manifest.lifecycle() == ContentLifecycle::Live {
                execute(
                    store,
                    storage.freeze_next_turn_item(
                        storage.revision(store).unwrap(),
                        FreezeNextTurnItem::new(
                            thread_id,
                            turn_id,
                            state.revision(),
                            ordinal,
                            item.id(),
                            state.updated_at(),
                        ),
                    ),
                );
            }
        }
        if generated_media_is_waiting(store, storage, &item) {
            return;
        }
        project_item_if_needed(store, storage, item.id());
        let state = storage
            .turn_state(store, turn_id, point_limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage.finalize_next_turn_item(
                storage.revision(store).unwrap(),
                FinalizeNextTurnItem::new(
                    thread_id,
                    turn_id,
                    state.revision(),
                    ordinal,
                    item.id(),
                    state.updated_at(),
                ),
            ),
        );
    }
}

fn generated_media_is_waiting(
    store: &HomeStore,
    storage: SyndicStorage,
    item: &syndic_storage::CanonicalItemRecord,
) -> bool {
    let CanonicalItemPresentation::GeneratedMedia { resource_id } = item.presentation() else {
        return false;
    };
    let resource = storage
        .resource(store, *resource_id, point_limit())
        .unwrap()
        .unwrap();
    matches!(
        resource.backing(),
        ResourceBacking::GeneratedMedia(
            GeneratedMediaResourceDisposition::PendingAsset
                | GeneratedMediaResourceDisposition::Unavailable(_)
        )
    )
}

fn project_item_if_needed(
    store: &HomeStore,
    storage: SyndicStorage,
    item_id: beryl_model::SyndicItemId,
) {
    let item = storage
        .canonical_item(store, item_id, point_limit())
        .unwrap()
        .unwrap();
    if item.projection_source().is_none() {
        return;
    }
    let head = storage
        .item_projection_head(store, item_id, point_limit())
        .unwrap();
    if head
        .as_ref()
        .is_some_and(|head| head.lifecycle() == ProjectionLifecycle::Current)
    {
        return;
    }
    let generation = head
        .as_ref()
        .map_or(ItemProjectionGeneration::FIRST, |head| {
            head.generation().checked_next().unwrap()
        });
    execute(
        store,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item_id, item.revision(), generation),
        ),
    );
    for _ in 0..4_096 {
        if storage
            .item_projection_head(store, item_id, point_limit())
            .unwrap()
            .as_ref()
            .is_some_and(|head| head.lifecycle() == ProjectionLifecycle::Current)
        {
            return;
        }
        let build = storage
            .item_projection_build(store, item_id, generation, point_limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage.advance_item_projection_build(
                storage.revision(store).unwrap(),
                AdvanceItemProjectionBuild::new(item_id, generation, build.revision()),
            ),
        );
    }
    panic!("bounded terminal item projection did not finish");
}

fn converge_transcript(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
) {
    let thread = storage
        .thread(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    if head.lifecycle() == ProjectionLifecycle::Current {
        return;
    }
    let generation = head.generation();
    execute(
        store,
        storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(thread_id, thread.revision(), head.revision()),
        ),
    );
    for _ in 0..4_096 {
        let build = storage
            .transcript_build(store, thread_id, generation, point_limit())
            .unwrap()
            .unwrap();
        if build.phase() == TranscriptBuildPhase::Complete {
            return;
        }
        execute(
            store,
            storage.advance_transcript_build(
                storage.revision(store).unwrap(),
                AdvanceTranscriptBuild::new(thread_id, generation, build.revision()),
            ),
        );
    }
    panic!("bounded terminal transcript build did not finish");
}
