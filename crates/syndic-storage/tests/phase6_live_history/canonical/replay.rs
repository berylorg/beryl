use super::*;

#[test]
fn replay_order_terminal_closure_and_frontier_finalization_are_exact() {
    let home = TestHome::new("phase6-replay-terminal-finalization");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn) = seed_pending_turn(&store, storage);
    let assistant = SyndicItemId::from_bytes([20; 16]);
    let cas_assistant = CasItemId::new("phase6-replay-assistant").unwrap();
    let source = establish_turn(&store, storage, thread, turn, timestamp(4));

    let activation = admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    let duplicate = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), activation.clone()),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&duplicate),
        SyndicMutationError::SourceEventAlreadyAdmitted
    ));

    let state = storage.turn_state(&store, turn, limit()).unwrap().unwrap();
    let gate = storage
        .input_gate(&store, thread, limit())
        .unwrap()
        .unwrap();
    let collision = LiveSourceEvent::new(
        thread,
        turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::FIRST,
        Some(source.clone()),
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
        ),
        timestamp(5),
    )
    .unwrap();
    let error = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), collision),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceEventCollision
    ));

    let out_of_order = LiveSourceEvent::new(
        thread,
        turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(3).unwrap(),
        Some(source.clone()),
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
        ),
        timestamp(5),
    )
    .unwrap();
    let error = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), out_of_order),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceEventSequenceConflict { .. }
    ));

    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        assistant,
        &source,
        agent_start(
            cas_assistant.clone(),
            "",
            Some(ProviderMessagePhaseV1::Commentary),
            timestamp(5),
        ),
        timestamp(5),
    );
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        assistant,
        &source,
        agent_delta(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_assistant.clone(),
            "unfinished but durable",
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
        agent_completion(
            ProviderFrameOrdinalV1::new(3).unwrap(),
            cas_assistant.clone(),
            "unfinished but durable",
            Some(ProviderMessagePhaseV1::Commentary),
            timestamp(7),
        ),
        timestamp(7),
    );
    let terminal = admit(
        &store,
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
        timestamp(8),
    );
    let state = storage.turn_state(&store, turn, limit()).unwrap().unwrap();
    assert_eq!(state.item_count(), 2);
    assert_eq!(state.finalized_item_count(), 0);
    let item = storage
        .canonical_item(&store, assistant, limit())
        .unwrap()
        .unwrap();
    let manifest = storage
        .content_manifest(
            &store,
            item.provider_content()
                .expect("canonical assistant message has provider content")
                .id(),
            limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(manifest.lifecycle(), ContentLifecycle::Live);

    let closed_item = SyndicItemId::from_bytes([21; 16]);
    let closed_frame = prepared_item_target(
        &store,
        storage,
        turn,
        closed_item,
        &source,
        agent_start(
            CasItemId::new("phase6-closed-item").unwrap(),
            "forbidden",
            None,
            timestamp(9),
        ),
    );
    let closed_event = next_event(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemFrame {
            item_id: closed_item,
            frame: Box::new(closed_frame),
        },
        timestamp(9),
    );
    let error = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), closed_event),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::TerminalTurnClosed
    ));

    let items = turn_items(&store, storage, turn);
    complete_item_frontier(
        &store,
        storage,
        thread,
        turn,
        items[0].ordinal(),
        items[0].item_id(),
        timestamp(9),
    );
    complete_item_frontier(
        &store,
        storage,
        thread,
        turn,
        TurnItemOrdinal::new(2).unwrap(),
        assistant,
        timestamp(10),
    );
    let state = storage.turn_state(&store, turn, limit()).unwrap().unwrap();
    assert_eq!(state.finalized_item_count(), 2);
    let item = storage
        .canonical_item(&store, assistant, limit())
        .unwrap()
        .unwrap();
    let manifest = storage
        .content_manifest(
            &store,
            item.provider_content()
                .expect("canonical assistant message has provider content")
                .id(),
            limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(manifest.lifecycle(), ContentLifecycle::Finalized);
    assert_eq!(
        projected_item_text(&store, storage, assistant),
        "unfinished but durable"
    );

    let error = execute(
        &store,
        storage.finalize_next_turn_item(
            storage.revision(&store).unwrap(),
            FinalizeNextTurnItem::new(
                thread,
                turn,
                state.revision(),
                TurnItemOrdinal::new(2).unwrap(),
                assistant,
                timestamp(11),
            ),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::CanonicalFinalizationConflict
    ));

    let duplicate_terminal = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), terminal),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&duplicate_terminal),
        SyndicMutationError::SourceEventAlreadyAdmitted
    ));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    assert_eq!(
        storage
            .turn_state(&reopened, turn, limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        2
    );
    reopened.close().unwrap();
}
