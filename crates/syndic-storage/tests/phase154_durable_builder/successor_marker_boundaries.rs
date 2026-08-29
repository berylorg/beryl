#[test]
fn rightward_move_preserves_following_frontier_across_restart() {
    let (home, store, storage, thread) = fixture("rightward-move-frontier", 130);
    let current = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &current, 131, 132);
    session = complete_staged(
        &storage,
        &store,
        &session,
        133,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abc".to_owned())],
        ),
        DraftLogicalExtentV1::new(3, 1),
    );
    let moved = marker(134, 7, 9);
    session = complete_staged(
        &storage,
        &store,
        &session,
        135,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(moved)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    moved,
                    DraftPieceMarkerEffectChargesV1::for_marker(moved),
                ),
            )),
        DraftLogicalExtentV1::new(3, 1),
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), moved.marker_id())
        .unwrap()
        .unwrap();
    let replacements = vec![
        DraftPieceReplacementV1::new(point(2), point(2), vec![DraftPieceV1::Marker(moved)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Move {
                removal: DraftPieceMarkerRemovalProofV1::new(
                    DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll),
                    occurrence,
                ),
                insertion: DraftPieceMarkerInsertionV1::new(
                    2,
                    moved,
                    DraftPieceMarkerEffectChargesV1::for_marker(moved),
                ),
            }),
        DraftPieceReplacementV1::new(point(3), point(3), vec![DraftPieceV1::Text("!".to_owned())]),
    ];
    let (prepared, identity, fragments) = stage_interleaved_replacements(
        &storage,
        &store,
        &session,
        136,
        replacements,
        DraftLogicalExtentV1::new(4, 1),
        point(0),
    );
    while open_build_fragments(&storage, &store, &prepared, &fragments)
        .marker_effect_continuation()
        .active()
        .is_none()
    {
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
    }
    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
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
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                session.newest_root(),
                DraftPieceMarkerAtV1::new(2, moved),
            )
            .unwrap()
    );
    assert_eq!(
        storage
            .draft_piece_text_demand(
                &store,
                session.newest_root(),
                syndic_storage::DraftPieceTextDemandV1::Forward(0),
                64,
            )
            .unwrap()
            .bytes(),
        b"abc!"
    );
}

#[test]
fn same_id_replacement_preserves_following_frontier_across_restart() {
    let (home, store, storage, thread) = fixture("same-id-frontier", 140);
    let current = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &current, 141, 142);
    session = complete_staged(
        &storage,
        &store,
        &session,
        143,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abc".to_owned())],
        ),
        DraftLogicalExtentV1::new(3, 1),
    );
    let original = marker(144, 7, 9);
    session = complete_staged(
        &storage,
        &store,
        &session,
        145,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(original)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    original,
                    DraftPieceMarkerEffectChargesV1::for_marker(original),
                ),
            )),
        DraftLogicalExtentV1::new(3, 1),
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), original.marker_id())
        .unwrap()
        .unwrap();
    let replacement = marker(144, 8, 9);
    let before = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    let replacements = vec![
        DraftPieceReplacementV1::new(before, before, vec![DraftPieceV1::Marker(replacement)])
            .with_marker_effect(DraftPieceMarkerEffectV1::SameIdReplacement {
                removal: DraftPieceMarkerRemovalProofV1::new(before, occurrence),
                insertion: DraftPieceMarkerInsertionV1::new(
                    1,
                    replacement,
                    DraftPieceMarkerEffectChargesV1::for_marker(replacement),
                ),
            }),
        DraftPieceReplacementV1::new(point(3), point(3), vec![DraftPieceV1::Text("!".to_owned())]),
    ];
    let (prepared, identity, fragments) = stage_interleaved_replacements(
        &storage,
        &store,
        &session,
        146,
        replacements,
        DraftLogicalExtentV1::new(4, 1),
        point(0),
    );
    while open_build_fragments(&storage, &store, &prepared, &fragments)
        .marker_effect_continuation()
        .active()
        .is_none()
    {
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
    }
    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
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
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                session.newest_root(),
                DraftPieceMarkerAtV1::new(1, replacement),
            )
            .unwrap()
    );
    assert_eq!(
        storage
            .draft_piece_text_demand(
                &store,
                session.newest_root(),
                syndic_storage::DraftPieceTextDemandV1::Forward(0),
                64,
            )
            .unwrap()
            .bytes(),
        b"abc!"
    );
}

#[test]
fn earlier_inner_anchor_preserves_later_inner_frontier() {
    let (_home, store, storage, thread) = fixture("earlier-inner-frontier", 150);
    let current = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &current, 151, 152);
    session = complete_staged(
        &storage,
        &store,
        &session,
        153,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abcd".to_owned())],
        ),
        DraftLogicalExtentV1::new(4, 1),
    );
    let inserted = marker(154, 7, 9);
    let replacements = vec![
        DraftPieceReplacementV1::new(point(3), point(3), vec![DraftPieceV1::Marker(inserted)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    inserted,
                    DraftPieceMarkerEffectChargesV1::for_marker(inserted),
                ),
            )),
        DraftPieceReplacementV1::new(point(4), point(4), vec![DraftPieceV1::Text("!".to_owned())]),
    ];
    let (prepared, identity, _) = stage_interleaved_replacements(
        &storage,
        &store,
        &session,
        155,
        replacements,
        DraftLogicalExtentV1::new(5, 1),
        point(0),
    );
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
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                session.newest_root(),
                DraftPieceMarkerAtV1::new(1, inserted),
            )
            .unwrap()
    );
    assert_eq!(
        storage
            .draft_piece_text_demand(
                &store,
                session.newest_root(),
                syndic_storage::DraftPieceTextDemandV1::Forward(0),
                64,
            )
            .unwrap()
            .bytes(),
        b"abcd!"
    );
}

#[test]
fn in_tree_anchor_after_physical_frontier_rejects() {
    let (_home, store, storage, thread) = fixture("ahead-of-frontier", 160);
    let current = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &current, 161, 162);
    session = complete_staged(
        &storage,
        &store,
        &session,
        163,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abc".to_owned())],
        ),
        DraftLogicalExtentV1::new(3, 1),
    );
    let inserted = marker(164, 7, 9);
    let replacement =
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(inserted)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    2,
                    inserted,
                    DraftPieceMarkerEffectChargesV1::for_marker(inserted),
                ),
            ));
    let (prepared, identity, _) = stage_replacement(
        &storage,
        &store,
        &session,
        165,
        replacement,
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
}
