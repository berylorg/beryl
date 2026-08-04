use super::*;

#[test]
fn terminal_history_converges_before_replacement_changes_the_selected_path() {
    let home = TestHome::new("phase7-off-path-finalization");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let old = seed_terminal_turn_with_open_assistant(&store, storage);
    assert_eq!(
        assistant_content_lifecycle(&store, storage, old),
        ContentLifecycle::Live
    );
    converge_and_release_terminal_history(&store, storage, old.thread, old.turn);
    assert_eq!(
        assistant_content_lifecycle(&store, storage, old),
        ContentLifecycle::Finalized
    );
    assert_eq!(
        storage
            .turn_state(&store, old.turn, point_limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        2
    );

    let (replacement_turn, _) = replace_with_completed_root(&store, storage, old);
    let selected_head = head(&store, storage, old.thread);
    let selected_summary = summary(&store, storage, old.thread);
    assert_eq!(selected_head.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(selected_head.committed_tail(), Some(replacement_turn));
    assert!(selected_summary.complete());
    assert_eq!(selected_summary.committed_tail(), Some(replacement_turn));
    assert_ne!(replacement_turn, old.turn);

    let old_state = storage
        .turn_state(&store, old.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(old_state.item_count(), 2);
    assert_eq!(old_state.finalized_item_count(), 2);
    assert_eq!(head(&store, storage, old.thread), selected_head);
    assert_eq!(summary(&store, storage, old.thread), selected_summary);
    store.validate_registered_domains().unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    assert_eq!(head(&reopened, storage, old.thread), selected_head);
    assert_eq!(summary(&reopened, storage, old.thread), selected_summary);
    assert_eq!(
        storage
            .turn_state(&reopened, old.turn, point_limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        2
    );
    reopened.close().unwrap();
}
