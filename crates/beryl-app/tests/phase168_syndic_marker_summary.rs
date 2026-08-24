include!("../../syndic-storage/tests/phase154_durable_builder/support.rs");

use std::num::NonZeroU64;

use beryl_home_store::{SidecarByteLimit, SidecarNamespace};
use beryl_model::{AssetId, AssetReferenceSetId, SealedAssetReferenceSetProof};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetOwner, AssetOwnerHeadAssertion,
    AssetOwnerHeadUpdate, AssetReferencePageEntry, BeginAssetReferenceSet, BerylState,
    PublishAssetMetadata, SealAssetReferenceSet, UpdateAssetOwnerHeads, ValidateAssetOwnerHeads,
};

#[test]
fn syndic_root_marker_proofs_seal_and_validate_asset_owner_heads_exactly() {
    let (_home, mut store, storage, thread) = fixture("app-marker-proof", 220);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let empty_seal = seal_root(&storage, &store, durable.draft().piece_root(), 221);
    let empty_proof = seal_reference_set(
        &store,
        &state,
        AssetReferenceSetId::from_bytes([221; 16]),
        empty_seal.sequential(),
        empty_seal.ordered_assets(),
        Box::new([]),
    );

    let mut session = open_session(storage, &store, &durable, 222, 223);
    session = complete_staged(
        &storage,
        &store,
        &session,
        224,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let asset = publish_asset(&store, &state);
    let placed = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([225; 16]),
        7,
        ImageLabelOrdinal::new(12).unwrap(),
        asset,
    );
    session = complete_staged(
        &storage,
        &store,
        &session,
        226,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(placed)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    placed,
                    DraftPieceMarkerEffectChargesV1::for_marker(placed),
                ),
            )),
        DraftLogicalExtentV1::new(1, 1),
    );
    let marker_seal = seal_root(&storage, &store, session.newest_root(), 227);
    let marker_source = marker_seal.sequential();
    let marker_proof = seal_reference_set(
        &store,
        &state,
        AssetReferenceSetId::from_bytes([227; 16]),
        marker_source,
        marker_seal.ordered_assets(),
        Box::from([AssetReferencePageEntry::new(
            placed.marker_id(),
            placed.label(),
            asset,
        )]),
    );

    let empty_owner = AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([228; 16]));
    let marker_owner = AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([229; 16]));
    committed(execute(
        &store,
        state.assets().update_owner_heads(
            state.assets().revision(&store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([
                AssetOwnerHeadUpdate::replace(empty_owner, None, Some(empty_proof)),
                AssetOwnerHeadUpdate::replace(marker_owner, None, Some(marker_proof)),
            ]))
            .unwrap(),
        ),
    ));
    let empty_head = state
        .assets()
        .owner_head(&store, empty_owner)
        .unwrap()
        .unwrap();
    let marker_head = state
        .assets()
        .owner_head(&store, marker_owner)
        .unwrap()
        .unwrap();
    let mut validation = HomeCommand::new(store.home_revision().unwrap());
    validation
        .add_validation(
            state.assets().validate_owner_heads(
                state.assets().revision(&store).unwrap(),
                ValidateAssetOwnerHeads::new(Box::from([
                    AssetOwnerHeadAssertion::new(empty_owner, Some(empty_head.expectation())),
                    AssetOwnerHeadAssertion::new(marker_owner, Some(marker_head.expectation())),
                ]))
                .unwrap(),
            ),
        )
        .unwrap()
        .add(
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    SyndicThreadId::from_bytes([231; 16]),
                    SyndicDraftId::from_bytes([232; 16]),
                    ExecutionBinding::new(
                        RuntimeId::from_bytes([233; 16]),
                        RootId::from_bytes([234; 16]),
                        RuntimeNativePath::from_admitted(
                            RuntimeMode::host(),
                            PathFlavor::Windows,
                            "C:\\syndic-phase168-owner-validation",
                        )
                        .unwrap(),
                    ),
                    SyndicTimestamp::from_unix_millis(2),
                    syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        )
        .unwrap();
    assert!(matches!(
        store.execute(validation),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));

    let mut mismatched_digest = marker_source.marker_digest();
    mismatched_digest[0] ^= 1;
    let mismatched_source = beryl_model::SequentialMarkerSummaryV1::new(
        mismatched_digest,
        marker_source.marker_count(),
        marker_source.maximum_image_label(),
    )
    .unwrap();
    let set = AssetReferenceSetId::from_bytes([230; 16]);
    let begin = BeginAssetReferenceSet::new(set);
    let authority = begin.staging_authority();
    committed(execute(
        &store,
        state
            .assets()
            .begin_reference_set(state.assets().revision(&store).unwrap(), begin),
    ));
    let building = state
        .assets()
        .staged_reference_set_manifest(&store, authority)
        .unwrap();
    committed(execute(
        &store,
        state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(
                building.build_proof(),
                Box::from([AssetReferencePageEntry::new(
                    placed.marker_id(),
                    placed.label(),
                    asset,
                )]),
            )
            .unwrap(),
        ),
    ));
    let building = state
        .assets()
        .staged_reference_set_manifest(&store, authority)
        .unwrap();
    let mut mismatched = HomeCommand::new(store.home_revision().unwrap());
    mismatched
        .add(
            state.assets().seal_reference_set(
                state.assets().revision(&store).unwrap(),
                SealAssetReferenceSet::new(
                    building.build_proof(),
                    mismatched_source,
                    building.ordered_assets(),
                )
                .unwrap(),
            ),
        )
        .unwrap()
        .add(
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    SyndicThreadId::from_bytes([252; 16]),
                    SyndicDraftId::from_bytes([253; 16]),
                    ExecutionBinding::new(
                        RuntimeId::from_bytes([254; 16]),
                        RootId::from_bytes([255; 16]),
                        RuntimeNativePath::from_admitted(
                            RuntimeMode::host(),
                            PathFlavor::Windows,
                            "C:\\syndic-phase168-stream-validation",
                        )
                        .unwrap(),
                    ),
                    SyndicTimestamp::from_unix_millis(3),
                    syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        )
        .unwrap();
    assert!(matches!(
        store.execute(mismatched),
        CommandOutcome::NotCommitted { .. }
    ));

    let mut mismatched_asset_digest = marker_seal.ordered_assets().marker_asset_digest();
    mismatched_asset_digest[0] ^= 1;
    let mismatched_assets = beryl_model::OrderedMarkerAssetSummaryV1::new(
        mismatched_asset_digest,
        marker_seal.ordered_assets().marker_count(),
    );
    let mut mismatched = HomeCommand::new(store.home_revision().unwrap());
    mismatched
        .add(
            state.assets().seal_reference_set(
                state.assets().revision(&store).unwrap(),
                SealAssetReferenceSet::new(
                    building.build_proof(),
                    marker_source,
                    mismatched_assets,
                )
                .unwrap(),
            ),
        )
        .unwrap();
    assert!(matches!(
        store.execute(mismatched),
        CommandOutcome::NotCommitted { .. }
    ));
}

#[test]
fn syndic_marker_seal_streams_each_authenticated_page_into_asset_staging_atomically() {
    let (_home, mut store, storage, thread) = fixture("app-marker-stream", 240);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &durable, 241, 242);
    session = complete_staged(
        &storage,
        &store,
        &session,
        243,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let asset = publish_asset(&store, &state);
    let first = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([244; 16]),
        8,
        ImageLabelOrdinal::new(14).unwrap(),
        asset,
    );
    session = complete_staged(
        &storage,
        &store,
        &session,
        245,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(first)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    first,
                    DraftPieceMarkerEffectChargesV1::for_marker(first),
                ),
            )),
        DraftLogicalExtentV1::new(1, 1),
    );
    let request = syndic_storage::DraftMarkerSealRequestV1::new(
        session.newest_root(),
        syndic_storage::DraftMarkerSealOperationIdV1::from_bytes([248; 16]),
    );
    let begin_seal = storage
        .prepare_draft_marker_seal_begin(&store, request)
        .unwrap();
    let begin_set = BeginAssetReferenceSet::new(AssetReferenceSetId::from_bytes([249; 16]));
    let staging = begin_set.staging_authority();
    let mut begin = HomeCommand::new(store.home_revision().unwrap());
    begin
        .add(storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin_seal))
        .unwrap()
        .add(
            state
                .assets()
                .begin_reference_set(state.assets().revision(&store).unwrap(), begin_set),
        )
        .unwrap();
    assert!(matches!(
        store.execute(begin),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));

    let first_advance = storage
        .prepare_draft_marker_seal_advance_with_limit(&store, request.key(), 1)
        .unwrap()
        .unwrap();
    let building = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    let rejected_entries = Box::from([AssetReferencePageEntry::new(
        first_advance.page().markers()[0].marker_id(),
        first_advance.page().markers()[0].label(),
        AssetId::sha256_v1([250; 32], NonZeroU64::new(1).unwrap()),
    )]);
    let mut rejected = HomeCommand::new(store.home_revision().unwrap());
    rejected
        .add(storage.advance_draft_marker_seal(storage.revision(&store).unwrap(), &first_advance))
        .unwrap()
        .add(state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(building.build_proof(), rejected_entries).unwrap(),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(rejected),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        storage
            .draft_marker_seal_status(&store, request.key())
            .unwrap(),
        syndic_storage::DraftMarkerSealStatusV1::Open {
            completed_marker_count: 0
        }
    ));
    let replayed = storage
        .prepare_draft_marker_seal_advance_with_limit(&store, request.key(), 1)
        .unwrap()
        .unwrap();
    assert_eq!(replayed.page(), first_advance.page());

    let mut next = Some(replayed);
    while let Some(advance) = next {
        let entries = advance
            .page()
            .markers()
            .iter()
            .map(|marker| {
                AssetReferencePageEntry::new(marker.marker_id(), marker.label(), marker.asset_id())
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let build = state
            .assets()
            .staged_reference_set_manifest(&store, staging)
            .unwrap()
            .build_proof();
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.advance_draft_marker_seal(storage.revision(&store).unwrap(), &advance))
            .unwrap()
            .add(state.assets().append_reference_page(
                state.assets().revision(&store).unwrap(),
                AppendAssetReferencePage::new(build, entries).unwrap(),
            ))
            .unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        next = storage
            .prepare_draft_marker_seal_advance_with_limit(&store, request.key(), 1)
            .unwrap();
    }

    let syndic_storage::DraftMarkerSealStatusV1::Sealed(syndic_proof, _) = storage
        .draft_marker_seal_status(&store, request.key())
        .unwrap()
    else {
        panic!("marker seal did not close");
    };
    let build = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap()
        .build_proof();
    let seal = SealAssetReferenceSet::new(
        build,
        syndic_proof.sequential(),
        syndic_proof.ordered_assets(),
    )
    .unwrap();
    let asset_proof = seal.sealed_proof();
    committed(execute(
        &store,
        state
            .assets()
            .seal_reference_set(state.assets().revision(&store).unwrap(), seal),
    ));
    assert_eq!(asset_proof.sequential(), syndic_proof.sequential());
    assert_eq!(asset_proof.ordered_assets(), syndic_proof.ordered_assets());
    let owner = AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([251; 16]));
    committed(execute(
        &store,
        state.assets().update_owner_heads(
            state.assets().revision(&store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                owner,
                None,
                Some(asset_proof),
            )]))
            .unwrap(),
        ),
    ));
    let head = state.assets().owner_head(&store, owner).unwrap().unwrap();
    let mut validation = HomeCommand::new(store.home_revision().unwrap());
    validation
        .add_validation(
            state.assets().validate_owner_heads(
                state.assets().revision(&store).unwrap(),
                ValidateAssetOwnerHeads::new(Box::from([AssetOwnerHeadAssertion::new(
                    owner,
                    Some(head.expectation()),
                )]))
                .unwrap(),
            ),
        )
        .unwrap()
        .add(
            storage.create_thread(
                storage.revision(&store).unwrap(),
                CreateThread::ordinary(
                    SyndicThreadId::from_bytes([252; 16]),
                    SyndicDraftId::from_bytes([253; 16]),
                    ExecutionBinding::new(
                        RuntimeId::from_bytes([254; 16]),
                        RootId::from_bytes([255; 16]),
                        RuntimeNativePath::from_admitted(
                            RuntimeMode::host(),
                            PathFlavor::Windows,
                            "C:\\syndic-phase168-stream-validation",
                        )
                        .unwrap(),
                    ),
                    SyndicTimestamp::from_unix_millis(3),
                    syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        )
        .unwrap();
    assert!(matches!(
        store.execute(validation),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn seal_reference_set(
    store: &HomeStore,
    state: &BerylState,
    set: AssetReferenceSetId,
    source: beryl_model::SequentialMarkerSummaryV1,
    ordered_assets: beryl_model::OrderedMarkerAssetSummaryV1,
    entries: Box<[AssetReferencePageEntry]>,
) -> SealedAssetReferenceSetProof {
    let begin = BeginAssetReferenceSet::new(set);
    let authority = begin.staging_authority();
    committed(execute(
        store,
        state
            .assets()
            .begin_reference_set(state.assets().revision(store).unwrap(), begin),
    ));
    let mut building = state
        .assets()
        .staged_reference_set_manifest(store, authority)
        .unwrap();
    if !entries.is_empty() {
        committed(execute(
            store,
            state.assets().append_reference_page(
                state.assets().revision(store).unwrap(),
                AppendAssetReferencePage::new(building.build_proof(), entries).unwrap(),
            ),
        ));
        building = state
            .assets()
            .staged_reference_set_manifest(store, authority)
            .unwrap();
    }
    let build = building.build_proof();
    let seal = SealAssetReferenceSet::new(build, source, ordered_assets).unwrap();
    let proof = seal.sealed_proof();
    committed(execute(
        store,
        state
            .assets()
            .seal_reference_set(state.assets().revision(store).unwrap(), seal),
    ));
    assert_eq!(
        state
            .assets()
            .sealed_reference_set_manifest(store, proof)
            .unwrap()
            .sequential(),
        source
    );
    assert_eq!(
        state
            .assets()
            .sealed_reference_set_manifest(store, proof)
            .unwrap()
            .ordered_assets(),
        ordered_assets
    );
    proof
}

fn seal_root(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    operation: u8,
) -> syndic_storage::DraftMarkerSealProofV1 {
    let request = syndic_storage::DraftMarkerSealRequestV1::new(
        root,
        syndic_storage::DraftMarkerSealOperationIdV1::from_bytes([operation; 16]),
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
    let syndic_storage::DraftMarkerSealStatusV1::Sealed(proof, _) = storage
        .draft_marker_seal_status(store, request.key())
        .unwrap()
    else {
        panic!("marker seal did not close");
    };
    proof
}

fn publish_asset(store: &HomeStore, state: &BerylState) -> AssetId {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"phase168-root-marker-proof",
            SidecarByteLimit::new(NonZeroU64::new(1_024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let expected = state.assets().revision(store).unwrap();
    let contribution = state
        .assets()
        .publish_metadata(
            expected,
            sidecar,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new("image/png").unwrap(),
                None,
                expected.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    contribution.add_to(&mut command).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    asset
}
