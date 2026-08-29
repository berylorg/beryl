#[cfg(feature = "test-faults")]
#[test]
fn active_marker_command_writer_cuts_recover_one_atomic_root_triplet() {
    for (cut, fault_point) in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ]
    .into_iter()
    .enumerate()
    {
        let faults = FaultController::new();
        let (_home, store, storage, thread) =
            fixture_with_faults(&format!("marker-cut-{cut}"), 30 + cut as u8, faults.clone());
        let current = current(&storage, &store, thread);
        let mut session = open_session(&storage, &store, &current, 40 + cut as u8, 50 + cut as u8);
        session = complete_staged(
            &storage,
            &store,
            &session,
            60,
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("abc".to_owned())],
            ),
            DraftLogicalExtentV1::new(3, 1),
        );
        let marker = marker(70 + cut as u8, 7, 9);
        session = complete_staged(
            &storage,
            &store,
            &session,
            61,
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
        let (prepared, identity, fragments) = stage_interleaved_replacements(
            &storage,
            &store,
            &session,
            62,
            vec![
                DraftPieceReplacementV1::new(
                    point(0),
                    DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll),
                    vec![DraftPieceV1::Text("a".to_owned())],
                ),
                moved,
                DraftPieceReplacementV1::new(
                    point(3),
                    point(3),
                    vec![DraftPieceV1::Text("!".to_owned())],
                ),
            ],
            DraftLogicalExtentV1::new(4, 1),
            point(0),
        );
        let source_roots = loop {
            let build = open_build_fragments(&storage, &store, &prepared, &fragments);
            if matches!(
                build.frontier(),
                DraftPieceBuildFrontierV1::Planning {
                    fragment_ordinal: 2
                }
            ) {
                break build.working_roots();
            }
            let preceding = storage
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
                storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), preceding),
            ));
        };
        let advance = storage
            .prepare_draft_piece_build_advance(
                &store,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            )
            .unwrap()
            .unwrap();
        let retry = advance.clone();
        faults.fail_next(fault_point);
        let outcome = execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        );
        let committed_target = match fault_point {
            FaultPoint::BeforeCommit => {
                assert!(matches!(
                    outcome,
                    CommandOutcome::NotCommitted {
                        evidence: CommandError::Commit { .. }
                    }
                ));
                false
            }
            FaultPoint::AfterCommitBeforePersist => {
                assert!(matches!(
                    outcome,
                    CommandOutcome::Indeterminate {
                        failure: CommandError::Persistence { .. },
                        ..
                    }
                ));
                true
            }
            FaultPoint::AfterPersist => {
                assert!(matches!(
                    outcome,
                    CommandOutcome::Committed {
                        later_failure: Some(CommandError::Persistence { .. }),
                        ..
                    }
                ));
                true
            }
            _ => unreachable!(),
        };
        let (store, storage) = if store.health().state() == HomeHealthState::Failed {
            let recovery = store.recover_same_home().unwrap();
            let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            (recovery.publish(), storage)
        } else {
            (store, storage)
        };
        let recovered = open_build_fragments(&storage, &store, &prepared, &fragments);
        assert_eq!(recovered.working_roots(), source_roots);
        let pending = recovered.marker_effect_continuation().active();
        assert_eq!(
            pending.is_some(),
            committed_target,
            "writer cut {fault_point:?}"
        );
        if let Some(pending) = pending {
            assert_ne!(pending.working_roots(), source_roots);
        } else {
            committed(execute(
                &store,
                storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), retry),
            ));
        }
        while let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                &store,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            )
            .unwrap_or_else(|error| {
                let build = open_build_fragments(&storage, &store, &prepared, &fragments);
                panic!(
                    "writer cut {fault_point:?} failed at {:?}, pending={:?}: {error:?}",
                    build.frontier(),
                    build.marker_effect_continuation().active()
                )
            })
        {
            let replay = advance.clone();
            committed(execute(
                &store,
                storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
            ));
            assert!(matches!(
                execute(
                    &store,
                    storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), replay),
                ),
                CommandOutcome::NotCommitted {
                    evidence: CommandError::EmptyContribution { .. }
                }
            ));
        }
        committed(execute(
            &store,
            storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), prepared),
        ));
        let session = active_session(&storage, &store, session.draft_id(), session.session_id());
        assert!(
            storage
                .validate_draft_marker_location(
                    &store,
                    session.newest_root(),
                    DraftPieceMarkerAtV1::new(2, marker),
                )
                .unwrap(),
            "writer cut {fault_point:?} did not publish the atomic moved root pair"
        );
        assert!(
            storage
                .draft_marker_identity(&store, session.newest_root(), marker.marker_id())
                .unwrap()
                .is_some()
        );
    }
}
