use super::*;

#[test]
fn assigned_marker_resolution_preserves_read_state_and_builder_consumes_once() {
    for preserve in [false, true] {
        let mut fixture = AcceptedFixture::new("assigned-marker-resolution", 100);
        let predecessor = fixture.session.clone();
        fixture.session = complete_staged(
            &fixture.storage,
            &fixture.store,
            &fixture.session,
            150,
            DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".into())]),
            DraftLogicalExtentV1::new(1, 1),
        );
        let operation = owner(&fixture.session, 151);
        ingest(
            &fixture,
            operation,
            152,
            1,
            true,
            vec![if preserve {
                fixture.association(153, fixture.thread)
            } else {
                fresh(153, fixture.asset_id)
            }],
            || {
                Some(if preserve {
                    fixture.factory()
                } else {
                    fresh_factory(&fixture)
                })
            },
        );
        let proof = assign(&fixture, operation, 154, 1);
        let expected_label = if preserve {
            fixture.label
        } else {
            proof.allocation_range().unwrap().first()
        };
        let identity = DraftMutationStagingIdentityV1::new(
            fixture.session.draft_id(),
            fixture.session.session_id(),
            DraftMutationOperationIdV1::from_bytes(*operation.operation_id().as_bytes()),
        );
        let begin = begin_input(identity, &fixture.session);
        let target_id = SyndicDraftMarkerId::from_bytes([153; 16]);
        let resolve = |request, target, asset, order| {
            fixture.storage.resolve_draft_mutation_staging_marker(
                &fixture.store,
                request,
                target,
                asset,
                order,
            )
        };
        assert!(resolve(begin, target_id, fixture.asset_id, 0).is_err());
        let (_, active, staged) = writer_support::begin_admitted_marker_edit(
            &fixture.storage,
            &fixture.store,
            &fixture.session,
            operation,
            proof,
        );
        let before = writer_support::snapshot(&fixture.storage, &fixture.store, operation);
        let home_revision = fixture.store.home_revision().unwrap();
        let revision = fixture.storage.revision(&fixture.store).unwrap();
        for order in [0, 7, u64::MAX, 0] {
            assert_eq!(
                resolve(begin, target_id, fixture.asset_id, order).unwrap(),
                DraftPieceMarkerV1::new(target_id, order, expected_label, fixture.asset_id)
            );
        }
        assert!(
            resolve(
                begin,
                SyndicDraftMarkerId::from_bytes([155; 16]),
                fixture.asset_id,
                0
            )
            .is_err()
        );
        assert!(resolve(begin, target_id, marker(155, 0, 1).asset_id(), 0).is_err());
        assert!(
            resolve(
                begin_input(identity, &predecessor),
                target_id,
                fixture.asset_id,
                0
            )
            .is_err()
        );
        assert!(
            resolve(
                begin_input(identity, &active),
                target_id,
                fixture.asset_id,
                0
            )
            .is_err()
        );
        let wrong_generation = DraftMutationBeginV1::new(
            identity,
            begin.session_generation(),
            begin.predecessor_candidate_generation() + 1,
            begin.predecessor_root(),
            begin.predecessor_history(),
            begin.predecessor_extent(),
            begin.predecessor_caret(),
            begin.predecessor_selection_anchor(),
            begin.predecessor_selection_head(),
            begin.replacement_start(),
            begin.replacement_end(),
            begin.source_initial_cursor(),
            begin.proposal_initial_cursor(),
        );
        assert!(resolve(wrong_generation, target_id, fixture.asset_id, 0).is_err());
        for wrong_identity in [
            DraftMutationStagingIdentityV1::new(
                SyndicDraftId::from_bytes([156; 16]),
                identity.session_id(),
                identity.operation_id(),
            ),
            DraftMutationStagingIdentityV1::new(
                identity.draft_id(),
                DraftEditorCandidateSessionIdV1::from_bytes([156; 16]),
                identity.operation_id(),
            ),
            DraftMutationStagingIdentityV1::new(
                identity.draft_id(),
                identity.session_id(),
                DraftMutationOperationIdV1::from_bytes([156; 16]),
            ),
        ] {
            assert!(
                resolve(
                    begin_input(wrong_identity, &fixture.session),
                    target_id,
                    fixture.asset_id,
                    0
                )
                .is_err()
            );
        }
        let target = resolve(begin, target_id, fixture.asset_id, 0).unwrap();
        let after = writer_support::snapshot(&fixture.storage, &fixture.store, operation);
        assert_eq!(before.head(), after.head());
        assert_eq!(before.receipt(), after.receipt());
        assert_eq!(before.capacity(), after.capacity());
        assert_eq!(fixture.store.home_revision().unwrap(), home_revision);
        assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), revision);
        assert_eq!(
            fixture
                .storage
                .draft_mutation_staging_head(&fixture.store, identity)
                .unwrap()
                .unwrap(),
            staged
        );
        let replacement =
            DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(target)])
                .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                    DraftPieceMarkerInsertionV1::new(
                        1,
                        target,
                        DraftPieceMarkerEffectChargesV1::for_marker(target),
                    ),
                ));
        let (prepared, _, _) = writer_support::finish_admitted_marker_staging(
            &fixture.storage,
            &fixture.store,
            active,
            staged,
            replacement,
            fixture.session.logical_extent(),
        );
        assert!(resolve(begin, target_id, fixture.asset_id, 0).is_err());
        while let Some(advance) = fixture
            .storage
            .prepare_draft_piece_build_advance(
                &fixture.store,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            )
            .unwrap()
        {
            committed(execute(
                &fixture.store,
                fixture.storage.advance_draft_piece_edit(
                    fixture.storage.revision(&fixture.store).unwrap(),
                    advance,
                ),
            ));
        }
        committed(execute(
            &fixture.store,
            fixture.storage.settle_draft_piece_edit(
                fixture.storage.revision(&fixture.store).unwrap(),
                prepared,
            ),
        ));
        let settled = writer_support::snapshot(&fixture.storage, &fixture.store, operation);
        assert_eq!(settled.head().unwrap().remaining_builder_count(), 0);
        assert_eq!(settled.head().unwrap().target_root().count(), 0);
        assert_eq!(settled.head().unwrap().occurrence_count(), 1);
        assert!(resolve(begin, target_id, fixture.asset_id, 0).is_err());
        fixture
            .storage
            .release_settled_draft_marker_writer(&fixture.store, operation)
            .unwrap();
    }
}

#[test]
fn assigned_marker_resolution_rejects_retired_home_and_reconstructed_owner() {
    let faults = FaultController::new();
    let fixture = AcceptedFixture::with_faults("retired-marker-resolution", 100, faults.clone());
    let operation = owner(&fixture.session, 151);
    ingest(
        &fixture,
        operation,
        152,
        1,
        true,
        vec![fresh(153, fixture.asset_id)],
        || Some(fresh_factory(&fixture)),
    );
    let proof = assign(&fixture, operation, 154, 1);
    let (identity, _, _) = writer_support::begin_admitted_marker_edit(
        &fixture.storage,
        &fixture.store,
        &fixture.session,
        operation,
        proof,
    );
    let begin = begin_input(identity, &fixture.session);
    let target = SyndicDraftMarkerId::from_bytes([153; 16]);
    assert!(
        fixture
            .storage
            .resolve_draft_mutation_staging_marker(
                &fixture.store,
                begin,
                target,
                fixture.asset_id,
                0
            )
            .is_ok()
    );
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
    assert!(
        fixture
            .storage
            .resolve_draft_mutation_staging_marker(
                &fixture.store,
                begin,
                target,
                fixture.asset_id,
                0
            )
            .is_err()
    );
    let recovery = fixture.store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    assert!(
        storage
            .resolve_draft_mutation_staging_marker(&store, begin, target, fixture.asset_id, 0)
            .is_err()
    );
}

#[test]
fn assigned_marker_resolution_requires_receiving_and_exact_durable_custody() {
    for boundary in 0..4 {
        let fixture = AcceptedFixture::new("marker-resolution-custody", 100);
        let operation = owner(&fixture.session, 151);
        ingest(
            &fixture,
            operation,
            152,
            1,
            true,
            (153..157)
                .map(|target| fresh(target, fixture.asset_id))
                .collect(),
            || Some(fresh_factory(&fixture)),
        );
        let proof = assign(&fixture, operation, 160, 4);
        let label = proof.allocation_range().unwrap().first();
        let (identity, active, head) = writer_support::begin_admitted_marker_edit(
            &fixture.storage,
            &fixture.store,
            &fixture.session,
            operation,
            proof,
        );
        let begin = begin_input(identity, &fixture.session);
        for target in 153..157 {
            let id = SyndicDraftMarkerId::from_bytes([target; 16]);
            syndic_storage::test_faults::reset_syndic_point_read_count();
            let resolved = fixture
                .storage
                .resolve_draft_mutation_staging_marker(
                    &fixture.store,
                    begin,
                    id,
                    fixture.asset_id,
                    0,
                )
                .unwrap();
            assert_eq!(
                resolved,
                DraftPieceMarkerV1::new(id, 0, label, fixture.asset_id)
            );
            assert!(syndic_storage::test_faults::syndic_point_read_count() <= 10);
        }
        let contribution = match boundary {
            0 => {
                let finish = fixture
                    .storage
                    .prepare_draft_mutation_staging_finish(
                        &head,
                        &active,
                        DraftMutationFinishInputV1::new(
                            head.source(),
                            head.proposal(),
                            fixture.session.logical_extent(),
                            point(0),
                            point(0),
                            point(0),
                            canonical_empty_draft_piece_fragment_chain_v1(),
                        ),
                    )
                    .unwrap();
                fixture.storage.draft_mutation_staging_command(
                    fixture.storage.revision(&fixture.store).unwrap(),
                    finish,
                )
            }
            1 => {
                let terminal = fixture
                    .storage
                    .prepare_draft_mutation_staging_terminal(
                        &head,
                        &active,
                        syndic_storage::DraftMutationStagingTerminalEvidenceV1::Cancelled {
                            request_id: identity.operation_id(),
                            source_lifecycle:
                                syndic_storage::DraftMutationStagingLifecycleV1::Receiving,
                            writer_admitted: true,
                            candidate_generation: active.newest_candidate_generation(),
                            root: active.newest_root(),
                            history: active.newest_history(),
                            session_revision: active.session_generation(),
                        },
                    )
                    .unwrap();
                fixture.storage.draft_mutation_staging_command(
                    fixture.storage.revision(&fixture.store).unwrap(),
                    terminal,
                )
            }
            2 => syndic_storage::test_faults::inject_draft_piece_session_generation_inflation(
                &fixture.store,
                &fixture.storage,
                identity.draft_id(),
                identity.session_id(),
            ),
            _ => syndic_storage::test_faults::delete_draft_mutation_staging_receipt(
                &fixture.store,
                &fixture.storage,
                syndic_storage::DraftMutationStagingProgressReceiptKeyV1::new(
                    identity,
                    head.receipt().transition_ordinal(),
                )
                .unwrap(),
            ),
        };
        committed(execute(&fixture.store, contribution));
        let before = writer_support::snapshot(&fixture.storage, &fixture.store, operation);
        assert!(
            fixture
                .storage
                .resolve_draft_mutation_staging_marker(
                    &fixture.store,
                    begin,
                    SyndicDraftMarkerId::from_bytes([153; 16]),
                    fixture.asset_id,
                    0,
                )
                .is_err()
        );
        let after = writer_support::snapshot(&fixture.storage, &fixture.store, operation);
        assert_eq!(before.head(), after.head());
        assert_eq!(before.capacity(), after.capacity());
    }
}
