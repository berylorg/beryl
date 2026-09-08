#![cfg(feature = "test-faults")]
#![allow(unused_imports, dead_code)]

include!("durable_builder/support.rs");

#[path = "build_mapping_continuation/support.rs"]
mod mapping_support;

use mapping_support::*;

#[test]
fn five_closed_marker_effects_and_text_share_original_coordinates() {
    let (_home, store, storage, thread) = fixture("mapping-mixed-effects", 31);
    let mut session = seeded_text(&storage, &store, thread, "abcdef", 34);
    let first = marker(40, 2, 1);
    let removed = marker(41, 3, 2);
    let replaced = marker(42, 4, 3);
    for (operation, anchor, item) in [(35, 1, first), (36, 3, removed), (37, 5, replaced)] {
        session = complete_checked(
            &storage,
            &store,
            &session,
            operation,
            insert_at(point(anchor), anchor, item),
            DraftLogicalExtentV1::new(6, 1),
        );
    }
    let source = session.newest_root();
    let occurrence = |item: DraftPieceMarkerV1| {
        storage
            .draft_marker_identity(&store, source, item.marker_id())
            .unwrap()
            .unwrap()
    };
    let moved = DraftPieceReplacementV1::new(point(2), point(2), vec![DraftPieceV1::Marker(first)])
        .with_marker_effect(DraftPieceMarkerEffectV1::Move {
            removal: DraftPieceMarkerRemovalProofV1::new(before(1), occurrence(first)),
            insertion: marker_insertion(0, first),
        });
    let removal = DraftPieceReplacementV1::new(before(3), before(3), vec![]).with_marker_effect(
        DraftPieceMarkerEffectV1::Remove {
            removal: DraftPieceMarkerRemovalProofV1::new(before(3), occurrence(removed)),
            charges: DraftPieceMarkerEffectChargesV1::for_marker(removed),
        },
    );
    let text =
        DraftPieceReplacementV1::new(before(3), point(4), vec![DraftPieceV1::Text("XY".into())]);
    let replacement =
        DraftPieceReplacementV1::new(before(5), before(5), vec![DraftPieceV1::Marker(replaced)])
            .with_marker_effect(DraftPieceMarkerEffectV1::SameIdReplacement {
                removal: DraftPieceMarkerRemovalProofV1::new(before(5), occurrence(replaced)),
                insertion: marker_insertion(4, replaced),
            });
    let new_first = marker(43, 10, 4);
    let new_second = marker(44, 11, 5);
    let (prepared, fragments) = stage(
        &storage,
        &store,
        &session,
        38,
        vec![
            moved,
            removal,
            text,
            replacement,
            insert_at(point(6), 7, new_first),
            insert_at(point(6), 7, new_second),
        ],
        before(0),
    );
    let (build, commands) = complete_measured(&storage, &store, &prepared, &fragments);
    assert_eq!(
        build
            .marker_effect_continuation()
            .scan()
            .completed_effect_count(),
        5
    );
    assert_eq!(
        build
            .marker_effect_continuation()
            .scan()
            .next_fragment_ordinal(),
        7
    );
    assert_eq!(build.base_frontier().rank(), source.summary().piece_count());
    assert_eq!(
        build.marker_effect_continuation().source_logical_frontier(),
        6
    );
    assert_eq!(
        build
            .marker_effect_continuation()
            .successor_logical_frontier(),
        7
    );
    assert_eq!(commands, 28 + 21 + 23 + 28 + 18 + 22 + 1);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());
    assert_eq!(
        read_text(&storage, &store, session.newest_root(), 20),
        "abcXYef"
    );
    assert_eq!(session.newest_root().summary().marker_count(), 4);
    assert!(
        storage
            .draft_marker_identity(&store, session.newest_root(), removed.marker_id())
            .unwrap()
            .is_none()
    );
    for (anchor, item) in [(0, first), (4, replaced), (7, new_first), (7, new_second)] {
        assert!(
            storage
                .validate_draft_marker_location(
                    &store,
                    session.newest_root(),
                    DraftPieceMarkerAtV1::new(anchor, item)
                )
                .unwrap()
        );
    }
}

#[test]
fn moved_original_unit_cannot_be_removed_again_after_reinsertion() {
    let (_home, store, storage, thread) = fixture("mapping-copy-consumed", 61);
    let mut session = seeded_text(&storage, &store, thread, "abcd", 64);
    let item = marker(70, 2, 1);
    session = complete_checked(
        &storage,
        &store,
        &session,
        65,
        insert_at(point(1), 1, item),
        DraftLogicalExtentV1::new(4, 1),
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), item.marker_id())
        .unwrap()
        .unwrap();
    let removal = DraftPieceMarkerRemovalProofV1::new(before(1), occurrence);
    let move_effect = |at| {
        DraftPieceReplacementV1::new(point(at), point(at), vec![DraftPieceV1::Marker(item)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Move {
                removal,
                insertion: marker_insertion(0, item),
            })
    };
    let (prepared, fragments) = stage(
        &storage,
        &store,
        &session,
        66,
        vec![move_effect(2), move_effect(3)],
        before(0),
    );
    let error = run_until_error(&storage, &store, &prepared);
    assert!(matches!(
        error,
        DraftPiecePrepareErrorV1::InvalidRoot
            | DraftPiecePrepareErrorV1::Rejected(DraftPieceRejectedReasonV1::Overlap)
    ));
    let build = observed(&storage, &store, &prepared, &fragments);
    assert_eq!(
        build
            .marker_effect_continuation()
            .scan()
            .completed_effect_count(),
        1
    );
    assert!(build.marker_effect_continuation().active().is_some());
    committed(execute(
        &store,
        storage.cancel_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    assert_eq!(
        active_session(&storage, &store, session.draft_id(), session.session_id()).newest_root(),
        session.newest_root()
    );
}

#[test]
fn moving_future_original_marker_preserves_intermediate_and_eof_cuts() {
    let (_home, store, storage, thread) = fixture("mapping-future-original", 141);
    let mut session = seeded_text(&storage, &store, thread, "abcdef", 144);
    let moved = marker(150, 2, 1);
    session = complete_checked(
        &storage,
        &store,
        &session,
        145,
        insert_at(point(4), 4, moved),
        DraftLogicalExtentV1::new(6, 1),
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), moved.marker_id())
        .unwrap()
        .unwrap();
    let future_move =
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Marker(moved)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Move {
                removal: DraftPieceMarkerRemovalProofV1::new(before(4), occurrence),
                insertion: marker_insertion(0, moved),
            });
    let at_eof = marker(151, 3, 2);
    let (prepared, fragments) = stage(
        &storage,
        &store,
        &session,
        146,
        vec![
            future_move,
            DraftPieceReplacementV1::new(point(1), point(3), vec![DraftPieceV1::Text("X".into())]),
            insert_at(point(6), 5, at_eof),
        ],
        before(0),
    );
    let (build, _) = complete_measured(&storage, &store, &prepared, &fragments);
    assert_eq!(
        build
            .marker_effect_continuation()
            .scan()
            .completed_effect_count(),
        2
    );
    assert_eq!(
        build.marker_effect_continuation().source_logical_frontier(),
        6
    );
    assert_eq!(
        build
            .marker_effect_continuation()
            .successor_logical_frontier(),
        5
    );
    let mapping = syndic_storage::test_faults::draft_build_mapping_snapshot(&build).unwrap();
    assert_eq!(mapping.completed_source_unit, 7);
    assert_eq!(mapping.current_target_units, 7);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());
    assert_eq!(
        read_text(&storage, &store, session.newest_root(), 16),
        "aXdef"
    );
    for (anchor, item) in [(0, moved), (5, at_eof)] {
        assert!(
            storage
                .validate_draft_marker_location(
                    &store,
                    session.newest_root(),
                    DraftPieceMarkerAtV1::new(anchor, item)
                )
                .unwrap()
        );
    }
}

#[test]
fn split_text_refresh_and_continuation_preserve_later_original_marker() {
    let (_home, store, storage, thread) = fixture("mapping-split-refresh", 81);
    let mut session = seeded_text(&storage, &store, thread, "abcdef", 84);
    let item = marker(90, 2, 1);
    session = complete_checked(
        &storage,
        &store,
        &session,
        85,
        insert_at(point(4), 4, item),
        DraftLogicalExtentV1::new(6, 1),
    );
    let payload = "é".repeat(20_000);
    let first = DraftPieceReplacementV1::new(
        point(1),
        point(3),
        vec![DraftPieceV1::Text(payload.clone())],
    );
    let second = DraftPieceReplacementV1::continuation(
        point(1),
        point(3),
        vec![DraftPieceV1::Text("Q".into())],
    );
    let occurrence = storage
        .draft_marker_identity(&store, session.newest_root(), item.marker_id())
        .unwrap()
        .unwrap();
    let moved = DraftPieceReplacementV1::new(point(5), point(5), vec![DraftPieceV1::Marker(item)])
        .with_marker_effect(DraftPieceMarkerEffectV1::Move {
            removal: DraftPieceMarkerRemovalProofV1::new(before(4), occurrence),
            insertion: marker_insertion(1, item),
        });
    let (prepared, fragments) = stage(
        &storage,
        &store,
        &session,
        86,
        vec![first, second, moved],
        point(0),
    );
    let (build, _) = complete_measured(&storage, &store, &prepared, &fragments);
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
            .successor_logical_frontier(),
        40_004
    );
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());
    assert_eq!(
        read_text(&storage, &store, session.newest_root(), 45_000),
        format!("a{payload}Qdef")
    );
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                session.newest_root(),
                DraftPieceMarkerAtV1::new(1, item)
            )
            .unwrap()
    );
}

#[test]
fn marker_insert_after_untouched_same_anchor_marker_can_exceed_mapped_gap() {
    let (_home, store, storage, thread) = fixture("mapping-marker-gap", 101);
    let mut session = seeded_text(&storage, &store, thread, "ab", 104);
    let old = marker(110, 2, 1);
    let new = marker(111, 3, 2);
    session = complete_checked(
        &storage,
        &store,
        &session,
        105,
        insert_at(point(1), 1, old),
        DraftLogicalExtentV1::new(2, 1),
    );
    let (prepared, fragments) = stage(
        &storage,
        &store,
        &session,
        106,
        vec![insert_at(before(1), 1, new)],
        point(0),
    );
    complete_measured(&storage, &store, &prepared, &fragments);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    session = active_session(&storage, &store, session.draft_id(), session.session_id());
    for item in [old, new] {
        assert!(
            storage
                .validate_draft_marker_location(
                    &store,
                    session.newest_root(),
                    DraftPieceMarkerAtV1::new(1, item)
                )
                .unwrap()
        );
    }
}

#[test]
fn one_sequence_leaf_removal_continues_across_seeded_mapping_leaves() {
    let (_home, store, storage, thread) = fixture("mapping-multiple-leaves", 121);
    let session = seeded_text(&storage, &store, thread, &"x".repeat(512), 124);
    let (prepared, fragments) = stage(
        &storage,
        &store,
        &session,
        125,
        vec![DraftPieceReplacementV1::new(point(0), point(512), vec![])],
        point(0),
    );
    loop {
        let build = observed(&storage, &store, &prepared, &fragments);
        if syndic_storage::test_faults::draft_build_mapping_snapshot(&build)
            .unwrap()
            .stage_tag
            == 5
        {
            syndic_storage::test_faults::seed_fragmented_copy_alignment_for_test(
                &storage, &store, &build,
            );
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
    let mut map_commands = 0;
    let mut sequence_commands = 0;
    let mut previous_pending = 512;
    for step in 0..128 {
        let source = observed(&storage, &store, &prepared, &fragments);
        let mapping = syndic_storage::test_faults::draft_build_mapping_snapshot(&source).unwrap();
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                &store,
                session.draft_id(),
                session.session_id(),
                prepared.header().operation_id(),
            )
            .unwrap_or_else(|error| {
                panic!(
                    "seeded command {step}, mapping {mapping:?}, frontier {:?}: {error:?}",
                    source.frontier()
                )
            })
        else {
            break;
        };
        let measurement = advance.clone();
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
        let target = observed(&storage, &store, &prepared, &fragments);
        let next_mapping =
            syndic_storage::test_faults::draft_build_mapping_snapshot(&target).unwrap();
        if mapping.stage_tag == 18 {
            map_commands += 1;
            assert_eq!(source.frontier(), target.frontier());
            assert_eq!(
                target
                    .working_roots()
                    .sequence_summary()
                    .logical_utf8_bytes(),
                512
            );
            assert_eq!(next_mapping.current_target_units, 512);
            let remaining = next_mapping.pending_target_units.unwrap();
            assert!(remaining < previous_pending);
            previous_pending = remaining;
        }
        if mapping.stage_tag == 21 && mapping.ready_kind == Some(0) {
            sequence_commands += 1;
            assert_eq!(
                target
                    .working_roots()
                    .sequence_summary()
                    .logical_utf8_bytes(),
                0
            );
            assert_eq!(next_mapping.current_source_units, 512);
            assert_eq!(next_mapping.current_target_units, 0);
        }
        let work = measurement.bounded_work().unwrap();
        assert!(
            work.stored_structure_records() <= 256
                && work.point_attempts() <= 512
                && work.peak_encoded_bytes() <= 4_194_304
        );
        assert!(step < 127);
    }
    assert_eq!(map_commands, 4);
    assert_eq!(sequence_commands, 1);
    assert_eq!(previous_pending, 0);
    let final_build = observed(&storage, &store, &prepared, &fragments);
    assert_eq!(final_build.frontier(), DraftPieceBuildFrontierV1::Complete);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
    ));
    assert_eq!(
        active_session(&storage, &store, session.draft_id(), session.session_id())
            .newest_root()
            .summary()
            .logical_utf8_bytes(),
        0
    );
}
