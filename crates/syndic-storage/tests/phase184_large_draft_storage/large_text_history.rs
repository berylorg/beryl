use super::*;

#[test]
fn multi_megabyte_draft_stages_traverses_adopts_publishes_reopens_and_materializes_boundedly() {
    let (home, mut store, mut storage, thread) = fixture("phase184-large-draft", 184);
    let durable = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &durable, 185, 186);

    for chunk in 0..LARGE_CHUNK_COUNT {
        let offset = (chunk * LARGE_CHUNK_BYTES) as u64;
        session = complete_staged_bounded(
            &storage,
            &store,
            &session,
            (chunk + 1) as u8,
            DraftPieceReplacementV1::new(
                point(offset),
                point(offset),
                vec![DraftPieceV1::Text(large_chunk(chunk))],
            ),
            DraftLogicalExtentV1::new(offset + LARGE_CHUNK_BYTES as u64, 1),
        );
    }
    assert_eq!(
        session.logical_extent().logical_utf8_bytes(),
        LARGE_DRAFT_BYTES
    );

    let edited_offset = LARGE_DRAFT_BYTES / 2 + 17;
    for offset in [
        0,
        LARGE_CHUNK_BYTES as u64 + 19,
        edited_offset,
        LARGE_DRAFT_BYTES - 4096,
    ] {
        assert_sparse_text(&storage, &store, session.newest_root(), offset, None);
    }

    let before_edit = session.newest_root();
    session = complete_staged_bounded(
        &storage,
        &store,
        &session,
        101,
        DraftPieceReplacementV1::new(
            point(edited_offset),
            point(edited_offset + 1),
            vec![DraftPieceV1::Text("!".to_owned())],
        ),
        DraftLogicalExtentV1::new(LARGE_DRAFT_BYTES, 1),
    );
    let edited_root = session.newest_root();
    assert_sparse_text(
        &storage,
        &store,
        edited_root,
        edited_offset - 16,
        Some(edited_offset),
    );

    session = adopt_history(
        &storage,
        &store,
        &session,
        102,
        DraftHistoricalRootDirectionV1::Undo,
    );
    assert_eq!(session.newest_root(), before_edit);
    session = adopt_history(
        &storage,
        &store,
        &session,
        103,
        DraftHistoricalRootDirectionV1::Redo,
    );
    assert_eq!(session.newest_root(), edited_root);

    let request = DraftEditorCandidatePublicationRequestV1::new(
        selector(&durable),
        session.session_id(),
        DraftPieceOperationIdV1::from_bytes([105; 16]),
        session.newest_candidate_generation(),
        DraftRootHistoryPairV1::new(session.newest_root(), session.newest_history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        SyndicTimestamp::from_unix_millis(2),
    );
    let source = capture_publication_source(&storage, &store, request);
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

    let mapping = materialize_bounded(&storage, &store, edited_root, 106);
    assert_eq!(mapping.source_utf8_bytes(), LARGE_DRAFT_BYTES);
    assert_eq!(mapping.source_marker_count(), 0);

    drop(store);
    store = HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    storage = SyndicStorage::register(&mut store).unwrap();
    let reopened = current(&storage, &store, thread).draft().piece_root();
    assert_eq!(reopened, edited_root);
    assert_sparse_text(
        &storage,
        &store,
        reopened,
        edited_offset - 16,
        Some(edited_offset),
    );
}
