use super::*;

pub(super) fn before(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::BeforeAll)
}

pub(super) fn marker_insertion(
    anchor: u64,
    marker: DraftPieceMarkerV1,
) -> DraftPieceMarkerInsertionV1 {
    DraftPieceMarkerInsertionV1::new(
        anchor,
        marker,
        DraftPieceMarkerEffectChargesV1::for_marker(marker),
    )
}

pub(super) fn insert_at(
    source: DraftCompositePositionV1,
    anchor: u64,
    marker: DraftPieceMarkerV1,
) -> DraftPieceReplacementV1 {
    DraftPieceReplacementV1::new(source, source, vec![DraftPieceV1::Marker(marker)])
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(marker_insertion(
            anchor, marker,
        )))
}

pub(super) fn seeded_text(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    text: &str,
    operation: u8,
) -> DraftEditorCandidateSessionV1 {
    let session = open_session(
        storage,
        store,
        &current(storage, store, thread),
        operation - 2,
        operation - 1,
    );
    let (prepared, _, fragment) = stage_replacement(
        storage,
        store,
        &session,
        operation,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text(text.into())]),
        DraftLogicalExtentV1::new(text.len() as u64, 1),
    );
    complete_measured(storage, store, &prepared, &[fragment]);
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
    active_session(storage, store, session.draft_id(), session.session_id())
}

pub(super) fn stage(
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
        before(0)
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

pub(super) fn complete_checked(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacement: DraftPieceReplacementV1,
    extent: DraftLogicalExtentV1,
) -> DraftEditorCandidateSessionV1 {
    let (prepared, _, fragment) =
        stage_replacement(storage, store, session, operation, replacement, extent);
    complete_measured(storage, store, &prepared, &[fragment]);
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
    active_session(storage, store, session.draft_id(), session.session_id())
}

pub(super) fn observed(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedDraftPieceEditV1,
    fragments: &[syndic_storage::DraftPieceBuildFragmentV1],
) -> syndic_storage::DraftPieceBuildRecordV1 {
    match storage
        .draft_piece_operation_status_page(store, prepared, 1, fragments)
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(
            DraftPieceOperationStatusV1::Open(build) | DraftPieceOperationStatusV1::Complete(build),
        ) => build,
        other => panic!("unexpected mapping build status: {other:?}"),
    }
}

pub(super) fn complete_measured(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedDraftPieceEditV1,
    fragments: &[syndic_storage::DraftPieceBuildFragmentV1],
) -> (syndic_storage::DraftPieceBuildRecordV1, usize) {
    for step in 0..20_000 {
        let before = observed(storage, store, prepared, fragments);
        let source_map =
            syndic_storage::test_faults::draft_build_mapping_snapshot(&before).unwrap();
        let effective = before
            .marker_effect_continuation()
            .active()
            .map_or(before.working_roots(), |active| active.working_roots());
        assert_eq!(
            source_map.current_source_units,
            u128::from(before.predecessor_root().summary().logical_utf8_bytes())
                + u128::from(before.predecessor_root().summary().marker_count())
        );
        assert_eq!(
            source_map.current_target_units,
            u128::from(effective.sequence_summary().logical_utf8_bytes())
                + u128::from(effective.sequence_summary().marker_count())
        );
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                store,
                prepared.header().draft_id(),
                prepared.header().session_id(),
                prepared.header().operation_id(),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "mapping command {step}, frontier {:?}, mapping {source_map:?}: {error:?}",
                    before.frontier()
                )
            })
        else {
            return (observed(storage, store, prepared, fragments), step);
        };
        let measurement = advance.clone();
        committed(execute(store, storage.advance_draft_piece_edit(advance)));
        let after = observed(storage, store, prepared, fragments);
        let target_map = syndic_storage::test_faults::draft_build_mapping_snapshot(&after).unwrap();
        if matches!(source_map.stage_tag, 18 | 19 | 20 | 22 | 23) {
            assert_eq!(before.frontier(), after.frontier());
            assert_eq!(before.working_roots(), after.working_roots());
            assert_eq!(
                source_map.current_source_units,
                target_map.current_source_units
            );
            assert_eq!(
                source_map.current_target_units,
                target_map.current_target_units
            );
            assert_eq!(before.base_frontier(), after.base_frontier());
            assert_eq!(before.successor_frontier(), after.successor_frontier());
            assert_eq!(
                before.marker_effect_continuation().scan(),
                after.marker_effect_continuation().scan()
            );
        }
        if source_map.stage_tag == 20 {
            assert_eq!(target_map.stage_tag, 21);
            assert_eq!(before.next_record_ordinal(), after.next_record_ordinal());
        }
        if matches!(source_map.stage_tag, 22 | 23) {
            assert_eq!(target_map.stage_tag, source_map.stage_tag + 1);
            assert_eq!(before.next_record_ordinal(), after.next_record_ordinal());
        }
        if source_map.stage_tag == 24 {
            assert_eq!(target_map.stage_tag, 0);
            assert_eq!(target_map.fragment_source_end_unit, None);
            assert_eq!(
                target_map.completed_source_unit,
                source_map.fragment_source_end_unit.unwrap()
            );
            assert_eq!(before.next_record_ordinal(), after.next_record_ordinal());
        }
        let work = measurement
            .bounded_work()
            .expect("every mapped command shares the construction ledger");
        assert!(work.stored_structure_records() <= 256, "{work:?}");
        assert!(work.point_attempts() <= 512, "{work:?}");
        assert!(work.encoded_bytes() <= 4_194_304, "{work:?}");
        assert!(work.peak_encoded_bytes() <= 4_194_304, "{work:?}");
    }
    panic!("bounded fixture did not finish");
}

pub(super) fn run_until_error(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedDraftPieceEditV1,
) -> DraftPiecePrepareErrorV1 {
    for _ in 0..2_000 {
        match storage.prepare_draft_piece_build_advance(
            store,
            prepared.header().draft_id(),
            prepared.header().session_id(),
            prepared.header().operation_id(),
        ) {
            Ok(Some(advance)) => {
                committed(execute(store, storage.advance_draft_piece_edit(advance)))
            }
            Ok(None) => panic!("invalid original-copy removal completed"),
            Err(error) => return error,
        }
    }
    panic!("bounded fixture did not reject");
}

pub(super) fn read_text(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    bytes: usize,
) -> String {
    String::from_utf8(
        storage
            .draft_piece_text_demand(
                store,
                root,
                syndic_storage::DraftPieceTextDemandV1::Forward(0),
                bytes,
            )
            .unwrap()
            .bytes()
            .to_vec(),
    )
    .unwrap()
}
