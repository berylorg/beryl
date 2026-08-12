use super::*;

#[test]
fn every_owner_variant_round_trips_and_multi_head_rejection_is_atomic() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let dimensions =
        AssetDimensions::new(NonZeroU64::new(640).unwrap(), NonZeroU64::new(480).unwrap());
    let (asset_id, sidecar_path) = publish_metadata_bytes(
        &store,
        &state,
        b"owner-round-trip-asset",
        AssetMediaType::new("image/avif").unwrap(),
        Some(dimensions),
    );
    let set_id = AssetReferenceSetId::from_bytes([43; 16]);
    let proof = seal_one_entry_set(
        &store,
        &state,
        set_id,
        marker(1),
        ImageLabelOrdinal::new(7).unwrap(),
        asset_id,
    );
    let owners = [
        AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([1; 16])),
        AssetOwner::AcceptedInput(SyndicAcceptedInputId::from_bytes([2; 16])),
        AssetOwner::SubmittedTurnItem(SyndicItemId::from_bytes([3; 16])),
        AssetOwner::RetryRecord(SyndicRetryRecordId::from_bytes([4; 16])),
        AssetOwner::TranscriptProjection(SyndicProjectionId::from_bytes([5; 16])),
    ];
    let missing_proof = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([u8::MAX; 16]),
        proof.source(),
        proof.entry_frontier(),
        proof.asset_chain_digest(),
    )
    .unwrap();
    let rejected = execute_outcome(
        &store,
        state.assets().update_owner_heads(
            state.assets().revision(&store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([
                AssetOwnerHeadUpdate::replace(owners[0], None, Some(proof)),
                AssetOwnerHeadUpdate::replace(owners[1], None, Some(missing_proof)),
            ]))
            .unwrap(),
        ),
    );
    let CommandOutcome::NotCommitted { evidence: rejected } = rejected else {
        panic!("expected rejected owner-proof command, got {rejected:?}");
    };
    assert!(matches!(
        asset_mutation_error(&rejected),
        AssetMutationError::ReferenceSetMissing(actual)
            if *actual == missing_proof.set_id()
    ));
    assert!(
        owners[..2].iter().all(|owner| state
            .assets()
            .owner_head(&store, *owner)
            .unwrap()
            .is_none())
    );

    let first_batch = owners[..ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES]
        .iter()
        .map(|owner| AssetOwnerHeadUpdate::replace(*owner, None, Some(proof)))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    assert_eq!(first_batch.len(), ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES);
    execute(
        &store,
        state.assets().update_owner_heads(
            state.assets().revision(&store).unwrap(),
            UpdateAssetOwnerHeads::new(first_batch).unwrap(),
        ),
    );
    execute(
        &store,
        state.assets().update_owner_heads(
            state.assets().revision(&store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                owners[4],
                None,
                Some(proof),
            )]))
            .unwrap(),
        ),
    );

    let metadata_before = state.assets().metadata(&store, asset_id).unwrap().unwrap();
    assert_eq!(metadata_before.media_type().as_str(), "image/avif");
    assert_eq!(metadata_before.dimensions(), Some(dimensions));
    let manifest_before = state
        .assets()
        .sealed_reference_set_manifest(&store, proof)
        .unwrap();
    let entries_before = state
        .assets()
        .reference_set_entries(
            &store,
            proof,
            None,
            CursorReadLimits::new(64, 65_536).unwrap(),
        )
        .unwrap();
    let heads_before =
        owners.map(|owner| state.assets().owner_head(&store, owner).unwrap().unwrap());

    store.close().unwrap();
    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let reopened_state = BerylState::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_state
            .assets()
            .metadata(&reopened, asset_id)
            .unwrap(),
        Some(metadata_before)
    );
    assert_eq!(
        reopened_state
            .assets()
            .sealed_reference_set_manifest(&reopened, proof)
            .unwrap(),
        manifest_before
    );
    assert_eq!(
        reopened_state
            .assets()
            .reference_set_entries(
                &reopened,
                proof,
                None,
                CursorReadLimits::new(64, 65_536).unwrap(),
            )
            .unwrap(),
        entries_before
    );
    assert_eq!(
        reopened_state
            .assets()
            .marker_reference(&reopened, proof, marker(1))
            .unwrap(),
        entries_before.records().first().cloned()
    );
    assert_eq!(
        reopened_state
            .assets()
            .label_first_reference(&reopened, proof, ImageLabelOrdinal::new(7).unwrap(),)
            .unwrap(),
        entries_before.records().first().cloned()
    );
    for (owner, expected) in owners.into_iter().zip(heads_before) {
        assert_eq!(
            reopened_state
                .assets()
                .owner_head(&reopened, owner)
                .unwrap(),
            Some(expected)
        );
    }
    assert_eq!(
        reopened_state
            .assets()
            .verify_sidecar(&reopened, asset_id)
            .unwrap()
            .path(),
        sidecar_path
    );
}

#[test]
fn authoritative_reference_reads_require_the_complete_sealed_proof() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let (asset_id, _) = publish_metadata_bytes(
        &store,
        &state,
        b"proof-bound-reference-read",
        AssetMediaType::new("image/png").unwrap(),
        None,
    );
    let set_id = AssetReferenceSetId::from_bytes([44; 16]);
    let proof = seal_one_entry_set(
        &store,
        &state,
        set_id,
        marker(1),
        ImageLabelOrdinal::FIRST,
        asset_id,
    );
    let wrong = SealedAssetReferenceSetProof::new(
        set_id,
        proof.source(),
        proof.entry_frontier(),
        AssetReferenceSetDigest::from_bytes([u8::MAX; 32]),
    )
    .unwrap();

    assert!(matches!(
        state.assets().sealed_reference_set_manifest(&store, wrong),
        Err(AssetReadError::SealedProofMismatch(actual)) if actual == set_id
    ));
    assert!(matches!(
        state.assets().reference_set_entries(
            &store,
            wrong,
            None,
            CursorReadLimits::new(64, 65_536).unwrap(),
        ),
        Err(AssetReadError::SealedProofMismatch(actual)) if actual == set_id
    ));
    assert!(matches!(
        state.assets().marker_reference(&store, wrong, marker(1)),
        Err(AssetReadError::SealedProofMismatch(actual)) if actual == set_id
    ));
    assert!(matches!(
        state
            .assets()
            .label_first_reference(&store, wrong, ImageLabelOrdinal::FIRST),
        Err(AssetReadError::SealedProofMismatch(actual)) if actual == set_id
    ));

    let exact = state
        .assets()
        .label_first_reference(&store, proof, ImageLabelOrdinal::FIRST)
        .unwrap()
        .unwrap();
    assert_eq!(exact.asset_id(), asset_id);
}
