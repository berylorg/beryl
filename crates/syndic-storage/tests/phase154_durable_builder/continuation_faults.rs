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
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
        DraftLogicalExtentV1::new(3, 1),
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), marker.marker_id())
        .unwrap()
        .unwrap();
    let moved =
        DraftPieceReplacementV1::new(point(2), point(2), vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Move {
                removal: DraftPieceMarkerRemovalProofV1::new(
                    DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll),
                    occurrence,
                ),
                insertion: DraftPieceMarkerInsertionV1::new(
                    2,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
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
            .marker_effect_continuation()
            .active()
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

#[cfg(feature = "test-faults")]
#[test]
fn marker_scan_and_active_identity_corruption_fail_closed() {
    for (case, corruption, seed) in [
        (
            "scan-next",
            DraftPieceBuildCorruption::MarkerScanNextOrdinal,
            101,
        ),
        (
            "scan-count",
            DraftPieceBuildCorruption::MarkerScanCount,
            111,
        ),
        (
            "scan-chain",
            DraftPieceBuildCorruption::MarkerScanChain,
            121,
        ),
        (
            "active-identity",
            DraftPieceBuildCorruption::ActiveMarkerIdentity,
            131,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, seed);
        let current = current(storage, &store, thread);
        let session = open_session(storage, &store, &current, seed + 1, seed + 2);
        let inserted = marker(seed + 3, 7, 9);
        let replacement =
            DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Marker(inserted)])
                .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                    DraftPieceMarkerInsertionV1::new(
                        0,
                        inserted,
                        DraftPieceMarkerEffectChargesV1::for_marker(inserted),
                    ),
                ));
        let (prepared, identity, fragment) = stage_replacement(
            &storage,
            &store,
            &session,
            seed + 4,
            replacement,
            DraftLogicalExtentV1::new(0, 0),
        );
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
        assert!(
            open_build(&storage, &store, &prepared, &fragment)
                .marker_effect_continuation()
                .active()
                .is_some()
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
                corruption,
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
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn each_published_and_hidden_active_root_fails_authentication_independently() {
    for (case, corruption, seed, advance_count) in [
        (
            "published-sequence-root",
            DraftPieceProgressRootCorruption::PublishedSequence,
            121,
            1,
        ),
        (
            "published-marker-index-root",
            DraftPieceProgressRootCorruption::PublishedMarkerIndex,
            131,
            1,
        ),
        (
            "published-marker-order-root",
            DraftPieceProgressRootCorruption::PublishedMarkerOrder,
            141,
            1,
        ),
        (
            "active-sequence-root-removal",
            DraftPieceProgressRootCorruption::ActiveSequence,
            151,
            1,
        ),
        (
            "active-index-root-range",
            DraftPieceProgressRootCorruption::ActiveMarkerIndex,
            161,
            2,
        ),
        (
            "active-marker-order-root-insertion-completion",
            DraftPieceProgressRootCorruption::ActiveMarkerOrder,
            171,
            3,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, seed);
        let current = current(storage, &store, thread);
        let mut session = open_session(storage, &store, &current, seed + 1, seed + 2);
        session = complete_staged(
            &storage,
            &store,
            &session,
            seed + 3,
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("abc".to_owned())],
            ),
            DraftLogicalExtentV1::new(3, 1),
        );
        let moved = marker(seed + 4, 7, 9);
        let retained = marker(seed + 5, 8, 10);
        for (operation, anchor, marker) in [(seed + 6, 1, moved), (seed + 7, 2, retained)] {
            session = complete_staged(
                &storage,
                &store,
                &session,
                operation,
                DraftPieceReplacementV1::new(
                    point(anchor),
                    point(anchor),
                    vec![DraftPieceV1::Marker(marker)],
                )
                .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                    DraftPieceMarkerInsertionV1::new(
                        anchor,
                        marker,
                        DraftPieceMarkerEffectChargesV1::for_marker(marker),
                    ),
                )),
                DraftLogicalExtentV1::new(3, 1),
            );
        }
        let occurrence = storage
            .draft_marker_identity(&store, session.newest_root(), moved.marker_id())
            .unwrap()
            .unwrap();
        let replacement =
            DraftPieceReplacementV1::new(point(3), point(3), vec![DraftPieceV1::Marker(moved)])
                .with_marker_effect(DraftPieceMarkerEffectV1::Move {
                    removal: DraftPieceMarkerRemovalProofV1::new(
                        DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll),
                        occurrence,
                    ),
                    insertion: DraftPieceMarkerInsertionV1::new(
                        3,
                        moved,
                        DraftPieceMarkerEffectChargesV1::for_marker(moved),
                    ),
                });
        let (prepared, identity, fragment) = stage_replacement(
            &storage,
            &store,
            &session,
            seed + 8,
            replacement,
            DraftLogicalExtentV1::new(3, 1),
        );
        for _ in 0..advance_count {
            let snapshot = open_build(&storage, &store, &prepared, &fragment);
            let advance = storage
                .prepare_draft_piece_build_advance(
                    &store,
                    identity.draft_id(),
                    identity.session_id(),
                    identity.operation_id().as_piece_operation(),
                )
                .unwrap_or_else(|error| {
                    panic!("{case} advance failed at {:?}: {error:?}", snapshot.frontier())
                })
                .unwrap();
            committed(execute(
                &store,
                storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
            ));
        }
        let build = open_build(&storage, &store, &prepared, &fragment);
        let active = build.marker_effect_continuation().active().unwrap();
        assert_eq!(build.working_roots().sequence_summary().marker_count(), 2);
        assert_eq!(active.working_roots().sequence_summary().marker_count(), 1);
        committed(execute(
            &store,
            inject_draft_piece_progress_root_corruption(
                &store,
                storage,
                syndic_storage::DraftPieceSettlementKeyV1::new(
                    identity.draft_id(),
                    identity.session_id(),
                    identity.operation_id().as_piece_operation(),
                ),
                corruption,
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
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn coordinated_receipt_count_and_chain_corruption_fail_between_effects() {
    for (case, corruption, seed) in [
        (
            "coordinated-scan-count",
            DraftPieceBuildCorruption::MarkerScanCount,
            181,
        ),
        (
            "coordinated-scan-chain",
            DraftPieceBuildCorruption::MarkerScanChain,
            191,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, seed);
        let current = current(storage, &store, thread);
        let mut session = open_session(storage, &store, &current, seed + 1, seed + 2);
        session = complete_staged(
            &storage,
            &store,
            &session,
            seed + 3,
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("abc".to_owned())],
            ),
            DraftLogicalExtentV1::new(3, 1),
        );
        let first = marker(seed + 4, 7, 9);
        let second = marker(seed + 5, 8, 10);
        let effect = |anchor, marker| {
            DraftPieceReplacementV1::new(
                point(anchor),
                point(anchor),
                vec![DraftPieceV1::Marker(marker)],
            )
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    anchor,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            ))
        };
        let (prepared, identity, fragments) = stage_interleaved_replacements(
            &storage,
            &store,
            &session,
            seed + 6,
            vec![effect(1, first), effect(2, second)],
            DraftLogicalExtentV1::new(3, 1),
            point(0),
        );
        loop {
            let build = open_build_fragments(&storage, &store, &prepared, &fragments);
            if build.frontier()
                == (DraftPieceBuildFrontierV1::Planning {
                    fragment_ordinal: 2,
                })
                && build.marker_effect_continuation().active().is_none()
            {
                assert_eq!(
                    build
                        .marker_effect_continuation()
                        .scan()
                        .completed_effect_count(),
                    1
                );
                break;
            }
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
                corruption,
            ),
        ));
        assert!(
            storage
                .draft_piece_operation_status_page(&store, &prepared, 1, &fragments)
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
    }
}
