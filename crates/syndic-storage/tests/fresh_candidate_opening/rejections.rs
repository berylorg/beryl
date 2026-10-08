use super::*;

#[test]
fn private_fresh_preparation_rejects_stale_foreign_occupied_and_corrupt_opening_facts() {
    for invalid in 0..4 {
        let (_home, store, storage, faults, thread) =
            fault_fixture("fresh-candidate-refusal", 30, 65_536);
        let selected = current(&storage, &store, thread);
        let mut request = open_request(&selected, 32, 33);
        match invalid {
            0 => {
                let opened = open_session(&storage, &store, &selected, 34, 35);
                let edit = transaction(&storage, &store, &opened, 36, "changed", point(0));
                build(&storage, &store, &edit);
                committed(execute(
                    &store,
                    storage.settle_draft_piece_edit(
                        storage.revision(&store).unwrap(),
                        edit.prepared.clone(),
                    ),
                ));
                let dirty = match settled(&storage, &store, &edit).closure() {
                    DraftPieceSettlementClosureV1::Committed(adoption) => {
                        adoption.adopted_session().clone()
                    }
                    other => panic!("edit did not commit: {other:?}"),
                };
                shared::publish_candidate(&storage, &store, &selected, &dirty, 37);
            }
            1 => {
                let _ = open_session(&storage, &store, &selected, 32, 33);
            }
            2 => committed(execute(
                &store,
                delete_draft_edit_history_frontier(
                    &store,
                    storage.clone(),
                    selected.draft().history().key(),
                ),
            )),
            _ => {
                let selected = selector(&selected);
                request = DraftEditorCandidateSessionOpenRequestV1::new(
                    DraftEditorCurrentSelectorV1::new(
                        selected.thread_id(),
                        ThreadRevision::new(selected.thread_revision().get() + 1).unwrap(),
                        selected.draft_id(),
                        selected.selector_revision(),
                        selected.root(),
                        selected.history(),
                    ),
                    request.session_id(),
                    request.operation_id(),
                );
            }
        }
        fail_home(&store, &faults);
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let (_foreign_home, foreign_store, foreign, _) =
            fixture("fresh-candidate-foreign", 40, 65_536);
        let access = recovery.recovery_access().unwrap();
        let revision = fresh.revision_candidate(&access).unwrap();
        for rejected in [&fresh, &storage, &foreign] {
            assert!(
                rejected
                    .prepare_open_draft_editor_candidate_session_candidate(&access, request)
                    .is_err()
            );
        }
        assert_eq!(fresh.revision_candidate(&access).unwrap(), revision);
        foreign_store.close().unwrap();
        recovery.abort().close().unwrap();
    }
}
