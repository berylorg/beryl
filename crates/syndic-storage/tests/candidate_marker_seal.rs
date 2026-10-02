#![cfg(feature = "test-faults")]
#![allow(dead_code, unused_imports)]

include!("candidate_marker_seal/support.rs");

#[path = "candidate_marker_seal/authority.rs"]
mod authority;
#[path = "candidate_marker_seal/outcomes.rs"]
mod outcomes;

#[test]
fn fresh_and_resumed_candidate_pages_prepare_real_asset_completion_atomically() {
    for resumed in [false, true] {
        let faults = FaultController::new();
        let (_home, store, storage, state, session) =
            marker_fixture("candidate-marker-assets", faults.clone());
        let request = DraftMarkerSealRequestV1::new(
            session.newest_root(),
            DraftMarkerSealOperationIdV1::from_bytes([169; 16]),
        );
        let authority = AssetReferenceSetStagingAuthority::new(
            AssetReferenceSetId::from_bytes([170; 16]),
            [171; 32],
        );
        if resumed {
            let begin = storage
                .prepare_draft_marker_seal_begin(&store, request)
                .unwrap();
            let assets = state.assets();
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin))
                .unwrap();
            command
                .add(assets.begin_reference_set(
                    assets.revision(&store).unwrap(),
                    BeginAssetReferenceSet::new(authority),
                ))
                .unwrap();
            committed(store.execute(command));
            let page = storage
                .prepare_draft_marker_seal_advance_with_limit(&store, request.key(), 1)
                .unwrap()
                .unwrap();
            let manifest = assets
                .staged_reference_set_manifest(&store, authority)
                .unwrap();
            let marker = page.page().markers()[0];
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(storage.advance_draft_marker_seal(storage.revision(&store).unwrap(), &page))
                .unwrap();
            command
                .add(
                    assets.append_reference_page(
                        assets.revision(&store).unwrap(),
                        AppendAssetReferencePage::new(
                            manifest.build_proof(),
                            Box::from([AssetReferencePageEntry::new(
                                marker.marker_id(),
                                marker.label(),
                                marker.asset_id(),
                            )]),
                        )
                        .unwrap(),
                    ),
                )
                .unwrap();
            committed(store.execute(command));
        }
        fail_home(&store, &faults);
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let assets = BerylState::reacquire_candidate(&recovery).unwrap().assets();
        let access = recovery.recovery_access().unwrap();
        if !resumed {
            let begin = fresh
                .prepare_draft_marker_seal_begin_candidate(&access, request)
                .unwrap();
            let mut command = HomeCommand::new(access.home_revision().unwrap());
            command
                .add(
                    fresh
                        .begin_draft_marker_seal(fresh.revision_candidate(&access).unwrap(), begin),
                )
                .unwrap();
            command
                .add(assets.begin_reference_set(
                    assets.revision_candidate(&access).unwrap(),
                    BeginAssetReferenceSet::new(authority),
                ))
                .unwrap();
            committed(access.execute(command));
        }
        let mut completed = if resumed { 1 } else { 0 };
        while let Some(page) = fresh
            .prepare_draft_marker_seal_advance_with_limit_candidate(&access, request.key(), 1)
            .unwrap()
        {
            assert_eq!(page.page().release().source_frontier(), completed);
            assert_eq!(page.page().markers().len(), 1);
            let manifest = assets
                .staged_reference_set_manifest_candidate(&access, authority)
                .unwrap();
            assert_eq!(manifest.entry_frontier(), completed);
            let marker = page.page().markers()[0];
            let mut command = HomeCommand::new(access.home_revision().unwrap());
            command
                .add(
                    fresh.advance_draft_marker_seal(
                        fresh.revision_candidate(&access).unwrap(),
                        &page,
                    ),
                )
                .unwrap();
            command
                .add(
                    assets.append_reference_page(
                        assets.revision_candidate(&access).unwrap(),
                        AppendAssetReferencePage::new(
                            manifest.build_proof(),
                            Box::from([AssetReferencePageEntry::new(
                                marker.marker_id(),
                                marker.label(),
                                marker.asset_id(),
                            )]),
                        )
                        .unwrap(),
                    ),
                )
                .unwrap();
            committed(access.execute(command));
            completed += 1;
            assert_eq!(page.page().release().target_frontier(), completed);
            assert_eq!(
                assets
                    .staged_reference_set_manifest_candidate(&access, authority)
                    .unwrap()
                    .entry_frontier(),
                completed
            );
            assert_eq!(page.page().exact_eof(), completed == 2);
        }
        let DraftMarkerSealStatusV1::Sealed(proof, release) = fresh
            .draft_marker_seal_status_candidate(&access, request.key())
            .unwrap()
        else {
            panic!("candidate marker seal incomplete");
        };
        assert_eq!(proof.source(), session.newest_root());
        assert_eq!(proof.sequential().marker_count(), 2);
        assert_eq!(release.completed_marker_count(), 2);
        let AssetReferenceSetCompletion::Building(manifest) = assets
            .complete_reference_set_candidate(
                &access,
                authority,
                proof.sequential(),
                proof.ordered_assets(),
            )
            .unwrap()
        else {
            panic!("asset set sealed before commit");
        };
        let seal = SealAssetReferenceSet::new(
            manifest.build_proof(),
            proof.sequential(),
            proof.ordered_assets(),
        )
        .unwrap();
        committed(candidate_execute(
            &access,
            assets.seal_reference_set(assets.revision_candidate(&access).unwrap(), seal),
        ));
        let AssetReferenceSetCompletion::Sealed(asset_proof) = assets
            .complete_reference_set_candidate(
                &access,
                authority,
                proof.sequential(),
                proof.ordered_assets(),
            )
            .unwrap()
        else {
            panic!("committed asset seal missing");
        };
        assert_eq!(asset_proof.sequential(), proof.sequential());
        assert_eq!(asset_proof.ordered_assets(), proof.ordered_assets());
        let wrong = AssetReferenceSetStagingAuthority::new(
            AssetReferenceSetId::from_bytes([170; 16]),
            [172; 32],
        );
        assert!(
            assets
                .complete_reference_set_candidate(
                    &access,
                    wrong,
                    proof.sequential(),
                    proof.ordered_assets()
                )
                .is_err()
        );
        recovery.publish().unwrap().close().unwrap();
    }
}

#[test]
fn candidate_terminal_preparation_preserves_exact_release_and_terminal_collisions() {
    for terminal in 0..3 {
        let faults = FaultController::new();
        let (_home, store, _storage, _state, session) =
            marker_fixture("candidate-marker-terminal", faults.clone());
        let request = DraftMarkerSealRequestV1::new(
            session.newest_root(),
            DraftMarkerSealOperationIdV1::from_bytes([180; 16]),
        );
        fail_home(&store, &faults);
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        let begin = fresh
            .prepare_draft_marker_seal_begin_candidate(&access, request)
            .unwrap();
        committed(candidate_execute(
            &access,
            fresh.begin_draft_marker_seal(fresh.revision_candidate(&access).unwrap(), begin),
        ));
        for limit in [0, DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS + 1] {
            assert!(matches!(
                fresh.prepare_draft_marker_seal_advance_with_limit_candidate(
                    &access,
                    request.key(),
                    limit
                ),
                Err(DraftMarkerSealErrorV1::InvalidPageLimit)
            ));
        }
        let page = fresh
            .prepare_draft_marker_seal_advance_with_limit_candidate(&access, request.key(), 1)
            .unwrap()
            .unwrap();
        committed(candidate_execute(
            &access,
            fresh.advance_draft_marker_seal(fresh.revision_candidate(&access).unwrap(), &page),
        ));
        let successor = DraftMarkerSealOperationIdV1::from_bytes([181; 16]);
        let contribution = match terminal {
            0 => fresh.cancel_draft_marker_seal(
                fresh.revision_candidate(&access).unwrap(),
                fresh
                    .prepare_draft_marker_seal_cancel_candidate(&access, request.key())
                    .unwrap(),
            ),
            1 => fresh.fail_draft_marker_seal(
                fresh.revision_candidate(&access).unwrap(),
                fresh
                    .prepare_draft_marker_seal_fail_candidate(
                        &access,
                        request.key(),
                        DraftMarkerSealFailureReasonV1::Operational,
                    )
                    .unwrap(),
            ),
            _ => fresh.supersede_draft_marker_seal(
                fresh.revision_candidate(&access).unwrap(),
                fresh
                    .prepare_draft_marker_seal_supersede_candidate(
                        &access,
                        request.key(),
                        successor,
                    )
                    .unwrap(),
            ),
        };
        committed(candidate_execute(&access, contribution));
        let status = fresh
            .draft_marker_seal_status_candidate(&access, request.key())
            .unwrap();
        let release = match status {
            DraftMarkerSealStatusV1::Cancelled(release) if terminal == 0 => release,
            DraftMarkerSealStatusV1::Failed {
                reason: DraftMarkerSealFailureReasonV1::Operational,
                release,
            } if terminal == 1 => release,
            DraftMarkerSealStatusV1::Superseded {
                successor: actual,
                release,
            } if terminal == 2 => {
                assert_eq!(actual, successor);
                release
            }
            _ => panic!("unexpected terminal seal status"),
        };
        assert_eq!(release.completed_marker_count(), 1);
        assert!(
            fresh
                .prepare_draft_marker_seal_advance_candidate(&access, request.key())
                .unwrap()
                .is_none()
        );
        let replay = match terminal {
            0 => fresh.cancel_draft_marker_seal(
                fresh.revision_candidate(&access).unwrap(),
                fresh
                    .prepare_draft_marker_seal_cancel_candidate(&access, request.key())
                    .unwrap(),
            ),
            1 => fresh.fail_draft_marker_seal(
                fresh.revision_candidate(&access).unwrap(),
                fresh
                    .prepare_draft_marker_seal_fail_candidate(
                        &access,
                        request.key(),
                        DraftMarkerSealFailureReasonV1::Operational,
                    )
                    .unwrap(),
            ),
            _ => fresh.supersede_draft_marker_seal(
                fresh.revision_candidate(&access).unwrap(),
                fresh
                    .prepare_draft_marker_seal_supersede_candidate(
                        &access,
                        request.key(),
                        successor,
                    )
                    .unwrap(),
            ),
        };
        assert!(matches!(
            candidate_execute(&access, replay),
            CommandOutcome::NotCommitted { .. }
        ));
        if terminal != 0 {
            assert!(
                fresh
                    .prepare_draft_marker_seal_cancel_candidate(&access, request.key())
                    .is_err()
            );
        }
        if terminal != 1 {
            assert!(
                fresh
                    .prepare_draft_marker_seal_fail_candidate(
                        &access,
                        request.key(),
                        DraftMarkerSealFailureReasonV1::Operational
                    )
                    .is_err()
            );
        }
        if terminal != 2 {
            assert!(
                fresh
                    .prepare_draft_marker_seal_supersede_candidate(
                        &access,
                        request.key(),
                        successor
                    )
                    .is_err()
            );
        }
        assert!(
            fresh
                .prepare_draft_marker_seal_supersede_candidate(
                    &access,
                    request.key(),
                    request.operation_id()
                )
                .is_err()
        );
        recovery.publish().unwrap().close().unwrap();
    }
}
