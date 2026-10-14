use super::*;
use syndic_storage::test_faults::draft_marker_program_snapshot_for_test;

#[derive(Clone, Copy, Debug)]
enum CancellationBoundary {
    Entered,
    MarkerMap,
    PendingSequence,
    PendingIdentity,
    PendingOrder,
    PrivateRetirement,
    PrefixCertified,
    PrefixMap,
    PrefixReady,
    RefreshMap,
    RefreshSequence,
    PublishReady,
}

fn at_boundary(build: &DraftPieceBuildRecordV1, boundary: CancellationBoundary) -> bool {
    let deferred = matches!(
        build.frontier(),
        DraftPieceBuildFrontierV1::Applying {
            fragment_ordinal: 1,
            ..
        } | DraftPieceBuildFrontierV1::Inserting {
            fragment_ordinal: 1,
            ..
        }
    ) && build
        .marker_effect_continuation()
        .scan()
        .next_fragment_ordinal()
        > 1;
    if !deferred {
        return false;
    }
    let map = draft_build_mapping_snapshot(build).unwrap();
    let active = build.marker_effect_continuation().active();
    let program = draft_marker_program_snapshot_for_test(build);
    match boundary {
        CancellationBoundary::Entered => {
            active.is_none()
                && build
                    .marker_effect_continuation()
                    .scan()
                    .next_fragment_ordinal()
                    == 2
        }
        CancellationBoundary::MarkerMap => {
            map.stage_tag == 18
                && active
                    .is_some_and(|active| active.phase() == DraftPieceActiveMarkerPhaseV1::Removing)
        }
        CancellationBoundary::PendingSequence => {
            program.is_some_and(|program| program.pending == 2)
        }
        CancellationBoundary::PendingIdentity => {
            program.is_some_and(|program| program.pending == 3)
        }
        CancellationBoundary::PendingOrder => program.is_some_and(|program| program.pending == 4),
        CancellationBoundary::PrivateRetirement => {
            active.is_none()
                && build
                    .marker_effect_continuation()
                    .scan()
                    .next_fragment_ordinal()
                    > 2
        }
        CancellationBoundary::PrefixCertified => {
            map.stage_tag == 0
                && active.is_some_and(|active| {
                    active.phase() == DraftPieceActiveMarkerPhaseV1::DerivingInsertionGap
                })
        }
        CancellationBoundary::PrefixMap => {
            map.stage_tag == 18
                && active.is_some_and(|active| {
                    active.phase() == DraftPieceActiveMarkerPhaseV1::DerivingInsertionGap
                })
        }
        CancellationBoundary::PrefixReady => {
            map.stage_tag == 21
                && active.is_some_and(|active| {
                    active.phase() == DraftPieceActiveMarkerPhaseV1::DerivingInsertionGap
                })
        }
        CancellationBoundary::RefreshMap => map.stage_tag == 22,
        CancellationBoundary::RefreshSequence => map.stage_tag == 23,
        CancellationBoundary::PublishReady => map.stage_tag == 24,
    }
}

#[test]
fn cancellation_at_each_deferred_prefix_boundary_preserves_candidate_and_reopened_terminal() {
    for boundary in [
        CancellationBoundary::Entered,
        CancellationBoundary::MarkerMap,
        CancellationBoundary::PendingSequence,
        CancellationBoundary::PendingIdentity,
        CancellationBoundary::PendingOrder,
        CancellationBoundary::PrivateRetirement,
        CancellationBoundary::PrefixCertified,
        CancellationBoundary::PrefixMap,
        CancellationBoundary::PrefixReady,
        CancellationBoundary::RefreshMap,
        CancellationBoundary::RefreshSequence,
        CancellationBoundary::PublishReady,
    ] {
        let (home, store, storage, session, selected) = seed(3);
        let (prepared, fragments) = deletion(&storage, &store, &session, &selected);
        let mut reached = false;
        for _ in 0..512 {
            let build = build_status(&storage, &store, &prepared, &fragments);
            if at_boundary(&build, boundary) {
                reached = true;
                break;
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
        }
        assert!(reached, "missing cancellation boundary {boundary:?}");
        committed(execute(
            &store,
            storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared.clone()),
        ));
        drop(store);
        let mut candidate = beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
            &home.0,
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(
            active_session(&storage, &store, session.draft_id(), session.session_id())
                .newest_root(),
            session.newest_root()
        );
        assert!(
            matches!(storage.draft_piece_operation_status_page(&store, &prepared, 1, &fragments).unwrap(),
            DraftPieceOperationVerificationV1::Status(DraftPieceOperationStatusV1::Settled(settlement))
            if settlement.outcome() == &syndic_storage::DraftPieceSettlementOutcomeV1::Cancelled)
        );
    }
}

#[test]
fn authenticated_eof_closes_the_deferred_group_without_a_tail_fragment() {
    let (_home, store, storage, session, selected) = seed(1);
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), selected[0].marker_id())
        .unwrap()
        .unwrap();
    let removal = DraftPieceReplacementV1::new(before(2), before(2), vec![]).with_marker_effect(
        DraftPieceMarkerEffectV1::Remove {
            removal: DraftPieceMarkerRemovalProofV1::new(before(2), occurrence),
            charges: DraftPieceMarkerEffectChargesV1::for_marker(selected[0]),
        },
    );
    let (prepared, fragments) = stage(
        &storage,
        &store,
        &session,
        188,
        vec![
            DraftPieceReplacementV1::new(after(1), before(2), vec![]),
            removal,
        ],
        after(1),
    );
    let build = finish_build(&storage, &store, &prepared, &fragments);
    assert_eq!(
        build
            .marker_effect_continuation()
            .scan()
            .completed_effect_count(),
        1
    );
    assert_eq!(
        build
            .marker_effect_continuation()
            .scan()
            .next_fragment_ordinal(),
        3
    );
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    assert_eq!(
        read_text(
            &storage,
            &store,
            active_session(&storage, &store, session.draft_id(), session.session_id())
                .newest_root(),
            8
        ),
        "ACDE"
    );
}

#[test]
fn same_byte_lookahead_with_a_different_original_gap_refuses_without_adoption() {
    let (_home, store, storage, session, selected) = seed(2);
    let wrong_gap = DraftCompositePositionV1::new(
        2,
        DraftCompositeGapWitnessV1::Between {
            left_order_key: selected[0].order_key(),
            left_marker_id: selected[0].marker_id(),
            right_order_key: selected[1].order_key(),
            right_marker_id: selected[1].marker_id(),
        },
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), selected[1].marker_id())
        .unwrap()
        .unwrap();
    let removal = DraftPieceReplacementV1::new(wrong_gap, wrong_gap, vec![]).with_marker_effect(
        DraftPieceMarkerEffectV1::Remove {
            removal: DraftPieceMarkerRemovalProofV1::new(wrong_gap, occurrence),
            charges: DraftPieceMarkerEffectChargesV1::for_marker(selected[1]),
        },
    );
    let (prepared, _) = stage(
        &storage,
        &store,
        &session,
        188,
        vec![
            DraftPieceReplacementV1::new(after(1), before(2), vec![]),
            removal,
        ],
        after(1),
    );
    let mut refused = false;
    for _ in 0..64 {
        match storage.prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            prepared.header().operation_id(),
        ) {
            Err(DraftPiecePrepareErrorV1::InvalidRoot) => {
                refused = true;
                break;
            }
            Ok(Some(advance)) => {
                committed(execute(&store, storage.advance_draft_piece_edit(advance)))
            }
            Err(error) => panic!("wrong gap should refuse as invalid authority: {error:?}"),
            Ok(None) => panic!("wrong gap completed without refusing"),
        }
    }
    assert!(refused);
    assert_eq!(
        active_session(&storage, &store, session.draft_id(), session.session_id()).newest_root(),
        session.newest_root()
    );
}

#[test]
fn coordinated_latest_original_boundary_substitution_after_text_ready_is_rejected() {
    let (_home, store, storage, session, selected) = seed(3);
    let (prepared, fragments) = deletion(&storage, &store, &session, &selected);
    let mut consumed_text = false;
    for _ in 0..512 {
        let before = build_status(&storage, &store, &prepared, &fragments);
        let consume = at_boundary(&before, CancellationBoundary::PrefixReady);
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
        if consume {
            consumed_text = true;
            break;
        }
    }
    assert!(consumed_text);
    let target = build_status(&storage, &store, &prepared, &fragments);
    syndic_storage::test_faults::verify_draft_marker_build_endpoint_for_test(
        &storage, &store, &target,
    )
    .unwrap();
    assert_eq!(
        active_session(&storage, &store, session.draft_id(), session.session_id()).newest_root(),
        session.newest_root()
    );
    syndic_storage::test_faults::inject_coordinated_draft_marker_program_byte_for_test(
        &store,
        &storage,
        syndic_storage::DraftPieceSettlementKeyV1::new(
            session.draft_id(),
            session.session_id(),
            prepared.header().operation_id(),
        ),
        10,
        4,
    );
    assert!(matches!(
        storage.prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            prepared.header().operation_id()
        ),
        Err(DraftPiecePrepareErrorV1::InvalidRoot)
    ));
    assert!(matches!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
    ));
}

#[test]
fn outcome_verifier_authenticates_the_unconsumed_removal_lookahead_after_private_retirement() {
    let (_home, store, storage, session, selected) = seed(3);
    let (prepared, fragments) = deletion(&storage, &store, &session, &selected);
    let mut retired = false;
    for _ in 0..512 {
        let build = build_status(&storage, &store, &prepared, &fragments);
        if at_boundary(&build, CancellationBoundary::PrivateRetirement) {
            retired = true;
            break;
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
    }
    assert!(retired);
    let target = build_status(&storage, &store, &prepared, &fragments);
    let work = syndic_storage::test_faults::verify_draft_marker_build_endpoint_for_test(
        &storage, &store, &target,
    )
    .unwrap();
    assert!(work.attempted_reads <= syndic_storage::STAGED_DRAFT_PIECE_OUTCOME_MAX_READS);
    assert_eq!(
        active_session(&storage, &store, session.draft_id(), session.session_id()).newest_root(),
        session.newest_root()
    );
    syndic_storage::test_faults::inject_draft_piece_fragment_corruption(
        &store,
        &storage,
        fragments[2].key(),
        syndic_storage::test_faults::DraftPieceFragmentCorruption::ChainDigest,
    )
    .unwrap();
    assert!(
        syndic_storage::test_faults::verify_draft_marker_build_endpoint_for_test(
            &storage, &store, &target
        )
        .is_err()
    );
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
}

#[test]
fn deferred_prefix_removes_multiple_utf8_text_leaves_with_one_last_marker_proof() {
    let (_home, store, storage, thread) = fixture("deferred-prefix-utf8-leaves", 191);
    let text = format!("A{}CDE", "界".repeat(12_000));
    let anchor = 36_001;
    let right_anchor = 36_003;
    let mut session = seeded_text(&storage, &store, thread, &text, 194);
    let left = marker(201, 9, 1);
    let selected = marker(202, 2, 2);
    let right = marker(203, 12, 3);
    for (operation, offset, item) in [
        (195, 1, left),
        (196, anchor, selected),
        (197, right_anchor, right),
    ] {
        session = complete_checked(
            &storage,
            &store,
            &session,
            operation,
            insert_at(point(offset), offset, item),
            DraftLogicalExtentV1::new(text.len() as u64, 1),
        );
    }
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), selected.marker_id())
        .unwrap()
        .unwrap();
    let removal = DraftPieceReplacementV1::new(before(anchor), before(anchor), vec![])
        .with_marker_effect(DraftPieceMarkerEffectV1::Remove {
            removal: DraftPieceMarkerRemovalProofV1::new(before(anchor), occurrence),
            charges: DraftPieceMarkerEffectChargesV1::for_marker(selected),
        });
    let (prepared, fragments) = stage(
        &storage,
        &store,
        &session,
        198,
        vec![
            DraftPieceReplacementV1::new(after(1), before(anchor), vec![]),
            removal,
            DraftPieceReplacementV1::new(after(anchor), before(right_anchor), vec![]),
        ],
        after(1),
    );
    let mut text_slices = 0;
    for step in 0..256 {
        let build = build_status(&storage, &store, &prepared, &fragments);
        if at_boundary(&build, CancellationBoundary::PrefixReady) {
            text_slices += 1;
        }
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                prepared.header().operation_id(),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "UTF8 prefix {step}, {:?}: {error:?}",
                    draft_build_mapping_snapshot(&build)
                )
            })
        else {
            break;
        };
        let measured = advance.clone();
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
        let work = measured.bounded_work().unwrap();
        assert!(
            work.stored_structure_records() <= 256
                && work.point_attempts() <= 512
                && work.peak_encoded_bytes() <= 4_194_304
        );
        assert!(step < 255);
    }
    assert!(text_slices >= 2);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    assert_eq!(
        read_text(
            &storage,
            &store,
            active_session(&storage, &store, session.draft_id(), session.session_id())
                .newest_root(),
            8
        ),
        "AE"
    );
}
