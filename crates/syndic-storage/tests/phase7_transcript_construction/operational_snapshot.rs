use super::*;

#[test]
fn operational_event_refreshes_current_path_snapshot_without_invalidating_transcript() {
    let home = TestHome::new("phase7-operational-current-path-snapshot");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(&store, storage);
    let submitted = submit_text(
        &store,
        storage,
        thread,
        "question",
        draft_id(3),
        SyndicItemId::from_bytes([40; 16]),
        timestamp(2),
        timestamp(3),
    );
    let source = establish_turn(&store, storage, thread, submitted.turn, timestamp(4));
    admit(
        &store,
        storage,
        thread,
        submitted.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    correlate_user_item(
        &store,
        storage,
        thread,
        submitted.turn,
        submitted.item,
        &source,
        timestamp(5),
    );
    project_item(&store, storage, submitted.item);
    let (generation, _) = start_transcript_build(&store, storage, thread);
    finish_transcript_build(&store, storage, thread, generation);

    let head_before = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(head_before.lifecycle(), ProjectionLifecycle::Current);
    let paths_before = path_turns(&store, storage, thread, generation);
    let entries_before = transcript_entries(&store, storage, thread, generation);
    let state_before = storage
        .turn_state(&store, submitted.turn, point_limit())
        .unwrap()
        .unwrap()
        .clone();

    admit_item_frame(
        &store,
        storage,
        thread,
        submitted.turn,
        SyndicItemId::from_bytes([41; 16]),
        &source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            CasItemId::new("phase7-operational-current-path").unwrap(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(timestamp(6).unix_millis()),
                item: ProviderItemV1::CommandExecution(ProviderCommandExecutionV1 {
                    command: ProviderTextV1::inline("cargo check"),
                    cwd: ProviderTextV1::inline("C:/workspace"),
                    process_id: None,
                    source: ProviderCommandSourceV1::Agent,
                    status: ProviderCommandStatusV1::InProgress,
                    command_actions: Vec::new(),
                    aggregated_output: None,
                    exit_code: None,
                    duration_ms: None,
                }),
            },
        ),
        timestamp(6),
    );

    let head_after = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head_after, head_before);
    assert_eq!(
        transcript_entries(&store, storage, thread, generation),
        entries_before
    );

    let paths_after = path_turns(&store, storage, thread, generation);
    assert_eq!(paths_after.len(), paths_before.len());
    let path_before = &paths_before[0];
    let path_after = &paths_after[0];
    assert_eq!(path_after.thread_id(), path_before.thread_id());
    assert_eq!(path_after.generation(), path_before.generation());
    assert_eq!(path_after.depth(), path_before.depth());
    assert_eq!(path_after.turn_id(), path_before.turn_id());
    assert_eq!(
        path_after.turn_path_digest(),
        path_before.turn_path_digest()
    );

    let state_after = storage
        .turn_state(&store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        state_after.source_event_count(),
        state_before.source_event_count() + 1
    );
    assert_eq!(path_after.state_revision(), state_after.revision());
    assert_eq!(path_after.lifecycle(), state_after.lifecycle());
    assert_eq!(
        path_after.source_event_count(),
        state_after.source_event_count()
    );
    assert_eq!(path_after.item_count(), state_after.item_count());
    assert_eq!(
        path_after.finalized_item_count(),
        state_after.finalized_item_count()
    );
    assert_eq!(path_after.updated_at(), state_after.updated_at());
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}
