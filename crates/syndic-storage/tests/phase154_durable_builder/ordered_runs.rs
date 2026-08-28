fn marker_page(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    scope: syndic_storage::DraftPieceMarkerScopeV1,
    direction: syndic_storage::DraftPieceMarkerDirectionV1,
    cursor: Option<syndic_storage::DraftCompositeSearchKeyV1>,
    max_objects: usize,
) -> syndic_storage::DraftPieceMarkerDemandResultV1 {
    storage
        .draft_piece_marker_demand(
            store,
            session.newest_root(),
            syndic_storage::DraftPieceMarkerDemandV1::new(
                scope,
                direction,
                cursor,
                max_objects,
                65_536,
            ),
        )
        .unwrap()
}

fn assert_same_anchor_marker_order(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    anchor: u64,
    expected: &[DraftPieceMarkerV1],
) {
    let result = marker_page(
        storage,
        store,
        session,
        syndic_storage::DraftPieceMarkerScopeV1::ExactAnchor(anchor),
        syndic_storage::DraftPieceMarkerDirectionV1::Forward,
        None,
        8,
    );
    let expected: Vec<_> = expected
        .iter()
        .copied()
        .map(|marker| DraftPieceMarkerAtV1::new(anchor, marker))
        .collect();
    assert_eq!(result.markers(), expected);
    assert!(result.requested_side_complete());
    assert!(result.continuation().is_none());
}

#[test]
fn sparse_first_middle_last_and_same_anchor_runs_fold_in_fragment_order() {
    let (_home, store, storage, thread) = fixture("sparse-same-anchor", 220);
    let sparse_current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &sparse_current, 221, 222);
    session = complete_staged(
        &storage,
        &store,
        &session,
        223,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abcdef".to_owned())],
        ),
        DraftLogicalExtentV1::new(6, 1),
    );
    let sparse = [marker(224, 1, 1), marker(225, 2, 2), marker(226, 3, 3)];
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
    let (prepared, identity, _) = stage_interleaved_replacements(
        &storage,
        &store,
        &session,
        227,
        vec![
            effect(1, sparse[0]),
            DraftPieceReplacementV1::new(
                point(1),
                point(2),
                vec![DraftPieceV1::Text("B".to_owned())],
            ),
            DraftPieceReplacementV1::new(
                point(2),
                point(3),
                vec![DraftPieceV1::Text("C".to_owned())],
            ),
            effect(3, sparse[1]),
            DraftPieceReplacementV1::new(
                point(3),
                point(4),
                vec![DraftPieceV1::Text("D".to_owned())],
            ),
            DraftPieceReplacementV1::new(
                point(4),
                point(5),
                vec![DraftPieceV1::Text("E".to_owned())],
            ),
            effect(6, sparse[2]),
        ],
        DraftLogicalExtentV1::new(6, 1),
        point(0),
    );
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )
        .unwrap()
    {
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
        b"aBCDEf"
    );
    for (anchor, marker) in [(1, sparse[0]), (3, sparse[1]), (6, sparse[2])] {
        assert!(
            storage
                .validate_draft_marker_location(
                    &store,
                    session.newest_root(),
                    DraftPieceMarkerAtV1::new(anchor, marker),
                )
                .unwrap()
        );
    }
    let sparse_all = sparse
        .iter()
        .enumerate()
        .map(|(index, marker)| DraftPieceMarkerAtV1::new([1, 3, 6][index], *marker))
        .collect::<Vec<_>>();
    let half_open = marker_page(
        &storage,
        &store,
        &session,
        syndic_storage::DraftPieceMarkerScopeV1::Range { start: 1, end: 6 },
        syndic_storage::DraftPieceMarkerDirectionV1::Forward,
        None,
        8,
    );
    assert_eq!(half_open.markers(), &sparse_all[..2]);
    let inclusive = marker_page(
        &storage,
        &store,
        &session,
        syndic_storage::DraftPieceMarkerScopeV1::InclusiveRange { start: 1, end: 6 },
        syndic_storage::DraftPieceMarkerDirectionV1::Forward,
        None,
        8,
    );
    assert_eq!(inclusive.markers(), sparse_all);
    assert!(inclusive.requested_side_complete());
    let reverse = marker_page(
        &storage,
        &store,
        &session,
        syndic_storage::DraftPieceMarkerScopeV1::InclusiveRange { start: 1, end: 6 },
        syndic_storage::DraftPieceMarkerDirectionV1::Backward,
        None,
        8,
    );
    assert_eq!(reverse.markers(), sparse_all);
    assert!(reverse.requested_side_complete());
    let terminal = marker_page(
        &storage,
        &store,
        &session,
        syndic_storage::DraftPieceMarkerScopeV1::InclusiveRange { start: 6, end: 6 },
        syndic_storage::DraftPieceMarkerDirectionV1::Forward,
        None,
        8,
    );
    assert_eq!(terminal.markers(), &sparse_all[2..]);

    let (_same_home, store, storage, thread) = fixture("same-anchor-pages", 235);
    let same_current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &same_current, 236, 237);
    session = complete_staged(
        &storage,
        &store,
        &session,
        238,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abcdef".to_owned())],
        ),
        DraftLogicalExtentV1::new(6, 1),
    );
    let same = [
        marker(228, 10, 10),
        marker(229, 11, 11),
        marker(230, 12, 12),
    ];
    let same_effect = |position, marker| {
        let replacement =
            DraftPieceReplacementV1::new(position, position, vec![DraftPieceV1::Marker(marker)]);
        replacement.with_marker_effect(DraftPieceMarkerEffectV1::Insert(
            DraftPieceMarkerInsertionV1::new(
                2,
                marker,
                DraftPieceMarkerEffectChargesV1::for_marker(marker),
            ),
        ))
    };
    let (prepared, identity, _) = stage_interleaved_replacements(
        &storage,
        &store,
        &session,
        231,
        vec![
            same_effect(point(2), same[1]),
            same_effect(point(2), same[0]),
            same_effect(point(2), same[2]),
        ],
        DraftLogicalExtentV1::new(6, 1),
        point(0),
    );
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )
        .unwrap()
    {
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
    for marker in same {
        assert!(
            storage
                .validate_draft_marker_location(
                    &store,
                    session.newest_root(),
                    DraftPieceMarkerAtV1::new(2, marker),
                )
                .unwrap()
        );
    }
    assert_same_anchor_marker_order(&storage, &store, &session, 2, &same);
    for direction in [
        syndic_storage::DraftPieceMarkerDirectionV1::Forward,
        syndic_storage::DraftPieceMarkerDirectionV1::Backward,
    ] {
        let first = marker_page(
            &storage,
            &store,
            &session,
            syndic_storage::DraftPieceMarkerScopeV1::InclusiveRange { start: 0, end: 2 },
            direction,
            None,
            2,
        );
        assert_eq!(first.markers().len(), 2);
        assert!(!first.requested_side_complete());
        let second = marker_page(
            &storage,
            &store,
            &session,
            syndic_storage::DraftPieceMarkerScopeV1::InclusiveRange { start: 0, end: 2 },
            direction,
            first.continuation(),
            2,
        );
        assert_eq!(second.markers().len(), 1);
        assert!(second.requested_side_complete());
        let mut paged = first.markers().to_vec();
        paged.extend_from_slice(second.markers());
        paged.sort_by_key(|marker| marker.marker().order_key());
        assert_eq!(
            paged,
            same
                .iter()
                .copied()
                .map(|marker| DraftPieceMarkerAtV1::new(2, marker))
                .collect::<Vec<_>>()
        );
    }

    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), same[0].marker_id())
        .unwrap()
        .unwrap();
    let before_middle = DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll);
    session = complete_staged(
        &storage,
        &store,
        &session,
        232,
        DraftPieceReplacementV1::new(before_middle, before_middle, Vec::new()).with_marker_effect(
            DraftPieceMarkerEffectV1::Remove {
                removal: DraftPieceMarkerRemovalProofV1::new(before_middle, occurrence),
                charges: DraftPieceMarkerEffectChargesV1::for_marker(same[0]),
            },
        ),
        DraftLogicalExtentV1::new(6, 1),
    );
    assert!(
        storage
            .draft_marker_identity(&store, session.newest_root(), same[0].marker_id())
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                session.newest_root(),
                DraftPieceMarkerAtV1::new(2, same[1]),
            )
            .unwrap()
    );
    assert_same_anchor_marker_order(&storage, &store, &session, 2, &same[1..]);

    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), same[1].marker_id())
        .unwrap()
        .unwrap();
    let before_only = DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll);
    session = complete_staged(
        &storage,
        &store,
        &session,
        233,
        DraftPieceReplacementV1::new(before_only, before_only, Vec::new()).with_marker_effect(
            DraftPieceMarkerEffectV1::Remove {
                removal: DraftPieceMarkerRemovalProofV1::new(before_only, occurrence),
                charges: DraftPieceMarkerEffectChargesV1::for_marker(same[1]),
            },
        ),
        DraftLogicalExtentV1::new(6, 1),
    );
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                session.newest_root(),
                DraftPieceMarkerAtV1::new(2, same[2]),
            )
            .unwrap()
    );
    assert_same_anchor_marker_order(&storage, &store, &session, 2, &same[2..]);

    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), same[2].marker_id())
        .unwrap()
        .unwrap();
    let before_last = DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll);
    session = complete_staged(
        &storage,
        &store,
        &session,
        234,
        DraftPieceReplacementV1::new(before_last, before_last, Vec::new()).with_marker_effect(
            DraftPieceMarkerEffectV1::Remove {
                removal: DraftPieceMarkerRemovalProofV1::new(before_last, occurrence),
                charges: DraftPieceMarkerEffectChargesV1::for_marker(same[2]),
            },
        ),
        DraftLogicalExtentV1::new(6, 1),
    );
    for marker in same {
        assert!(
            storage
                .draft_marker_identity(&store, session.newest_root(), marker.marker_id())
                .unwrap()
                .is_none()
        );
    }
    assert_same_anchor_marker_order(&storage, &store, &session, 2, &[]);
}
