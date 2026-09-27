use super::{edit_support::commit_edit, publication_support::*, support::*};

#[test]
fn fresh_candidate_observes_saved_and_unsaved_checkpoints_without_writes() {
    for state in 0..4 {
        let (_home, store, storage, faults, thread) =
            fault_fixture("candidate-checkpoint", 100, 65_536);
        let initial = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &initial, 102, 103);
        let session = if state == 0 {
            opened
        } else {
            let edit = commit_edit(&storage, &store, &opened, 104, "checkpoint text");
            if state == 1 {
                edit.adopted_session().clone()
            } else {
                publish_candidate(&storage, &store, &initial, edit.adopted_session(), 105);
                if state == 2 {
                    head(&storage, &store, edit.adopted_session())
                } else {
                    open_session(
                        &storage,
                        &store,
                        &current(&storage, &store, thread),
                        106,
                        107,
                    )
                }
            }
        };
        let binding = DraftEditorCandidateActivationBindingV1::from_head(&session);
        let selected = selector(&current(&storage, &store, thread));
        let expected = state != 1;
        assert_eq!(
            storage
                .draft_editor_candidate_is_saved(&store, binding, selected)
                .unwrap(),
            expected
        );
        let revision = storage.revision(&store).unwrap();
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        assert!(
            storage
                .draft_editor_candidate_is_saved_candidate(&access, binding, selected)
                .is_err()
        );
        let (_foreign_home, _foreign_store, foreign, _) =
            fixture("foreign-checkpoint", 110, 65_536);
        assert!(
            foreign
                .draft_editor_candidate_is_saved_candidate(&access, binding, selected)
                .is_err()
        );
        for _ in 0..2 {
            assert_eq!(
                fresh
                    .draft_editor_candidate_is_saved_candidate(&access, binding, selected)
                    .unwrap(),
                expected
            );
        }
        let store = recovery.publish().unwrap();
        assert_eq!(fresh.revision(&store).unwrap(), revision);
        assert_eq!(head(&fresh, &store, &session), session);
        assert_eq!(
            fresh
                .draft_editor_candidate_is_saved(&store, binding, selected)
                .unwrap(),
            expected
        );
    }
}

#[test]
fn candidate_checkpoint_rejects_stale_binding_selector_and_missing_authority() {
    for invalid in 0..4 {
        let (_home, store, storage, faults, thread) =
            fault_fixture("candidate-checkpoint-invalid", 120, 65_536);
        let initial = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &initial, 122, 123);
        let mut binding = DraftEditorCandidateActivationBindingV1::from_head(&opened);
        let mut selected = selector(&initial);
        match invalid {
            0 => {
                commit_edit(&storage, &store, &opened, 124, "new candidate");
            }
            1 => {
                let edit = commit_edit(&storage, &store, &opened, 124, "new selector");
                publish_candidate(&storage, &store, &initial, edit.adopted_session(), 125);
            }
            2 => {
                committed(execute(
                    &store,
                    delete_draft_edit_history_frontier(
                        &store,
                        storage.clone(),
                        opened.newest_history().key(),
                    ),
                ));
            }
            _ => {
                let edit = commit_edit(&storage, &store, &opened, 124, "published history");
                publish_candidate(&storage, &store, &initial, edit.adopted_session(), 125);
                binding = DraftEditorCandidateActivationBindingV1::from_head(&head(
                    &storage,
                    &store,
                    edit.adopted_session(),
                ));
                selected = selector(&current(&storage, &store, thread));
                committed(execute(
                    &store,
                    delete_draft_edit_history_frontier(
                        &store,
                        storage.clone(),
                        current(&storage, &store, thread).draft().history().key(),
                    ),
                ));
            }
        }
        assert!(
            storage
                .draft_editor_candidate_is_saved(&store, binding, selected)
                .is_err()
        );
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        assert!(
            fresh
                .draft_editor_candidate_is_saved_candidate(&access, binding, selected)
                .is_err()
        );
        recovery.abort().close().unwrap();
    }
}

#[test]
fn candidate_checkpoint_read_failure_prevents_publication() {
    let (_home, store, storage, faults, thread) =
        fault_fixture("candidate-checkpoint-read-failure", 140, 65_536);
    let initial = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &initial, 142, 143);
    let binding = DraftEditorCandidateActivationBindingV1::from_head(&opened);
    let selected = selector(&initial);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    for _ in 0..2 {
        assert!(
            fresh
                .draft_editor_candidate_is_saved_candidate(&access, binding, selected)
                .is_err()
        );
    }
    assert!(recovery.publish().is_err());
}
