#[test]
fn more_than_256_marker_effects_use_one_fixed_continuation() {
    const EFFECTS: u64 = 257;
    let (_home, store, storage, thread) = fixture("many-marker-effects", 140);
    let current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &current, 141, 142);
    session = complete_staged(
        &storage,
        &store,
        &session,
        143,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("x".repeat(EFFECTS as usize))],
        ),
        DraftLogicalExtentV1::new(EFFECTS, 1),
    );
    let mut replacements = Vec::with_capacity(EFFECTS as usize);
    let mut first = None;
    let mut last = None;
    for ordinal in 0..EFFECTS {
        let mut id = [0u8; 16];
        id[..8].copy_from_slice(&ordinal.to_be_bytes());
        id[8..].copy_from_slice(&(ordinal + 1).to_be_bytes());
        let marker = DraftPieceMarkerV1::new(
            SyndicDraftMarkerId::from_bytes(id),
            ordinal + 1,
            ImageLabelOrdinal::new(ordinal + 1).unwrap(),
            beryl_model::AssetId::sha256_v1(
                [ordinal as u8; 32],
                std::num::NonZeroU64::new(ordinal + 1).unwrap(),
            ),
        );
        first.get_or_insert(marker);
        last = Some(marker);
        replacements.push(
            DraftPieceReplacementV1::new(
                point(ordinal),
                point(ordinal),
                vec![DraftPieceV1::Marker(marker)],
            )
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    ordinal,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
        );
    }
    let chain = canonical_draft_piece_fragment_chain_v1(&replacements);
    let successor_position =
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll);
    let header = DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        DraftPieceOperationIdV1::from_bytes([144; 16]),
        point(0),
        point(0),
        successor_position,
        successor_position,
        EFFECTS,
        chain,
    );
    let prepared = storage
        .prepare_draft_piece_edit(&store, header, &session)
        .unwrap();
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), prepared.clone()),
    ));
    let mut preceding = canonical_empty_draft_piece_fragment_chain_v1();
    for (index, replacement) in replacements.into_iter().enumerate() {
        let fragment = storage
            .prepare_draft_piece_fragment(&prepared, index as u64 + 1, preceding, replacement)
            .unwrap();
        preceding = fragment.chain_digest();
        committed(execute(
            &store,
            storage.stage_draft_piece_fragment(
                storage.revision(&store).unwrap(),
                prepared.clone(),
                fragment,
            ),
        ));
    }
    let mut advances = 0_u64;
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            prepared.header().operation_id(),
        )
        .unwrap_or_else(|error| panic!("advance {advances} failed: {error:?}"))
    {
        advances += 1;
        assert!(advances < 10_000);
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
    assert_eq!(session.newest_root().summary().marker_count(), EFFECTS);
    for marker in [first.unwrap(), last.unwrap()] {
        assert!(
            storage
                .draft_marker_identity(&store, session.newest_root(), marker.marker_id())
                .unwrap()
                .is_some()
        );
    }
    assert!(std::mem::size_of::<DraftPieceMarkerEffectContinuationV1>() < 4_096);
}
