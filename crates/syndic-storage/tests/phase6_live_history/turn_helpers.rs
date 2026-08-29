use super::*;

pub(super) fn seed_pending_turn(
    store: &HomeStore,
    storage: &SyndicStorage,
) -> (SyndicThreadId, SyndicTurnId) {
    let thread = id(1);
    let draft = draft_id(2);
    assert_committed(execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                crate::support::exact_cas::execution_binding(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));

    let turn = submit_current_draft(
        store,
        storage.clone(),
        thread,
        draft_id(3),
        SyndicItemId::from_bytes([4; 16]),
        "question",
        timestamp(3),
    );
    (thread, turn)
}

pub(super) fn next_event(
    store: &HomeStore,
    storage: &SyndicStorage,
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
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        Some(source.clone()),
        payload,
        observed_at,
    )
    .unwrap()
}

pub(super) fn admit(
    store: &HomeStore,
    storage: &SyndicStorage,
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
    admit_event(
        store,
        storage.clone(),
        thread,
        turn,
        source,
        payload,
        observed_at,
    );
    event
}

pub(super) fn provider_content_id(item_id: SyndicItemId) -> SyndicContentId {
    let mut bytes = *item_id.as_bytes();
    for byte in &mut bytes {
        *byte ^= 0xa5;
    }
    SyndicContentId::from_bytes(bytes)
}

pub(super) fn prepare_item_frame(
    store: &HomeStore,
    storage: &SyndicStorage,
    turn: SyndicTurnId,
    item_id: SyndicItemId,
    source: &CasTurnSource,
    frame: ProviderItemFrameV1,
) -> PreparedProviderFrame {
    let state = storage.turn_state(store, turn, limit()).unwrap().unwrap();
    let source_event = SourceEventSequence::new(state.source_event_count() + 1).unwrap();
    let prior = storage
        .canonical_item(store, item_id, limit())
        .unwrap()
        .and_then(|item| item.provider().cloned());
    let item_source = CasItemSource::new(source.clone(), frame.item_id().clone());
    let plan = match prior {
        Some(prior) => ProviderFramePreparationPlan::subsequent(
            item_id,
            turn,
            item_source,
            source_event,
            prior,
            frame,
        ),
        None => ProviderFramePreparationPlan::first(
            item_id,
            turn,
            item_source,
            source_event,
            provider_content_id(item_id),
            frame,
        ),
    };
    prepare_provider_frame(plan).unwrap()
}

pub(super) fn prepared_item_target(
    store: &HomeStore,
    storage: &SyndicStorage,
    turn: SyndicTurnId,
    item_id: SyndicItemId,
    source: &CasTurnSource,
    frame: ProviderItemFrameV1,
) -> SealedProviderFrameReference {
    prepare_item_frame(store, storage, turn, item_id, source, frame)
        .target()
        .clone()
}

pub(super) fn stage_item_frame_for_publication(
    store: &HomeStore,
    storage: &SyndicStorage,
    turn: SyndicTurnId,
    item_id: SyndicItemId,
    source: &CasTurnSource,
    frame: ProviderItemFrameV1,
) -> SealedProviderFrameReference {
    let prepared = prepare_item_frame(store, storage, turn, item_id, source, frame);
    assert_committed(execute(
        store,
        storage.begin_provider_frame_build(storage.revision(store).unwrap(), &prepared),
    ));
    let mut build = match stage_provider_frame(
        &prepared,
        prepared.initial_build().clone(),
        &mut |batch: &ProviderFrameStageBatch| {
            execute(
                store,
                storage.stage_provider_frame_batch(storage.revision(store).unwrap(), batch.clone()),
            )
        },
    )
    .expect("provider-frame staging traversal must remain valid")
    {
        ProviderFrameStageOutcome::Committed {
            value,
            later_failure: None,
            ..
        } => value,
        outcome => panic!("expected clean provider-frame staging, got {outcome:?}"),
    };
    for _ in 0..4_096 {
        if build.lifecycle() == ProviderItemBuildLifecycle::Sealed {
            assert_eq!(build.target(), prepared.target());
            return prepared.target().clone();
        }
        assert_committed(execute(
            store,
            storage.compare_provider_completion(storage.revision(store).unwrap(), build),
        ));
        build = storage
            .provider_item_build(store, item_id, limit())
            .unwrap()
            .unwrap()
            .clone();
    }
    panic!("bounded provider completion comparison did not finish");
}

pub(super) fn correlate_submitted_user_item(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: &CasTurnSource,
    observed_at: SyndicTimestamp,
) {
    let index = turn_items(store, storage, turn)
        .into_iter()
        .next()
        .expect("submitted turn has its local user item");
    correlate_user_item(
        store,
        storage.clone(),
        thread,
        turn,
        index.item_id(),
        source,
        observed_at,
    );
}

pub(super) fn read_content_bytes(
    store: &HomeStore,
    storage: &SyndicStorage,
    content: beryl_model::SyndicContentId,
) -> Vec<u8> {
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
    bytes
}

pub(super) fn current_provider_frame(
    store: &HomeStore,
    storage: &SyndicStorage,
    item_id: SyndicItemId,
) -> ProviderItemFrameV1 {
    let item = storage
        .canonical_item(store, item_id, limit())
        .unwrap()
        .unwrap();
    let provider = item.provider().unwrap();
    let content = read_content_bytes(store, storage, provider.content().id());
    let start = usize::try_from(provider.frame().encoded_start()).unwrap();
    let end = usize::try_from(provider.frame().encoded_end()).unwrap();
    decode_bounded_provider_item_frame_v1(
        &content[start..end],
        PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES,
        provider.frame().encoded_start(),
    )
    .unwrap()
}

pub(super) fn projected_item_text(
    store: &HomeStore,
    storage: &SyndicStorage,
    item_id: SyndicItemId,
) -> String {
    let head = storage
        .item_projection_head(store, item_id, limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Current);
    let mut text = String::new();
    let mut after = None;
    loop {
        let page = storage
            .item_projections(
                store,
                item_id,
                head.generation(),
                after,
                CursorReadLimits::new(64, 1_000_000).unwrap(),
            )
            .unwrap();
        for index in page.records() {
            let projection = storage
                .projection(store, index.projection_id(), limit())
                .unwrap()
                .unwrap();
            if let Some(source) = projection.payload().inline_source() {
                text.push_str(source);
            }
            after = Some(index.ordinal());
        }
        if !page.has_more() {
            return text;
        }
    }
}

pub(super) fn source_events(
    store: &HomeStore,
    storage: &SyndicStorage,
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

pub(super) fn turn_items(
    store: &HomeStore,
    storage: &SyndicStorage,
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

pub(super) fn project_item(store: &HomeStore, storage: &SyndicStorage, item_id: SyndicItemId) {
    let item = storage
        .canonical_item(store, item_id, limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
    assert_committed(execute(
        store,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item_id, item.revision(), generation),
        ),
    ));
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
        assert_committed(execute(
            store,
            storage.advance_item_projection_build(
                storage.revision(store).unwrap(),
                AdvanceItemProjectionBuild::new(item_id, generation, build.revision()),
            ),
        ));
    }
}

pub(super) fn complete_item_frontier(
    store: &HomeStore,
    storage: &SyndicStorage,
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
    let provider_is_live = item.provider_content().is_some_and(|content| {
        storage
            .content_manifest(store, content.id(), limit())
            .unwrap()
            .unwrap()
            .lifecycle()
            == ContentLifecycle::Live
    });
    if provider_is_live {
        let state = storage.turn_state(store, turn, limit()).unwrap().unwrap();
        assert_committed(execute(
            store,
            storage.freeze_next_turn_item(
                storage.revision(store).unwrap(),
                FreezeNextTurnItem::new(
                    thread,
                    turn,
                    state.revision(),
                    ordinal,
                    item_id,
                    updated_at,
                ),
            ),
        ));
    }
    let item = storage
        .canonical_item(store, item_id, limit())
        .unwrap()
        .unwrap();
    if item.projection_source().is_some() {
        project_item(store, storage, item_id);
    }
    let state = storage.turn_state(store, turn, limit()).unwrap().unwrap();
    assert_committed(execute(
        store,
        storage.finalize_next_turn_item(
            storage.revision(store).unwrap(),
            FinalizeNextTurnItem::new(thread, turn, state.revision(), ordinal, item_id, updated_at),
        ),
    ));
}
