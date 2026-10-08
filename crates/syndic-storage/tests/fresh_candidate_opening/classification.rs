use super::*;

#[test]
fn fresh_classifier_refuses_later_edits_without_narrowing_original_opening_reconciliation() {
    let (_home, store, storage, faults, thread) =
        fault_fixture("fresh-candidate-later-edit", 110, 65_536);
    let selected = current(&storage, &store, thread);
    let request = open_request(&selected, 112, 113);
    let prepared = storage
        .prepare_open_draft_editor_candidate_session(&store, request)
        .unwrap();
    let original = execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    let head = match storage
        .draft_editor_candidate_session(&store, request.selector().draft_id(), request.session_id())
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
        other => panic!("original opening unavailable: {other:?}"),
    };
    let opening_binding = DraftEditorCandidateActivationBindingV1::from_head(&head);
    let edit = transaction(&storage, &store, &head, 114, "changed", point(0));
    build(&storage, &store, &edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared),
    ));
    let replay = execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    fail_home(&store, &faults);
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    assert!(matches!(
        fresh
            .reconcile_draft_editor_candidate_session_open_candidate(&access, &prepared, replay)
            .unwrap(),
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(_)
            | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(_)
    ));
    assert!(
        fresh
            .reconcile_fresh_draft_editor_candidate_session_open_candidate(
                &access, &prepared, original
            )
            .is_err()
    );
    assert!(
        fresh
            .candidate_draft_piece_text_demand_candidate(
                &access,
                opening_binding,
                DraftPieceTextDemandV1::Forward(0),
                4
            )
            .is_err()
    );
    recovery.publish().unwrap().close().unwrap();
}
