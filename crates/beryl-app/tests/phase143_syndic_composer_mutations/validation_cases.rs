use super::*;

#[test]
fn malformed_keys_fragments_positions_metadata_gaps_and_overflow_fail_before_admission() {
    let (_home, store, storage, thread) = fixture("phase143-malformed", 31);
    let (mut host, binding) = activated(storage, &store, thread, 32, 33);
    let zero = source_position(0);

    for kind in [MutationKind::Undo, MutationKind::Redo] {
        let request = mutation_request(
            binding,
            34 + u8::from(kind == MutationKind::Redo),
            kind,
            range(zero, zero),
            Vec::new(),
            MutationPositions::collapsed(zero),
            Vec::new(),
        );
        assert!(matches!(
            host.begin_mutation(&store, request),
            Err(ComposerHostError::MutationUnavailable)
        ));
    }

    let terminal_only = mutation_request(
        binding,
        35,
        MutationKind::Edit,
        range(zero, zero),
        Vec::new(),
        MutationPositions::collapsed(zero),
        Vec::new(),
    );
    assert!(matches!(
        host.begin_mutation(&store, terminal_only),
        Err(ComposerHostError::MutationMalformed)
    ));

    let mut wrong_key = text_request(binding, 36, 0, 0, &["x"], 1);
    let foreign_key = MutationKey::new(
        BindingId::new(9),
        SourceRevision::new(9),
        OperationId::new(9),
    );
    wrong_key = ComposerHostMutationRequest::new(
        binding,
        wrong_key.proposal(),
        operation_id(36),
        vec![
            MutationFragment::new(
                foreign_key,
                0,
                MutationFragmentPayload::Utf8 {
                    inserted_offset: 0,
                    text: "x".to_owned(),
                },
            ),
            MutationFragment::new(
                wrong_key.proposal().key(),
                1,
                MutationFragmentPayload::Terminal {
                    intended: MutationPositions::collapsed(source_position(1)),
                },
            ),
        ]
        .into_boxed_slice(),
        Box::new([]),
    );
    assert!(matches!(
        host.begin_mutation(&store, wrong_key),
        Err(ComposerHostError::MutationMalformed)
    ));

    let expected = mutation_key(binding, 60);
    let wrong_binding = MutationKey::new(
        BindingId::new(binding.host_generation().get() + 1),
        expected.base_revision(),
        expected.operation(),
    );
    let wrong_revision = MutationKey::new(
        expected.binding(),
        SourceRevision::new(binding.candidate().candidate_generation() + 1),
        expected.operation(),
    );
    let wrong_operation = MutationKey::new(
        expected.binding(),
        expected.base_revision(),
        OperationId::new(61),
    );
    for key in [wrong_binding, wrong_revision, wrong_operation] {
        assert!(matches!(
            host.begin_mutation(&store, keyed_text_request(binding, key, 60)),
            Err(ComposerHostError::MutationMalformed)
        ));
    }

    let key = mutation_key(binding, 37);
    let proposal = MutationProposal::new(key, MutationKind::Edit, range(zero, zero), 0);
    let terminal_not_last = ComposerHostMutationRequest::new(
        binding,
        proposal,
        operation_id(37),
        vec![
            MutationFragment::new(
                key,
                0,
                MutationFragmentPayload::Terminal {
                    intended: MutationPositions::collapsed(zero),
                },
            ),
            MutationFragment::new(
                key,
                1,
                MutationFragmentPayload::Utf8 {
                    inserted_offset: 0,
                    text: "x".to_owned(),
                },
            ),
        ]
        .into_boxed_slice(),
        Box::new([]),
    );
    assert!(matches!(
        host.begin_mutation(&store, terminal_not_last),
        Err(ComposerHostError::MutationMalformed)
    ));

    let mismatched_positions = mutation_request(
        binding,
        38,
        MutationKind::Edit,
        range(zero, zero),
        vec![MutationFragmentPayload::Utf8 {
            inserted_offset: 0,
            text: "x".to_owned(),
        }],
        MutationPositions::new(source_position(1), zero, zero),
        Vec::new(),
    );
    assert!(matches!(
        host.begin_mutation(&store, mismatched_positions),
        Err(ComposerHostError::MutationMalformed)
    ));

    let id = InlineObjectId::new(40);
    let overflow = SuccessorObject::new(
        id,
        ByteOffset::new(0),
        InlineObjectOrder::new(u128::from(u64::MAX) + 1),
        1,
        1,
    );
    let overflow_request = mutation_request(
        binding,
        39,
        MutationKind::Edit,
        range(zero, zero),
        vec![MutationFragmentPayload::Object(ObjectChange::Insert {
            at: zero,
            object: overflow,
        })],
        MutationPositions::collapsed(zero),
        vec![ComposerHostImageMarkerMetadata::new(
            id,
            ImageLabelOrdinal::FIRST,
        )],
    );
    assert!(matches!(
        host.begin_mutation(&store, overflow_request),
        Err(ComposerHostError::MutationMalformed)
    ));

    let (left, _) = populate(storage, &store, thread, 41);
    let (mut marker_host, marker_binding) = activated(storage, &store, thread, 41, 51);
    let marker_id = inline_id(left.marker_id());
    let marker_order = InlineObjectOrder::new(u128::from(left.order_key()));
    let neighbor = InlineObjectNeighbor::new(marker_id, marker_order);
    let before = SourcePosition::new(ByteOffset::new(3), InlineObjectGap::before(neighbor));
    let after = SourcePosition::new(ByteOffset::new(3), InlineObjectGap::after(neighbor));
    let target = ObjectTarget::new(range(before, after), marker_id, marker_order).unwrap();
    let wrong_id_same_order = SourcePosition::new(
        ByteOffset::new(3),
        InlineObjectGap::before(InlineObjectNeighbor::new(
            InlineObjectId::new(marker_id.get() + 1),
            marker_order,
        )),
    );
    let wrong_anchor = SourcePosition::new(
        ByteOffset::new(2),
        InlineObjectGap::before(InlineObjectNeighbor::new(marker_id, marker_order)),
    );
    for (seed, position) in [(54, wrong_id_same_order), (55, wrong_anchor)] {
        assert!(matches!(
            marker_host.begin_mutation(
                &store,
                mutation_request(
                    marker_binding,
                    seed,
                    MutationKind::Edit,
                    range(position, position),
                    vec![MutationFragmentPayload::Utf8 {
                        inserted_offset: 0,
                        text: "x".to_owned(),
                    }],
                    MutationPositions::collapsed(position),
                    Vec::new(),
                ),
            ),
            Err(ComposerHostError::MutationMalformed)
        ));
        assert_eq!(marker_host.binding(), Some(marker_binding));
        assert_eq!(marker_host.mutation_status(), None);
    }
    let predecessor_text = candidate_text(storage, &store, marker_binding);
    let wrong_translated_terminal = SourcePosition::new(
        ByteOffset::new(4),
        InlineObjectGap::before(InlineObjectNeighbor::new(marker_id, marker_order)),
    );
    assert!(matches!(
        marker_host.begin_mutation(
            &store,
            mutation_request(
                marker_binding,
                56,
                MutationKind::Edit,
                range(source_position(0), source_position(0)),
                vec![MutationFragmentPayload::Utf8 {
                    inserted_offset: 0,
                    text: "xx".to_owned(),
                }],
                MutationPositions::collapsed(wrong_translated_terminal),
                Vec::new(),
            ),
        ),
        Err(ComposerHostError::MutationMalformed)
    ));
    assert_eq!(marker_host.binding(), Some(marker_binding));
    assert_eq!(marker_host.mutation_status(), None);
    assert_eq!(
        candidate_text(storage, &store, marker_binding),
        predecessor_text
    );
    for metadata in [
        Vec::new(),
        vec![ComposerHostImageMarkerMetadata::new(
            marker_id,
            ImageLabelOrdinal::new(left.label().get() + 1).unwrap(),
        )],
    ] {
        let request = mutation_request(
            marker_binding,
            52 + metadata.len() as u8,
            MutationKind::Edit,
            range(before, after),
            vec![MutationFragmentPayload::Object(ObjectChange::Remove {
                target,
            })],
            MutationPositions::collapsed(before),
            metadata,
        );
        assert!(matches!(
            marker_host.begin_mutation(&store, request),
            Err(ComposerHostError::MutationMalformed)
        ));
    }

    let translated_terminal = SourcePosition::new(
        ByteOffset::new(5),
        InlineObjectGap::before(InlineObjectNeighbor::new(marker_id, marker_order)),
    );
    let translated = commit_request(
        &mut marker_host,
        &store,
        mutation_request(
            marker_binding,
            57,
            MutationKind::Edit,
            range(source_position(0), source_position(0)),
            vec![MutationFragmentPayload::Utf8 {
                inserted_offset: 0,
                text: "xx".to_owned(),
            }],
            MutationPositions::collapsed(translated_terminal),
            Vec::new(),
        ),
    );
    let mut expected_text = b"xx".to_vec();
    expected_text.extend_from_slice(&predecessor_text);
    assert_eq!(candidate_text(storage, &store, translated), expected_text);
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                translated.root(),
                DraftPieceMarkerAtV1::new(
                    5,
                    DraftPieceMarkerV1::new(left.marker_id(), left.order_key(), left.label()),
                ),
            )
            .unwrap()
    );
}
