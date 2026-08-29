use super::*;

#[test]
fn selected_tail_advance_preserves_the_completed_release_build() {
    let home = TestHome::new("phase7-transcript-release-build");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(&store, storage.clone());
    let root = submit_text(
        &store,
        storage.clone(),
        thread,
        "root before supersession",
        draft_id(3),
        SyndicItemId::from_bytes([30; 16]),
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
    let released_head = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(released_head.lifecycle(), ProjectionLifecycle::Current);
    let old_generation = released_head.generation();
    let released_build = storage
        .transcript_build(&store, thread, old_generation, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(released_build.phase(), TranscriptBuildPhase::Complete);

    let new_tail = submit_text(
        &store,
        storage.clone(),
        thread,
        "new selected tail",
        draft_id(4),
        SyndicItemId::from_bytes([31; 16]),
        timestamp(8),
    );
    let retained = storage
        .transcript_build(&store, thread, old_generation, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(retained, released_build);

    let head = storage
        .transcript_view_head(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.generation(), old_generation.checked_next().unwrap());
    assert_eq!(head.committed_tail(), Some(new_tail.turn));
    assert_eq!(head.entry_count(), 0);
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Stale);
    assert!(!transcript_entries(&store, storage, thread, old_generation).is_empty());

    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(
        storage
            .transcript_build(&reopened, thread, old_generation, point_limit())
            .unwrap()
            .unwrap()
            .phase(),
        TranscriptBuildPhase::Complete
    );
    reopened.close().unwrap();
}
