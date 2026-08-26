#[test]
fn nonempty_replacement_continuations_consume_source_once_across_marker_effects_and_reopen() {
    let (home, store, storage, thread) = fixture("nonempty-continuation", 100);
    let current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &current, 101, 102);
    session = complete_staged(
        &storage,
        &store,
        &session,
        103,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abcdef".to_owned())],
        ),
        DraftLogicalExtentV1::new(6, 1),
    );
    let leading = marker(104, 7, 9);
    let trailing = marker(105, 8, 10);
    let marker_insert = |point_anchor, successor_anchor, marker| {
        DraftPieceReplacementV1::new(
            point(point_anchor),
            point(point_anchor),
            vec![DraftPieceV1::Marker(marker)],
        )
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
            DraftPieceMarkerInsertionV1::new(
                successor_anchor,
                marker,
                DraftPieceMarkerEffectChargesV1::for_marker(marker),
            ),
        ))
    };
    let replacements = vec![
        marker_insert(0, 0, leading),
        DraftPieceReplacementV1::new(point(1), point(5), vec![DraftPieceV1::Text("X".to_owned())]),
        DraftPieceReplacementV1::continuation(
            point(1),
            point(5),
            vec![DraftPieceV1::Text("Y".to_owned())],
        ),
        DraftPieceReplacementV1::continuation(
            point(1),
            point(5),
            vec![DraftPieceV1::Text("Z".to_owned())],
        ),
        marker_insert(5, 4, trailing),
    ];
    let (prepared, identity, fragments) = stage_interleaved_replacements(
        &storage,
        &store,
        &session,
        106,
        replacements,
        DraftLogicalExtentV1::new(5, 1),
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll),
    );
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )
        .unwrap_or_else(|error| {
            panic!(
                "advance failed at {:?}: {error:?}",
                open_build_fragments(&storage, &store, &prepared, &fragments).frontier()
            )
        })
    {
        committed(execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        ));
    }
    let completed = complete_continued_build(&storage, &store, &prepared, &fragments);
    assert_eq!(completed.frontier(), DraftPieceBuildFrontierV1::Complete);
    let replayed = complete_continued_build(&storage, &store, &prepared, &fragments);
    assert_eq!(replayed, completed);

    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    assert_eq!(
        complete_continued_build(&storage, &store, &prepared, &fragments),
        completed
    );
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
        b"aXYZf"
    );
    for (anchor, marker) in [(0, leading), (4, trailing)] {
        assert!(
            storage
                .validate_draft_marker_location(
                    &store,
                    session.newest_root(),
                    DraftPieceMarkerAtV1::new(anchor, marker),
                )
                .unwrap(),
            "marker {} was not at expected anchor {anchor}",
            marker.order_key()
        );
    }
}

fn complete_continued_build(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedDraftPieceEditV1,
    fragments: &[syndic_storage::DraftPieceBuildFragmentV1],
) -> syndic_storage::DraftPieceBuildRecordV1 {
    match storage
        .draft_piece_operation_status_page(store, prepared, 1, fragments)
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(DraftPieceOperationStatusV1::Complete(build)) => {
            build
        }
        other => panic!("operation was not an authenticated complete build: {other:?}"),
    }
}
