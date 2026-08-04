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
    let first = "alpha\n";
    let second = "Î²".repeat(70_000);
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
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        assistant,
        &source,
        agent_start(cas_assistant.clone(), "", None, timestamp(5)),
        timestamp(5),
    );
    for (ordinal, text, at) in [
        (ProviderFrameOrdinalV1::new(2).unwrap(), first.to_owned(), 6),
        (ProviderFrameOrdinalV1::new(3).unwrap(), second, 7),
    ] {
        admit_item_frame(
            &store,
            storage,
            thread,
            turn,
            assistant,
            &source,
            agent_delta(ordinal, cas_assistant.clone(), text),
            timestamp(at),
        );
    }
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        assistant,
        &source,
        agent_completion(
            ProviderFrameOrdinalV1::new(4).unwrap(),
            cas_assistant,
            expected_assistant.clone(),
            Some(ProviderMessagePhaseV1::FinalAnswer),
            timestamp(8),
        ),
        timestamp(8),
    );
    let narrative_only_head = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(narrative_only_head.source_frontier(), 7);
    assert_eq!(narrative_only_head.logical_row_count(), 0);
    assert_eq!(narrative_only_head.running_row_count(), 0);
    assert!(
        storage
            .activity_query_page(
                &store,
                &narrative_only_head,
                None,
                CursorReadLimits::new(16, 1_000_000).unwrap(),
            )
            .unwrap()
            .records()
            .is_empty()
    );
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        operational,
        &source,
        command_start(cas_operational.clone(), timestamp(9)),
        timestamp(9),
    );
    let running_head = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(running_head.source_frontier(), 8);
    assert_eq!(running_head.logical_row_count(), 1);
    assert_eq!(running_head.running_row_count(), 1);
    assert_eq!(running_head.completed_retention_cutoff(), None);
    let running_page = storage
        .activity_query_page(
            &store,
            &running_head,
            None,
            CursorReadLimits::new(16, 1_000_000).unwrap(),
        )
        .unwrap();
    let running = &running_page.records()[0];
    assert_eq!(running.item_id(), operational);
    assert!(running.order().running());
    assert_eq!(running.order().updated_at(), timestamp(9));
    assert_eq!(running.source_event(), SourceEventSequence::new(8).unwrap());
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        operational,
        &source,
        command_delta(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_operational.clone(),
            "tool activity",
        ),
        timestamp(10),
    );
    let delta_head = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(delta_head.source_frontier(), 9);
    assert!(matches!(
        storage.activity_query_page(
            &store,
            &running_head,
            None,
            CursorReadLimits::new(16, 1_000_000).unwrap(),
        ),
        Err(SyndicReadError::StaleActivityQuery)
    ));
    let delta_page = storage
        .activity_query_page(
            &store,
            &delta_head,
            None,
            CursorReadLimits::new(16, 1_000_000).unwrap(),
        )
        .unwrap();
    assert!(delta_page.records()[0].order().running());
    assert_eq!(
        delta_page.records()[0].source_event(),
        SourceEventSequence::new(9).unwrap()
    );
    admit_item_frame(
        &store,
        storage,
        thread,
        turn,
        operational,
        &source,
        command_completion(
            ProviderFrameOrdinalV1::new(3).unwrap(),
            cas_operational,
            "tool activity",
            timestamp(11),
        ),
        timestamp(11),
    );
    let completed_head = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(completed_head.source_frontier(), 10);
    assert_eq!(completed_head.logical_row_count(), 1);
    assert_eq!(completed_head.running_row_count(), 0);
    assert_eq!(
        completed_head.completed_retention_cutoff(),
        Some(ActivityQueryOrder::new(false, timestamp(9), operational))
    );
    let completed_page = storage
        .activity_query_page(
            &store,
            &completed_head,
            None,
            CursorReadLimits::new(16, 1_000_000).unwrap(),
        )
        .unwrap();
    let completed = &completed_page.records()[0];
    assert_eq!(completed.item_id(), operational);
    assert!(!completed.order().running());
    assert_eq!(completed.order().updated_at(), timestamp(9));
    assert_eq!(
        completed.source_event(),
        SourceEventSequence::new(10).unwrap()
    );
    assert_eq!(
        completed.provider_lifecycle(),
        ProviderItemLifecycle::Completed
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
    let terminal_head = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(terminal_head.source_frontier(), 11);
    assert_eq!(terminal_head.logical_row_count(), 1);
    assert_eq!(terminal_head.running_row_count(), 0);

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
    assert_eq!(state.lifecycle(), TurnLifecycle::Complete);
    assert_eq!(state.source_event_count(), 11);
    assert_eq!(state.item_count(), 3);
    assert_eq!(state.finalized_item_count(), 3);
    let gate = storage
        .input_gate(&store, thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::FinalizingHistory(turn));

    assert_eq!(items.len(), 3);
    assert_eq!(items[1].item_id(), assistant);
    assert_eq!(items[2].item_id(), operational);
    let assistant_record = storage
        .canonical_item(&store, assistant, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        assistant_record.kind(),
        CanonicalItemKind::AssistantMessage(AssistantMessagePhase::FinalAnswer)
    );
    assert_eq!(assistant_record.source_event_count(), 4);
    let assistant_content = assistant_record
        .provider_content()
        .expect("canonical assistant message has provider content");
    assert_eq!(
        projected_item_text(&store, storage, assistant),
        expected_assistant
    );
    let assistant_manifest = storage
        .content_manifest(&store, assistant_content.id(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(assistant_manifest.lifecycle(), ContentLifecycle::Finalized);
    assert_eq!(assistant_manifest.owner(), Some(assistant));

    let operational_record = storage
        .canonical_item(&store, operational, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        operational_record.kind(),
        CanonicalItemKind::Operational(ProviderItemKind::CommandExecution)
    );
    assert_eq!(operational_record.source_event_count(), 3);
    let ProviderItemObservationV1::Completed {
        item: ProviderItemV1::CommandExecution(command),
        ..
    } = current_provider_frame(&store, storage, operational)
        .observation()
        .clone()
    else {
        panic!("canonical operational item did not retain its typed completion frame");
    };
    assert_eq!(command.status, ProviderCommandStatusV1::Completed);
    assert_eq!(
        command
            .aggregated_output
            .as_ref()
            .and_then(ProviderTextV1::inline_str),
        Some("tool activity")
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
    assert_eq!(head.entry_count(), 0);
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Stale);
    assert!(
        !storage
            .history_summary(&store, thread, limit())
            .unwrap()
            .unwrap()
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
    assert_eq!(assistant_record.provider_content(), Some(assistant_content));
    assert_eq!(
        projected_item_text(&reopened, storage, assistant),
        expected_assistant
    );
    assert_eq!(source_events(&reopened, storage, turn).len(), 11);
    let reopened_activity = storage
        .activity_query_head(&reopened, thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(reopened_activity, terminal_head);
    let reopened_page = storage
        .activity_query_page(
            &reopened,
            &reopened_activity,
            None,
            CursorReadLimits::new(16, 1_000_000).unwrap(),
        )
        .unwrap();
    assert_eq!(reopened_page.records(), completed_page.records());
    reopened.close().unwrap();
}
