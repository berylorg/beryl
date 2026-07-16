use super::*;

#[test]
fn coalesced_assistant_and_operational_history_reopens_exactly() {
    let home = TestHome::new("phase6-canonical-live-history");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn) = seed_pending_turn(&store, storage);
    let source = establish_turn(&store, storage, thread, turn, timestamp(4));
    let assistant = SyndicItemId::from_bytes([10; 16]);
    let operational = SyndicItemId::from_bytes([11; 16]);
    let cas_assistant = CasItemId::new("phase6-canonical-assistant").unwrap();
    let cas_operational = CasItemId::new("phase6-canonical-operational").unwrap();
    let assistant_descriptor = SourceItemDescriptor::new(
        assistant,
        cas_assistant.clone(),
        ProviderItemKind::AgentMessage,
        ProviderItemDisposition::CanonicalText,
    )
    .unwrap();
    let operational_descriptor = SourceItemDescriptor::new(
        operational,
        cas_operational.clone(),
        ProviderItemKind::CommandExecution,
        ProviderItemDisposition::CanonicalText,
    )
    .unwrap();
    let first = "alpha\n";
    let second = "β".repeat(70_000);
    let expected_assistant = format!("{first}{second}");

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
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemStarted {
            item: assistant_descriptor.clone(),
            assistant_phase: Some(AssistantMessagePhase::Unknown),
        },
        timestamp(5),
    );
    for (text, at) in [(first.to_owned(), 6), (second, 7)] {
        admit(
            &store,
            storage,
            thread,
            turn,
            &source,
            SourceEventPayload::ItemDelta {
                item_id: assistant,
                cas_item_id: cas_assistant.clone(),
                expected_kind: ProviderItemKind::AgentMessage,
                text: SourceEventText::new(text).unwrap(),
            },
            timestamp(at),
        );
    }
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemCompleted {
            item: assistant_descriptor,
            assistant_phase: Some(AssistantMessagePhase::FinalAnswer),
        },
        timestamp(8),
    );
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemStarted {
            item: operational_descriptor.clone(),
            assistant_phase: None,
        },
        timestamp(9),
    );
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemDelta {
            item_id: operational,
            cas_item_id: cas_operational,
            expected_kind: ProviderItemKind::CommandExecution,
            text: SourceEventText::new("tool activity").unwrap(),
        },
        timestamp(10),
    );
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemCompleted {
            item: operational_descriptor,
            assistant_phase: None,
        },
        timestamp(11),
    );
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(12),
    );

    let items = turn_items(&store, storage, turn);
    for (index, item) in items.iter().enumerate() {
        complete_item_frontier(
            &store,
            storage,
            thread,
            turn,
            item.ordinal(),
            item.item_id(),
            timestamp(13 + index as u64),
        );
    }

    store.validate_registered_domains().unwrap();
    let state = storage.turn_state(&store, turn, limit()).unwrap().unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Complete);
    assert_eq!(state.record().source_event_count(), 11);
    assert_eq!(state.record().item_count(), 3);
    assert_eq!(state.record().finalized_item_count(), 3);
    let gate = storage
        .input_gate(&store, thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().state(), &InputGateState::Idle);

    assert_eq!(items.len(), 3);
    assert_eq!(items[1].item_id(), assistant);
    assert_eq!(items[2].item_id(), operational);
    let assistant_record = storage
        .canonical_item(&store, assistant, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        assistant_record.record().kind(),
        CanonicalItemKind::AssistantMessage(AssistantMessagePhase::FinalAnswer)
    );
    assert_eq!(assistant_record.record().source_event_count(), 4);
    let assistant_content = assistant_record
        .record()
        .payload()
        .content()
        .expect("canonical assistant message has content");
    assert_eq!(
        read_utf8(&store, storage, assistant_content.id()),
        expected_assistant
    );
    let assistant_manifest = storage
        .content_manifest(&store, assistant_content.id(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        assistant_manifest.record().lifecycle(),
        ContentLifecycle::Finalized
    );
    assert_eq!(assistant_manifest.record().owner(), Some(assistant));

    let operational_record = storage
        .canonical_item(&store, operational, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        operational_record.record().kind(),
        CanonicalItemKind::Operational(ProviderItemKind::CommandExecution)
    );
    assert_eq!(operational_record.record().source_event_count(), 3);
    assert_eq!(
        read_utf8(
            &store,
            storage,
            operational_record
                .record()
                .payload()
                .content()
                .expect("canonical command execution has content")
                .id(),
        ),
        "tool activity"
    );
    assert_eq!(source_events(&store, storage, turn).len(), 11);
    assert_eq!(
        storage
            .item_source_events(
                &store,
                assistant,
                None,
                CursorReadLimits::new(8, 1_000_000).unwrap(),
            )
            .unwrap()
            .records()
            .len(),
        4
    );
    assert_eq!(
        storage
            .item_source_events(
                &store,
                operational,
                None,
                CursorReadLimits::new(8, 1_000_000).unwrap(),
            )
            .unwrap()
            .records()
            .len(),
        3
    );
    let head = storage
        .transcript_view_head(&store, thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.record().entry_count(), 0);
    assert_eq!(head.record().lifecycle(), ProjectionLifecycle::Stale);
    assert!(
        !storage
            .history_summary(&store, thread, limit())
            .unwrap()
            .unwrap()
            .record()
            .complete()
    );

    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    let assistant_record = storage
        .canonical_item(&reopened, assistant, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        read_utf8(
            &reopened,
            storage,
            assistant_record
                .record()
                .payload()
                .content()
                .expect("canonical assistant message has content")
                .id(),
        ),
        expected_assistant
    );
    assert_eq!(source_events(&reopened, storage, turn).len(), 11);
    reopened.close().unwrap();
}

#[test]
fn replay_order_terminal_closure_and_frontier_finalization_are_exact() {
    let home = TestHome::new("phase6-replay-terminal-finalization");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn) = seed_pending_turn(&store, storage);
    let assistant = SyndicItemId::from_bytes([20; 16]);
    let cas_assistant = CasItemId::new("phase6-replay-assistant").unwrap();
    let source = establish_turn(&store, storage, thread, turn, timestamp(4));
    let assistant_descriptor = SourceItemDescriptor::new(
        assistant,
        cas_assistant.clone(),
        ProviderItemKind::AgentMessage,
        ProviderItemDisposition::CanonicalText,
    )
    .unwrap();

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
        state.record().revision(),
        gate.record().revision(),
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
        state.record().revision(),
        gate.record().revision(),
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

    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemStarted {
            item: assistant_descriptor,
            assistant_phase: Some(AssistantMessagePhase::Commentary),
        },
        timestamp(5),
    );
    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemDelta {
            item_id: assistant,
            cas_item_id: cas_assistant.clone(),
            expected_kind: ProviderItemKind::AgentMessage,
            text: SourceEventText::new("unfinished but durable").unwrap(),
        },
        timestamp(6),
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
        timestamp(7),
    );
    let state = storage.turn_state(&store, turn, limit()).unwrap().unwrap();
    assert_eq!(state.record().item_count(), 2);
    assert_eq!(state.record().finalized_item_count(), 0);
    let item = storage
        .canonical_item(&store, assistant, limit())
        .unwrap()
        .unwrap();
    let manifest = storage
        .content_manifest(
            &store,
            item.record()
                .payload()
                .content()
                .expect("canonical assistant message has content")
                .id(),
            limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(manifest.record().lifecycle(), ContentLifecycle::Live);

    let closed_event = next_event(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemDelta {
            item_id: assistant,
            cas_item_id: cas_assistant,
            expected_kind: ProviderItemKind::AgentMessage,
            text: SourceEventText::new("forbidden").unwrap(),
        },
        timestamp(8),
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
    assert_eq!(state.record().finalized_item_count(), 2);
    let item = storage
        .canonical_item(&store, assistant, limit())
        .unwrap()
        .unwrap();
    let manifest = storage
        .content_manifest(
            &store,
            item.record()
                .payload()
                .content()
                .expect("canonical assistant message has content")
                .id(),
            limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(manifest.record().lifecycle(), ContentLifecycle::Finalized);
    assert_eq!(
        read_utf8(
            &store,
            storage,
            item.record()
                .payload()
                .content()
                .expect("canonical assistant message has content")
                .id(),
        ),
        "unfinished but durable"
    );

    let error = execute(
        &store,
        storage.finalize_next_turn_item(
            storage.revision(&store).unwrap(),
            FinalizeNextTurnItem::new(
                thread,
                turn,
                state.record().revision(),
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
            .record()
            .finalized_item_count(),
        2
    );
    reopened.close().unwrap();
}
