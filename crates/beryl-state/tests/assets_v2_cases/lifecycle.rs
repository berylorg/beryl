use super::*;

#[test]
fn paged_reference_set_seals_binds_reopens_and_becomes_unreachable() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let (asset_id, sidecar_path) = publish_metadata(&store, state);
    let set_id = AssetReferenceSetId::from_bytes([9; 16]);
    let source = marker_summary((1..=65).map(|index| {
        let label = if index == 65 {
            ImageLabelOrdinal::new(100).unwrap()
        } else {
            ImageLabelOrdinal::FIRST
        };
        (marker(index), label)
    }));

    let staging = begin_reference_set(&store, state, set_id, source);
    let mut manifest = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    let first_page = (1..=ASSET_REFERENCE_PAGE_MAX_ENTRIES as u64)
        .map(|index| {
            AssetReferencePageEntry::new(marker(index), ImageLabelOrdinal::FIRST, asset_id)
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    execute(
        &store,
        state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(manifest.build_proof(), first_page).unwrap(),
        ),
    );
    manifest = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    execute(
        &store,
        state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(
                manifest.build_proof(),
                Box::from([AssetReferencePageEntry::new(
                    marker(65),
                    ImageLabelOrdinal::new(100).unwrap(),
                    asset_id,
                )]),
            )
            .unwrap(),
        ),
    );
    manifest = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    assert_eq!(manifest.marker_count(), 65);
    assert_eq!(
        manifest.maximum_image_label(),
        Some(ImageLabelOrdinal::new(100).unwrap())
    );
    assert_eq!(manifest.entry_frontier(), 65);
    let premature_proof = SealedAssetReferenceSetProof::new(
        set_id,
        source,
        manifest.entry_frontier(),
        manifest.asset_chain_digest(),
    )
    .unwrap();
    assert!(matches!(
        state.assets().reference_set_entries(
            &store,
            premature_proof,
            None,
            CursorReadLimits::new(4, 4096).unwrap(),
        ),
        Err(AssetReadError::ReferenceSetNotSealed(actual)) if actual == set_id
    ));

    let wrong_source = marker_summary(std::iter::empty());
    let wrong_staging = BeginAssetReferenceSet::new(set_id, wrong_source).staging_authority();
    assert!(matches!(
        state
            .assets()
            .staged_reference_set_manifest(&store, wrong_staging),
        Err(AssetReadError::StagingAuthorityMismatch(actual)) if actual == set_id
    ));
    let wrong = state.assets().seal_reference_set(
        state.assets().revision(&store).unwrap(),
        SealAssetReferenceSet::new(manifest.build_proof(), wrong_source),
    );
    let mut wrong_command = HomeCommand::new(store.home_revision().unwrap());
    wrong_command.add(wrong).unwrap();
    assert!(matches!(
        store.execute(wrong_command),
        CommandOutcome::NotCommitted { .. }
    ));

    let build = manifest.build_proof();
    let proof = build.sealed_proof().unwrap();
    execute(
        &store,
        state.assets().seal_reference_set(
            state.assets().revision(&store).unwrap(),
            SealAssetReferenceSet::new(build, source),
        ),
    );
    manifest = state
        .assets()
        .sealed_reference_set_manifest(&store, proof)
        .unwrap();
    assert_eq!(manifest.lifecycle(), AssetReferenceSetLifecycle::Sealed);
    assert_eq!(manifest.sealed_proof(), Some(proof));
    assert!(matches!(
        state
            .assets()
            .staged_reference_set_manifest(&store, staging),
        Err(AssetReadError::ReferenceSetNotBuilding(actual)) if actual == set_id
    ));
    let page = state
        .assets()
        .reference_set_entries(
            &store,
            proof,
            Some(AssetReferenceOrdinal::new(63).unwrap()),
            CursorReadLimits::new(4, 4096).unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 2);
    assert_eq!(
        page.records()[0].label_disposition(),
        AssetLabelDisposition::Repeated {
            first_ordinal: AssetReferenceOrdinal::FIRST,
        }
    );
    assert_eq!(
        state
            .assets()
            .marker_reference(&store, proof, marker(65))
            .unwrap()
            .unwrap()
            .ordinal(),
        AssetReferenceOrdinal::new(65).unwrap()
    );
    assert_eq!(
        state
            .assets()
            .label_first_reference(&store, proof, ImageLabelOrdinal::new(100).unwrap(),)
            .unwrap()
            .unwrap()
            .ordinal(),
        AssetReferenceOrdinal::new(65).unwrap()
    );
    let clamped = state
        .assets()
        .reference_set_entries(
            &store,
            proof,
            None,
            CursorReadLimits::new(10_000, 10_000_000).unwrap(),
        )
        .unwrap();
    assert_eq!(clamped.records().len(), ASSET_REFERENCE_PAGE_MAX_ENTRIES);
    assert!(clamped.has_more());
    let owner = AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([10; 16]));
    execute(
        &store,
        state.assets().update_owner_heads(
            state.assets().revision(&store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                owner,
                None,
                Some(proof),
            )]))
            .unwrap(),
        ),
    );
    assert_eq!(
        state
            .assets()
            .owner_head(&store, owner)
            .unwrap()
            .unwrap()
            .set(),
        proof
    );
    assert_eq!(
        state
            .assets()
            .verify_sidecar(&store, asset_id)
            .unwrap()
            .path(),
        sidecar_path
    );

    store.close().unwrap();
    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let reopened_state = BerylState::register(&mut reopened).unwrap();
    let head = reopened_state
        .assets()
        .owner_head(&reopened, owner)
        .unwrap()
        .unwrap();
    assert_eq!(head.set(), proof);
    execute(
        &reopened,
        reopened_state.assets().update_owner_heads(
            reopened_state.assets().revision(&reopened).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                owner,
                Some(head.expectation()),
                Some(proof),
            )]))
            .unwrap(),
        ),
    );
    let rebound = reopened_state
        .assets()
        .owner_head(&reopened, owner)
        .unwrap()
        .unwrap();
    assert_eq!(
        rebound.owner_revision().get(),
        head.owner_revision().get() + 1
    );
    execute(
        &reopened,
        reopened_state.assets().update_owner_heads(
            reopened_state.assets().revision(&reopened).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                owner,
                Some(rebound.expectation()),
                None,
            )]))
            .unwrap(),
        ),
    );
    assert!(reopened_state
        .assets()
        .owner_head(&reopened, owner)
        .unwrap()
        .is_none());
    assert!(reopened_state
        .assets()
        .sealed_reference_set_manifest(&reopened, proof)
        .is_ok());
    assert!(sidecar_path.is_file());
}

#[test]
fn command_pages_are_physically_bounded_without_an_asset_length_ceiling() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let set_id = AssetReferenceSetId::from_bytes([21; 16]);
    let source = marker_summary(std::iter::empty());
    let staging = begin_reference_set(&store, state, set_id, source);
    let proof = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap()
        .build_proof();
    let fake_asset = AssetId::sha256_v1([22; 32], NonZeroU64::new(1).unwrap());
    let oversized = (0..=ASSET_REFERENCE_PAGE_MAX_ENTRIES)
        .map(|index| {
            AssetReferencePageEntry::new(
                marker(index as u64 + 1),
                ImageLabelOrdinal::FIRST,
                fake_asset,
            )
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    assert!(matches!(
        AppendAssetReferencePage::new(proof, oversized),
        Err(AssetReferencePageError::TooMany { actual })
            if actual == ASSET_REFERENCE_PAGE_MAX_ENTRIES + 1
    ));

    let manifest = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    let sealed = manifest.build_proof().sealed_proof().unwrap();
    execute(
        &store,
        state.assets().seal_reference_set(
            state.assets().revision(&store).unwrap(),
            SealAssetReferenceSet::new(manifest.build_proof(), source),
        ),
    );
    assert!(state
        .assets()
        .sealed_reference_set_manifest(&store, sealed)
        .is_ok());

    let representable = AssetId::sha256_v1([23; 32], NonZeroU64::new(u64::MAX).unwrap());
    assert!(matches!(
        state.assets().verify_sidecar(&store, representable),
        Err(SidecarError::Missing)
    ));
}
