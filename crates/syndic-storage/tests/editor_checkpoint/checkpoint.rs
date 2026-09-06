use super::{edit_support::commit_edit, publication_support::*, support::*};
use syndic_storage::{
    DraftHistoricalRootDirectionV1, DraftHistoricalRootSelectionIntentV1,
    DraftHistoricalRootSelectionV1,
};

#[test]
fn untouched_empty_and_published_nonzero_openings_are_saved_without_publication() {
    for populated in [false, true] {
        let (home, store, storage, thread) = fixture("opening", 10, 65_536);
        let initial = current(&storage, &store, thread);
        if populated {
            let opened = open_session(&storage, &store, &initial, 12, 13);
            let edit = commit_edit(&storage, &store, &opened, 14, "already durable");
            publish_candidate(&storage, &store, &initial, edit.adopted_session(), 15);
            let published = head(&storage, &store, edit.adopted_session());
            assert!(
                storage
                    .draft_editor_candidate_is_saved(
                        &store,
                        DraftEditorCandidateActivationBindingV1::from_head(&published),
                        selector(&current(&storage, &store, thread)),
                    )
                    .unwrap()
            );
        }
        let durable = current(&storage, &store, thread);
        assert_eq!(
            durable.draft().history().candidate_generation() > 0,
            populated
        );
        drop(store);
        let mut store = open(&home);
        let storage = SyndicStorage::register(&mut store).unwrap();
        assert_eq!(current(&storage, &store, thread), durable);
        let opened = open_session(&storage, &store, &durable, 16, 17);
        assert_eq!(opened.newest_root(), durable.draft().piece_root());
        assert_ne!(opened.newest_history(), durable.draft().history());
        assert_eq!(
            opened.newest_candidate_generation(),
            durable.draft().history().candidate_generation()
        );
        let before = (
            store.home_revision().unwrap(),
            storage.revision(&store).unwrap(),
        );
        for _ in 0..3 {
            assert!(
                storage
                    .draft_editor_candidate_is_saved(
                        &store,
                        DraftEditorCandidateActivationBindingV1::from_head(&opened),
                        selector(&durable),
                    )
                    .unwrap()
            );
        }
        assert_eq!(head(&storage, &store, &opened), opened);
        assert_eq!(current(&storage, &store, thread), durable);
        assert_eq!(
            before,
            (
                store.home_revision().unwrap(),
                storage.revision(&store).unwrap()
            )
        );
    }
}

#[test]
fn edited_and_historical_candidates_do_not_reuse_opening_saved_correspondence() {
    let (_home, store, storage, thread) = fixture("later-adoption", 30, 65_536);
    let durable = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &durable, 32, 33);
    let edit = commit_edit(&storage, &store, &opened, 34, "changed");
    let edited = edit.adopted_session();
    assert!(
        !storage
            .draft_editor_candidate_is_saved(
                &store,
                DraftEditorCandidateActivationBindingV1::from_head(edited),
                selector(&durable),
            )
            .unwrap()
    );
    assert!(
        storage
            .draft_editor_candidate_is_saved(
                &store,
                DraftEditorCandidateActivationBindingV1::from_head(&opened),
                selector(&durable),
            )
            .is_err()
    );
    let intent = DraftHistoricalRootSelectionIntentV1::new(
        DraftEditorCandidateActivationBindingV1::from_head(edited),
        DraftPieceOperationIdV1::from_bytes([35; 16]),
        DraftHistoricalRootDirectionV1::Undo,
    );
    let DraftHistoricalRootSelectionV1::Prepared(prepared) = storage
        .prepare_draft_historical_root_selection(&store, intent)
        .unwrap()
    else {
        panic!("undo was not available");
    };
    committed(execute(
        &store,
        storage.adopt_draft_historical_root(storage.revision(&store).unwrap(), prepared),
    ));
    let undone = head(&storage, &store, edited);
    assert_eq!(undone.newest_root(), opened.newest_root());
    assert_ne!(undone.newest_history(), opened.newest_history());
    assert!(
        !storage
            .draft_editor_candidate_is_saved(
                &store,
                DraftEditorCandidateActivationBindingV1::from_head(&undone),
                selector(&durable),
            )
            .unwrap()
    );
    assert_eq!(current(&storage, &store, thread), durable);
    publish_candidate(&storage, &store, &durable, &undone, 36);
    let published_undo = head(&storage, &store, &undone);
    let undo_durable = current(&storage, &store, thread);
    assert_eq!(undo_durable.draft().piece_root(), opened.newest_root());
    assert!(
        storage
            .draft_editor_candidate_is_saved(
                &store,
                DraftEditorCandidateActivationBindingV1::from_head(&published_undo),
                selector(&undo_durable),
            )
            .unwrap()
    );
    let redo = DraftHistoricalRootSelectionIntentV1::new(
        DraftEditorCandidateActivationBindingV1::from_head(&published_undo),
        DraftPieceOperationIdV1::from_bytes([37; 16]),
        DraftHistoricalRootDirectionV1::Redo,
    );
    let DraftHistoricalRootSelectionV1::Prepared(prepared) = storage
        .prepare_draft_historical_root_selection(&store, redo)
        .unwrap()
    else {
        panic!("redo was not available after publishing the undo checkpoint");
    };
    committed(execute(
        &store,
        storage.adopt_draft_historical_root(storage.revision(&store).unwrap(), prepared),
    ));
    let redone = head(&storage, &store, &published_undo);
    assert_eq!(redone.newest_root(), edited.newest_root());
    assert!(redone.newest_candidate_generation() > edited.newest_candidate_generation());
    assert!(
        !storage
            .draft_editor_candidate_is_saved(
                &store,
                DraftEditorCandidateActivationBindingV1::from_head(&redone),
                selector(&undo_durable),
            )
            .unwrap()
    );
    publish_candidate(&storage, &store, &undo_durable, &redone, 38);
    let published_redo = head(&storage, &store, &redone);
    let redo_durable = current(&storage, &store, thread);
    assert_eq!(redo_durable.draft().piece_root(), edited.newest_root());
    assert!(
        storage
            .draft_editor_candidate_is_saved(
                &store,
                DraftEditorCandidateActivationBindingV1::from_head(&published_redo),
                selector(&redo_durable),
            )
            .unwrap()
    );
}

#[test]
fn substituted_session_forks_and_stale_selectors_are_rejected_without_mutation() {
    let (_home, store, storage, thread) = fixture("substitution", 50, 65_536);
    let durable = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &durable, 52, 53);
    let other = open_session(&storage, &store, &durable, 54, 55);
    let exact = DraftEditorCandidateActivationBindingV1::from_head(&opened);
    let substituted = DraftEditorCandidateActivationBindingV1::new(
        exact.draft_id(),
        exact.session_id(),
        exact.session_generation(),
        exact.candidate_generation(),
        exact.root(),
        other.newest_history(),
        exact.logical_extent(),
    );
    let before = store.home_revision().unwrap();
    assert!(
        storage
            .draft_editor_candidate_is_saved(&store, substituted, selector(&durable))
            .is_err()
    );
    assert_eq!(store.home_revision().unwrap(), before);
    assert_eq!(head(&storage, &store, &opened), opened);
    let edit = commit_edit(&storage, &store, &other, 56, "new durable");
    publish_candidate(&storage, &store, &durable, edit.adopted_session(), 57);
    let changed = current(&storage, &store, thread);
    let before = store.home_revision().unwrap();
    assert!(
        storage
            .draft_editor_candidate_is_saved(&store, exact, selector(&durable))
            .is_err()
    );
    assert_eq!(store.home_revision().unwrap(), before);
    assert_eq!(current(&storage, &store, thread), changed);
    assert_eq!(head(&storage, &store, &opened), opened);
}

#[test]
fn missing_opening_history_authority_cannot_be_treated_as_saved() {
    let (_home, store, storage, thread) = fixture("missing-history", 70, 65_536);
    let durable = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &durable, 72, 73);
    committed(execute(
        &store,
        delete_draft_edit_history_frontier(&store, storage.clone(), opened.newest_history().key()),
    ));
    let before = store.home_revision().unwrap();
    assert!(
        storage
            .draft_editor_candidate_is_saved(
                &store,
                DraftEditorCandidateActivationBindingV1::from_head(&opened),
                selector(&durable)
            )
            .is_err()
    );
    assert_eq!(store.home_revision().unwrap(), before);
    assert_eq!(current(&storage, &store, thread), durable);
}
