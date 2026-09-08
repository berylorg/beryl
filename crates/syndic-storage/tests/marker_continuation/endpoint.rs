use super::{continuation_support::*, restart::observed_build, *};
use syndic_storage::test_faults::{
    capture_draft_marker_source_endpoint_for_test, draft_marker_program_snapshot_for_test,
    inject_coordinated_draft_marker_secondary_for_test,
    restore_draft_marker_source_endpoint_for_test,
};

#[test]
fn exact_occupied_surgery_targets_do_not_advance_a_restored_same_source() {
    for pending in 5..=7 {
        let (_home, store, storage, thread) = fixture("marker-occupied-surgery", 161 + pending);
        let session = open_session(
            &storage,
            &store,
            &current(&storage, &store, thread),
            171,
            172,
        );
        let (prepared, fragments) = stage_fragments(
            &storage,
            &store,
            &session,
            173,
            vec![insertion(point(0), marker(174, 7, 1))],
            DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll),
        );
        let source = loop {
            let build = observed_build(&storage, &store, &prepared, &fragments);
            if draft_marker_program_snapshot_for_test(&build)
                .is_some_and(|value| value.pending == pending)
            {
                break build;
            }
            let advance = storage
                .prepare_draft_piece_build_advance(
                    &store,
                    session.draft_id(),
                    session.session_id(),
                    prepared.header().operation_id(),
                )
                .unwrap()
                .unwrap();
            committed(execute(&store, storage.advance_draft_piece_edit(advance)));
        };
        let checkpoint = capture_draft_marker_source_endpoint_for_test(&store, &storage, &source);
        let advance = storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                prepared.header().operation_id(),
            )
            .unwrap()
            .unwrap();
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
        let target = observed_build(&storage, &store, &prepared, &fragments);
        assert_ne!(target.progress_receipt(), source.progress_receipt());
        restore_draft_marker_source_endpoint_for_test(
            &store,
            &storage,
            &checkpoint,
            target.progress_receipt(),
        );
        assert_eq!(
            observed_build(&storage, &store, &prepared, &fragments),
            source
        );
        match storage.prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            prepared.header().operation_id(),
        ) {
            Ok(Some(advance)) => assert!(matches!(
                execute(&store, storage.advance_draft_piece_edit(advance)),
                CommandOutcome::NotCommitted { .. }
            )),
            Err(DraftPiecePrepareErrorV1::InvalidRoot) => {}
            Err(other) => panic!("occupied target did not reject exactly: {other:?}"),
            Ok(None) => panic!("occupied surgery was incorrectly complete"),
        }
        assert_eq!(
            observed_build(&storage, &store, &prepared, &fragments),
            source
        );
        assert_eq!(
            active_session(&storage, &store, session.draft_id(), session.session_id())
                .newest_root(),
            session.newest_root()
        );
    }
}

#[test]
fn secondary_source_proof_cannot_replace_an_unambiguous_primary_result() {
    let (home, store, storage, thread) = fixture("marker-invalid-secondary", 181);
    let session = open_session(
        &storage,
        &store,
        &current(&storage, &store, thread),
        183,
        184,
    );
    let session = complete_staged(
        &storage,
        &store,
        &session,
        182,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("x".into())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let (prepared, fragments) = stage_fragments(
        &storage,
        &store,
        &session,
        185,
        vec![insertion(point(0), marker(186, 7, 1))],
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll),
    );
    let build = loop {
        let build = observed_build(&storage, &store, &prepared, &fragments);
        if draft_marker_program_snapshot_for_test(&build)
            .is_some_and(|value| value.proof == Some((12, 0)))
        {
            break build;
        }
        let advance = storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                prepared.header().operation_id(),
            )
            .unwrap()
            .unwrap();
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
    };
    inject_coordinated_draft_marker_secondary_for_test(&store, &storage, &build);
    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    assert!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                prepared.header().operation_id()
            )
            .is_err()
    );
    assert!(
        storage
            .draft_piece_operation_status_page(&store, &prepared, 1, &fragments)
            .is_err()
    );
}
