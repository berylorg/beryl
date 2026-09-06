#![cfg(feature = "test-faults")]

include!("durable_builder/support.rs");

use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidatePublicationEvidenceV1,
    DraftEditorCandidatePublicationOutcomeV1, DraftEditorCandidatePublicationRequestV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1, DraftRootHistoryPairV1,
    ThreadCatalogSummaryPreparation,
};

#[test]
fn typed_then_deleted_current_draft_remains_pristine_after_publication() {
    let (_home, store, storage, thread) = fixture("type-delete", 236);
    let durable = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &durable, 237, 238);
    session = complete_staged(
        &storage,
        &store,
        &session,
        1,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("typed then deleted".to_owned())],
        ),
        DraftLogicalExtentV1::new(18, 1),
    );
    session = complete_staged(
        &storage,
        &store,
        &session,
        2,
        DraftPieceReplacementV1::new(point(0), point(18), Vec::new()),
        DraftLogicalExtentV1::new(0, 0),
    );
    assert_eq!(session.newest_root().summary().logical_utf8_bytes(), 0);
    assert_eq!(session.newest_root().summary().marker_count(), 0);

    let request = DraftEditorCandidatePublicationRequestV1::new(
        selector(&durable),
        session.session_id(),
        DraftPieceOperationIdV1::from_bytes([239; 16]),
        session.newest_candidate_generation(),
        DraftRootHistoryPairV1::new(session.newest_root(), session.newest_history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        SyndicTimestamp::from_unix_millis(3),
    );
    let head = active_session(
        &storage,
        &store,
        request.selector().draft_id(),
        request.session_id(),
    );
    let candidate = DraftEditorCandidateActivationBindingV1::new(
        request.selector().draft_id(),
        request.session_id(),
        head.session_generation(),
        request.candidate_generation(),
        request.candidate().root(),
        request.candidate().history(),
        request.candidate().root().summary().logical_extent(),
    );
    let source = storage
        .capture_draft_editor_candidate_publication_source(
            &store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                request.selector(),
                candidate,
                request.operation_id(),
                request.published_at(),
            ),
        )
        .unwrap();
    let publication = storage
        .prepare_draft_editor_candidate_publication(&store, source, request.evidence())
        .unwrap();
    let outcome = execute(
        &store,
        storage
            .publish_draft_editor_candidate(storage.revision(&store).unwrap(), publication.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &publication, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    match storage
        .prepare_thread_catalog_summary(&store, thread)
        .unwrap()
        .unwrap()
    {
        ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => committed(execute(
            &store,
            storage.rebuild_thread_catalog_summary(prepared),
        )),
        ThreadCatalogSummaryPreparation::ExactCurrent(_) => {}
    }

    let binding = storage
        .thread_execution(
            &store,
            thread,
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let candidate = storage
        .inspect_pristine_thread(&store, thread, &binding)
        .unwrap()
        .expect("type-delete publication must remain reusable");
    assert_eq!(candidate.thread_id(), thread);
    assert_eq!(candidate.draft_id(), durable.draft().id());
}
