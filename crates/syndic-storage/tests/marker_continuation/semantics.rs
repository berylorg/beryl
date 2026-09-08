use super::{continuation_support::*, *};

#[test]
fn equal_order_key_at_later_anchor_is_valid_but_same_anchor_is_occupied() {
    let (_home, store, storage, thread) = fixture("marker-order-anchor", 21);
    let mut session = open_session(&storage, &store, &current(&storage, &store, thread), 23, 24);
    session = complete_staged(
        &storage,
        &store,
        &session,
        25,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("abc".into())]),
        DraftLogicalExtentV1::new(3, 1),
    );
    let early = marker(31, 2, 1);
    let later = marker(32, 7, 2);
    for (op, anchor, item) in [(26, 1, early), (27, 2, later)] {
        session = complete_staged(
            &storage,
            &store,
            &session,
            op,
            insertion(point(anchor), item),
            DraftLogicalExtentV1::new(3, 1),
        );
    }
    let before = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    let after = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll);
    let inserted = marker(33, 7, 3);
    session = complete_staged(
        &storage,
        &store,
        &session,
        28,
        insertion(after, inserted),
        DraftLogicalExtentV1::new(3, 1),
    );
    for (anchor, item) in [(1, early), (1, inserted), (2, later)] {
        assert!(
            storage
                .validate_draft_marker_location(
                    &store,
                    session.newest_root(),
                    DraftPieceMarkerAtV1::new(anchor, item)
                )
                .unwrap()
        );
    }
    let original_root = session.newest_root();
    let (prepared, identity, _) = stage_replacement(
        &storage,
        &store,
        &session,
        29,
        insertion(before, marker(34, 7, 4)),
        DraftLogicalExtentV1::new(3, 1),
    );
    assert!(matches!(
        advance_error(&storage, &store, identity),
        DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::DuplicateMarkerOrder)
    ));
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    assert_eq!(
        active_session(&storage, &store, session.draft_id(), session.session_id()).newest_root(),
        original_root
    );
}

#[test]
fn removed_stable_identity_cannot_be_reinserted_after_an_ordinary_fragment() {
    let (_home, store, storage, thread) = fixture("marker-original-identity", 41);
    let mut session = open_session(&storage, &store, &current(&storage, &store, thread), 43, 44);
    session = complete_staged(
        &storage,
        &store,
        &session,
        45,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("abcd".into())]),
        DraftLogicalExtentV1::new(4, 1),
    );
    let item = marker(51, 7, 1);
    session = complete_staged(
        &storage,
        &store,
        &session,
        46,
        insertion(point(1), item),
        DraftLogicalExtentV1::new(4, 1),
    );
    let original_root = session.newest_root();
    let occurrence = storage
        .draft_marker_identity(&store, original_root, item.marker_id())
        .unwrap()
        .unwrap();
    let before = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    let remove = DraftPieceReplacementV1::new(before, before, vec![]).with_marker_effect(
        DraftPieceMarkerEffectV1::Remove {
            removal: DraftPieceMarkerRemovalProofV1::new(before, occurrence),
            charges: DraftPieceMarkerEffectChargesV1::for_marker(item),
        },
    );
    let ordinary =
        DraftPieceReplacementV1::new(point(2), point(3), vec![DraftPieceV1::Text("x".into())]);
    let (prepared, _) = stage_fragments(
        &storage,
        &store,
        &session,
        47,
        vec![remove, ordinary, insertion(point(4), item)],
        point(0),
    );
    assert!(matches!(
        advance_all(&storage, &store, &prepared),
        Err(DraftPiecePrepareErrorV1::Rejected(
            DraftPieceRejectedReasonV1::DuplicateMarkerIdentity
        ))
    ));
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    let restored = active_session(&storage, &store, session.draft_id(), session.session_id());
    assert_eq!(restored.newest_root(), original_root);
    assert_eq!(
        storage
            .draft_marker_identity(&store, restored.newest_root(), item.marker_id())
            .unwrap(),
        Some(occurrence)
    );
}
