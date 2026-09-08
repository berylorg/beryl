use super::*;

fn observed_build(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedDraftPieceEditV1,
    fragments: &[DraftPieceBuildFragmentV1],
) -> DraftPieceBuildRecordV1 {
    match storage
        .draft_piece_operation_status_page(store, prepared, 1, fragments)
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(
            DraftPieceOperationStatusV1::Open(build) | DraftPieceOperationStatusV1::Complete(build),
        ) => build,
        other => panic!("expected open or complete build: {other:?}"),
    }
}

fn ceiling(
    program: Option<&syndic_storage::test_faults::DraftMarkerProgramSnapshotForTest>,
) -> inventory::Ceiling {
    let index = match program {
        Some(program) if program.phase == 3 => 9,
        Some(program) => match program.pending {
            1 => {
                if matches!(program.proof.unwrap().0, 0..=5 | 12) {
                    1
                } else {
                    2
                }
            }
            2 => 3,
            3 => 5,
            4 => 7,
            5 => 4,
            6 => 6,
            7 => 8,
            _ => 0,
        },
        None => 0,
    };
    inventory::ceilings()[index]
}

#[test]
fn admitted_publishing_and_utf8_split_share_construction_submission_and_stale_clone_ledger() {
    for text in ["a", "αβγ"] {
        let (_home, store, storage, thread) = fixture("marker-publishing-ledger", 81);
        let mut session =
            open_session(&storage, &store, &current(&storage, &store, thread), 82, 83);
        if !text.is_empty() {
            session = complete_staged(
                &storage,
                &store,
                &session,
                84,
                DraftPieceReplacementV1::new(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text(text.to_owned())],
                ),
                DraftLogicalExtentV1::new(text.len() as u64, 1),
            );
        }
        let position = if text == "a" { 1 } else { 2 };
        let target = marker(85, 0, 1);
        let admission = owner(&session, 86);
        let proof = storage
            .seed_draft_marker_writer_ready_target_for_test(&store, &session, admission, target)
            .unwrap();
        let replacement = DraftPieceReplacementV1::new(
            point(position),
            point(position),
            vec![DraftPieceV1::Marker(target)],
        )
        .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
            DraftPieceMarkerInsertionV1::new(
                position,
                target,
                DraftPieceMarkerEffectChargesV1::for_marker(target),
            ),
        ));
        let (prepared, identity, fragments) = support::stage_admitted_marker_edit(
            &storage,
            &store,
            &session,
            admission,
            proof,
            replacement,
        );
        let mut publishing_count = 0;
        let mut refresh_stages = Vec::new();
        let mut surgeries = Vec::new();
        let mut reached_complete = false;
        for _ in 0..80 {
            let before = observed_build(&storage, &store, &prepared, &fragments);
            if before.frontier() == DraftPieceBuildFrontierV1::Complete {
                reached_complete = true;
                break;
            }
            let program = draft_marker_program_snapshot_for_test(&before);
            let mapping =
                syndic_storage::test_faults::draft_build_mapping_snapshot(&before).unwrap();
            let admitted_before = support::snapshot(&storage, &store, admission);
            let before_count = admitted_before.head().unwrap().target_root().count();
            syndic_storage::test_faults::reset_home_store_syndic_point_acquisition_count();
            let advance = storage
                .prepare_draft_piece_build_advance(
                    &store,
                    identity.draft_id(),
                    identity.session_id(),
                    identity.operation_id().as_piece_operation(),
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "text {text:?}, frontier {:?}, program {program:?}: {error:?}",
                        before.frontier()
                    )
                })
                .unwrap();
            let observer = advance.clone();
            let stale = advance.clone();
            let constructed = observer.bounded_work();
            let staged_records = advance.staged_record_count();
            committed(execute(&store, storage.advance_draft_piece_edit(advance)));
            let physical_points =
                syndic_storage::test_faults::home_store_syndic_point_acquisition_count();
            let submitted = observer.bounded_work();
            if let Some(program) = &program {
                let constructed =
                    constructed.expect("every active marker command has the shared ledger");
                let submitted = submitted.expect("submission preserves the original ledger");
                ceiling(Some(program)).assert_contains(submitted);
                assert_eq!(
                    physical_points,
                    submitted.point_attempts(),
                    "all actual acquisitions belong to the shared ledger"
                );
                if program.phase == 3 {
                    assert_eq!(
                        constructed.stored_structure_records(),
                        submitted.stored_structure_records(),
                        "Publishing reuses its prepared admission structure result"
                    );
                }
                assert!(submitted.point_attempts() > constructed.point_attempts());
                assert!(submitted.encoded_bytes() > constructed.encoded_bytes());
                assert!(
                    submitted.stored_structure_records() >= constructed.stored_structure_records()
                );
                if (2..=7).contains(&program.pending) {
                    surgeries.push(program.pending);
                    assert!(staged_records >= 2);
                }
            }
            let after = observed_build(&storage, &store, &prepared, &fragments);
            let admitted_after = support::snapshot(&storage, &store, admission);
            let after_head = admitted_after.head().unwrap();
            let refreshing = program.as_ref().is_some_and(|value| value.phase == 3);
            let publishing = refreshing && mapping.stage_tag == 24;
            if refreshing {
                refresh_stages.push(mapping.stage_tag);
            }
            assert_eq!(
                before_count - after_head.target_root().count(),
                u64::from(publishing)
            );
            if publishing {
                publishing_count += 1;
                assert_eq!(before_count, 1);
                assert_eq!(after_head.remaining_builder_count(), 0);
                let writer =
                    syndic_storage::test_faults::marker_writer_admission_snapshot_for_test(&after)
                        .unwrap();
                assert_eq!(writer.target_root(), after_head.target_root());
                assert_eq!(
                    writer.remaining_count(),
                    after_head.remaining_builder_count()
                );
                assert!(after.marker_effect_continuation().active().is_none());
                assert_eq!(
                    after
                        .working_roots()
                        .sequence_summary()
                        .logical_utf8_bytes(),
                    text.len() as u64
                );
                assert_eq!(
                    after.working_roots().sequence_summary().piece_count(),
                    if text == "a" { 2 } else { 3 }
                );
            } else if program.is_some() {
                assert_eq!(
                    admitted_before.head().unwrap().digest(),
                    after_head.digest()
                );
                assert_eq!(
                    admitted_before.capacity().unwrap().digest(),
                    admitted_after.capacity().unwrap().digest()
                );
                assert_eq!(before.working_roots(), after.working_roots());
                if refreshing {
                    assert!(matches!(mapping.stage_tag, 22 | 23));
                    assert_eq!(before.frontier(), after.frontier());
                    assert_eq!(
                        before.marker_effect_continuation(),
                        after.marker_effect_continuation()
                    );
                    assert_eq!(
                        syndic_storage::test_faults::draft_build_mapping_snapshot(&after)
                            .unwrap()
                            .stage_tag,
                        mapping.stage_tag + 1
                    );
                }
            }
            syndic_storage::test_faults::reset_home_store_syndic_point_acquisition_count();
            let stale_outcome = execute(&store, storage.advance_draft_piece_edit(stale));
            assert!(matches!(
                stale_outcome,
                CommandOutcome::NotCommitted {
                    evidence: CommandError::Conflict { .. }
                }
            ));
            assert_eq!(
                observer.bounded_work(),
                submitted,
                "stale admission must reject before growing the original ledger"
            );
            assert_eq!(
                syndic_storage::test_faults::home_store_syndic_point_acquisition_count(),
                0,
                "stale construction must reject before any database callback"
            );
            let unchanged = support::snapshot(&storage, &store, admission);
            assert_eq!(unchanged.head().unwrap().digest(), after_head.digest());
            assert_eq!(
                unchanged.capacity().unwrap().digest(),
                admitted_after.capacity().unwrap().digest()
            );
        }
        assert!(reached_complete);
        assert_eq!(publishing_count, 1);
        assert_eq!(refresh_stages, [22, 23, 24]);
        assert_eq!(surgeries, [5, 6, 7]);
        committed(execute(
            &store,
            storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
        ));
        let terminal = support::snapshot(&storage, &store, admission);
        assert_eq!(
            terminal.head().unwrap().lifecycle(),
            DraftMarkerAdmissionLifecycleV1::Settled
        );
        assert_eq!(terminal.head().unwrap().target_root().count(), 0);
        storage
            .release_settled_draft_marker_writer(&store, admission)
            .unwrap();
    }
}
