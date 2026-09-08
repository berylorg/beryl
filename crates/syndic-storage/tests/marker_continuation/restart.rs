use super::{continuation_support::*, *};
use std::collections::BTreeSet;
use syndic_storage::test_faults::{
    draft_build_mapping_snapshot, draft_marker_program_codec_rejects_for_test,
    draft_marker_program_roundtrip_for_test, draft_marker_program_snapshot_for_test,
};

pub(super) fn observed_build(
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
        other => panic!("expected open build: {other:?}"),
    }
}

fn run_reopening(
    home: &TestHome,
    mut store: HomeStore,
    mut storage: SyndicStorage,
    prepared: &PreparedDraftPieceEditV1,
    fragments: &[syndic_storage::DraftPieceBuildFragmentV1],
) -> (HomeStore, SyndicStorage, BTreeSet<(u8, u8, u8)>) {
    let mut observed = BTreeSet::new();
    let mut mapping_stages = BTreeSet::new();
    for step in 0..512 {
        let before = observed_build(&storage, &store, prepared, fragments);
        let mapping = draft_build_mapping_snapshot(&before).unwrap();
        mapping_stages.insert(mapping.stage_tag);
        let program = draft_marker_program_snapshot_for_test(&before);
        if let Some(program) = &program {
            let (purpose, component) = program.proof.unwrap_or((255, 255));
            observed.insert((program.pending, purpose, component));
            assert!(program.bytes.len() <= 256);
            if program.pending == 7 {
                assert_eq!(program.bytes.len(), 256);
            }
            if program.pending == 4 {
                assert!(program.bytes.len() <= 218);
                if before
                    .marker_effect_continuation()
                    .active()
                    .unwrap()
                    .working_roots()
                    .sequence_summary()
                    .marker_count()
                    > 1
                {
                    assert_eq!(program.bytes.len(), 218);
                }
            }
            assert!(draft_marker_program_roundtrip_for_test(&before));
            if program.bytes.len() > 4 {
                assert!(draft_marker_program_codec_rejects_for_test(&before, 0, 2));
                assert!(draft_marker_program_codec_rejects_for_test(
                    &before,
                    program.pending_offset,
                    255
                ));
                if program.proof.is_some() {
                    assert!(draft_marker_program_codec_rejects_for_test(
                        &before,
                        program.pending_offset + 1,
                        13
                    ));
                    assert!(draft_marker_program_codec_rejects_for_test(
                        &before,
                        program.pending_offset + 2,
                        2
                    ));
                    if program.proof.unwrap().1 == 0 {
                        assert!(draft_marker_program_codec_rejects_for_test(
                            &before,
                            program.pending_offset + 3,
                            2
                        ));
                    }
                }
            }
        }
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                &store,
                prepared.header().draft_id(),
                prepared.header().session_id(),
                prepared.header().operation_id(),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "operation {:?}, step {step}, frontier {:?}, program {program:?}: {error:?}",
                    prepared.header().operation_id(),
                    before.frontier()
                )
            })
        else {
            for stage in [22, 23, 24] {
                assert!(
                    mapping_stages.contains(&stage),
                    "missing refresh stage {stage}"
                );
            }
            if observed.contains(&(2, 255, 255)) {
                for stage in [10, 11, 12, 18, 20, 21] {
                    assert!(
                        mapping_stages.contains(&stage),
                        "missing removal stage {stage}"
                    );
                }
            }
            if observed.contains(&(5, 255, 255)) {
                for stage in [13, 14, 15, 19, 20, 21] {
                    assert!(
                        mapping_stages.contains(&stage),
                        "missing insertion stage {stage}"
                    );
                }
            }
            return (store, storage, observed);
        };
        let measured = advance.clone();
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
        if let Some(work) = measured.bounded_work() {
            assert!(work.point_attempts() <= 512);
            assert!(work.stored_structure_records() <= 256);
            assert!(work.encoded_bytes() <= 4_194_304);
            assert!(work.peak_encoded_bytes() <= 4_194_304);
        }
        let after = observed_build(&storage, &store, prepared, fragments);
        if let Some(program) = program {
            if program.phase != 3 {
                assert_eq!(after.working_roots(), before.working_roots());
            } else if mapping.stage_tag == 24 {
                assert!(after.marker_effect_continuation().active().is_none());
                let after_mapping = draft_build_mapping_snapshot(&after).unwrap();
                assert_eq!(after_mapping.stage_tag, 0);
                assert_eq!(
                    Some(after_mapping.completed_source_unit),
                    mapping.fragment_source_end_unit
                );
                assert_eq!(after_mapping.fragment_source_end_unit, None);
            } else {
                assert!(matches!(mapping.stage_tag, 22 | 23));
                assert_eq!(
                    after.marker_effect_continuation(),
                    before.marker_effect_continuation()
                );
                assert_eq!(after.frontier(), before.frontier());
                assert_eq!(after.base_frontier(), before.base_frontier());
                assert_eq!(after.successor_frontier(), before.successor_frontier());
                assert_eq!(after.working_roots(), before.working_roots());
                assert_eq!(
                    draft_build_mapping_snapshot(&after).unwrap().stage_tag,
                    mapping.stage_tag + 1
                );
            }
        }
        drop(store);
        store = HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
        storage = SyndicStorage::register(&mut store).unwrap();
        let reopened = observed_build(&storage, &store, prepared, fragments);
        assert_eq!(reopened, after);
    }
    panic!("reopen fixture did not finish");
}

#[test]
fn utf8_interior_insert_move_and_same_identity_replacement_reopen_every_boundary() {
    let (home, store, storage, thread) = fixture("marker-program-reopen", 61);
    let mut session = open_session(&storage, &store, &current(&storage, &store, thread), 63, 64);
    session = complete_staged(
        &storage,
        &store,
        &session,
        65,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("é中z".into())]),
        DraftLogicalExtentV1::new(6, 1),
    );
    let item = marker(71, 7, 1);
    let (prepared, fragments) = stage_fragments(
        &storage,
        &store,
        &session,
        66,
        vec![insertion(point(2), item)],
        point(0),
    );
    let (store, storage, inserted) = run_reopening(&home, store, storage, &prepared, &fragments);
    for purpose in [0, 12, 8, 9] {
        assert!(
            inserted.contains(&(1, purpose, 0)),
            "missing proof {purpose}"
        );
    }
    for pending in [5, 6, 7] {
        assert!(inserted.contains(&(pending, 255, 255)));
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
                DraftPieceMarkerAtV1::new(2, item)
            )
            .unwrap()
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), item.marker_id())
        .unwrap()
        .unwrap();
    let moving = DraftPieceReplacementV1::new(point(5), point(5), vec![DraftPieceV1::Marker(item)])
        .with_marker_effect(DraftPieceMarkerEffectV1::Move {
            removal: DraftPieceMarkerRemovalProofV1::new(
                DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
                occurrence,
            ),
            insertion: DraftPieceMarkerInsertionV1::new(
                5,
                item,
                DraftPieceMarkerEffectChargesV1::for_marker(item),
            ),
        });
    let (prepared, fragments) =
        stage_fragments(&storage, &store, &session, 67, vec![moving], point(0));
    let (store, storage, moved) = run_reopening(&home, store, storage, &prepared, &fragments);
    for purpose in [0, 1, 2, 3, 8, 9] {
        assert!(moved.contains(&(1, purpose, 0)), "missing proof {purpose}");
    }
    for pending in 2..=7 {
        assert!(moved.contains(&(pending, 255, 255)));
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
                DraftPieceMarkerAtV1::new(5, item)
            )
            .unwrap()
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), item.marker_id())
        .unwrap()
        .unwrap();
    let replacement = marker(71, 8, 1);
    let before = DraftCompositePositionV1::new(5, DraftCompositeGapWitnessV1::BeforeAll);
    let edit =
        DraftPieceReplacementV1::new(before, before, vec![DraftPieceV1::Marker(replacement)])
            .with_marker_effect(DraftPieceMarkerEffectV1::SameIdReplacement {
                removal: DraftPieceMarkerRemovalProofV1::new(before, occurrence),
                insertion: DraftPieceMarkerInsertionV1::new(
                    5,
                    replacement,
                    DraftPieceMarkerEffectChargesV1::for_marker(replacement),
                ),
            });
    let (prepared, fragments) =
        stage_fragments(&storage, &store, &session, 68, vec![edit], point(0));
    let (store, storage, _) = run_reopening(&home, store, storage, &prepared, &fragments);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), item.marker_id())
        .unwrap()
        .unwrap();
    assert_eq!(occurrence.order_key(), 8);
    assert_eq!(occurrence.label(), replacement.label());
    assert_eq!(session.newest_root().summary().logical_utf8_bytes(), 6);
}

#[test]
fn same_anchor_composite_proofs_reopen_primary_and_secondary_independently() {
    let (home, store, storage, thread) = fixture("marker-composite-proof", 81);
    let mut session = open_session(&storage, &store, &current(&storage, &store, thread), 83, 84);
    session = complete_staged(
        &storage,
        &store,
        &session,
        80,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("ab".into())]),
        DraftLogicalExtentV1::new(2, 1),
    );
    let left = marker(91, 1, 1);
    let right = marker(92, 4, 2);
    session = complete_staged(
        &storage,
        &store,
        &session,
        85,
        insertion(point(1), left),
        DraftLogicalExtentV1::new(2, 1),
    );
    session = complete_staged(
        &storage,
        &store,
        &session,
        86,
        insertion(
            DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll),
            right,
        ),
        DraftLogicalExtentV1::new(2, 1),
    );
    let between = DraftCompositePositionV1::new(
        1,
        DraftCompositeGapWitnessV1::Between {
            left_order_key: 1,
            left_marker_id: left.marker_id(),
            right_order_key: 4,
            right_marker_id: right.marker_id(),
        },
    );
    let (prepared, fragments) = stage_fragments(
        &storage,
        &store,
        &session,
        87,
        vec![
            insertion(between, marker(93, 2, 3)),
            insertion(between, marker(94, 3, 4)),
        ],
        point(0),
    );
    let (store, storage, observed) = run_reopening(&home, store, storage, &prepared, &fragments);
    for purpose in [0, 4, 5] {
        for component in [0, 1] {
            assert!(
                observed.contains(&(1, purpose, component)),
                "missing {purpose}/{component}"
            );
        }
    }
    assert!(observed.contains(&(1, 10, 0)));
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    let session = active_session(&storage, &store, session.draft_id(), session.session_id());
    assert_eq!(session.newest_root().summary().marker_count(), 4);
    let after_all = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll);
    let (prepared, fragments) = stage_fragments(
        &storage,
        &store,
        &session,
        88,
        vec![insertion(after_all, marker(95, 5, 5))],
        point(0),
    );
    let (store, storage, observed) = run_reopening(&home, store, storage, &prepared, &fragments);
    assert!(observed.contains(&(1, 0, 0)));
    assert!(observed.contains(&(1, 0, 1)));
    assert!(observed.contains(&(1, 11, 0)));
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    let session = active_session(&storage, &store, session.draft_id(), session.session_id());
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), right.marker_id())
        .unwrap()
        .unwrap();
    let moved = DraftPieceReplacementV1::new(point(2), point(2), vec![DraftPieceV1::Marker(right)])
        .with_marker_effect(DraftPieceMarkerEffectV1::Move {
            removal: DraftPieceMarkerRemovalProofV1::new(
                DraftCompositePositionV1::new(
                    1,
                    DraftCompositeGapWitnessV1::Between {
                        left_order_key: 3,
                        left_marker_id: marker(94, 3, 4).marker_id(),
                        right_order_key: 4,
                        right_marker_id: right.marker_id(),
                    },
                ),
                occurrence,
            ),
            insertion: DraftPieceMarkerInsertionV1::new(
                2,
                right,
                DraftPieceMarkerEffectChargesV1::for_marker(right),
            ),
        });
    let (prepared, fragments) =
        stage_fragments(&storage, &store, &session, 89, vec![moved], point(0));
    let (store, storage, observed) = run_reopening(&home, store, storage, &prepared, &fragments);
    assert!(observed.contains(&(4, 255, 255)));
    assert!(observed.contains(&(1, 1, 0)));
    assert!(observed.contains(&(1, 1, 1)));
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
}

#[test]
fn removing_the_only_marker_collapses_all_roots_to_the_canonical_empty_tree() {
    let (home, store, storage, thread) = fixture("marker-empty-collapse", 141);
    let mut session = open_session(
        &storage,
        &store,
        &current(&storage, &store, thread),
        143,
        144,
    );
    let item = marker(151, 7, 1);
    let before = DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll);
    let (prepared, fragments) = stage_fragments(
        &storage,
        &store,
        &session,
        145,
        vec![insertion(point(0), item)],
        before,
    );
    let (store, storage, _) = run_reopening(&home, store, storage, &prepared, &fragments);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), item.marker_id())
        .unwrap()
        .unwrap();
    let removal = DraftPieceReplacementV1::new(before, before, vec![]).with_marker_effect(
        DraftPieceMarkerEffectV1::Remove {
            removal: DraftPieceMarkerRemovalProofV1::new(before, occurrence),
            charges: DraftPieceMarkerEffectChargesV1::for_marker(item),
        },
    );
    let (prepared, fragments) =
        stage_fragments(&storage, &store, &session, 146, vec![removal], point(0));
    let (store, storage, observed) = run_reopening(&home, store, storage, &prepared, &fragments);
    for pending in 2..=4 {
        assert!(observed.contains(&(pending, 255, 255)));
    }
    let build = observed_build(&storage, &store, &prepared, &fragments);
    assert!(build.working_roots().sequence_root().is_none());
    assert!(build.working_roots().marker_index_root().is_none());
    assert!(build.working_roots().marker_order_root().is_none());
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    let session = active_session(&storage, &store, session.draft_id(), session.session_id());
    assert_eq!(session.newest_root().summary().piece_count(), 0);
}
