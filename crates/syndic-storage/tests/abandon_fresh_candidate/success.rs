use crate::{shared::*, support::*};

use syndic_storage::{
    DraftEditorCandidateSessionAbandonFreshOutcomeV1,
    test_faults::{reset_syndic_point_read_count, syndic_point_read_count},
};

#[test]
fn fresh_abandonment_preserves_selector_and_replays_across_reopen() {
    let (home, store, storage, thread) = fixture("abandon-success", 10, 65_536);
    let selected = current(&storage, &store, thread);
    let selected_before = selector(&selected);
    let opened = open_session(&storage, &store, &selected, 12, 13);
    let forked_history = opened.newest_history();
    assert_ne!(forked_history, opened.published_history());
    let request = abandon_request(&opened, 14);

    reset_syndic_point_read_count();
    let prepared = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    assert_eq!(syndic_point_read_count(), 3);
    let outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    let abandoned = match storage
        .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &prepared, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(head) => head,
        other => panic!("fresh session was not abandoned: {other:?}"),
    };
    assert_eq!(
        selector(&current(&storage, &store, thread)),
        selected_before
    );
    assert_eq!(
        abandoned.session_generation(),
        opened.session_generation() + 1
    );
    assert_eq!(abandoned.published_root(), opened.published_root());
    assert_eq!(abandoned.newest_root(), opened.newest_root());
    assert_eq!(abandoned.newest_history(), opened.published_history());
    assert_eq!(abandoned.dirty_generation(), 0);
    assert_eq!(
        abandoned.disposal_operation_id(),
        Some(request.operation_id())
    );
    assert!(abandoned.active_operation().is_none());
    assert_eq!(head(&storage, &store, &opened), abandoned);

    let replay = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    let replay_outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            replay.clone(),
        ),
    );
    let receipt = match storage
        .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &replay, replay_outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(receipt) => receipt,
        other => panic!("abandonment did not replay: {other:?}"),
    };
    assert_eq!(receipt.frontier().reference(), forked_history);
    assert_ne!(receipt.frontier().reference(), abandoned.newest_history());
    assert_eq!(receipt.after_head(), &abandoned);

    drop(store);
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    assert_eq!(head(&storage, &store, &opened), abandoned);
    let replay = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            replay.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &replay, outcome)
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::ExactReplay(_)
    ));
    assert_eq!(head(&storage, &store, &opened), abandoned);
}
