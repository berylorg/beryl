include!("../durable_builder/support.rs");

#[path = "../support/composer_asset_fixture.rs"]
mod asset_fixture;

use beryl_model::AssetReferenceSetId;
use beryl_state::{
    AppendAssetReferencePage, AssetOwner, AssetOwnerHeadUpdate, AssetReferencePageEntry,
    AssetReferenceSetStagingAuthority, BeginAssetReferenceSet, BerylState, SealAssetReferenceSet,
    UpdateAssetOwnerHeads,
};
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidatePublicationCommandErrorV1,
    DraftEditorCandidatePublicationEvidenceV1, DraftEditorCandidatePublicationOutcomeV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1, DraftMarkerSealOperationIdV1,
    DraftMarkerSealProofV1, DraftMarkerSealRequestV1, DraftMarkerSealStatusV1,
};

fn seal(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
) -> DraftMarkerSealProofV1 {
    let request = DraftMarkerSealRequestV1::new(
        session.newest_root(),
        DraftMarkerSealOperationIdV1::from_bytes([169; 16]),
    );
    let begin = storage
        .prepare_draft_marker_seal_begin(store, request)
        .unwrap();
    committed(execute(
        store,
        storage.begin_draft_marker_seal(storage.revision(store).unwrap(), begin),
    ));
    while let Some(advance) = storage
        .prepare_draft_marker_seal_advance(store, request.key())
        .unwrap()
    {
        committed(execute(
            store,
            storage.advance_draft_marker_seal(storage.revision(store).unwrap(), &advance),
        ));
    }
    let DraftMarkerSealStatusV1::Sealed(proof, _) = storage
        .draft_marker_seal_status(store, request.key())
        .unwrap()
    else {
        panic!("marker seal not closed")
    };
    proof
}

#[test]
fn candidate_marker_save_keeps_real_asset_and_syndic_publication_atomic() {
    for cut in [
        None,
        Some(FaultPoint::BeforeCommit),
        Some(FaultPoint::AfterCommitBeforePersist),
    ] {
        let home = TestHome::new("retained-marker-assets");
        let faults = FaultController::new();
        let mut opening = beryl_home_store::HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut opening).unwrap();
        let state = BerylState::register(&mut opening).unwrap();
        let assets = state.assets();
        let store = opening
            .prepare_publication(
                SyndicStorage::required_domains()
                    .unwrap()
                    .merge(BerylState::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        let thread = SyndicThreadId::from_bytes([150; 16]);
        let draft = SyndicDraftId::from_bytes([151; 16]);
        committed(execute(
            &store,
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    thread,
                    draft,
                    ExecutionBinding::new(
                        RuntimeId::from_bytes([171; 16]),
                        RootId::from_bytes([172; 16]),
                        RuntimeNativePath::from_admitted(
                            RuntimeMode::host(),
                            PathFlavor::Windows,
                            "C:\\retained-marker",
                        )
                        .unwrap(),
                    ),
                    SyndicTimestamp::from_unix_millis(1),
                    syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        ));
        let selected = current(&storage, &store, thread);
        let opened = open_session(&storage, &store, &selected, 152, 153);
        let with_text = complete_staged(
            &storage,
            &store,
            &opened,
            154,
            DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("abc".into())],
            ),
            DraftLogicalExtentV1::new(3, 1),
        );
        let asset_id = asset_fixture::publish_asset(&store, &assets);
        let marker = DraftPieceMarkerV1::new(
            SyndicDraftMarkerId::from_bytes([155; 16]),
            7,
            ImageLabelOrdinal::new(9).unwrap(),
            asset_id,
        );
        let candidate = complete_staged(
            &storage,
            &store,
            &with_text,
            156,
            DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
                .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                    DraftPieceMarkerInsertionV1::new(
                        1,
                        marker,
                        DraftPieceMarkerEffectChargesV1::for_marker(marker),
                    ),
                )),
            DraftLogicalExtentV1::new(1, 1),
        );
        let seal_proof = seal(&storage, &store, &candidate);
        let begin = BeginAssetReferenceSet::new(AssetReferenceSetStagingAuthority::new(
            AssetReferenceSetId::from_bytes([157; 16]),
            [158; 32],
        ));
        let staging = begin.staging_authority();
        committed(execute(
            &store,
            assets.begin_reference_set(assets.revision(&store).unwrap(), begin),
        ));
        let build = assets
            .staged_reference_set_manifest(&store, staging)
            .unwrap()
            .build_proof();
        committed(execute(
            &store,
            assets.append_reference_page(
                assets.revision(&store).unwrap(),
                AppendAssetReferencePage::new(
                    build,
                    Box::from([AssetReferencePageEntry::new(
                        marker.marker_id(),
                        marker.label(),
                        marker.asset_id(),
                    )]),
                )
                .unwrap(),
            ),
        ));
        let build = assets
            .staged_reference_set_manifest(&store, staging)
            .unwrap()
            .build_proof();
        let seal_set =
            SealAssetReferenceSet::new(build, seal_proof.sequential(), seal_proof.ordered_assets())
                .unwrap();
        let asset_proof = seal_set.sealed_proof();
        committed(execute(
            &store,
            assets.seal_reference_set(assets.revision(&store).unwrap(), seal_set),
        ));
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let assets = BerylState::reacquire_candidate(&recovery).unwrap().assets();
        let access = recovery.recovery_access().unwrap();
        let request = DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
            selector(&selected),
            DraftEditorCandidateActivationBindingV1::from_head(&candidate),
            DraftPieceOperationIdV1::from_bytes([159; 16]),
            SyndicTimestamp::from_unix_millis(3),
        );
        let source = fresh
            .capture_draft_editor_candidate_publication_source_candidate(&access, request)
            .unwrap();
        let error = match fresh.prepare_draft_editor_candidate_publication_candidate(
            &access,
            source,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        ) {
            Err(error) => error,
            Ok(_) => panic!("marker source accepted empty publication evidence"),
        };
        let (source, error) = error.into_parts();
        assert!(matches!(
            error,
            DraftEditorCandidatePublicationCommandErrorV1::Invariant
        ));
        let prepared = fresh
            .prepare_draft_editor_candidate_publication_candidate(
                &access,
                source,
                DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
                    seal_proof,
                    asset_proof,
                },
            )
            .unwrap();
        let mut command = HomeCommand::new(access.home_revision().unwrap());
        command
            .add(
                fresh
                    .publish_draft_editor_candidate_candidate(
                        &access,
                        fresh.revision_candidate(&access).unwrap(),
                        prepared.clone(),
                    )
                    .unwrap(),
            )
            .unwrap();
        command
            .add(
                assets.update_owner_heads(
                    assets.revision_candidate(&access).unwrap(),
                    UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                        AssetOwner::CurrentDraft(draft),
                        None,
                        Some(asset_proof),
                    )]))
                    .unwrap(),
                ),
            )
            .unwrap();
        if let Some(cut) = cut {
            faults.fail_next(cut);
        }
        let outcome = access.execute(command);
        let (mut recovery, fresh, assets) = if cut.is_some() {
            let recovery = recovery.abort().recover_same_home().unwrap();
            let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            let assets = BerylState::reacquire_candidate(&recovery).unwrap().assets();
            (recovery, fresh, assets)
        } else {
            (recovery, fresh, assets)
        };
        let access = recovery.recovery_access().unwrap();
        let result = fresh
            .reconcile_draft_editor_candidate_publication_candidate(&access, &prepared, outcome);
        let expected_commit = cut != Some(FaultPoint::BeforeCommit);
        if expected_commit {
            assert!(matches!(
                result.unwrap(),
                DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
            ));
            fresh
                .qualify_published_draft_editor_candidate_candidate(
                    &access,
                    DraftEditorCandidateActivationBindingV1::from_head(&candidate),
                    &prepared,
                )
                .unwrap();
        } else {
            assert!(matches!(
                result,
                Err(DraftEditorCandidatePublicationCommandErrorV1::NotCommitted)
            ));
        }
        let store = recovery.publish().unwrap();
        assert_eq!(
            current(&fresh, &store, thread).draft().piece_root(),
            if expected_commit {
                candidate.newest_root()
            } else {
                selected.draft().piece_root()
            }
        );
        let head = assets
            .owner_head(&store, AssetOwner::CurrentDraft(draft))
            .unwrap();
        assert_eq!(head.is_some(), expected_commit);
        store.close().unwrap();
    }
}
