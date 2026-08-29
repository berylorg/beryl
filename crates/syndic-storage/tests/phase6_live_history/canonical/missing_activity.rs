use super::*;

#[test]
fn provider_publication_fails_closed_when_activity_entry_is_missing() {
    let home = TestHome::new("phase13-activity-publication-corruption");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn) = seed_pending_turn(&store, &storage);
    let source = establish_turn(&store, storage.clone(), thread, turn, timestamp(4));
    let item = SyndicItemId::from_bytes([12; 16]);
    let generated = SyndicItemId::from_bytes([13; 16]);
    let cas_item = CasItemId::new("phase13-activity-corruption").unwrap();

    admit(
        &store,
        &storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    correlate_submitted_user_item(&store, &storage, thread, turn, &source, timestamp(4));
    admit_item_frame(
        &store,
        storage.clone(),
        thread,
        turn,
        generated,
        &source,
        image_generation_start(
            CasItemId::new("phase13-activity-generated-media").unwrap(),
            timestamp(5),
        ),
        timestamp(5),
    );
    let excluded_head = storage
        .activity_query_head(&store, thread, limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(excluded_head.source_frontier(), 4);
    assert_eq!(excluded_head.logical_row_count(), 0);
    admit_item_frame(
        &store,
        storage.clone(),
        thread,
        turn,
        item,
        &source,
        command_start(cas_item.clone(), timestamp(6)),
        timestamp(6),
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    let mut corruption = syndic_storage::test_faults::FixtureBatch::new();
    corruption
        .delete(
            syndic_storage::test_faults::FixtureDelete::ActivityQueryEntry {
                thread,
                work_period: ActivityWorkPeriod::FIRST,
                order: ActivityQueryOrder::new(true, timestamp(6), item),
            },
        )
        .unwrap();
    assert_committed(execute(
        &store,
        storage.fixture_contribution(storage.revision(&store).unwrap(), corruption),
    ));

    let frame = stage_item_frame_for_publication(
        &store,
        &storage,
        turn,
        item,
        &source,
        command_delta(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            cas_item,
            "must not publish",
        ),
    );
    let event = next_event(
        &store,
        &storage,
        thread,
        turn,
        &source,
        SourceEventPayload::ItemFrame {
            item_id: item,
            frame: Box::new(frame),
        },
        timestamp(7),
    );
    let beryl_home_store::CommandOutcome::NotCommitted { evidence: error } = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), event),
    ) else {
        panic!("expected definitive missing-activity rejection");
    };
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::ActivityQueryConflict
    ));
    assert_eq!(
        storage
            .canonical_item(&store, item, limit())
            .unwrap()
            .unwrap()
            .source_event_count(),
        1
    );

    store.close().unwrap();
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    let error = reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("activity-query head counters or retention cutoff disagree")
    );
    reopened.close().unwrap();
}
