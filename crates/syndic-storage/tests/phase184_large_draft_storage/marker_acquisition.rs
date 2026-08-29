use super::*;

#[test]
fn same_anchor_marker_pages_restart_seal_and_materialize_without_registry_residency() {
    let (home, mut store, mut storage, thread) = marker_fixture("phase184-same-anchor", 190);
    let durable = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &durable, 191, 192);
    session = complete_staged_bounded(
        &storage,
        &store,
        &session,
        1,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("anchor".to_owned())],
        ),
        DraftLogicalExtentV1::new(6, 1),
    );

    for index in 0..MARKER_COUNT {
        let marker = large_marker(index);
        session = complete_staged_bounded(
            &storage,
            &store,
            &session,
            (index + 2) as u8,
            DraftPieceReplacementV1::new(
                if index == 0 {
                    point(3)
                } else {
                    DraftCompositePositionV1::new(3, DraftCompositeGapWitnessV1::AfterAll)
                },
                if index == 0 {
                    point(3)
                } else {
                    DraftCompositePositionV1::new(3, DraftCompositeGapWitnessV1::AfterAll)
                },
                vec![DraftPieceV1::Marker(marker)],
            )
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    3,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
            DraftLogicalExtentV1::new(6, 1),
        );
    }
    assert!(session.active_operation().is_none());
    let root = session.newest_root();
    assert_eq!(root.marker_commitment().marker_count(), MARKER_COUNT as u64);
    assert_marker_pages(&storage, &store, root, DraftPieceMarkerDirectionV1::Forward);
    assert_marker_pages(
        &storage,
        &store,
        root,
        DraftPieceMarkerDirectionV1::Backward,
    );
    assert_marker_edges(&storage, &store, root);

    drop(store);
    store = HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    storage = SyndicStorage::register(&mut store).unwrap();
    assert_marker_pages(&storage, &store, root, DraftPieceMarkerDirectionV1::Forward);

    let seal = seal_with_page_limit(&storage, &store, root, 193, MARKER_PAGE);
    assert_eq!(seal.sequential().marker_count(), MARKER_COUNT as u64);
    assert_eq!(seal.ordered_assets().marker_count(), MARKER_COUNT as u64);
    let mapping = materialize_bounded(&storage, &store, root, 194);
    assert_eq!(mapping.source_marker_count(), MARKER_COUNT as u64);
    assert_eq!(mapping.source_utf8_bytes(), 6);
}

#[cfg(feature = "test-faults")]
#[test]
fn marker_bearing_third_full_window_charges_the_complete_acquisition_maximum() {
    let (_home, store, storage, thread) = fixture("phase184-window-maximum", 195);
    let durable = current(&storage, &store, thread);
    let mut session = open_session(&storage, &store, &durable, 196, 197);
    session = complete_staged_bounded(
        &storage,
        &store,
        &session,
        1,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("anchor".to_owned())],
        ),
        DraftLogicalExtentV1::new(6, 1),
    );
    let marker = large_marker(0);
    session = complete_staged_bounded(
        &storage,
        &store,
        &session,
        2,
        DraftPieceReplacementV1::new(point(3), point(3), vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    3,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
        DraftLogicalExtentV1::new(6, 1),
    );
    assert_eq!(session.newest_root().marker_commitment().marker_count(), 1);

    let identity = DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes([198; 16]),
    );
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    let mut active = begin.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
    ));
    let mut chain = canonical_empty_draft_piece_fragment_chain_v1();
    for ordinal in 1..=3 * DRAFT_PIECE_BUILD_WINDOW_MAX_PAGES as u64 {
        let replacement = if ordinal == 1 {
            DraftPieceReplacementV1::new(
                point(3),
                point(3),
                vec![DraftPieceV1::Text("x".to_owned())],
            )
        } else {
            DraftPieceReplacementV1::continuation(
                point(3),
                point(3),
                vec![DraftPieceV1::Text("x".to_owned())],
            )
        };
        chain = draft_piece_fragment_chain_link_v1(chain, ordinal, &replacement);
        let head = storage
            .draft_mutation_staging_head(&store, identity)
            .unwrap()
            .unwrap();
        let page = prepare_one_page(
            &storage,
            &head,
            &active,
            DraftMutationStagingPageItemV1::Proposal(replacement),
        );
        active = page.target_session().unwrap().clone();
        committed(execute(
            &store,
            storage.draft_mutation_staging_page_batch(storage.revision(&store).unwrap(), page),
        ));
    }
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let finish = storage
        .prepare_draft_mutation_staging_finish(
            &head,
            &active,
            DraftMutationFinishInputV1::new(
                head.source(),
                head.proposal(),
                DraftLogicalExtentV1::new(774, 1),
                point(774),
                point(774),
                point(774),
                chain,
            ),
        )
        .unwrap();
    active = finish.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), finish),
    ));
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&head, &active)
        .unwrap();
    committed(execute(
        &store,
        storage.transfer_draft_mutation_staging_to_builder(
            storage.revision(&store).unwrap(),
            transfer,
        ),
    ));

    for _ in 0..2 {
        let DraftMutationStagingStatusV1::Building { build, .. } = storage
            .draft_mutation_staging_status(&store, identity)
            .unwrap()
        else {
            panic!("durable window lost builder custody");
        };
        let window = storage
            .prepare_next_durable_draft_piece_window(
                &store,
                identity,
                build,
                DraftPieceDurableBuildWindowLimitsV1::maximum(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(window.page_count(), DRAFT_PIECE_BUILD_WINDOW_MAX_PAGES);
        committed(execute(
            &store,
            storage
                .stage_next_durable_draft_piece_window(storage.revision(&store).unwrap(), window),
        ));
    }
    let DraftMutationStagingStatusV1::Building { build, .. } = storage
        .draft_mutation_staging_status(&store, identity)
        .unwrap()
    else {
        panic!("third durable window lost builder custody");
    };
    reset_syndic_point_read_count();
    let window = storage
        .prepare_next_durable_draft_piece_window(
            &store,
            identity,
            build,
            DraftPieceDurableBuildWindowLimitsV1::maximum(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(window.page_count(), DRAFT_PIECE_BUILD_WINDOW_MAX_PAGES);
    assert_eq!(
        window.acquisition_read_count(),
        DRAFT_PIECE_BUILD_WINDOW_MAX_READS
    );
    assert_eq!(
        window.acquisition_encoded_value_byte_budget(),
        DRAFT_PIECE_BUILD_WINDOW_MAX_ENCODED_VALUE_BYTES
    );
    assert_eq!(
        syndic_point_read_count(),
        DRAFT_PIECE_BUILD_WINDOW_MAX_READS
    );
}

fn marker_fixture(name: &str, seed: u8) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new(name);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase184",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65_536, MARKER_COUNT as u64).unwrap(),
            ),
        ),
    ));
    (home, store, storage, thread)
}

fn large_marker(index: usize) -> DraftPieceMarkerV1 {
    let ordinal = index as u64 + 1;
    let mut marker_id = [0_u8; 16];
    marker_id[..8].copy_from_slice(&ordinal.to_le_bytes());
    DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes(marker_id),
        ordinal,
        ImageLabelOrdinal::new(ordinal).unwrap(),
        beryl_model::AssetId::sha256_v1(
            [index as u8; 32],
            std::num::NonZeroU64::new(ordinal).unwrap(),
        ),
    )
}

fn assert_marker_pages(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    direction: DraftPieceMarkerDirectionV1,
) {
    let mut cursor = None;
    let mut remaining = MARKER_COUNT;
    while remaining != 0 {
        let result = storage
            .draft_piece_marker_demand(
                store,
                root,
                DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::ExactAnchor(3),
                    direction,
                    cursor,
                    MARKER_PAGE,
                    DRAFT_PIECE_PAGE_MAX_BYTES,
                ),
            )
            .unwrap();
        assert!(!result.markers().is_empty());
        assert!(result.markers().len() <= MARKER_PAGE);
        assert!(result.markers().len() <= DRAFT_PIECE_PAGE_MAX_RECORDS);
        assert!(result.retained_bytes() <= DRAFT_PIECE_PAGE_MAX_BYTES);
        assert!(result.records_read() <= 2 * u64::from(DRAFT_PIECE_MAX_HEIGHT) + 2);
        let start = match direction {
            DraftPieceMarkerDirectionV1::Forward => MARKER_COUNT - remaining,
            DraftPieceMarkerDirectionV1::Backward => remaining - result.markers().len(),
        };
        for (position, actual) in result.markers().iter().copied().enumerate() {
            assert_eq!(
                actual,
                DraftPieceMarkerAtV1::new(3, large_marker(start + position))
            );
        }
        remaining -= result.markers().len();
        if remaining == 0 {
            assert!(result.requested_side_complete());
            assert!(result.continuation().is_none());
        } else {
            assert!(!result.requested_side_complete());
            cursor = result.continuation();
            assert!(cursor.is_some());
        }
    }
}

fn assert_marker_edges(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
) {
    let first = DraftPieceMarkerAtV1::new(3, large_marker(0));
    let last = DraftPieceMarkerAtV1::new(3, large_marker(MARKER_COUNT - 1));
    let adjacent_left = DraftPieceMarkerAtV1::new(3, large_marker(MARKER_PAGE - 1));
    let adjacent_right = DraftPieceMarkerAtV1::new(3, large_marker(MARKER_PAGE));
    for (request, expected) in [
        (
            DraftPieceMarkerEdgeProofRequestV1::First { marker: first },
            DraftPieceMarkerEdgeProofV1::First { marker: first },
        ),
        (
            DraftPieceMarkerEdgeProofRequestV1::Last { marker: last },
            DraftPieceMarkerEdgeProofV1::Last { marker: last },
        ),
        (
            DraftPieceMarkerEdgeProofRequestV1::Adjacent {
                left: adjacent_left,
                right: adjacent_right,
            },
            DraftPieceMarkerEdgeProofV1::Adjacent {
                left: adjacent_left,
                right: adjacent_right,
            },
        ),
        (
            DraftPieceMarkerEdgeProofRequestV1::Absence { anchor: 2 },
            DraftPieceMarkerEdgeProofV1::Absence { anchor: 2 },
        ),
    ] {
        assert_eq!(
            storage
                .draft_piece_marker_edge_proof(store, root, request, DRAFT_PIECE_PAGE_MAX_BYTES)
                .unwrap(),
            Some(expected)
        );
    }
}

fn seal_with_page_limit(
    storage: &SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    operation: u8,
    page_limit: usize,
) -> syndic_storage::DraftMarkerSealProofV1 {
    assert!(page_limit > 0);
    assert!(page_limit <= DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS);
    let request = DraftMarkerSealRequestV1::new(
        root,
        DraftMarkerSealOperationIdV1::from_bytes([operation; 16]),
    );
    let begin = storage
        .prepare_draft_marker_seal_begin(store, request)
        .unwrap();
    committed(execute(
        store,
        storage.begin_draft_marker_seal(storage.revision(store).unwrap(), begin),
    ));
    while let Some(advance) = storage
        .prepare_draft_marker_seal_advance_with_limit(store, request.key(), page_limit)
        .unwrap()
    {
        assert!(advance.page().markers().len() <= page_limit);
        assert!(advance.page().markers().len() <= DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS);
        assert!(
            advance.page().release().target_frontier() - advance.page().release().source_frontier()
                <= page_limit as u64
        );
        committed(execute(
            store,
            storage.advance_draft_marker_seal(storage.revision(store).unwrap(), &advance),
        ));
    }
    let DraftMarkerSealStatusV1::Sealed(proof, release) = storage
        .draft_marker_seal_status(store, request.key())
        .unwrap()
    else {
        panic!("marker seal did not close");
    };
    assert_eq!(
        release.completed_marker_count(),
        root.marker_commitment().marker_count()
    );
    assert!(std::mem::size_of_val(&release) <= 256);
    proof
}
