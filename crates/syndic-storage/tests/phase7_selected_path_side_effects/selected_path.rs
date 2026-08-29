use super::*;

#[test]
fn selected_path_finalization_stales_transcript_and_updates_history_summary() {
    let home = TestHome::new("phase7-selected-path-finalization");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let selected = seed_terminal_turn_with_open_assistant(&store, &storage);
    let before_head = head(&store, &storage, selected.thread);
    let before_summary = summary(&store, &storage, selected.thread);
    assert_eq!(before_head.lifecycle(), ProjectionLifecycle::Current);
    assert!(!before_summary.complete());

    freeze_assistant(&store, &storage, selected, timestamp(9));
    let frozen_head = head(&store, &storage, selected.thread);
    let frozen_summary = summary(&store, &storage, selected.thread);
    assert_ne!(frozen_head, before_head);
    assert_eq!(frozen_head.lifecycle(), ProjectionLifecycle::Stale);
    assert_eq!(frozen_head.entry_count(), 0);
    assert_eq!(frozen_head.committed_tail(), before_head.committed_tail());
    assert_eq!(
        frozen_head.selected_path_digest(),
        before_head.selected_path_digest()
    );
    assert_eq!(frozen_summary.last_activity_at(), timestamp(9));
    assert!(!frozen_summary.complete());
    assert_ne!(frozen_summary, before_summary);

    project_item(&store, &storage, selected.assistant_item);
    finalize_item(
        &store,
        &storage,
        selected.thread,
        selected.turn,
        TurnItemOrdinal::new(2).unwrap(),
        selected.assistant_item,
        timestamp(10),
    );
    let finalized_head = head(&store, &storage, selected.thread);
    let finalized_summary = summary(&store, &storage, selected.thread);
    assert_eq!(finalized_head.lifecycle(), ProjectionLifecycle::Stale);
    assert_eq!(finalized_summary.last_activity_at(), timestamp(10));
    assert!(!finalized_summary.complete());
    assert_eq!(
        storage
            .turn_state(&store, selected.turn, point_limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        2
    );

    let rebuilt = publish_transcript(&store, &storage, selected.thread);
    let rebuilt_summary = summary(&store, &storage, selected.thread);
    assert_eq!(rebuilt.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(rebuilt.entry_count(), 2);
    assert!(!rebuilt_summary.complete());
    assert_eq!(rebuilt_summary.last_activity_at(), timestamp(10));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}
