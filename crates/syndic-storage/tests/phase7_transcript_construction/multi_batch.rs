use super::*;

#[test]
fn multi_batch_publication_resumes_and_orders_root_to_tail() {
    let home = TestHome::new("phase7-transcript-multi-batch");
    let mut store = open(home.path());
    let mut storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(&store, storage.clone());

    let root = submit_text(
        &store,
        storage.clone(),
        thread,
        "root",
        draft_id(3),
        SyndicItemId::from_bytes([20; 16]),
        timestamp(3),
    );
    complete_turn(
        &store,
        storage.clone(),
        thread,
        root,
        timestamp(4),
        timestamp(5),
        timestamp(6),
    );
    converge_and_release_terminal_history(&store, storage.clone(), thread, root.turn);
    let middle = submit_text(
        &store,
        storage.clone(),
        thread,
        "middle",
        draft_id(4),
        SyndicItemId::from_bytes([21; 16]),
        timestamp(8),
    );
    complete_turn(
        &store,
        storage.clone(),
        thread,
        middle,
        timestamp(9),
        timestamp(10),
        timestamp(11),
    );
    converge_and_release_terminal_history(&store, storage.clone(), thread, middle.turn);
    let large_tail = "x".repeat(MARKDOWN_SPAN_MAX_BYTES * 65);
    let tail = submit_text(
        &store,
        storage.clone(),
        thread,
        &large_tail,
        draft_id(5),
        SyndicItemId::from_bytes([22; 16]),
        timestamp(13),
    );
    complete_turn(
        &store,
        storage.clone(),
        thread,
        tail,
        timestamp(14),
        timestamp(15),
        timestamp(16),
    );

    let root_projections = item_projection_ids(&store, storage.clone(), root.item);
    let middle_projections = item_projection_ids(&store, storage.clone(), middle.item);
    let tail_projections = item_projection_ids(&store, storage.clone(), tail.item);
    assert_eq!(root_projections.len(), 1);
    assert_eq!(middle_projections.len(), 1);
    assert_eq!(tail_projections.len(), 65);

    let (generation, started) = start_transcript_build(&store, storage.clone(), thread);
    assert_eq!(
        started.phase(),
        TranscriptBuildPhase::Collecting {
            next_turn: Some(tail.turn),
        }
    );
    let interrupted_collecting =
        advance_transcript_build(&store, storage.clone(), thread, generation);
    assert_eq!(interrupted_collecting.path_turn_count(), 1);
    assert_eq!(
        interrupted_collecting.phase(),
        TranscriptBuildPhase::Collecting {
            next_turn: Some(middle.turn),
        }
    );

    store.close().unwrap();
    store = open(home.path());
    storage = SyndicStorage::register(&mut store).unwrap();
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(
        storage
            .transcript_build(&store, thread, generation, point_limit())
            .unwrap()
            .unwrap(),
        interrupted_collecting
    );

    let collecting_middle = advance_transcript_build(&store, storage.clone(), thread, generation);
    assert_eq!(collecting_middle.path_turn_count(), 2);
    assert_eq!(
        collecting_middle.phase(),
        TranscriptBuildPhase::Collecting {
            next_turn: Some(root.turn),
        }
    );
    let ready_to_publish = advance_transcript_build(&store, storage.clone(), thread, generation);
    assert_eq!(ready_to_publish.path_turn_count(), 3);
    assert_eq!(
        ready_to_publish.phase(),
        TranscriptBuildPhase::Publishing {
            next_depth: TurnDepth::FIRST,
            next_item: TurnItemOrdinal::FIRST,
            next_projection: syndic_storage::ProjectionOrdinal::FIRST,
        }
    );

    let path = path_turns(&store, storage.clone(), thread, generation);
    assert_eq!(path.len(), 3);
    assert_eq!(
        path.iter()
            .map(|record| (record.depth().get(), record.turn_id()))
            .collect::<Vec<_>>(),
        [(1, root.turn), (2, middle.turn), (3, tail.turn)]
    );
    assert_unpublished_head(&store, storage.clone(), thread, generation);

    let after_root = advance_transcript_build(&store, storage.clone(), thread, generation);
    assert_eq!(after_root.entry_count(), 1);
    let after_middle = advance_transcript_build(&store, storage.clone(), thread, generation);
    assert_eq!(after_middle.entry_count(), 2);
    let interrupted_publishing =
        advance_transcript_build(&store, storage.clone(), thread, generation);
    assert_eq!(interrupted_publishing.entry_count(), 66);
    assert_eq!(
        interrupted_publishing.phase(),
        TranscriptBuildPhase::Publishing {
            next_depth: TurnDepth::new(3).unwrap(),
            next_item: TurnItemOrdinal::FIRST,
            next_projection: syndic_storage::ProjectionOrdinal::new(65).unwrap(),
        }
    );
    assert_unpublished_head(&store, storage.clone(), thread, generation);

    store.close().unwrap();
    store = open(home.path());
    storage = SyndicStorage::register(&mut store).unwrap();
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(
        storage
            .transcript_build(&store, thread, generation, point_limit())
            .unwrap()
            .unwrap(),
        interrupted_publishing
    );

    let completed = advance_transcript_build(&store, storage.clone(), thread, generation);
    assert_eq!(completed.phase(), TranscriptBuildPhase::Complete);
    assert_eq!(completed.entry_count(), 67);
    let head = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.generation(), generation);
    assert_eq!(head.entry_count(), 67);
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Current);
    assert!(
        storage
            .history_summary(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .complete()
    );

    let mut expected = Vec::new();
    expected.extend(
        root_projections
            .iter()
            .copied()
            .map(|projection| (root.item, projection)),
    );
    expected.extend(
        middle_projections
            .iter()
            .copied()
            .map(|projection| (middle.item, projection)),
    );
    expected.extend(
        tail_projections
            .iter()
            .copied()
            .map(|projection| (tail.item, projection)),
    );
    let entries = transcript_entries(&store, storage.clone(), thread, generation);
    assert_eq!(entries.len(), expected.len());
    for (index, (entry, (item, projection))) in entries.iter().zip(expected.into_iter()).enumerate()
    {
        assert_eq!(
            entry.position(),
            TranscriptPosition::new(index as u64 + 1).unwrap()
        );
        assert_eq!(entry.item_id(), item);
        assert_eq!(entry.projection_id(), projection);
    }

    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
