use super::*;

#[test]
fn marker_insert_move_and_remove_authenticate_exact_identity_label_order_and_location() {
    let (_home, store, storage, thread) = fixture("phase143-markers", 11);
    let (mut host, base) = activated(storage, &store, thread, 12, 13);
    let id_value = u128::from_be_bytes([0x80, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 0xfe]);
    let id = InlineObjectId::new(id_value);
    let order_one = InlineObjectOrder::new(1);
    let object = SuccessorObject::new(id, ByteOffset::new(1), order_one, 17, 5);
    let zero = source_position(0);
    let one = source_position(1);
    let text_base = commit_request(&mut host, &store, text_request(base, 14, 0, 0, &["x"], 1));
    let after_one = SourcePosition::new(
        ByteOffset::new(1),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order_one)),
    );
    let inserted = commit_request(
        &mut host,
        &store,
        mutation_request(
            text_base,
            15,
            MutationKind::Edit,
            range(one, one),
            vec![MutationFragmentPayload::Object(ObjectChange::Insert {
                at: one,
                object,
            })],
            MutationPositions::collapsed(after_one),
            vec![ComposerHostImageMarkerMetadata::new(
                id,
                ImageLabelOrdinal::new(9).unwrap(),
            )],
        ),
    );
    let marker_id = SyndicDraftMarkerId::from_bytes(id_value.to_be_bytes());
    let stored = storage
        .draft_marker_identity(&store, inserted.root(), marker_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.marker_id(), marker_id);
    assert_eq!(stored.order_key(), 1);
    assert_eq!(stored.label(), ImageLabelOrdinal::new(9).unwrap());
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                inserted.root(),
                DraftPieceMarkerAtV1::new(
                    1,
                    DraftPieceMarkerV1::new(
                        stored.marker_id(),
                        stored.order_key(),
                        stored.label(),
                    ),
                ),
            )
            .unwrap()
    );

    let before_one = SourcePosition::new(
        ByteOffset::new(1),
        InlineObjectGap::before(InlineObjectNeighbor::new(id, order_one)),
    );
    let target = ObjectTarget::new(range(before_one, after_one), id, order_one).unwrap();
    let order_two = InlineObjectOrder::new(2);
    let moved_object = SuccessorObject::new(id, ByteOffset::new(0), order_two, 17, 5);
    let after_two = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order_two)),
    );
    let moved = commit_request(
        &mut host,
        &store,
        mutation_request(
            inserted,
            16,
            MutationKind::Edit,
            range(zero, after_one),
            vec![
                MutationFragmentPayload::Object(ObjectChange::Move {
                    target,
                    to: zero,
                    object: moved_object,
                }),
                MutationFragmentPayload::Utf8 {
                    inserted_offset: 0,
                    text: "x".to_owned(),
                },
            ],
            MutationPositions::collapsed(after_two),
            vec![ComposerHostImageMarkerMetadata::new(
                id,
                ImageLabelOrdinal::new(9).unwrap(),
            )],
        ),
    );
    assert_eq!(candidate_text(storage, &store, moved), b"x");
    let moved_marker = DraftPieceMarkerV1::new(marker_id, 2, ImageLabelOrdinal::new(9).unwrap());
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                moved.root(),
                DraftPieceMarkerAtV1::new(0, moved_marker),
            )
            .unwrap()
    );

    let before_two = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::before(InlineObjectNeighbor::new(id, order_two)),
    );
    let after_two = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(id, order_two)),
    );
    let removed = commit_request(
        &mut host,
        &store,
        mutation_request(
            moved,
            17,
            MutationKind::Edit,
            range(before_two, after_two),
            vec![MutationFragmentPayload::Object(ObjectChange::Remove {
                target: ObjectTarget::new(range(before_two, after_two), id, order_two).unwrap(),
            })],
            MutationPositions::collapsed(source_position(0)),
            vec![ComposerHostImageMarkerMetadata::new(
                id,
                ImageLabelOrdinal::new(9).unwrap(),
            )],
        ),
    );
    assert_eq!(candidate_text(storage, &store, removed), b"x");
    assert_eq!(
        storage
            .draft_marker_identity(&store, removed.root(), marker_id)
            .unwrap(),
        None
    );
}

#[test]
fn named_marker_edges_target_envelopes_and_move_pairing_are_exact() {
    let (_home, store, storage, thread) = fixture("phase143-marker-edges", 17);
    let (mut host, base) = activated(storage, &store, thread, 18, 19);
    let left_id = InlineObjectId::new(0x101);
    let right_id = InlineObjectId::new(0x202);
    let left_order = InlineObjectOrder::new(1);
    let right_order = InlineObjectOrder::new(2);
    let zero = source_position(0);
    let after_right = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(right_id, right_order)),
    );
    let with_markers = commit_request(
        &mut host,
        &store,
        mutation_request(
            base,
            20,
            MutationKind::Edit,
            range(zero, zero),
            vec![
                MutationFragmentPayload::Object(ObjectChange::Insert {
                    at: zero,
                    object: SuccessorObject::new(left_id, ByteOffset::new(0), left_order, 1, 1),
                }),
                MutationFragmentPayload::Object(ObjectChange::Insert {
                    at: zero,
                    object: SuccessorObject::new(right_id, ByteOffset::new(0), right_order, 1, 1),
                }),
            ],
            MutationPositions::collapsed(after_right),
            vec![
                ComposerHostImageMarkerMetadata::new(left_id, ImageLabelOrdinal::new(1).unwrap()),
                ComposerHostImageMarkerMetadata::new(right_id, ImageLabelOrdinal::new(2).unwrap()),
            ],
        ),
    );

    let wrong_before_right = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::before(InlineObjectNeighbor::new(right_id, right_order)),
    );
    assert!(matches!(
        host.begin_mutation(
            &store,
            mutation_request(
                with_markers,
                21,
                MutationKind::Edit,
                range(wrong_before_right, wrong_before_right),
                vec![MutationFragmentPayload::Utf8 {
                    inserted_offset: 0,
                    text: "wrong".to_owned(),
                }],
                MutationPositions::collapsed(after_right),
                Vec::new(),
            )
        ),
        Err(ComposerHostError::MutationMalformed)
    ));

    let before_left = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::before(InlineObjectNeighbor::new(left_id, left_order)),
    );
    let between = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::between(
            InlineObjectNeighbor::new(left_id, left_order),
            InlineObjectNeighbor::new(right_id, right_order),
        )
        .unwrap(),
    );
    let left_target = ObjectTarget::new(range(before_left, between), left_id, left_order).unwrap();

    let wrong_gap_id = InlineObjectId::new(0x303);
    assert!(matches!(
        host.begin_mutation(
            &store,
            mutation_request(
                with_markers,
                23,
                MutationKind::Edit,
                range(zero, zero),
                vec![MutationFragmentPayload::Object(ObjectChange::Insert {
                    at: zero,
                    object: SuccessorObject::new(
                        wrong_gap_id,
                        ByteOffset::new(0),
                        InlineObjectOrder::new(3),
                        1,
                        1,
                    ),
                })],
                MutationPositions::collapsed(zero),
                vec![ComposerHostImageMarkerMetadata::new(
                    wrong_gap_id,
                    ImageLabelOrdinal::new(3).unwrap(),
                )],
            ),
        ),
        Err(ComposerHostError::MutationMalformed)
    ));

    let moved_left = SuccessorObject::new(left_id, ByteOffset::new(0), left_order, 1, 1);
    assert!(matches!(
        host.begin_mutation(
            &store,
            mutation_request(
                with_markers,
                24,
                MutationKind::Edit,
                range(between, after_right),
                vec![MutationFragmentPayload::Object(ObjectChange::Move {
                    target: left_target,
                    to: after_right,
                    object: moved_left,
                })],
                MutationPositions::collapsed(after_right),
                vec![ComposerHostImageMarkerMetadata::new(
                    left_id,
                    ImageLabelOrdinal::new(1).unwrap(),
                )],
            ),
        ),
        Err(ComposerHostError::MutationMalformed)
    ));
    assert!(matches!(
        host.begin_mutation(
            &store,
            mutation_request(
                with_markers,
                25,
                MutationKind::Edit,
                range(before_left, between),
                vec![MutationFragmentPayload::Object(ObjectChange::Move {
                    target: left_target,
                    to: after_right,
                    object: moved_left,
                })],
                MutationPositions::collapsed(after_right),
                vec![ComposerHostImageMarkerMetadata::new(
                    left_id,
                    ImageLabelOrdinal::new(1).unwrap(),
                )],
            ),
        ),
        Err(ComposerHostError::MutationMalformed)
    ));

    let wrong_edge_id = InlineObjectId::new(0x404);
    let before_wrong_edge = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::before(InlineObjectNeighbor::new(
            wrong_edge_id,
            InlineObjectOrder::new(3),
        )),
    );
    assert!(matches!(
        host.begin_mutation(
            &store,
            mutation_request(
                with_markers,
                26,
                MutationKind::Edit,
                range(after_right, after_right),
                vec![MutationFragmentPayload::Object(ObjectChange::Insert {
                    at: after_right,
                    object: SuccessorObject::new(
                        wrong_edge_id,
                        ByteOffset::new(0),
                        InlineObjectOrder::new(3),
                        1,
                        1,
                    ),
                })],
                MutationPositions::collapsed(before_wrong_edge),
                vec![ComposerHostImageMarkerMetadata::new(
                    wrong_edge_id,
                    ImageLabelOrdinal::new(3).unwrap(),
                )],
            ),
        ),
        Err(ComposerHostError::MutationMalformed)
    ));
    assert_eq!(host.binding(), Some(with_markers));

    assert!(matches!(
        host.begin_mutation(
            &store,
            mutation_request(
                with_markers,
                22,
                MutationKind::Edit,
                range(between, after_right),
                vec![MutationFragmentPayload::Object(ObjectChange::Remove {
                    target: left_target,
                })],
                MutationPositions::collapsed(between),
                vec![ComposerHostImageMarkerMetadata::new(
                    left_id,
                    ImageLabelOrdinal::new(1).unwrap(),
                )],
            )
        ),
        Err(ComposerHostError::MutationMalformed)
    ));

    let collision_id = InlineObjectId::new(0x505);
    assert!(matches!(
        host.begin_mutation(
            &store,
            mutation_request(
                with_markers,
                20,
                MutationKind::Edit,
                range(after_right, after_right),
                vec![MutationFragmentPayload::Object(ObjectChange::Insert {
                    at: after_right,
                    object: SuccessorObject::new(
                        collision_id,
                        ByteOffset::new(0),
                        InlineObjectOrder::new(3),
                        1,
                        1,
                    ),
                })],
                MutationPositions::collapsed(SourcePosition::new(
                    ByteOffset::new(0),
                    InlineObjectGap::after(InlineObjectNeighbor::new(
                        collision_id,
                        InlineObjectOrder::new(3),
                    )),
                )),
                vec![ComposerHostImageMarkerMetadata::new(
                    collision_id,
                    ImageLabelOrdinal::new(3).unwrap(),
                )],
            ),
        ),
        Err(ComposerHostError::MutationIdentityCollision)
    ));
    assert_eq!(host.binding(), Some(with_markers));
}
