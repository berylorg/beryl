#[test]
fn staged_marker_effects_derive_current_placement_and_close_identity_collisions() {
    let (home, store, storage, thread) = fixture("marker-effects", 1);
    let current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &current, 3, 4);

    let text = DraftPieceReplacementV1::new(
        point(0),
        point(0),
        vec![DraftPieceV1::Text("abc".to_owned())],
    );
    session = complete_staged(
        &storage,
        &store,
        &session,
        5,
        text,
        DraftLogicalExtentV1::new(3, 1),
    );

    let original = marker(10, 7, 9);
    let insert =
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(original)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    original,
                    DraftPieceMarkerEffectChargesV1::for_marker(original),
                ),
            ));
    session = complete_staged(
        &storage,
        &store,
        &session,
        6,
        insert,
        DraftLogicalExtentV1::new(3, 1),
    );
    let accepted_anchors: Vec<_> = (0..=3)
        .filter(|anchor| {
            storage
                .validate_draft_marker_location(
                    &store,
                    session.newest_root(),
                    DraftPieceMarkerAtV1::new(*anchor, original),
                )
                .unwrap()
        })
        .collect();
    assert_eq!(accepted_anchors, vec![1]);

    let occupied_order_marker = marker(11, 7, 9);
    let occupied_order_position =
        DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    let occupied_order = DraftPieceReplacementV1::new(
        occupied_order_position,
        occupied_order_position,
        vec![DraftPieceV1::Marker(occupied_order_marker)],
    )
    .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
        DraftPieceMarkerInsertionV1::new(
            1,
            occupied_order_marker,
            DraftPieceMarkerEffectChargesV1::for_marker(occupied_order_marker),
        ),
    ));
    let (prepared, identity, _) = stage_replacement(
        &storage,
        &store,
        &session,
        12,
        occupied_order,
        DraftLogicalExtentV1::new(3, 1),
    );
    let error = advance_error(&storage, &store, identity);
    assert!(
        matches!(
            error,
            DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::DuplicateMarkerOrder)
        ),
        "unexpected occupied-order outcome: {error:?}"
    );
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());

    let future = marker(12, 9, 9);
    let future_anchor =
        DraftPieceReplacementV1::new(point(3), point(3), vec![DraftPieceV1::Marker(future)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    4,
                    future,
                    DraftPieceMarkerEffectChargesV1::for_marker(future),
                ),
            ));
    let (prepared, identity, _) = stage_replacement(
        &storage,
        &store,
        &session,
        13,
        future_anchor,
        DraftLogicalExtentV1::new(3, 1),
    );
    assert!(matches!(
        advance_error(&storage, &store, identity),
        DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::OutOfOrder)
    ));
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());

    let wrong_charge_marker = marker(13, 9, 9);
    let wrong_charge = DraftPieceReplacementV1::new(
        point(2),
        point(2),
        vec![DraftPieceV1::Marker(wrong_charge_marker)],
    )
    .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
        DraftPieceMarkerInsertionV1::new(
            2,
            wrong_charge_marker,
            DraftPieceMarkerEffectChargesV1::new(0, 1, 120),
        ),
    ));
    let (prepared, identity, _) = stage_replacement(
        &storage,
        &store,
        &session,
        14,
        wrong_charge,
        DraftLogicalExtentV1::new(3, 1),
    );
    let error = advance_error(&storage, &store, identity);
    assert!(
        matches!(error, DraftPiecePrepareErrorV1::InvalidRoot),
        "unexpected charge-mismatch outcome: {error:?}"
    );
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());

    let repeated =
        DraftPieceReplacementV1::new(point(2), point(2), vec![DraftPieceV1::Marker(original)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    2,
                    original,
                    DraftPieceMarkerEffectChargesV1::for_marker(original),
                ),
            ));
    let (prepared, identity, _) = stage_replacement(
        &storage,
        &store,
        &session,
        7,
        repeated,
        DraftLogicalExtentV1::new(3, 1),
    );
    assert!(matches!(
        advance_error(&storage, &store, identity),
        DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::DuplicateMarkerIdentity)
    ));
    committed(execute(
        &store,
        storage.reject_draft_piece_edit(
            storage.revision(&store).unwrap(),
            prepared,
            DraftPieceRejectedReasonV1::DuplicateMarkerIdentity,
        ),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), original.marker_id())
        .unwrap()
        .unwrap();

    let removal = DraftPieceMarkerRemovalProofV1::new(
        DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll),
        occurrence,
    );
    let moved =
        DraftPieceReplacementV1::new(point(2), point(2), vec![DraftPieceV1::Marker(original)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Move {
                removal,
                insertion: DraftPieceMarkerInsertionV1::new(
                    2,
                    original,
                    DraftPieceMarkerEffectChargesV1::for_marker(original),
                ),
            });
    let (prepared, identity, fragment) = stage_replacement(
        &storage,
        &store,
        &session,
        8,
        moved,
        DraftLogicalExtentV1::new(3, 1),
    );
    let source_roots = open_build(&storage, &store, &prepared, &fragment).working_roots();
    loop {
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
        let build = open_build(&storage, &store, &prepared, &fragment);
        if let Some(pending) = build.marker_effect_continuation().active() {
            assert_eq!(build.working_roots(), source_roots);
            assert_eq!(pending.source_roots(), source_roots);
            assert_ne!(pending.working_roots(), source_roots);
            break;
        }
    }
    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let reopened = open_build(&storage, &store, &prepared, &fragment);
    let pending = reopened.marker_effect_continuation().active().unwrap();
    assert_eq!(reopened.working_roots(), source_roots);
    assert_ne!(pending.working_roots(), source_roots);
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )
        .unwrap()
    {
        committed(execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        ));
    }
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());
    let moved_occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), original.marker_id())
        .unwrap()
        .unwrap();
    assert_ne!(
        moved_occurrence.sequence_leaf_id(),
        occurrence.sequence_leaf_id()
    );
    let moved_anchors: Vec<_> = (0..=3)
        .filter(|anchor| {
            storage
                .validate_draft_marker_location(
                    &store,
                    session.newest_root(),
                    DraftPieceMarkerAtV1::new(*anchor, original),
                )
                .unwrap()
        })
        .collect();
    assert_eq!(moved_anchors, vec![2]);

    let relabeled_order = marker(10, 8, 9);
    let before_marker_at_two =
        DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll);
    let replacement = DraftPieceReplacementV1::new(
        before_marker_at_two,
        before_marker_at_two,
        vec![DraftPieceV1::Marker(relabeled_order)],
    )
    .with_marker_effect(DraftPieceMarkerEffectV1::SameIdReplacement {
        removal: DraftPieceMarkerRemovalProofV1::new(
            DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
            moved_occurrence,
        ),
        insertion: DraftPieceMarkerInsertionV1::new(
            2,
            relabeled_order,
            DraftPieceMarkerEffectChargesV1::for_marker(relabeled_order),
        ),
    });
    session = complete_staged(
        &storage,
        &store,
        &session,
        9,
        replacement,
        DraftLogicalExtentV1::new(3, 1),
    );
    let final_occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), original.marker_id())
        .unwrap()
        .unwrap();
    assert_eq!(final_occurrence.order_key(), 8);

    let removal =
        DraftPieceReplacementV1::new(before_marker_at_two, before_marker_at_two, Vec::new())
            .with_marker_effect(DraftPieceMarkerEffectV1::Remove {
                removal: DraftPieceMarkerRemovalProofV1::new(
                    DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
                    final_occurrence,
                ),
                charges: DraftPieceMarkerEffectChargesV1::for_marker(relabeled_order),
            });
    session = complete_staged(
        &storage,
        &store,
        &session,
        11,
        removal,
        DraftLogicalExtentV1::new(3, 1),
    );
    assert!(
        storage
            .draft_marker_identity(&store, session.newest_root(), original.marker_id())
            .unwrap()
            .is_none()
    );
}
