use super::*;

const FIRST_SEGMENT_BYTES: usize = 60_000;
const SECOND_SEGMENT_BYTES: usize = 10_000;
const MISMATCH_OFFSET: u64 = 65_600;

fn stage_mismatched_completion(
    store: &HomeStore,
    storage: SyndicStorage,
    turn: SyndicTurnId,
    item: SyndicItemId,
    source: &CasTurnSource,
    frame: ProviderItemFrameV1,
) -> SealedProviderFrameReference {
    let prepared = prepare_item_frame(store, storage, turn, item, source, frame);
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
    .expect("mismatched provider-frame staging traversal must remain valid")
    {
        ProviderFrameStageOutcome::Committed {
            value,
            later_failure: None,
            ..
        } => value,
        outcome => panic!("expected clean provider-frame staging, got {outcome:?}"),
    };
    assert_eq!(build.lifecycle(), ProviderItemBuildLifecycle::Staging);

    assert_committed(execute(
        store,
        storage.compare_provider_completion(storage.revision(store).unwrap(), build),
    ));
    build = storage
        .provider_item_build(store, item, limit())
        .unwrap()
        .unwrap()
        .clone();
    let ProviderNarrativeCompletionState::Pending(frontier) =
        build.completion_check().unwrap().state()
    else {
        panic!("the first bounded comparison page must remain pending");
    };
    assert_eq!(frontier.compared_utf8_bytes(), 65_536);

    assert_committed(execute(
        store,
        storage.compare_provider_completion(storage.revision(store).unwrap(), build),
    ));
    build = storage
        .provider_item_build(store, item, limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(build.lifecycle(), ProviderItemBuildLifecycle::Sealed);
    assert_eq!(
        build.completion_check().unwrap().state(),
        ProviderNarrativeCompletionState::Mismatch {
            utf8_byte_offset: MISMATCH_OFFSET,
        }
    );
    assert_eq!(build.target(), prepared.target());
    prepared.target().clone()
}

fn selected_path(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> SelectedPathProof {
    let thread = storage.thread(store, thread, limit()).unwrap().unwrap();
    SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    )
}

#[test]
fn segmented_completion_mismatch_retains_live_narrative_and_blocks_recovery_after_reopen() {
    let home = TestHome::new("phase6-segmented-completion-mismatch");
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
    correlate_submitted_user_item(&store, storage, thread, turn, &source, timestamp(4));

    let assistant = SyndicItemId::from_bytes([61; 16]);
    let cas_item = CasItemId::new("phase6-segmented-mismatch-assistant").unwrap();
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        assistant,
        &source,
        agent_start(
            cas_item.clone(),
            "",
            Some(ProviderMessagePhaseV1::FinalAnswer),
            timestamp(5),
        ),
        timestamp(5),
    );
    let first = "a".repeat(FIRST_SEGMENT_BYTES);
    let second = "a".repeat(SECOND_SEGMENT_BYTES);
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        assistant,
        &source,
        agent_delta(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_item.clone(),
            first.clone(),
        ),
        timestamp(6),
    );
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        assistant,
        &source,
        agent_delta(
            ProviderFrameOrdinalV1::new(3).unwrap(),
            cas_item.clone(),
            second.clone(),
        ),
        timestamp(7),
    );

    let live_text = first + &second;
    let live_item = storage
        .canonical_item(&store, assistant, limit())
        .unwrap()
        .unwrap();
    let live_narrative = live_item.provider().unwrap().narrative().unwrap();
    assert_eq!(live_narrative.logical_utf8_bytes(), 70_000);

    let mut completion_text = live_text[..live_text.len() - 1].to_owned();
    let mismatch = usize::try_from(MISMATCH_OFFSET).unwrap();
    completion_text.replace_range(mismatch..mismatch + 1, "b");
    let completion_frame = agent_completion(
        ProviderFrameOrdinalV1::new(4).unwrap(),
        cas_item,
        completion_text,
        Some(ProviderMessagePhaseV1::FinalAnswer),
        timestamp(8),
    );
    let completion =
        stage_mismatched_completion(&store, storage, turn, assistant, &source, completion_frame);
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemFrame {
            item_id: assistant,
            frame: Box::new(completion.clone()),
        },
        timestamp(8),
    );

    let canonical = storage
        .canonical_item(&store, assistant, limit())
        .unwrap()
        .unwrap();
    assert_eq!(canonical.provider(), Some(&completion));
    assert_eq!(
        canonical.narrative_completion(),
        Some(ProviderNarrativeCompletionDisposition::Mismatch {
            utf8_byte_offset: MISMATCH_OFFSET,
        })
    );
    assert_eq!(
        canonical.projection_source(),
        Some(ProjectionTextSource::provider_narrative(live_narrative))
    );
    let cas_source = canonical.cas_source().unwrap().clone();
    let capture = storage
        .capture_item(&store, &cas_source, limit())
        .unwrap()
        .unwrap();
    let page = storage
        .capture_item_text_range(&store, &capture, MISMATCH_OFFSET - 8, 32, limit())
        .unwrap();
    assert_eq!(page.text(), "a".repeat(32));
    assert!(source_events(&store, storage, turn).iter().any(|event| {
        event.payload()
            == &SourceEventPayload::ItemFrame {
                item_id: assistant,
                frame: Box::new(completion.clone()),
            }
    }));

    let state = storage.turn_state(&store, turn, limit()).unwrap().unwrap();
    assert_eq!(state.history_blocking_item_count(), 1);
    let rejected = next_event(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(9),
    );
    let beryl_home_store::CommandOutcome::NotCommitted { evidence: error } = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), rejected),
    ) else {
        panic!("expected definitive terminal-item audit rejection");
    };
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::TerminalItemAuditConflict
    ));
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Complete,
                Some(TurnIncompleteReason::CompletionMismatch),
            )
            .unwrap(),
        ),
        timestamp(9),
    );

    let items = turn_items(&store, storage, turn);
    assert_eq!(items.len(), 2);
    complete_item_frontier(
        &store,
        storage,
        thread,
        turn,
        TurnItemOrdinal::FIRST,
        items[0].item_id(),
        timestamp(10),
    );
    complete_item_frontier(
        &store,
        storage,
        thread,
        turn,
        TurnItemOrdinal::new(2).unwrap(),
        assistant,
        timestamp(11),
    );
    assert_eq!(projected_item_text(&store, storage, assistant), live_text);
    assert!(!storage
        .history_summary(&store, thread, limit())
        .unwrap()
        .unwrap()
        .complete());
    let selected = selected_path(&store, storage, thread);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let canonical = storage
        .canonical_item(&reopened, assistant, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        canonical.narrative_completion(),
        Some(ProviderNarrativeCompletionDisposition::Mismatch {
            utf8_byte_offset: MISMATCH_OFFSET,
        })
    );
    assert_eq!(
        canonical.projection_source(),
        Some(ProjectionTextSource::provider_narrative(live_narrative))
    );
    let provider = canonical.provider().unwrap();
    assert_eq!(provider.frame(), completion.frame());
    assert_eq!(provider.observation(), completion.observation());
    assert_eq!(provider.narrative(), completion.narrative());
    assert_eq!(provider.content().id(), completion.content().id());
    assert_eq!(provider.content().summary(), completion.content().summary());
    assert_eq!(
        storage
            .content_manifest(&reopened, provider.content().id(), limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        ContentLifecycle::Finalized
    );
    assert!(source_events(&reopened, storage, turn).iter().any(|event| {
        event.payload()
            == &SourceEventPayload::ItemFrame {
                item_id: assistant,
                frame: Box::new(completion.clone()),
            }
    }));
    assert_eq!(
        projected_item_text(&reopened, storage, assistant),
        live_text
    );

    let state = storage
        .turn_state(&reopened, turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        state.terminal_outcome(),
        Some(TurnTerminalOutcome::Complete)
    );
    assert_eq!(
        state.incomplete_reason(),
        Some(TurnIncompleteReason::CompletionMismatch)
    );
    assert_eq!(state.history_blocking_item_count(), 1);
    assert_eq!(state.finalized_item_count(), state.item_count());
    let recovery = storage
        .prepare_recovery_projection(
            &reopened,
            RecoveryProjectionRequest::for_current_selected_path(thread, selected, Some(1_000_000)),
        )
        .unwrap_err();
    assert!(matches!(
        recovery,
        RecoveryProjectionError::IncompleteHistory { .. }
    ));
    reopened.close().unwrap();
}
