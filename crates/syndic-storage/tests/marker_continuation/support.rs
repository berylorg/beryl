use super::*;

pub(super) fn insertion(
    position: DraftCompositePositionV1,
    marker: DraftPieceMarkerV1,
) -> DraftPieceReplacementV1 {
    DraftPieceReplacementV1::new(position, position, vec![DraftPieceV1::Marker(marker)])
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
            DraftPieceMarkerInsertionV1::new(
                position.utf8_offset(),
                marker,
                DraftPieceMarkerEffectChargesV1::for_marker(marker),
            ),
        ))
}

pub(super) fn stage_fragments(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacements: Vec<DraftPieceReplacementV1>,
    successor: DraftCompositePositionV1,
) -> (
    PreparedDraftPieceEditV1,
    Vec<syndic_storage::DraftPieceBuildFragmentV1>,
) {
    let source = if session.newest_root().summary().logical_utf8_bytes() == 0
        && session.newest_root().summary().marker_count() != 0
    {
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll)
    } else {
        point(0)
    };
    let header = DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        source,
        source,
        successor,
        successor,
        replacements.len() as u64,
        canonical_draft_piece_fragment_chain_v1(&replacements),
    );
    let prepared = storage
        .prepare_draft_piece_edit(store, header, session)
        .unwrap();
    committed(execute(
        store,
        storage.begin_draft_piece_edit(storage.revision(store).unwrap(), prepared.clone()),
    ));
    let mut preceding = canonical_empty_draft_piece_fragment_chain_v1();
    let mut fragments = Vec::new();
    for (index, replacement) in replacements.into_iter().enumerate() {
        let builder = storage.unadmitted_marker_builder_for_test();
        let fragment = builder
            .prepare_fragment(&prepared, index as u64 + 1, preceding, replacement)
            .unwrap();
        preceding = fragment.chain_digest();
        committed(execute(
            store,
            builder.stage_fragment(
                storage.revision(store).unwrap(),
                prepared.clone(),
                fragment.clone(),
            ),
        ));
        fragments.push(fragment);
    }
    (prepared, fragments)
}

pub(super) fn advance_all(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedDraftPieceEditV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    for _ in 0..512 {
        let Some(advance) = storage.prepare_draft_piece_build_advance(
            store,
            prepared.header().draft_id(),
            prepared.header().session_id(),
            prepared.header().operation_id(),
        )?
        else {
            return Ok(());
        };
        committed(execute(store, storage.advance_draft_piece_edit(advance)));
    }
    panic!("bounded fixture did not finish");
}
