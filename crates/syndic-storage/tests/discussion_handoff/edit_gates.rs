use super::*;

#[test]
fn unpublished_edit_and_candidate_cannot_publish_across_handoff_gate() {
    let home = TestHome::new("discussion-edit-gate");
    let (store, storage, mut request) = seeded(&home, FaultController::new());
    let selected = storage
        .current_draft(&store, id(36), limit())
        .unwrap()
        .unwrap();
    let session = support::open_session(&storage, &store, &selected, 230, 231);
    let edit = support::transaction(
        &storage,
        &store,
        &session,
        232,
        "draft change",
        support::point(12),
    );
    support::build(&storage, &store, &edit);
    let prepared = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request.clone()))
        .unwrap();
    let pending = prepared.intent().new_gate();
    execute(&store, prepared);
    blocked(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    );
    let released = storage
        .prepare_discussion_handoff(
            &store,
            DiscussionHandoffMutation::Release { expected: pending },
        )
        .unwrap();
    request.handoff_gate_revision = released.intent().new_gate().revision();
    execute(&store, released);
    committed(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    );
    let settlement = support::settled(&storage, &store, &edit);
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        panic!("edit should commit after release")
    };
    let session = adoption.adopted_session();
    let next_edit = support::transaction(&storage, &store, session, 234, "new", support::point(3));
    let DraftHistoricalRootSelectionV1::Prepared(undo) = storage
        .prepare_draft_historical_root_selection(
            &store,
            DraftHistoricalRootSelectionIntentV1::new(
                DraftEditorCandidateActivationBindingV1::from_head(session),
                DraftPieceOperationIdV1::from_bytes([235; 16]),
                DraftHistoricalRootDirectionV1::Undo,
            ),
        )
        .unwrap()
    else {
        panic!("expected undo")
    };
    let source = storage
        .capture_draft_editor_candidate_publication_source(
            &store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                support::selector(&selected),
                DraftEditorCandidateActivationBindingV1::from_head(session),
                DraftPieceOperationIdV1::from_bytes([233; 16]),
                timestamp(200),
            ),
        )
        .unwrap();
    let publication = storage
        .prepare_draft_editor_candidate_publication(
            &store,
            source,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        )
        .unwrap();
    let prepared = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let pending = prepared.intent().new_gate();
    execute(&store, prepared);
    blocked(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), next_edit.prepared),
    );
    blocked(
        &store,
        storage.adopt_draft_historical_root(storage.revision(&store).unwrap(), undo),
    );
    blocked(
        &store,
        storage
            .publish_draft_editor_candidate(storage.revision(&store).unwrap(), publication.clone()),
    );
    assert_eq!(
        storage
            .current_draft(&store, id(36), limit())
            .unwrap()
            .unwrap()
            .draft(),
        selected.draft()
    );
    execute(
        &store,
        storage
            .prepare_discussion_handoff(
                &store,
                DiscussionHandoffMutation::Release { expected: pending },
            )
            .unwrap(),
    );
    committed(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), publication),
    );
    assert!(
        storage
            .current_draft(&store, id(36), limit())
            .unwrap()
            .unwrap()
            .draft()
            .revision()
            > selected.draft().revision()
    );
    store.close().unwrap();
}
