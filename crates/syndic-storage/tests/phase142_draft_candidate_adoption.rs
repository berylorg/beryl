use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(feature = "test-faults")]
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
use beryl_model::{
    ExecutionBinding, ImageLabelOrdinal, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicDraftMarkerId, SyndicThreadId,
};
use syndic_storage::{
    CreateThread, DraftCompositeGapWitnessV1, DraftCompositePositionV1,
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionOpenRequestV1,
    DraftEditorCandidateSessionReadOutcomeV1, DraftEditorCandidateSessionV1,
    DraftEditorCurrentSelectorV1, DraftPieceBuildFragmentV1, DraftPieceEditHeaderV1,
    DraftPieceErrorReasonV1, DraftPieceMarkerAtV1, DraftPieceMarkerMoveV1, DraftPieceMarkerV1,
    DraftPieceOperationIdV1, DraftPieceOperationStatusV1, DraftPieceOperationVerificationV1,
    DraftPiecePrepareErrorV1, DraftPieceRejectedReasonV1, DraftPieceReplacementV1,
    DraftPieceSettlementOutcomeV1, DraftPieceTextDemandV1, DraftPieceTransactionOutcomeV1,
    DraftPieceV1, PreparedDraftPieceEditV1, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    canonical_draft_piece_fragment_chain_v1, canonical_empty_draft_piece_fragment_chain_v1,
};
#[cfg(feature = "test-faults")]
use syndic_storage::{
    DraftPieceBuildFragmentKeyV1, DraftPieceBuildFrontierV1,
    test_faults::{
        DraftEditorCandidateOpenReceiptCorruption, DraftPieceBuildCorruption,
        DraftPieceCandidateRootCollision, DraftPieceDescendantCorruption,
        DraftPieceDescendantTarget, DraftPieceFragmentCorruption, DraftPieceImmutableDeletion,
        DraftPieceProgressReceiptCorruption, delete_draft_piece_immutable_record,
        delete_draft_piece_terminal_build, draft_piece_fragment_is_stored_exactly,
        draft_piece_fragment_zero_ordinal_codec_rejections,
        inject_draft_editor_candidate_open_receipt_corruption,
        inject_draft_editor_candidate_session_published_beyond_newest,
        inject_draft_piece_build_corruption, inject_draft_piece_candidate_root_collision,
        inject_draft_piece_coordinated_stage_target_replacement,
        inject_draft_piece_custody_endpoint_corruption, inject_draft_piece_descendant_corruption,
        inject_draft_piece_fragment_ahead, inject_draft_piece_fragment_corruption,
        inject_draft_piece_occupied_stage_target, inject_draft_piece_progress_receipt_corruption,
        inject_draft_piece_session_generation_inflation,
        inject_draft_piece_settlement_closure_corruption,
    },
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase142-{name}-{}-{}",
            std::process::id(),
            NEXT_HOME.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone)]
struct Transaction {
    prepared: PreparedDraftPieceEditV1,
    fragments: Vec<DraftPieceBuildFragmentV1>,
}

#[cfg(feature = "test-faults")]
#[test]
fn fragment_ordinals_are_one_based_in_codec_and_durable_storage() {
    let (_home, store, storage, thread) = fixture("one-based-fragment-ordinals", 6);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 7, 8);
    let edit = transaction(
        storage,
        &session,
        9,
        vec![
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("first".to_owned())],
            ),
            DraftPieceReplacementV1::continuation(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("second".to_owned())],
            ),
        ],
        point(11),
    );
    assert_eq!(edit.fragments[0].key().ordinal(), 1);
    assert_eq!(edit.fragments[1].key().ordinal(), 2);
    assert_eq!(
        draft_piece_fragment_zero_ordinal_codec_rejections(&edit.fragments[0]),
        [true, true, true, true]
    );

    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    assert!(draft_piece_fragment_is_stored_exactly(
        &store,
        storage,
        &edit.fragments[0],
    ));
}

#[test]
fn large_continued_edit_advances_only_the_named_candidate() {
    let (home, store, storage, thread) = fixture("candidate-only", 10);
    let durable_before = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable_before, 20, 21);
    let first = "a".repeat(60_000);
    let second = "é".repeat(20_000);
    let fragments = vec![
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text(first.clone())]),
        DraftPieceReplacementV1::continuation(
            point(0),
            point(0),
            vec![DraftPieceV1::Text(second.clone())],
        ),
    ];
    let edit = transaction(storage, &session, 22, fragments, point(100_000));
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    for fragment in &edit.fragments {
        committed(execute(
            &store,
            storage.stage_draft_piece_fragment(
                storage.revision(&store).unwrap(),
                edit.prepared.clone(),
                fragment.clone(),
            ),
        ));
    }
    let first_advance = storage
        .prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            edit.prepared.header().operation_id(),
        )
        .unwrap()
        .expect("large edit unexpectedly completed before its first advance");
    committed(execute(
        &store,
        storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), first_advance),
    ));
    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            edit.prepared.header().operation_id(),
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
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let settlement = settled(storage, &store, &edit);
    let DraftPieceSettlementOutcomeV1::Committed {
        candidate_generation,
        successor,
        ..
    } = settlement.outcome()
    else {
        panic!("candidate edit did not commit")
    };
    assert_eq!(*candidate_generation, 1);
    assert_eq!(successor.summary().logical_utf8_bytes(), 100_000);

    let adopted = match settlement.closure() {
        syndic_storage::DraftPieceSettlementClosureV1::Committed(adoption) => {
            adoption.adopted_session().clone()
        }
        _ => panic!("committed settlement lacked adoption closure"),
    };
    let expected = format!("{first}{second}").into_bytes();
    assert_eq!(candidate_bytes(storage, &store, &adopted), expected);
    assert_eq!(current(storage, &store, thread), durable_before);

    let deletion = transaction(
        storage,
        &adopted,
        23,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(100_000),
            Vec::new(),
        )],
        point(0),
    );
    begin_stage_build(storage, &store, &deletion);
    committed(execute(
        &store,
        storage
            .settle_draft_piece_edit(storage.revision(&store).unwrap(), deletion.prepared.clone()),
    ));
    let emptied = adopted_head(storage, &store, &deletion);
    assert_eq!(emptied.newest_candidate_generation(), 2);
    assert!(candidate_bytes(storage, &store, &emptied).is_empty());
    assert_eq!(current(storage, &store, thread), durable_before);

    drop(store);
    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(current(reopened_storage, &reopened, thread), durable_before);
    let fresh = open_session(reopened_storage, &reopened, &durable_before, 24, 25);
    assert_eq!(fresh.newest_candidate_generation(), 0);
    assert!(candidate_bytes(reopened_storage, &reopened, &fresh).is_empty());
}

#[test]
fn same_anchor_move_adjacent_ranges_and_candidate_drift_are_exact() {
    let (_home, store, storage, thread) = fixture("marker-and-drift", 30);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 31, 32);
    let left = marker(33, 1, 1);
    let right = marker(34, 2, 2);
    let seed = transaction(
        storage,
        &session,
        35,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![
                DraftPieceV1::Text("ab".to_owned()),
                DraftPieceV1::Marker(left),
                DraftPieceV1::Marker(right),
                DraftPieceV1::Text("cd".to_owned()),
            ],
        )],
        point(4),
    );
    begin_stage_build(storage, &store, &seed);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), seed.prepared.clone()),
    ));
    let head = adopted_head(storage, &store, &seed);
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                head.newest_root(),
                DraftPieceMarkerAtV1::new(2, left),
            )
            .unwrap()
    );
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                head.newest_root(),
                DraftPieceMarkerAtV1::new(2, right),
            )
            .unwrap()
    );

    let winner = transaction(
        storage,
        &head,
        36,
        vec![
            DraftPieceReplacementV1::new(
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::AfterAll),
                Vec::new(),
            ),
            DraftPieceReplacementV1::new(
                point(4),
                point(4),
                vec![DraftPieceV1::Marker(left), DraftPieceV1::Marker(right)],
            )
            .with_moves(vec![
                DraftPieceMarkerMoveV1::new(DraftPieceMarkerAtV1::new(2, left), left, 1),
                DraftPieceMarkerMoveV1::new(DraftPieceMarkerAtV1::new(2, right), right, 1),
            ]),
        ],
        DraftCompositePositionV1::new(4, DraftCompositeGapWitnessV1::AfterAll),
    );
    let loser = transaction(
        storage,
        &head,
        37,
        vec![
            DraftPieceReplacementV1::new(
                point(0),
                point(1),
                vec![DraftPieceV1::Text("X".to_owned())],
            ),
            DraftPieceReplacementV1::new(
                point(1),
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
                vec![DraftPieceV1::Text("Y".to_owned())],
            ),
        ],
        point(4),
    );
    begin_stage_build(storage, &store, &winner);
    not_committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), loser.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), winner.prepared.clone()),
    ));
    not_committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), loser.prepared.clone()),
    ));
    let moved = adopted_head(storage, &store, &winner);
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                moved.newest_root(),
                DraftPieceMarkerAtV1::new(4, left),
            )
            .unwrap()
    );
    assert_eq!(current(storage, &store, thread), durable);
}

#[test]
fn marker_order_slots_and_declared_move_pairing_are_exact() {
    let (_home, store, storage, thread) = fixture("marker-order-and-move-pairing", 40);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 41, 42);
    let original = marker(43, 1, 7);
    let seed = transaction(
        storage,
        &session,
        44,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![
                DraftPieceV1::Text("ab".to_owned()),
                DraftPieceV1::Marker(original),
                DraftPieceV1::Text("cd".to_owned()),
            ],
        )],
        point(4),
    );
    begin_stage_build(storage, &store, &seed);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), seed.prepared.clone()),
    ));
    let seeded = adopted_head(storage, &store, &seed);

    let successor = marker(43, 3, 7);
    let same_anchor = transaction(
        storage,
        &seeded,
        45,
        vec![
            DraftPieceReplacementV1::new(
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::AfterAll),
                vec![DraftPieceV1::Marker(successor)],
            )
            .with_moves(vec![DraftPieceMarkerMoveV1::new(
                DraftPieceMarkerAtV1::new(2, original),
                successor,
                1,
            )]),
        ],
        DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::AfterAll),
    );
    begin_stage_build(storage, &store, &same_anchor);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(
            storage.revision(&store).unwrap(),
            same_anchor.prepared.clone(),
        ),
    ));
    let mut head = adopted_head(storage, &store, &same_anchor);
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                head.newest_root(),
                DraftPieceMarkerAtV1::new(2, successor),
            )
            .unwrap()
    );

    let same_slot = transaction(
        storage,
        &head,
        46,
        vec![DraftPieceReplacementV1::new(
            DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
            DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
            vec![DraftPieceV1::Marker(marker(47, 3, 8))],
        )],
        point(4),
    );
    assert_eq!(
        build_and_reject(storage, &store, &same_slot),
        DraftPieceRejectedReasonV1::TreeLimit
    );
    head = settled_head(storage, &store, &same_slot);

    let relabeled = marker(43, 4, 8);
    let relabel = transaction(
        storage,
        &head,
        48,
        vec![
            DraftPieceReplacementV1::new(
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::AfterAll),
                vec![DraftPieceV1::Marker(relabeled)],
            )
            .with_moves(vec![DraftPieceMarkerMoveV1::new(
                DraftPieceMarkerAtV1::new(2, successor),
                relabeled,
                1,
            )]),
        ],
        point(4),
    );
    assert_eq!(
        build_and_reject(storage, &store, &relabel),
        DraftPieceRejectedReasonV1::DuplicateMarkerIdentity
    );
    head = settled_head(storage, &store, &relabel);

    let no_matching_removal = transaction(
        storage,
        &head,
        49,
        vec![
            DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Marker(successor)])
                .with_moves(vec![DraftPieceMarkerMoveV1::new(
                    DraftPieceMarkerAtV1::new(2, successor),
                    successor,
                    1,
                )]),
        ],
        point(4),
    );
    assert_eq!(
        build_and_reject(storage, &store, &no_matching_removal),
        DraftPieceRejectedReasonV1::Overlap
    );
    head = settled_head(storage, &store, &no_matching_removal);

    let no_matching_insertion = transaction(
        storage,
        &head,
        50,
        vec![
            DraftPieceReplacementV1::new(
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::AfterAll),
                Vec::new(),
            )
            .with_moves(vec![DraftPieceMarkerMoveV1::new(
                DraftPieceMarkerAtV1::new(2, successor),
                successor,
                1,
            )]),
        ],
        point(4),
    );
    assert_eq!(
        build_and_reject(storage, &store, &no_matching_insertion),
        DraftPieceRejectedReasonV1::TooManyReplacements
    );
    head = settled_head(storage, &store, &no_matching_insertion);

    let undeclared = transaction(
        storage,
        &head,
        51,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Marker(successor)],
        )],
        point(4),
    );
    assert_eq!(
        build_and_reject(storage, &store, &undeclared),
        DraftPieceRejectedReasonV1::DuplicateMarkerIdentity
    );
    head = settled_head(storage, &store, &undeclared);

    let duplicate_declarations = transaction(
        storage,
        &head,
        52,
        vec![
            DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Marker(successor)])
                .with_moves(vec![DraftPieceMarkerMoveV1::new(
                    DraftPieceMarkerAtV1::new(2, successor),
                    successor,
                    3,
                )]),
            DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(successor)])
                .with_moves(vec![DraftPieceMarkerMoveV1::new(
                    DraftPieceMarkerAtV1::new(2, successor),
                    successor,
                    3,
                )]),
            DraftPieceReplacementV1::new(
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::AfterAll),
                Vec::new(),
            ),
        ],
        point(4),
    );
    assert_eq!(
        build_and_reject(storage, &store, &duplicate_declarations),
        DraftPieceRejectedReasonV1::DuplicateMarkerIdentity
    );
    head = settled_head(storage, &store, &duplicate_declarations);
    assert_eq!(candidate_bytes(storage, &store, &head), b"abcd");
    assert_eq!(current(storage, &store, thread), durable);
}

#[test]
fn multi_range_delete_reinsert_requires_exact_predecessor_declared_move() {
    for (case, backward) in [
        ("move-forward-ranges", false),
        ("move-backward-ranges", true),
    ] {
        let (_home, store, storage, thread) = fixture(case, if backward { 150 } else { 144 });
        let durable = current(storage, &store, thread);
        let session = open_session(storage, &store, &durable, 145, 146);
        let original = marker(147, 1, 9);
        let seed = transaction(
            storage,
            &session,
            148,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![
                    DraftPieceV1::Text("ab".to_owned()),
                    DraftPieceV1::Marker(original),
                    DraftPieceV1::Text("cd".to_owned()),
                ],
            )],
            point(4),
        );
        begin_stage_build(storage, &store, &seed);
        committed(execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), seed.prepared.clone()),
        ));
        let seeded = adopted_head(storage, &store, &seed);
        let removal = DraftPieceReplacementV1::new(
            DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
            DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::AfterAll),
            Vec::new(),
        );
        let successor_anchor = if backward { 0 } else { 4 };
        let successor = marker(147, 4, 9);
        let insertion = DraftPieceReplacementV1::new(
            point(successor_anchor),
            point(successor_anchor),
            vec![DraftPieceV1::Marker(successor)],
        );
        let undeclared_replacements = if backward {
            vec![insertion.clone(), removal.clone()]
        } else {
            vec![removal.clone(), insertion.clone()]
        };
        let final_caret = if backward {
            point(4)
        } else {
            DraftCompositePositionV1::new(4, DraftCompositeGapWitnessV1::AfterAll)
        };
        let undeclared = transaction(storage, &seeded, 149, undeclared_replacements, final_caret);
        assert_eq!(
            build_and_reject(storage, &store, &undeclared),
            DraftPieceRejectedReasonV1::DuplicateMarkerIdentity
        );

        let removal_ordinal = u64::from(backward) + 1;
        let declared_insertion = insertion
            .clone()
            .with_moves(vec![DraftPieceMarkerMoveV1::new(
                DraftPieceMarkerAtV1::new(2, original),
                successor,
                removal_ordinal,
            )]);
        let declared_replacements = if backward {
            vec![declared_insertion, removal]
        } else {
            vec![removal, declared_insertion]
        };
        let session_after_rejection = active_session(storage, &store, &seeded);
        let declared = transaction(
            storage,
            &session_after_rejection,
            151,
            declared_replacements,
            final_caret,
        );
        committed(execute(
            &store,
            storage.begin_draft_piece_edit(
                storage.revision(&store).unwrap(),
                declared.prepared.clone(),
            ),
        ));
        for fragment in &declared.fragments {
            committed(execute(
                &store,
                storage.stage_draft_piece_fragment(
                    storage.revision(&store).unwrap(),
                    declared.prepared.clone(),
                    fragment.clone(),
                ),
            ));
        }
        loop {
            let before = status(storage, &store, &declared);
            match storage.prepare_draft_piece_build_advance(
                &store,
                seeded.draft_id(),
                seeded.session_id(),
                declared.prepared.header().operation_id(),
            ) {
                Ok(Some(advance)) => committed(execute(
                    &store,
                    storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
                )),
                Ok(None) => break,
                Err(error) => panic!("{case} failed from {before:?}: {error:?}"),
            }
        }
        committed(execute(
            &store,
            storage.settle_draft_piece_edit(
                storage.revision(&store).unwrap(),
                declared.prepared.clone(),
            ),
        ));
        let moved = adopted_head(storage, &store, &declared);
        assert!(
            storage
                .validate_draft_marker_location(
                    &store,
                    moved.newest_root(),
                    DraftPieceMarkerAtV1::new(successor_anchor, successor),
                )
                .unwrap()
        );
        assert_eq!(current(storage, &store, thread), durable);
    }
}

#[test]
fn text_bearing_child_boundary_preserves_marker_order_slot_uniqueness() {
    let (_home, store, storage, thread) = fixture("text-marker-child-boundary", 53);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 54, 55);
    let mut pieces = vec![DraftPieceV1::Text("x".to_owned())];
    pieces
        .extend((1_u8..=129).map(|order| {
            DraftPieceV1::Marker(marker(order.wrapping_add(60), u64::from(order), 1))
        }));
    let seed = transaction(
        storage,
        &session,
        56,
        vec![DraftPieceReplacementV1::new(point(0), point(0), pieces)],
        DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll),
    );
    begin_stage_build(storage, &store, &seed);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), seed.prepared.clone()),
    ));
    let mut head = adopted_head(storage, &store, &seed);
    let collision = transaction(
        storage,
        &head,
        57,
        vec![DraftPieceReplacementV1::new(
            DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll),
            DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll),
            vec![DraftPieceV1::Marker(marker(250, 129, 2))],
        )],
        DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll),
    );
    assert_eq!(
        build_and_reject(storage, &store, &collision),
        DraftPieceRejectedReasonV1::TreeLimit
    );
    head = settled_head(storage, &store, &collision);
    assert_eq!(candidate_bytes(storage, &store, &head), b"x");
}

#[test]
fn replay_collision_cancellation_and_old_session_isolation_are_closed() {
    let (_home, store, storage, thread) = fixture("terminals", 50);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 51, 52);
    let cancelled = transaction(
        storage,
        &session,
        53,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("cancel".to_owned())],
        )],
        point(6),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
        ),
    ));
    let claimed = active_session(storage, &store, &session);
    assert_eq!(
        claimed
            .active_operation()
            .unwrap()
            .receipt()
            .key()
            .transition_ordinal(),
        1
    );
    replay_succeeded(execute(
        &store,
        storage.begin_draft_piece_edit(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
        ),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
            cancelled.fragments[0].clone(),
        ),
    ));
    let staged = active_session(storage, &store, &session);
    assert_eq!(
        staged
            .active_operation()
            .unwrap()
            .receipt()
            .key()
            .transition_ordinal(),
        2
    );
    not_committed(execute(
        &store,
        storage.begin_draft_piece_edit(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
        ),
    ));
    assert_eq!(active_session(storage, &store, &session), staged);
    replay_succeeded(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
            cancelled.fragments[0].clone(),
        ),
    ));
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(
            storage.revision(&store).unwrap(),
            cancelled.prepared.clone(),
        ),
    ));
    assert!(matches!(
        settled(storage, &store, &cancelled).outcome(),
        DraftPieceSettlementOutcomeV1::Cancelled
    ));
    assert!(
        active_session(storage, &store, &session)
            .active_operation()
            .is_none()
    );

    let session_after_cancel = active_session(storage, &store, &session);
    let accepted = transaction(
        storage,
        &session_after_cancel,
        54,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("accepted".to_owned())],
        )],
        point(8),
    );
    begin_stage_build(storage, &store, &accepted);
    committed(execute(
        &store,
        storage
            .settle_draft_piece_edit(storage.revision(&store).unwrap(), accepted.prepared.clone()),
    ));
    assert!(matches!(
        settled(storage, &store, &accepted).outcome(),
        DraftPieceSettlementOutcomeV1::Committed { .. }
    ));
    let accepted_session = adopted_head(storage, &store, &accepted);
    let colliding = transaction(
        storage,
        &accepted_session,
        54,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("different".to_owned())],
        )],
        point(9),
    );
    assert!(matches!(
        status(storage, &store, &colliding),
        DraftPieceOperationStatusV1::Collision(_)
    ));

    let isolated = open_session(storage, &store, &durable, 55, 56);
    let isolated_edit = transaction(
        storage,
        &isolated,
        57,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("isolated".to_owned())],
        )],
        point(8),
    );
    begin_stage_build(storage, &store, &isolated_edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(
            storage.revision(&store).unwrap(),
            isolated_edit.prepared.clone(),
        ),
    ));
    assert_eq!(
        candidate_bytes(storage, &store, &adopted_head(storage, &store, &accepted)),
        b"accepted"
    );
    assert_eq!(
        candidate_bytes(
            storage,
            &store,
            &adopted_head(storage, &store, &isolated_edit)
        ),
        b"isolated"
    );
    assert_eq!(current(storage, &store, thread), durable);
}

#[test]
fn partial_and_terminal_first_cancellation_reconcile_staged_endpoints() {
    let (_home, store, storage, thread) = fixture("partial-terminal-cancellation", 58);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 59, 60);
    let partial = transaction(
        storage,
        &session,
        61,
        vec![
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("one".to_owned())],
            ),
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("two".to_owned())],
            ),
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("three".to_owned())],
            ),
        ],
        point(13),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), partial.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            partial.prepared.clone(),
            partial.fragments[0].clone(),
        ),
    ));
    let partial_fragments = partial.fragments.clone();
    let partial_outcome = execute(
        &store,
        storage
            .cancel_draft_piece_edit(storage.revision(&store).unwrap(), partial.prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(
                &store,
                &partial.prepared,
                partial_outcome,
                |start| partial_fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .cloned()
                    .collect(),
            )
            .unwrap(),
        syndic_storage::DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Cancelled(_)
        )
    ));
    let partial_settlement = settled(storage, &store, &partial);
    assert_eq!(partial_settlement.fragment_count(), 3);
    let partial_source = partial_settlement
        .terminal_source()
        .expect("partial cancellation retains its exact staged source");
    assert_eq!(partial_source.staged_fragment_count(), 1);
    assert_eq!(
        partial_source.staged_fragment_chain(),
        partial.fragments[0].chain_digest()
    );
    assert_eq!(
        partial_settlement
            .terminal_receipt()
            .key()
            .transition_ordinal(),
        3
    );
    assert!(
        active_session(storage, &store, &session)
            .active_operation()
            .is_none()
    );
    replay_succeeded(execute(
        &store,
        storage
            .cancel_draft_piece_edit(storage.revision(&store).unwrap(), partial.prepared.clone()),
    ));
    not_committed(execute(
        &store,
        storage
            .settle_draft_piece_edit(storage.revision(&store).unwrap(), partial.prepared.clone()),
    ));
    not_committed(execute(
        &store,
        storage.error_draft_piece_edit(
            storage.revision(&store).unwrap(),
            partial.prepared.clone(),
            DraftPieceErrorReasonV1::ResourceLimit,
        ),
    ));
    assert_eq!(settled(storage, &store, &partial), partial_settlement);

    let terminal_first_source = active_session(storage, &store, &session);
    let terminal_first = transaction(
        storage,
        &terminal_first_source,
        62,
        vec![
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("never staged".to_owned())],
            ),
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("also unstaged".to_owned())],
            ),
        ],
        point(25),
    );
    let terminal_first_fragments = terminal_first.fragments.clone();
    let terminal_first_outcome = execute(
        &store,
        storage.cancel_draft_piece_edit(
            storage.revision(&store).unwrap(),
            terminal_first.prepared.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(
                &store,
                &terminal_first.prepared,
                terminal_first_outcome,
                |start| terminal_first_fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .cloned()
                    .collect(),
            )
            .unwrap(),
        syndic_storage::DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Cancelled(_)
        )
    ));
    let terminal_first_settlement = settled(storage, &store, &terminal_first);
    assert_eq!(terminal_first_settlement.fragment_count(), 2);
    assert!(terminal_first_settlement.terminal_source().is_none());
    assert_eq!(
        terminal_first_settlement
            .terminal_receipt()
            .key()
            .transition_ordinal(),
        1
    );
    assert!(
        active_session(storage, &store, &session)
            .active_operation()
            .is_none()
    );
    replay_succeeded(execute(
        &store,
        storage.cancel_draft_piece_edit(
            storage.revision(&store).unwrap(),
            terminal_first.prepared.clone(),
        ),
    ));
    not_committed(execute(
        &store,
        storage.error_draft_piece_edit(
            storage.revision(&store).unwrap(),
            terminal_first.prepared.clone(),
            DraftPieceErrorReasonV1::ResourceLimit,
        ),
    ));
    not_committed(execute(
        &store,
        storage.begin_draft_piece_edit(
            storage.revision(&store).unwrap(),
            terminal_first.prepared.clone(),
        ),
    ));
    assert_eq!(
        settled(storage, &store, &terminal_first),
        terminal_first_settlement
    );
    assert_eq!(current(storage, &store, thread), durable);
}

#[test]
fn terminal_first_rejects_a_clean_session_generation_race_before_claim() {
    let (_home, store, storage, thread) = fixture("terminal-first-source-race", 63);
    let durable = current(storage, &store, thread);
    let source = open_session(storage, &store, &durable, 64, 65);
    let stale = transaction(
        storage,
        &source,
        66,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("stale".to_owned())],
        )],
        point(5),
    );
    let intervening = transaction(
        storage,
        &source,
        67,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("intervening".to_owned())],
        )],
        point(12),
    );
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(
            storage.revision(&store).unwrap(),
            intervening.prepared.clone(),
        ),
    ));
    assert!(matches!(
        status(storage, &store, &intervening),
        DraftPieceOperationStatusV1::Settled(_)
    ));
    let advanced = active_session(storage, &store, &source);
    assert_eq!(
        advanced.session_generation(),
        source.session_generation() + 2
    );
    assert_eq!(
        advanced.newest_candidate_generation(),
        source.newest_candidate_generation()
    );
    assert_eq!(advanced.newest_root(), source.newest_root());
    assert_eq!(current(storage, &store, thread), durable);

    let revision = storage.revision(&store).unwrap();
    not_committed(execute(
        &store,
        storage.cancel_draft_piece_edit(revision, stale.prepared.clone()),
    ));
    assert_eq!(storage.revision(&store).unwrap(), revision);
    assert_eq!(active_session(storage, &store, &source), advanced);
    assert!(matches!(
        status(storage, &store, &stale),
        DraftPieceOperationStatusV1::Absent
    ));

    let fresh = transaction(
        storage,
        &advanced,
        66,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("stale".to_owned())],
        )],
        point(5),
    );
    let fresh_fragments = fresh.fragments.clone();
    let outcome = execute(
        &store,
        storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), fresh.prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(&store, &fresh.prepared, outcome, |start| {
                fresh_fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .cloned()
                    .collect()
            },)
            .unwrap(),
        syndic_storage::DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Cancelled(_)
        )
    ));
    assert!(settled(storage, &store, &fresh).terminal_source().is_none());
    assert_eq!(current(storage, &store, thread), durable);
}

#[test]
fn rejected_error_duplicate_empty_and_marker_only_paths_are_exact() {
    let (_home, store, storage, thread) = fixture("five-way-and-marker", 60);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 61, 62);
    let empty_header = DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        DraftPieceOperationIdV1::from_bytes([60; 16]),
        point(0),
        point(0),
        0,
        canonical_empty_draft_piece_fragment_chain_v1(),
    );
    let empty = Transaction {
        prepared: storage
            .prepare_draft_piece_edit(empty_header, &session)
            .unwrap(),
        fragments: Vec::new(),
    };
    assert_eq!(
        empty.prepared.prebuild_rejection(),
        Some(DraftPieceRejectedReasonV1::EmptyTransaction)
    );
    let retry = execute(
        &store,
        storage.reject_draft_piece_edit(
            storage.revision(&store).unwrap(),
            empty.prepared.clone(),
            DraftPieceRejectedReasonV1::EmptyTransaction,
        ),
    );
    assert!(matches!(
        retry,
        CommandOutcome::NotCommitted { .. }
            | CommandOutcome::Committed {
                later_failure: None,
                ..
            }
    ));
    let first_empty_settlement = settled(storage, &store, &empty);
    assert!(first_empty_settlement.terminal_source().is_none());
    assert_eq!(
        first_empty_settlement
            .terminal_receipt()
            .key()
            .transition_ordinal(),
        1
    );
    assert!(
        active_session(storage, &store, &session)
            .active_operation()
            .is_none()
    );
    assert!(matches!(
        first_empty_settlement.outcome(),
        DraftPieceSettlementOutcomeV1::Rejected(DraftPieceRejectedReasonV1::EmptyTransaction)
    ));
    let retry = execute(
        &store,
        storage.reject_draft_piece_edit(
            storage.revision(&store).unwrap(),
            empty.prepared.clone(),
            DraftPieceRejectedReasonV1::EmptyTransaction,
        ),
    );
    assert!(matches!(
        retry,
        CommandOutcome::NotCommitted { .. }
            | CommandOutcome::Committed {
                later_failure: None,
                ..
            }
    ));
    assert_eq!(settled(storage, &store, &empty), first_empty_settlement);
    let session_after_empty = active_session(storage, &store, &session);
    let duplicate = transaction(
        storage,
        &session_after_empty,
        63,
        vec![
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("a".to_owned())],
            ),
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("b".to_owned())],
            ),
        ],
        point(0),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(
            storage.revision(&store).unwrap(),
            duplicate.prepared.clone(),
        ),
    ));
    for fragment in &duplicate.fragments {
        committed(execute(
            &store,
            storage.stage_draft_piece_fragment(
                storage.revision(&store).unwrap(),
                duplicate.prepared.clone(),
                fragment.clone(),
            ),
        ));
    }
    let reason = loop {
        match storage.prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            duplicate.prepared.header().operation_id(),
        ) {
            Err(DraftPiecePrepareErrorV1::Rejected(reason)) => break reason,
            Ok(Some(advance)) => committed(execute(
                &store,
                storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
            )),
            Ok(None) => panic!("duplicate empty ranges unexpectedly completed"),
            Err(error) => panic!("unexpected duplicate-range error: {error:?}"),
        }
    };
    assert_eq!(reason, DraftPieceRejectedReasonV1::DuplicateEmptyRange);
    committed(execute(
        &store,
        storage.reject_draft_piece_edit(
            storage.revision(&store).unwrap(),
            duplicate.prepared.clone(),
            reason,
        ),
    ));
    assert!(matches!(
        settled(storage, &store, &duplicate).outcome(),
        DraftPieceSettlementOutcomeV1::Rejected(DraftPieceRejectedReasonV1::DuplicateEmptyRange)
    ));

    let session_after_duplicate = active_session(storage, &store, &session);
    let failed = transaction(
        storage,
        &session_after_duplicate,
        64,
        vec![DraftPieceReplacementV1::new(point(0), point(0), Vec::new())],
        point(0),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), failed.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            failed.prepared.clone(),
            failed.fragments[0].clone(),
        ),
    ));
    committed(execute(
        &store,
        storage.error_draft_piece_edit(
            storage.revision(&store).unwrap(),
            failed.prepared.clone(),
            DraftPieceErrorReasonV1::ResourceLimit,
        ),
    ));
    assert!(matches!(
        settled(storage, &store, &failed).outcome(),
        DraftPieceSettlementOutcomeV1::Error(DraftPieceErrorReasonV1::ResourceLimit)
    ));

    let object = marker(65, 1, 1);
    let session_after_error = active_session(storage, &store, &session);
    let marker_only = transaction(
        storage,
        &session_after_error,
        66,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Marker(object)],
        )],
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll),
    );
    begin_stage_build(storage, &store, &marker_only);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(
            storage.revision(&store).unwrap(),
            marker_only.prepared.clone(),
        ),
    ));
    let head = adopted_head(storage, &store, &marker_only);
    assert!(candidate_bytes(storage, &store, &head).is_empty());
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                head.newest_root(),
                DraftPieceMarkerAtV1::new(0, object),
            )
            .unwrap()
    );
    assert_eq!(current(storage, &store, thread), durable);
}

#[cfg(feature = "test-faults")]
#[test]
fn indeterminate_begin_fragment_advance_and_adoption_reconcile_from_durable_identity() {
    let home = TestHome::new("fault-cuts");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(storage, &store, 70);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 71, 72);
    let edit = transaction(
        storage,
        &session,
        73,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("faulted".to_owned())],
        )],
        point(7),
    );
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let begin = execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    );
    assert!(matches!(begin, CommandOutcome::Indeterminate { .. }));
    assert!(matches!(
        status(storage, &store, &edit),
        DraftPieceOperationStatusV1::Open(_)
    ));

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let staged = execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    );
    assert!(matches!(staged, CommandOutcome::Indeterminate { .. }));
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            edit.prepared.header().operation_id(),
        )
        .unwrap()
    {
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let outcome = execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        );
        assert!(matches!(outcome, CommandOutcome::Indeterminate { .. }));
    }
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let outcome = execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    );
    let fragments = edit.fragments.clone();
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(&store, &edit.prepared, outcome, |start| {
                fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .cloned()
                    .collect()
            })
            .unwrap(),
        syndic_storage::DraftPieceReconciledCommandV1::Terminal(
            DraftPieceTransactionOutcomeV1::Committed(_)
        )
    ));
    assert_eq!(current(storage, &store, thread), durable);

    let disposable = open_session(storage, &store, &durable, 74, 75);
    let stale_completion = transaction(
        storage,
        &disposable,
        76,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("disposed".to_owned())],
        )],
        point(8),
    );
    begin_stage_build(storage, &store, &stale_completion);
    not_committed(execute(
        &store,
        storage.test_dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            disposable.draft_id(),
            disposable.session_id(),
        ),
    ));
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(
            storage.revision(&store).unwrap(),
            stale_completion.prepared.clone(),
        ),
    ));
    assert!(matches!(
        settled(storage, &store, &stale_completion).outcome(),
        DraftPieceSettlementOutcomeV1::Cancelled
    ));
    committed(execute(
        &store,
        storage.test_dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            disposable.draft_id(),
            disposable.session_id(),
        ),
    ));

    let after_disposal = transaction(
        storage,
        &disposable,
        77,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("absent".to_owned())],
        )],
        point(6),
    );
    assert!(matches!(
        execute(
            &store,
            storage.begin_draft_piece_edit(
                storage.revision(&store).unwrap(),
                after_disposal.prepared.clone(),
            ),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        status(storage, &store, &after_disposal),
        DraftPieceOperationStatusV1::Absent
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn occupied_roots_and_impossible_session_or_build_records_fail_closed() {
    for (case, collision) in [
        ("exact-root", DraftPieceCandidateRootCollision::Exact),
        (
            "different-root",
            DraftPieceCandidateRootCollision::DifferentCanonicalBytes,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 80);
        let durable = current(storage, &store, thread);
        let session = open_session(storage, &store, &durable, 81, 82);
        let edit = transaction(
            storage,
            &session,
            83,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("root".to_owned())],
            )],
            point(4),
        );
        begin_stage_build(storage, &store, &edit);
        let DraftPieceOperationStatusV1::Complete(build) = status(storage, &store, &edit) else {
            panic!("fixture build is not complete")
        };
        let successor = build.successor().unwrap();
        let collision_outcome = execute(
            &store,
            inject_draft_piece_candidate_root_collision(&store, storage, successor, collision),
        );
        committed(collision_outcome);
        let session_before = active_session(storage, &store, &session);
        not_committed(execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        ));
        assert_eq!(active_session(storage, &store, &session), session_before);
        assert!(
            storage
                .draft_piece_operation_status_page(&store, &edit.prepared, 1, &edit.fragments)
                .is_err()
        );
        not_committed(execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        ));
        assert_eq!(current(storage, &store, thread), durable);
    }

    let (_home, store, storage, thread) = fixture("session-frontier-corruption", 84);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 85, 86);
    committed(execute(
        &store,
        inject_draft_editor_candidate_session_published_beyond_newest(
            &store,
            storage,
            session.draft_id(),
            session.session_id(),
        ),
    ));
    assert!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .is_err()
    );

    for (case, corruption, settle_first) in [
        (
            "malformed-open-build",
            DraftPieceBuildCorruption::OpenWithCompleteFrontier,
            false,
        ),
        (
            "malformed-complete-build",
            DraftPieceBuildCorruption::CompleteWithoutSuccessor,
            false,
        ),
        (
            "malformed-terminal-build",
            DraftPieceBuildCorruption::CompleteWithoutSuccessor,
            true,
        ),
        (
            "disagreeing-terminal-build",
            DraftPieceBuildCorruption::TerminalLifecycle,
            true,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 87);
        let durable = current(storage, &store, thread);
        let session = open_session(storage, &store, &durable, 88, 89);
        let edit = transaction(
            storage,
            &session,
            90,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("build".to_owned())],
            )],
            point(5),
        );
        begin_stage_build(storage, &store, &edit);
        if settle_first {
            committed(execute(
                &store,
                storage.settle_draft_piece_edit(
                    storage.revision(&store).unwrap(),
                    edit.prepared.clone(),
                ),
            ));
        }
        committed(execute(
            &store,
            inject_draft_piece_build_corruption(
                &store,
                storage,
                syndic_storage::DraftPieceSettlementKeyV1::new(
                    edit.prepared.header().draft_id(),
                    edit.prepared.header().session_id(),
                    edit.prepared.header().operation_id(),
                ),
                corruption,
            ),
        ));
        assert!(
            storage
                .draft_piece_operation_status_page(&store, &edit.prepared, 1, &edit.fragments,)
                .is_err(),
            "corruption {corruption:?} was accepted"
        );
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn move_frontier_reopens_and_duplicate_marker_order_corruption_fails_closed() {
    let home = TestHome::new("move-frontier-reopen");
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(storage, &store, 90);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 91, 92);
    let left = marker(93, 1, 1);
    let right = marker(94, 2, 2);
    let seed = transaction(
        storage,
        &session,
        95,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![
                DraftPieceV1::Text("ab".to_owned()),
                DraftPieceV1::Marker(left),
                DraftPieceV1::Marker(right),
                DraftPieceV1::Text("cd".to_owned()),
            ],
        )],
        point(4),
    );
    begin_stage_build(storage, &store, &seed);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), seed.prepared.clone()),
    ));
    let seeded = adopted_head(storage, &store, &seed);
    let moved = transaction(
        storage,
        &seeded,
        96,
        vec![
            DraftPieceReplacementV1::new(
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::AfterAll),
                vec![DraftPieceV1::Marker(left), DraftPieceV1::Marker(right)],
            )
            .with_moves(vec![
                DraftPieceMarkerMoveV1::new(DraftPieceMarkerAtV1::new(2, left), left, 1),
                DraftPieceMarkerMoveV1::new(DraftPieceMarkerAtV1::new(2, right), right, 1),
            ]),
        ],
        point(4),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), moved.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            moved.prepared.clone(),
            moved.fragments[0].clone(),
        ),
    ));
    let advance = storage
        .prepare_draft_piece_build_advance(
            &store,
            seeded.draft_id(),
            seeded.session_id(),
            moved.prepared.header().operation_id(),
        )
        .unwrap()
        .unwrap();
    committed(execute(
        &store,
        storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
    ));
    drop(store);

    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    let DraftPieceOperationStatusV1::Open(build) = status(reopened_storage, &reopened, &moved)
    else {
        panic!("move build did not reopen as open")
    };
    assert_eq!(
        build.frontier(),
        DraftPieceBuildFrontierV1::ReconcilingMoves {
            fragment_ordinal: 1,
            next_move: 1,
        }
    );
    while let Some(advance) = reopened_storage
        .prepare_draft_piece_build_advance(
            &reopened,
            seeded.draft_id(),
            seeded.session_id(),
            moved.prepared.header().operation_id(),
        )
        .unwrap()
    {
        committed(execute(
            &reopened,
            reopened_storage
                .advance_draft_piece_edit(reopened_storage.revision(&reopened).unwrap(), advance),
        ));
    }
    committed(execute(
        &reopened,
        reopened_storage.settle_draft_piece_edit(
            reopened_storage.revision(&reopened).unwrap(),
            moved.prepared.clone(),
        ),
    ));

    let corruption_session = open_session(reopened_storage, &reopened, &durable, 97, 98);
    let pieces = (0_u16..200)
        .map(|ordinal| DraftPieceV1::Marker(marker(ordinal as u8, ordinal as u64 + 1, 1)))
        .collect();
    let many_markers = transaction(
        reopened_storage,
        &corruption_session,
        99,
        vec![DraftPieceReplacementV1::new(point(0), point(0), pieces)],
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll),
    );
    begin_stage_build(reopened_storage, &reopened, &many_markers);
    committed(execute(
        &reopened,
        reopened_storage.settle_draft_piece_edit(
            reopened_storage.revision(&reopened).unwrap(),
            many_markers.prepared.clone(),
        ),
    ));
    let marker_head = adopted_head(reopened_storage, &reopened, &many_markers);
    committed(execute(
        &reopened,
        inject_draft_piece_descendant_corruption(
            &reopened,
            reopened_storage,
            marker_head.newest_root(),
            DraftPieceDescendantTarget::Sequence,
            DraftPieceDescendantCorruption::DuplicateMarkerOrderSlot,
        ),
    ));
    assert!(
        reopened_storage
            .validate_draft_marker_location(
                &reopened,
                marker_head.newest_root(),
                DraftPieceMarkerAtV1::new(0, marker(0, 1, 1)),
            )
            .is_err()
    );
}

#[cfg(feature = "test-faults")]
#[test]
fn fragment_authentication_phase_cursors_and_text_marker_boundaries_fail_closed() {
    for (case, corruption) in [
        (
            "replacement-bytes",
            DraftPieceFragmentCorruption::ReplacementBytes,
        ),
        ("chain", DraftPieceFragmentCorruption::ChainDigest),
        ("preceding", DraftPieceFragmentCorruption::PrecedingDigest),
        ("oversized", DraftPieceFragmentCorruption::OversizedText),
        ("empty-text", DraftPieceFragmentCorruption::EmptyText),
        (
            "continuation-moves",
            DraftPieceFragmentCorruption::ContinuationMoves,
        ),
        (
            "duplicate-moves",
            DraftPieceFragmentCorruption::DuplicateMoveDeclarations,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 101);
        let durable = current(storage, &store, thread);
        let session = open_session(storage, &store, &durable, 102, 103);
        let edit = transaction(
            storage,
            &session,
            104,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("source".to_owned())],
            )],
            point(6),
        );
        committed(execute(
            &store,
            storage
                .begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        ));
        committed(execute(
            &store,
            storage.stage_draft_piece_fragment(
                storage.revision(&store).unwrap(),
                edit.prepared.clone(),
                edit.fragments[0].clone(),
            ),
        ));
        inject_draft_piece_fragment_corruption(
            &store,
            storage,
            DraftPieceBuildFragmentKeyV1::new(
                session.draft_id(),
                session.session_id(),
                edit.prepared.header().operation_id(),
                1,
            ),
            corruption,
        )
        .unwrap();
        assert!(
            storage
                .prepare_draft_piece_build_advance(
                    &store,
                    session.draft_id(),
                    session.session_id(),
                    edit.prepared.header().operation_id(),
                )
                .is_err(),
            "fragment corruption {corruption:?} was consumed"
        );
    }

    for (case, corruption) in [
        (
            "cursor-reconciling",
            DraftPieceBuildCorruption::ReconcilingMovesNextMove,
        ),
        (
            "cursor-next-piece",
            DraftPieceBuildCorruption::InsertingNextPiece,
        ),
        (
            "cursor-next-byte",
            DraftPieceBuildCorruption::InsertingByteCursor,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 105);
        let durable = current(storage, &store, thread);
        let session = open_session(storage, &store, &durable, 106, 107);
        let edit = transaction(
            storage,
            &session,
            108,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("éclair".repeat(9_000))],
            )],
            point(0),
        );
        committed(execute(
            &store,
            storage
                .begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        ));
        committed(execute(
            &store,
            storage.stage_draft_piece_fragment(
                storage.revision(&store).unwrap(),
                edit.prepared.clone(),
                edit.fragments[0].clone(),
            ),
        ));
        if corruption != DraftPieceBuildCorruption::ReconcilingMovesNextMove {
            loop {
                let DraftPieceOperationStatusV1::Open(build) = status(storage, &store, &edit)
                else {
                    panic!("cursor fixture build stopped being open")
                };
                if matches!(
                    build.frontier(),
                    DraftPieceBuildFrontierV1::Inserting { .. }
                ) {
                    break;
                }
                let advance = storage
                    .prepare_draft_piece_build_advance(
                        &store,
                        session.draft_id(),
                        session.session_id(),
                        edit.prepared.header().operation_id(),
                    )
                    .unwrap()
                    .unwrap();
                committed(execute(
                    &store,
                    storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
                ));
            }
        }
        committed(execute(
            &store,
            inject_draft_piece_build_corruption(
                &store,
                storage,
                syndic_storage::DraftPieceSettlementKeyV1::new(
                    session.draft_id(),
                    session.session_id(),
                    edit.prepared.header().operation_id(),
                ),
                corruption,
            ),
        ));
        assert!(
            storage
                .prepare_draft_piece_build_advance(
                    &store,
                    session.draft_id(),
                    session.session_id(),
                    edit.prepared.header().operation_id(),
                )
                .is_err(),
            "build cursor corruption {corruption:?} advanced"
        );
    }

    let (_home, store, storage, thread) = fixture("cursor-phase-boundary", 109);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 110, 111);
    let seed = transaction(
        storage,
        &session,
        112,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("delete-me".to_owned())],
        )],
        point(9),
    );
    begin_stage_build(storage, &store, &seed);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), seed.prepared.clone()),
    ));
    let seeded = adopted_head(storage, &store, &seed);
    let deletion = transaction(
        storage,
        &seeded,
        113,
        vec![DraftPieceReplacementV1::new(point(0), point(9), Vec::new())],
        point(0),
    );
    committed(execute(
        &store,
        storage
            .begin_draft_piece_edit(storage.revision(&store).unwrap(), deletion.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            deletion.prepared.clone(),
            deletion.fragments[0].clone(),
        ),
    ));
    loop {
        let DraftPieceOperationStatusV1::Open(build) = status(storage, &store, &deletion) else {
            panic!("phase-boundary fixture build stopped being open")
        };
        if matches!(build.frontier(), DraftPieceBuildFrontierV1::Removing { .. }) {
            break;
        }
        let advance = storage
            .prepare_draft_piece_build_advance(
                &store,
                seeded.draft_id(),
                seeded.session_id(),
                deletion.prepared.header().operation_id(),
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
                seeded.draft_id(),
                seeded.session_id(),
                deletion.prepared.header().operation_id(),
            ),
            DraftPieceBuildCorruption::AdjacentPhaseBoundary,
        ),
    ));
    assert!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                seeded.draft_id(),
                seeded.session_id(),
                deletion.prepared.header().operation_id(),
            )
            .is_err()
    );

    let (_home, store, storage, thread) = fixture("text-marker-persisted-boundary", 114);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 115, 116);
    let mut pieces = vec![DraftPieceV1::Text("x".to_owned())];
    pieces
        .extend((1_u8..=129).map(|order| {
            DraftPieceV1::Marker(marker(order.wrapping_add(70), u64::from(order), 1))
        }));
    let edit = transaction(
        storage,
        &session,
        117,
        vec![DraftPieceReplacementV1::new(point(0), point(0), pieces)],
        DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll),
    );
    begin_stage_build(storage, &store, &edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let head = adopted_head(storage, &store, &edit);
    committed(execute(
        &store,
        inject_draft_piece_descendant_corruption(
            &store,
            storage,
            head.newest_root(),
            DraftPieceDescendantTarget::Sequence,
            DraftPieceDescendantCorruption::TextBearingMarkerOrderSlot,
        ),
    ));
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                head.newest_root(),
                DraftPieceMarkerAtV1::new(1, marker(71, 1, 1)),
            )
            .is_err()
    );
}

#[cfg(feature = "test-faults")]
#[test]
fn candidate_adoption_closure_and_open_receipt_bytes_fail_closed() {
    for case in [
        "missing-root",
        "missing-settlement",
        "missing-build",
        "bad-settlement",
    ] {
        let (_home, store, storage, thread) = fixture(case, 120);
        let durable = current(storage, &store, thread);
        let session = open_session(storage, &store, &durable, 121, 122);
        let first = transaction(
            storage,
            &session,
            123,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("first".to_owned())],
            )],
            point(5),
        );
        begin_stage_build(storage, &store, &first);
        committed(execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), first.prepared.clone()),
        ));
        let first_head = adopted_head(storage, &store, &first);
        let second = transaction(
            storage,
            &first_head,
            124,
            vec![DraftPieceReplacementV1::new(
                point(5),
                point(5),
                vec![DraftPieceV1::Text("-second".to_owned())],
            )],
            point(12),
        );
        begin_stage_build(storage, &store, &second);
        committed(execute(
            &store,
            storage.settle_draft_piece_edit(
                storage.revision(&store).unwrap(),
                second.prepared.clone(),
            ),
        ));
        let head = adopted_head(storage, &store, &second);
        let pending = transaction(
            storage,
            &head,
            125,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("pending".to_owned())],
            )],
            point(7),
        );
        let pending_contribution = storage
            .begin_draft_piece_edit(storage.revision(&store).unwrap(), pending.prepared.clone());
        let key = syndic_storage::DraftPieceSettlementKeyV1::new(
            head.draft_id(),
            head.session_id(),
            head.newest_root().key().operation_id(),
        );
        let corruption = match case {
            "missing-root" => delete_draft_piece_immutable_record(
                &store,
                storage,
                head.newest_root(),
                DraftPieceImmutableDeletion::Root,
            ),
            "missing-settlement" => delete_draft_piece_immutable_record(
                &store,
                storage,
                head.newest_root(),
                DraftPieceImmutableDeletion::Settlement,
            ),
            "missing-build" => delete_draft_piece_terminal_build(&store, storage, key),
            "bad-settlement" => {
                inject_draft_piece_settlement_closure_corruption(&store, storage, key)
            }
            _ => unreachable!(),
        };
        committed(execute(&store, corruption));
        assert!(
            !matches!(
                storage.draft_editor_candidate_session(&store, head.draft_id(), head.session_id(),),
                Ok(DraftEditorCandidateSessionReadOutcomeV1::Active(_))
            ),
            "candidate session accepted {case}"
        );
        assert!(
            storage
                .candidate_draft_piece_text_demand(
                    &store,
                    DraftEditorCandidateActivationBindingV1::from_head(&head),
                    DraftPieceTextDemandV1::Forward(0),
                    65_536,
                )
                .is_err(),
            "candidate range accepted {case}"
        );
        if store.home_revision().is_ok() {
            assert!(matches!(
                execute(&store, pending_contribution),
                CommandOutcome::NotCommitted { .. }
            ));
        } else {
            assert!(storage.revision(&store).is_err());
        }
    }

    for (case, corruption) in [
        (
            "receipt-malformed",
            DraftEditorCandidateOpenReceiptCorruption::Malformed,
        ),
        (
            "receipt-truncated",
            DraftEditorCandidateOpenReceiptCorruption::Truncated,
        ),
        (
            "receipt-noncanonical",
            DraftEditorCandidateOpenReceiptCorruption::Noncanonical,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 126);
        let durable = current(storage, &store, thread);
        let session = open_session(storage, &store, &durable, 127, 128);
        let request = DraftEditorCandidateSessionOpenRequestV1::new(
            selector(&durable),
            session.session_id(),
            session.open_operation_id(),
        );
        let prepared = storage
            .prepare_open_draft_editor_candidate_session(&store, request)
            .unwrap();
        let outcome = execute(
            &store,
            storage.open_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        );
        committed(execute(
            &store,
            inject_draft_editor_candidate_open_receipt_corruption(
                &store,
                storage,
                session.draft_id(),
                session.session_id(),
                session.open_operation_id(),
                corruption,
            ),
        ));
        assert!(
            storage
                .draft_editor_candidate_session(&store, session.draft_id(), session.session_id(),)
                .is_err()
        );
        assert!(
            storage
                .prepare_open_draft_editor_candidate_session(&store, request)
                .is_err()
        );
        assert!(
            storage
                .reconcile_draft_editor_candidate_session_open(&store, &prepared, outcome)
                .is_err()
        );
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn realistic_in_range_build_phase_jumps_fail_progress_authentication() {
    for (case, corruption, target) in [
        (
            "jump-cross-validating",
            DraftPieceBuildCorruption::StagedToCrossValidating,
            0_u8,
        ),
        (
            "jump-next-piece",
            DraftPieceBuildCorruption::InsertingNextPieceInRange,
            4,
        ),
        (
            "jump-next-byte",
            DraftPieceBuildCorruption::InsertingScalarByteSkip,
            4,
        ),
        (
            "jump-planning-applying",
            DraftPieceBuildCorruption::PlanningToApplying,
            1,
        ),
        (
            "jump-removing-applying",
            DraftPieceBuildCorruption::RemovingToApplying,
            2,
        ),
        (
            "jump-applying-inserting",
            DraftPieceBuildCorruption::ApplyingToInserting,
            3,
        ),
        (
            "jump-adjacent-fragment",
            DraftPieceBuildCorruption::AdjacentFragmentJump,
            1,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 130);
        let durable = current(storage, &store, thread);
        let session = open_session(storage, &store, &durable, 131, 132);
        let seed = transaction(
            storage,
            &session,
            133,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("ab".to_owned())],
            )],
            point(2),
        );
        begin_stage_build(storage, &store, &seed);
        committed(execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), seed.prepared.clone()),
        ));
        let seeded = adopted_head(storage, &store, &seed);
        let edit = transaction(
            storage,
            &seeded,
            134,
            vec![
                DraftPieceReplacementV1::new(
                    point(0),
                    point(1),
                    vec![
                        DraftPieceV1::Text("éx".to_owned()),
                        DraftPieceV1::Text("tail".to_owned()),
                    ],
                ),
                DraftPieceReplacementV1::new(
                    point(1),
                    point(2),
                    vec![DraftPieceV1::Text("z".to_owned())],
                ),
            ],
            point(1),
        );
        committed(execute(
            &store,
            storage
                .begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        ));
        for fragment in &edit.fragments {
            committed(execute(
                &store,
                storage.stage_draft_piece_fragment(
                    storage.revision(&store).unwrap(),
                    edit.prepared.clone(),
                    fragment.clone(),
                ),
            ));
        }
        loop {
            let DraftPieceOperationStatusV1::Open(build) = status(storage, &store, &edit) else {
                panic!("phase-jump fixture stopped being open")
            };
            let reached = matches!(
                (target, build.frontier()),
                (
                    0,
                    DraftPieceBuildFrontierV1::ReconcilingMoves {
                        fragment_ordinal: 1,
                        ..
                    }
                ) | (
                    1,
                    DraftPieceBuildFrontierV1::Planning {
                        fragment_ordinal: 1
                    }
                ) | (
                    2,
                    DraftPieceBuildFrontierV1::Removing {
                        fragment_ordinal: 1,
                        ..
                    }
                ) | (
                    3,
                    DraftPieceBuildFrontierV1::Applying {
                        fragment_ordinal: 1,
                        ..
                    }
                ) | (
                    4,
                    DraftPieceBuildFrontierV1::Inserting {
                        fragment_ordinal: 1,
                        ..
                    }
                )
            );
            if reached {
                break;
            }
            let advance = storage
                .prepare_draft_piece_build_advance(
                    &store,
                    seeded.draft_id(),
                    seeded.session_id(),
                    edit.prepared.header().operation_id(),
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
                    seeded.draft_id(),
                    seeded.session_id(),
                    edit.prepared.header().operation_id(),
                ),
                corruption,
            ),
        ));
        assert!(
            storage
                .prepare_draft_piece_build_advance(
                    &store,
                    seeded.draft_id(),
                    seeded.session_id(),
                    edit.prepared.header().operation_id(),
                )
                .is_err(),
            "phase corruption {corruption:?} advanced"
        );
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn immutable_progress_receipt_endpoint_closes_advance_status_and_settlement() {
    for (case, corruption) in [
        (
            "progress-delete",
            DraftPieceProgressReceiptCorruption::Delete,
        ),
        (
            "progress-delete-previous",
            DraftPieceProgressReceiptCorruption::DeletePrevious,
        ),
        (
            "progress-state-mismatch",
            DraftPieceProgressReceiptCorruption::StateMismatch,
        ),
        (
            "progress-previous-state-mismatch",
            DraftPieceProgressReceiptCorruption::PreviousStateMismatch,
        ),
        (
            "progress-head-mismatch",
            DraftPieceProgressReceiptCorruption::HeadEndpointMismatch,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(case, 138);
        let durable = current(storage, &store, thread);
        let session = open_session(storage, &store, &durable, 139, 140);
        let edit = transaction(
            storage,
            &session,
            141,
            vec![
                DraftPieceReplacementV1::new(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text("receipt".to_owned())],
                ),
                DraftPieceReplacementV1::continuation(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text("tail".to_owned())],
                ),
            ],
            point(7),
        );
        committed(execute(
            &store,
            storage
                .begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        ));
        committed(execute(
            &store,
            storage.stage_draft_piece_fragment(
                storage.revision(&store).unwrap(),
                edit.prepared.clone(),
                edit.fragments[0].clone(),
            ),
        ));
        let key = syndic_storage::DraftPieceSettlementKeyV1::new(
            session.draft_id(),
            session.session_id(),
            edit.prepared.header().operation_id(),
        );
        committed(execute(
            &store,
            inject_draft_piece_progress_receipt_corruption(&store, storage, key, corruption),
        ));
        not_committed(execute(
            &store,
            storage.stage_draft_piece_fragment(
                storage.revision(&store).unwrap(),
                edit.prepared.clone(),
                edit.fragments[1].clone(),
            ),
        ));
        assert!(
            storage
                .prepare_draft_piece_build_advance(
                    &store,
                    session.draft_id(),
                    session.session_id(),
                    edit.prepared.header().operation_id(),
                )
                .is_err(),
            "receipt corruption {corruption:?} advanced"
        );
        assert!(
            storage
                .draft_piece_operation_status_page(&store, &edit.prepared, 1, &edit.fragments[..1],)
                .is_err(),
            "receipt corruption {corruption:?} produced status"
        );
        assert!(matches!(
            storage
                .draft_editor_candidate_session(&store, session.draft_id(), session.session_id(),)
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
        ));
        assert!(
            !matches!(
                execute(
                    &store,
                    storage.settle_draft_piece_edit(
                        storage.revision(&store).unwrap(),
                        edit.prepared.clone(),
                    ),
                ),
                CommandOutcome::Committed { .. }
            ),
            "receipt corruption {corruption:?} settled"
        );
    }

    let (_home, store, storage, thread) = fixture("progress-forged-cross-validating", 152);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 153, 154);
    let edit = transaction(
        storage,
        &session,
        155,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("forged".to_owned())],
        )],
        point(6),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    committed(execute(
        &store,
        inject_draft_piece_build_corruption(
            &store,
            storage,
            syndic_storage::DraftPieceSettlementKeyV1::new(
                session.draft_id(),
                session.session_id(),
                edit.prepared.header().operation_id(),
            ),
            DraftPieceBuildCorruption::StagedToCrossValidating,
        ),
    ));
    assert!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                edit.prepared.header().operation_id(),
            )
            .is_err()
    );
    assert!(
        storage
            .draft_piece_operation_status_page(&store, &edit.prepared, 1, &[])
            .is_err()
    );
    assert!(!matches!(
        execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone(),),
        ),
        CommandOutcome::Committed { .. }
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn progress_target_fork_fragment_ahead_and_custody_drift_fail_closed() {
    let (_home, store, storage, thread) = fixture("occupied-progress-target", 156);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 157, 158);
    let edit = transaction(
        storage,
        &session,
        159,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("fork".to_owned())],
        )],
        point(4),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let key = syndic_storage::DraftPieceSettlementKeyV1::new(
        session.draft_id(),
        session.session_id(),
        edit.prepared.header().operation_id(),
    );
    committed(execute(
        &store,
        inject_draft_piece_occupied_stage_target(&store, storage, key, edit.fragments[0].clone()),
    ));
    not_committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    assert!(
        storage
            .draft_piece_operation_status_page(&store, &edit.prepared, 1, &[])
            .is_err()
    );
    assert!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                edit.prepared.header().operation_id(),
            )
            .is_err()
    );
    assert!(matches!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
    ));

    let (_home, store, storage, thread) = fixture("fragment-ahead", 160);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 161, 162);
    let edit = transaction(
        storage,
        &session,
        163,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("ahead".to_owned())],
        )],
        point(5),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        inject_draft_piece_fragment_ahead(&store, storage, edit.fragments[0].clone()),
    ));
    assert!(
        storage
            .draft_piece_operation_status_page(&store, &edit.prepared, 1, &[])
            .is_err()
    );
    assert!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                edit.prepared.header().operation_id(),
            )
            .is_err()
    );
    not_committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    assert!(matches!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
    ));

    let (_home, store, storage, thread) = fixture("custody-endpoint-drift", 164);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 165, 166);
    let edit = transaction(
        storage,
        &session,
        167,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("custody".to_owned())],
        )],
        point(7),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    committed(execute(
        &store,
        inject_draft_piece_custody_endpoint_corruption(
            &store,
            storage,
            session.draft_id(),
            session.session_id(),
        ),
    ));
    not_committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    assert!(
        storage
            .draft_piece_operation_status_page(&store, &edit.prepared, 1, &[])
            .is_err()
    );
    assert!(
        storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                edit.prepared.header().operation_id(),
            )
            .is_err()
    );
    assert!(matches!(
        storage
            .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn begin_replay_rejects_codec_valid_session_generation_inflation() {
    let (_home, store, storage, thread) = fixture("begin-session-generation", 180);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 181, 182);
    let edit = transaction(
        storage,
        &session,
        183,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("begin".to_owned())],
        )],
        point(5),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        inject_draft_piece_session_generation_inflation(
            &store,
            storage,
            session.draft_id(),
            session.session_id(),
        ),
    ));
    not_committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn stage_replay_rejects_codec_valid_session_generation_inflation() {
    let (_home, store, storage, thread) = fixture("stage-session-generation", 184);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 185, 186);
    let edit = transaction(
        storage,
        &session,
        187,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("stage".to_owned())],
        )],
        point(5),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    committed(execute(
        &store,
        inject_draft_piece_session_generation_inflation(
            &store,
            storage,
            session.draft_id(),
            session.session_id(),
        ),
    ));
    not_committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn advance_replay_rejects_codec_valid_session_generation_inflation() {
    let (_home, store, storage, thread) = fixture("advance-session-generation", 188);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 189, 190);
    let edit = transaction(
        storage,
        &session,
        191,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("advance".to_owned())],
        )],
        point(7),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    let advance = storage
        .prepare_draft_piece_build_advance(
            &store,
            session.draft_id(),
            session.session_id(),
            edit.prepared.header().operation_id(),
        )
        .unwrap()
        .expect("fixture has one build advance");
    committed(execute(
        &store,
        storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance.clone()),
    ));
    committed(execute(
        &store,
        inject_draft_piece_session_generation_inflation(
            &store,
            storage,
            session.draft_id(),
            session.session_id(),
        ),
    ));
    not_committed(execute(
        &store,
        storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn stage_replay_rejects_coordinated_codec_valid_target_replacement() {
    let (_home, store, storage, thread) = fixture("stage-coordinated-target", 192);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 193, 194);
    let edit = transaction(
        storage,
        &session,
        195,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("closure".to_owned())],
        )],
        point(7),
    );
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
    let key = syndic_storage::DraftPieceSettlementKeyV1::new(
        session.draft_id(),
        session.session_id(),
        edit.prepared.header().operation_id(),
    );
    committed(execute(
        &store,
        inject_draft_piece_coordinated_stage_target_replacement(&store, storage, key),
    ));
    not_committed(execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    ));
}

fn transaction(
    storage: SyndicStorage,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacements: Vec<DraftPieceReplacementV1>,
    caret: DraftCompositePositionV1,
) -> Transaction {
    let operation = DraftPieceOperationIdV1::from_bytes([operation; 16]);
    let chain = canonical_draft_piece_fragment_chain_v1(&replacements);
    let header = DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        operation,
        caret,
        caret,
        replacements.len() as u64,
        chain,
    );
    let prepared = storage.prepare_draft_piece_edit(header, session).unwrap();
    let mut preceding = canonical_empty_draft_piece_fragment_chain_v1();
    let fragments = replacements
        .into_iter()
        .enumerate()
        .map(|(ordinal, replacement)| {
            let fragment = storage
                .prepare_draft_piece_fragment(&prepared, ordinal as u64 + 1, preceding, replacement)
                .unwrap();
            preceding = fragment.chain_digest();
            fragment
        })
        .collect();
    Transaction {
        prepared,
        fragments,
    }
}

fn begin_stage_build(storage: SyndicStorage, store: &HomeStore, transaction: &Transaction) {
    committed(execute(
        store,
        storage.begin_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
    for fragment in &transaction.fragments {
        committed(execute(
            store,
            storage.stage_draft_piece_fragment(
                storage.revision(store).unwrap(),
                transaction.prepared.clone(),
                fragment.clone(),
            ),
        ));
    }
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            transaction.prepared.header().draft_id(),
            transaction.prepared.header().session_id(),
            transaction.prepared.header().operation_id(),
        )
        .unwrap()
    {
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
}

fn build_and_reject(
    storage: SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> DraftPieceRejectedReasonV1 {
    committed(execute(
        store,
        storage.begin_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
    for fragment in &transaction.fragments {
        committed(execute(
            store,
            storage.stage_draft_piece_fragment(
                storage.revision(store).unwrap(),
                transaction.prepared.clone(),
                fragment.clone(),
            ),
        ));
    }
    let reason = loop {
        match storage.prepare_draft_piece_build_advance(
            store,
            transaction.prepared.header().draft_id(),
            transaction.prepared.header().session_id(),
            transaction.prepared.header().operation_id(),
        ) {
            Ok(Some(advance)) => committed(execute(
                store,
                storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
            )),
            Err(DraftPiecePrepareErrorV1::Rejected(reason)) => break reason,
            Ok(None) => panic!("invalid marker edit unexpectedly completed"),
            Err(error) => panic!("unexpected marker edit build error: {error:?}"),
        }
    };
    committed(execute(
        store,
        storage.reject_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
            reason,
        ),
    ));
    assert!(matches!(
        settled(storage, store, transaction).outcome(),
        DraftPieceSettlementOutcomeV1::Rejected(actual) if *actual == reason
    ));
    reason
}

fn settled(
    storage: SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> syndic_storage::DraftPieceSettlementV1 {
    match status(storage, store, transaction) {
        DraftPieceOperationStatusV1::Settled(settlement) => settlement,
        other => panic!("operation is not settled: {other:?}"),
    }
}

fn status(
    storage: SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> DraftPieceOperationStatusV1 {
    match storage
        .draft_piece_operation_status_page(store, &transaction.prepared, 1, &transaction.fragments)
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(status) => status,
        DraftPieceOperationVerificationV1::More { .. } => panic!("small proposal did not verify"),
    }
}

fn adopted_head(
    storage: SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> DraftEditorCandidateSessionV1 {
    match settled(storage, store, transaction).closure() {
        syndic_storage::DraftPieceSettlementClosureV1::Committed(adoption) => {
            adoption.adopted_session().clone()
        }
        _ => panic!("operation was not adopted"),
    }
}

fn active_session(
    storage: SyndicStorage,
    store: &HomeStore,
    expected: &DraftEditorCandidateSessionV1,
) -> DraftEditorCandidateSessionV1 {
    match storage
        .draft_editor_candidate_session(store, expected.draft_id(), expected.session_id())
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
        other => panic!("candidate session is not active: {other:?}"),
    }
}

fn settled_head(
    storage: SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> DraftEditorCandidateSessionV1 {
    match settled(storage, store, transaction).closure() {
        syndic_storage::DraftPieceSettlementClosureV1::Committed(adoption) => {
            adoption.adopted_session().clone()
        }
        syndic_storage::DraftPieceSettlementClosureV1::Noncommit(noncommit) => {
            noncommit.observed_session().clone()
        }
    }
}

fn candidate_bytes(
    storage: SyndicStorage,
    store: &HomeStore,
    head: &DraftEditorCandidateSessionV1,
) -> Vec<u8> {
    let binding = DraftEditorCandidateActivationBindingV1::from_head(head);
    let mut offset = 0;
    let mut bytes = Vec::new();
    loop {
        let page = storage
            .candidate_draft_piece_text_demand(
                store,
                binding,
                DraftPieceTextDemandV1::Forward(offset),
                65_536,
            )
            .unwrap()
            .value()
            .clone();
        bytes.extend_from_slice(page.bytes());
        match page.following() {
            syndic_storage::DraftPieceTextEdgeFactV1::Continuation(next) => offset = next,
            syndic_storage::DraftPieceTextEdgeFactV1::DocumentEnd => break,
            _ => panic!("forward page returned a preceding edge"),
        }
    }
    bytes
}

fn open_session(
    storage: SyndicStorage,
    store: &HomeStore,
    current: &syndic_storage::SyndicCurrentDraft,
    session: u8,
    operation: u8,
) -> DraftEditorCandidateSessionV1 {
    let request = DraftEditorCandidateSessionOpenRequestV1::new(
        selector(current),
        DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
    );
    let prepared = storage
        .prepare_open_draft_editor_candidate_session(store, request)
        .unwrap();
    let outcome = execute(
        store,
        storage.open_draft_editor_candidate_session(
            storage.revision(store).unwrap(),
            prepared.clone(),
        ),
    );
    match storage
        .reconcile_draft_editor_candidate_session_open(store, &prepared, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head)
        | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(head) => head,
        other => panic!("session did not open: {other:?}"),
    }
}

fn fixture(name: &str, seed: u8) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new(name);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(storage, &store, seed);
    (home, store, storage, thread)
}

fn create_thread(storage: SyndicStorage, store: &HomeStore, seed: u8) -> SyndicThreadId {
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
    committed(execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase142",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
            ),
        ),
    ));
    thread
}

fn execute(store: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "command was not cleanly committed: {outcome:?}"
    );
}

fn not_committed(outcome: CommandOutcome) {
    assert!(matches!(outcome, CommandOutcome::NotCommitted { .. }));
}

fn replay_succeeded(outcome: CommandOutcome) {
    assert!(matches!(
        outcome,
        CommandOutcome::NotCommitted { .. }
            | CommandOutcome::Committed {
                later_failure: None,
                ..
            }
    ));
}

fn current(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
) -> syndic_storage::SyndicCurrentDraft {
    storage
        .current_draft(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap()
}

fn selector(current: &syndic_storage::SyndicCurrentDraft) -> DraftEditorCurrentSelectorV1 {
    DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
    )
}

fn marker(seed: u8, order: u64, label: u64) -> DraftPieceMarkerV1 {
    DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([seed; 16]),
        order,
        ImageLabelOrdinal::new(label).unwrap(),
    )
}

fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}
