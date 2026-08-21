#[cfg(feature = "test-faults")]
#[test]
fn missing_durable_continuation_fails_status_advance_and_reopen_closed() {
    let (home, store, storage, thread) = fixture("missing-continuation", 90);
    let current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &current, 92, 93);
    session = complete_staged(
        &storage,
        &store,
        &session,
        94,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abc".to_owned())],
        ),
        DraftLogicalExtentV1::new(3, 1),
    );
    let marker = marker(95, 7, 9);
    session = complete_staged(
        &storage,
        &store,
        &session,
        96,
        DraftPieceReplacementV1::new(
            point(1),
            point(1),
            vec![DraftPieceV1::Marker(marker)],
        )
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
            DraftPieceMarkerInsertionV1::new(
                1,
                marker,
                DraftPieceMarkerEffectChargesV1::canonical_single_marker(),
            ),
        )),
        DraftLogicalExtentV1::new(3, 1),
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), marker.marker_id())
        .unwrap()
        .unwrap();
    let moved = DraftPieceReplacementV1::new(
        point(2),
        point(2),
        vec![DraftPieceV1::Marker(marker)],
    )
    .with_marker_effect(DraftPieceMarkerEffectV1::Move {
        removal: DraftPieceMarkerRemovalProofV1::new(
            DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll),
            occurrence,
        ),
        insertion: DraftPieceMarkerInsertionV1::new(
            2,
            marker,
            DraftPieceMarkerEffectChargesV1::canonical_single_marker(),
        ),
    });
    let (prepared, identity, fragment) = stage_replacement(
        &storage,
        &store,
        &session,
        97,
        moved,
        DraftLogicalExtentV1::new(3, 1),
    );
    let source_roots = open_build(&storage, &store, &prepared, &fragment).working_roots();
    let advance = storage
        .prepare_draft_piece_build_advance(
            &store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )
        .unwrap()
        .unwrap();
    committed(execute(
        &store,
        storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
    ));
    let pending = open_build(&storage, &store, &prepared, &fragment);
    assert_eq!(pending.working_roots(), source_roots);
    assert_ne!(
        pending
            .durable_continuation()
            .and_then(|continuation| continuation.pending_marker_effect())
            .unwrap()
            .working_roots(),
        source_roots
    );
    committed(execute(
        &store,
        inject_draft_piece_build_corruption(
            &store,
            storage,
            syndic_storage::DraftPieceSettlementKeyV1::new(
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            ),
            DraftPieceBuildCorruption::DropDurableContinuation,
        ),
    ));
    assert!(
        storage
            .draft_piece_operation_status_page(
                &store,
                &prepared,
                1,
                std::slice::from_ref(&fragment),
            )
            .is_err()
    );
    assert!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            )
            .is_err()
    );
    drop(store);
    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert!(
        reopened_storage
            .draft_piece_operation_status_page(
                &reopened,
                &prepared,
                1,
                std::slice::from_ref(&fragment),
            )
            .is_err()
    );
    assert!(
        reopened_storage
            .prepare_draft_piece_build_advance(
                &reopened,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            )
            .is_err()
    );
}
