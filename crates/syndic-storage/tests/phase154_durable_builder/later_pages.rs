fn stage_interleaved_replacements(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacements: Vec<DraftPieceReplacementV1>,
    final_extent: DraftLogicalExtentV1,
    final_position: DraftCompositePositionV1,
) -> (
    PreparedDraftPieceEditV1,
    DraftMutationStagingIdentityV1,
    Vec<syndic_storage::DraftPieceBuildFragmentV1>,
) {
    let identity = DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes([operation; 16]),
    );
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, session), session)
        .unwrap();
    let mut active = begin.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), begin),
    ));
    let mut chain = canonical_empty_draft_piece_fragment_chain_v1();
    for (index, replacement) in replacements.iter().enumerate() {
        let head = storage
            .draft_mutation_staging_head(store, identity)
            .unwrap()
            .unwrap();
        let source = prepare_one_page(
            *storage,
            &head,
            &active,
            DraftMutationStagingPageItemV1::SourcePosition(point(0)),
        );
        active = source.target_session().unwrap().clone();
        committed(execute(
            store,
            storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), source),
        ));
        let head = storage
            .draft_mutation_staging_head(store, identity)
            .unwrap()
            .unwrap();
        let proposal = prepare_one_page(
            *storage,
            &head,
            &active,
            DraftMutationStagingPageItemV1::Proposal(replacement.clone()),
        );
        active = proposal.target_session().unwrap().clone();
        committed(execute(
            store,
            storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), proposal),
        ));
        chain = draft_piece_fragment_chain_link_v1(chain, index as u64 + 1, replacement);
    }
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    assert_eq!(head.source().item_total(), replacements.len() as u64);
    assert_eq!(head.proposal().item_total(), replacements.len() as u64);
    let finish = storage
        .prepare_draft_mutation_staging_finish(
            &head,
            &active,
            DraftMutationFinishInputV1::new(
                head.source(),
                head.proposal(),
                final_extent,
                final_position,
                final_position,
                final_position,
                chain,
            ),
        )
        .unwrap();
    active = finish.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), finish),
    ));
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&head, &active)
        .unwrap();
    let prepared = transfer.prepared_edit().clone();
    committed(execute(
        store,
        storage
            .transfer_draft_mutation_staging_to_builder(storage.revision(store).unwrap(), transfer),
    ));
    loop {
        let DraftMutationStagingStatusV1::Building { build, .. } = storage
            .draft_mutation_staging_status(store, identity)
            .unwrap()
        else {
            panic!("interleaved staging lost builder custody");
        };
        let Some(window) = storage
            .prepare_next_durable_draft_piece_window(
                store,
                identity,
                build,
                DraftPieceDurableBuildWindowLimitsV1::maximum(),
            )
            .unwrap()
        else {
            break;
        };
        committed(execute(
            store,
            storage.stage_next_durable_draft_piece_window(storage.revision(store).unwrap(), window),
        ));
    }
    let mut preceding = canonical_empty_draft_piece_fragment_chain_v1();
    let fragments = replacements
        .into_iter()
        .enumerate()
        .map(|(index, replacement)| {
            let ordinal = index as u64 + 1;
            let fragment = storage
                .prepare_draft_piece_fragment(&prepared, ordinal, preceding, replacement)
                .unwrap();
            preceding = fragment.chain_digest();
            fragment
        })
        .collect();
    (prepared, identity, fragments)
}

fn open_build_fragments(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedDraftPieceEditV1,
    fragments: &[syndic_storage::DraftPieceBuildFragmentV1],
) -> syndic_storage::DraftPieceBuildRecordV1 {
    match storage
        .draft_piece_operation_status_page(store, prepared, 1, fragments)
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(DraftPieceOperationStatusV1::Open(build)) => {
            build
        }
        other => panic!("operation was not an open authenticated build: {other:?}"),
    }
}

#[test]
fn leading_marker_and_following_text_delete_in_one_atomic_build() {
    let (_home, store, storage, thread) = fixture("marker-text-delete", 70);
    let current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &current, 71, 72);
    session = complete_staged(
        &storage,
        &store,
        &session,
        73,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("ABC".to_owned())],
        ),
        DraftLogicalExtentV1::new(3, 1),
    );
    let inserted = marker(74, 7, 9);
    let before = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    let after = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll);
    session = complete_staged(
        &storage,
        &store,
        &session,
        75,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(inserted)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    inserted,
                    DraftPieceMarkerEffectChargesV1::for_marker(inserted),
                ),
            )),
        DraftLogicalExtentV1::new(3, 1),
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), inserted.marker_id())
        .unwrap()
        .unwrap();
    let predecessor_generation = session.newest_candidate_generation();
    let replacements = vec![
        DraftPieceReplacementV1::new(before, before, Vec::new()).with_marker_effect(
            DraftPieceMarkerEffectV1::Remove {
                removal: DraftPieceMarkerRemovalProofV1::new(before, occurrence),
                charges: DraftPieceMarkerEffectChargesV1::for_marker(inserted),
            },
        ),
        DraftPieceReplacementV1::new(after, point(2), Vec::new()),
    ];
    let (prepared, identity, _) = stage_interleaved_replacements(
        &storage,
        &store,
        &session,
        76,
        replacements,
        DraftLogicalExtentV1::new(2, 1),
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
    assert_ne!(
        session.newest_candidate_generation(),
        predecessor_generation
    );
    assert_eq!(session.newest_root().summary().marker_count(), 0);
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
        b"AC"
    );
}

#[test]
fn later_interleaved_marker_effect_survives_following_fragments_and_restart() {
    let (home, store, storage, thread) = fixture("later-effect", 80);
    let current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &current, 81, 82);
    session = complete_staged(
        &storage,
        &store,
        &session,
        83,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abcd".to_owned())],
        ),
        DraftLogicalExtentV1::new(4, 1),
    );
    let inserted = marker(84, 7, 9);
    let replacements = vec![
        DraftPieceReplacementV1::new(point(0), point(1), vec![DraftPieceV1::Text("A".to_owned())]),
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(inserted)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    inserted,
                    DraftPieceMarkerEffectChargesV1::for_marker(inserted),
                ),
            )),
        DraftPieceReplacementV1::new(point(2), point(3), vec![DraftPieceV1::Text("C".to_owned())]),
    ];
    let (prepared, identity, fragments) = stage_interleaved_replacements(
        &storage,
        &store,
        &session,
        85,
        replacements,
        DraftLogicalExtentV1::new(4, 1),
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
            .unwrap()
            .unwrap();
        committed(execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        ));
        if open_build_fragments(&storage, &store, &prepared, &fragments)
            .marker_effect_continuation()
            .active()
            .is_some()
        {
            break;
        }
    }
    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    assert!(
        open_build_fragments(&storage, &store, &prepared, &fragments)
            .marker_effect_continuation()
            .active()
            .is_some()
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
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                session.newest_root(),
                DraftPieceMarkerAtV1::new(1, inserted),
            )
            .unwrap()
    );
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
        b"AbCd"
    );
}

#[test]
fn later_marker_effects_complete_in_canonical_fragment_order() {
    let (_home, store, storage, thread) = fixture("later-effect-overtake", 90);
    let current = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &current, 91, 92);
    session = complete_staged(
        &storage,
        &store,
        &session,
        93,
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abc".to_owned())],
        ),
        DraftLogicalExtentV1::new(3, 1),
    );
    let first = marker(94, 7, 9);
    let second = marker(95, 8, 9);
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
        96,
        vec![
            DraftPieceReplacementV1::new(
                point(0),
                point(1),
                vec![DraftPieceV1::Text("A".to_owned())],
            ),
            effect(1, first),
            effect(2, second),
        ],
        DraftLogicalExtentV1::new(3, 1),
        point(0),
    );
    for _ in 0..3 {
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
    for (anchor, marker) in [(1, first), (2, second)] {
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
}
