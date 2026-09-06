fn complete_same_anchor_marker_setup(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    markers: &[DraftPieceMarkerV1],
) -> DraftEditorCandidateSessionV1 {
    let replacements = markers
        .iter()
        .copied()
        .map(|marker| {
            DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
                .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                    DraftPieceMarkerInsertionV1::new(
                        1,
                        marker,
                        DraftPieceMarkerEffectChargesV1::for_marker(marker),
                    ),
                ))
        })
        .collect();
    let (prepared, identity, _) = stage_interleaved_replacements(
        storage,
        store,
        session,
        operation,
        replacements,
        DraftLogicalExtentV1::new(4, 1),
        point(0),
    );
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )
        .unwrap()
    {
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
    active_session(storage, store, session.draft_id(), session.session_id())
}

#[test]
fn markerless_nonempty_ranges_reject_without_implicit_marker_deletion() {
    for (case, seed, marker_count) in [
        ("markerless-range-one-marker", 40, 1_usize),
        ("markerless-range-two-markers", 50, 2_usize),
    ] {
        let (home, mut store, mut storage, thread) = fixture(case, seed);
        let current = current(&storage, &store, thread);
        let mut session = open_session(&storage, &store, &current, seed + 1, seed + 2);
        session = complete_staged(
            &storage,
            &store,
            &session,
            seed + 3,
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("abcd".to_owned())],
            ),
            DraftLogicalExtentV1::new(4, 1),
        );
        let markers: Vec<_> = (0..marker_count)
            .map(|index| marker(seed + 4 + index as u8, 10 + index as u64, 20 + index as u64))
            .collect();
        session = complete_same_anchor_marker_setup(&storage, &store, &session, seed + 7, &markers);
        let before = session.clone();
        let start = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
        let (prepared, identity, fragment) = stage_replacement(
            &storage,
            &store,
            &session,
            seed + 8,
            DraftPieceReplacementV1::new(start, point(3), Vec::new()),
            DraftLogicalExtentV1::new(2, 1),
        );
        let initial = open_build(&storage, &store, &prepared, &fragment);
        assert_eq!(
            initial
                .marker_effect_continuation()
                .scan()
                .completed_effect_count(),
            0
        );
        assert_eq!(
            initial.marker_effect_continuation().scan().effect_chain(),
            syndic_storage::canonical_empty_marker_effect_chain_v1()
        );
        assert!(matches!(
            storage.prepare_draft_piece_build_advance(
                &store,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            ),
            Err(DraftPiecePrepareErrorV1::InvalidRoot)
        ));
        assert_eq!(open_build(&storage, &store, &prepared, &fragment), initial);

        drop(store);
        store = HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
        storage = SyndicStorage::register(&mut store).unwrap();
        assert!(matches!(
            storage.prepare_draft_piece_build_advance(
                &store,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            ),
            Err(DraftPiecePrepareErrorV1::InvalidRoot)
        ));
        assert_eq!(open_build(&storage, &store, &prepared, &fragment), initial);
        let reopened = active_session(&storage, &store, session.draft_id(), session.session_id());
        assert_eq!(reopened.newest_root(), before.newest_root());
        assert_eq!(reopened.newest_history(), before.newest_history());
        for marker in markers {
            assert!(
                storage
                    .validate_draft_marker_location(
                        &store,
                        reopened.newest_root(),
                        DraftPieceMarkerAtV1::new(1, marker),
                    )
                    .unwrap()
            );
        }
        committed(execute(
            &store,
            storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
        ));
        let cancelled = active_session(&storage, &store, session.draft_id(), session.session_id());
        assert_eq!(cancelled.newest_root(), before.newest_root());
        assert_eq!(cancelled.newest_history(), before.newest_history());
    }
}

#[test]
fn explicit_same_anchor_removals_precede_text_range_and_enumerate_every_effect() {
    let (_home, store, storage, thread) = fixture("same-anchor-removals-before-range", 60);
    let current = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &current, 61, 62);
    session = complete_staged(
        &storage,
        &store,
        &session,
        63,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abcd".to_owned())],
        ),
        DraftLogicalExtentV1::new(4, 1),
    );
    let markers = [marker(64, 10, 20), marker(65, 11, 21)];
    session = complete_same_anchor_marker_setup(&storage, &store, &session, 66, &markers);
    assert_same_anchor_marker_order(&storage, &store, &session, 1, &markers);
    let before = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    let between = DraftCompositePositionV1::new(
        1,
        DraftCompositeGapWitnessV1::Between {
            left_order_key: markers[0].order_key(),
            left_marker_id: markers[0].marker_id(),
            right_order_key: markers[1].order_key(),
            right_marker_id: markers[1].marker_id(),
        },
    );
    let after = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll);
    let mut removals = Vec::new();
    for (position, marker) in [(before, markers[0]), (between, markers[1])] {
        let occurrence = storage
            .draft_marker_identity(&store, session.newest_root(), marker.marker_id())
            .unwrap()
            .unwrap();
        removals.push(
            DraftPieceReplacementV1::new(position, position, Vec::new()).with_marker_effect(
                DraftPieceMarkerEffectV1::Remove {
                    removal: DraftPieceMarkerRemovalProofV1::new(position, occurrence),
                    charges: DraftPieceMarkerEffectChargesV1::for_marker(marker),
                },
            ),
        );
    }
    removals.push(DraftPieceReplacementV1::new(after, point(3), Vec::new()));
    let (prepared, identity, fragments) = stage_interleaved_replacements(
        &storage,
        &store,
        &session,
        68,
        removals,
        DraftLogicalExtentV1::new(2, 1),
        point(0),
    );
    loop {
        let advance = storage
            .prepare_draft_piece_build_advance(
                &store,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            )
            .unwrap_or_else(|error| {
                let snapshot = open_build_fragments(&storage, &store, &prepared, &fragments);
                panic!(
                    "same-anchor removal advance failed at {:?}: {error:?}",
                    snapshot.frontier()
                )
            });
        let Some(advance) = advance else {
            break;
        };
        committed(execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        ));
    }
    let complete = match storage
        .draft_piece_operation_status_page(&store, &prepared, 1, &fragments)
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(DraftPieceOperationStatusV1::Complete(build)) => {
            build
        }
        other => panic!("operation was not an authenticated complete build: {other:?}"),
    };
    let scan = complete.marker_effect_continuation().scan();
    assert_eq!(scan.completed_effect_count(), markers.len() as u64);
    assert_ne!(
        scan.effect_chain(),
        syndic_storage::canonical_empty_marker_effect_chain_v1()
    );
    assert!(complete.marker_effect_continuation().active().is_none());
    assert_eq!(scan.next_fragment_ordinal(), fragments.len() as u64 + 1);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());
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
        b"ad"
    );
    for marker in markers {
        assert!(
            storage
                .draft_marker_identity(&store, session.newest_root(), marker.marker_id())
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn repeated_empty_ranges_reject_when_either_item_lacks_a_marker_effect() {
    for (case, seed, first_effect, second_effect) in [
        ("two-markerless-empty-ranges", 70, false, false),
        ("effect-then-markerless-empty", 80, true, false),
        ("markerless-then-effect-empty", 90, false, true),
    ] {
        let (_home, store, storage, thread) = fixture(case, seed);
        let current = current(&storage, &store, thread);
        let mut session = open_session(&storage, &store, &current, seed + 1, seed + 2);
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
        let replacement = |with_effect, marker| {
            if with_effect {
                DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
                    .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                        DraftPieceMarkerInsertionV1::new(
                            1,
                            marker,
                            DraftPieceMarkerEffectChargesV1::for_marker(marker),
                        ),
                    ))
            } else {
                DraftPieceReplacementV1::new(
                    point(1),
                    point(1),
                    vec![DraftPieceV1::Text("X".to_owned())],
                )
            }
        };
        let markerless_count = usize::from(!first_effect) + usize::from(!second_effect);
        let (prepared, identity, _) = stage_interleaved_replacements(
            &storage,
            &store,
            &session,
            seed + 6,
            vec![
                replacement(first_effect, marker(seed + 4, 10, 20)),
                replacement(second_effect, marker(seed + 5, 11, 21)),
            ],
            DraftLogicalExtentV1::new(3 + markerless_count as u64, 1),
            point(0),
        );
        let mut rejected = false;
        for _ in 0..32 {
            match storage.prepare_draft_piece_build_advance(
                &store,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            ) {
                Err(DraftPiecePrepareErrorV1::Rejected(
                    DraftPieceRejectedReasonV1::DuplicateEmptyRange,
                )) => {
                    rejected = true;
                    break;
                }
                Ok(Some(advance)) => committed(execute(
                    &store,
                    storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
                )),
                _ => panic!("expected duplicate-empty rejection"),
            }
        }
        assert!(rejected);
        committed(execute(
            &store,
            storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
        ));
        let cancelled = active_session(&storage, &store, session.draft_id(), session.session_id());
        assert_eq!(cancelled.newest_root(), session.newest_root());
        assert_eq!(cancelled.newest_history(), session.newest_history());
    }
}
