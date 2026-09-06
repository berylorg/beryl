use super::*;

pub fn converge_and_release_terminal_history(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) {
    converge_items(store, storage, thread, turn);
    converge_transcript(store, storage, thread);
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.complete_terminal_history(
            storage.revision(store).unwrap(),
            CompleteTerminalHistory::new(
                thread,
                turn,
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
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) {
    for _ in 0..CONVERGENCE_LIMIT {
        let state = storage
            .turn_state(store, turn, point_limit())
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
                turn,
                after,
                CursorReadLimits::new(1, 1_000_000).unwrap(),
            )
            .unwrap();
        let index = page.records().first().unwrap();
        assert_eq!(index.ordinal(), ordinal);
        let item = storage
            .canonical_item(store, index.item_id(), point_limit())
            .unwrap()
            .unwrap();
        if item.provider_lifecycle() != syndic_storage::ProviderItemLifecycle::Completed {
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
                            thread,
                            turn,
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
                    state.revision(),
                    ordinal,
                    item.id(),
                    state.updated_at(),
                ),
            ),
        );
    }
    panic!("bounded terminal item convergence did not finish")
}

fn generated_media_is_waiting(
    store: &HomeStore,
    storage: &SyndicStorage,
    item: &CanonicalItemRecord,
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

fn project_item_if_needed(store: &HomeStore, storage: &SyndicStorage, item: SyndicItemId) {
    let record = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    if record.projection_source().is_none() {
        return;
    }
    let head = storage
        .item_projection_head(store, item, point_limit())
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
            StartItemProjectionBuild::new(item, record.revision(), generation),
        ),
    );
    for _ in 0..CONVERGENCE_LIMIT {
        if storage
            .item_projection_head(store, item, point_limit())
            .unwrap()
            .as_ref()
            .is_some_and(|head| head.lifecycle() == ProjectionLifecycle::Current)
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
    panic!("bounded item-projection convergence did not finish")
}

fn converge_transcript(store: &HomeStore, storage: &SyndicStorage, thread: SyndicThreadId) {
    let record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread, point_limit())
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
            StartTranscriptBuild::new(thread, record.revision(), head.revision()),
        ),
    );
    for _ in 0..CONVERGENCE_LIMIT {
        let build = storage
            .transcript_build(store, thread, generation, point_limit())
            .unwrap()
            .unwrap();
        if build.phase() == TranscriptBuildPhase::Complete {
            return;
        }
        execute(
            store,
            storage.advance_transcript_build(
                storage.revision(store).unwrap(),
                AdvanceTranscriptBuild::new(thread, generation, build.revision()),
            ),
        );
    }
    panic!("bounded transcript convergence did not finish")
}
