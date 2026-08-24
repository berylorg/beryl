use super::*;

#[cfg(feature = "test-faults")]
use beryl_home_store::DomainRegistrationError;
#[cfg(feature = "test-faults")]
use beryl_state::AssetReferenceSetManifestCorruption;
#[cfg(feature = "test-faults")]
use beryl_state::BerylStateRegistrationError;

#[test]
fn completion_read_returns_only_the_current_open_manifest_without_advancing_state() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let (asset_id, _) = publish_metadata(&store, &state);
    let set_id = AssetReferenceSetId::from_bytes([81; 16]);
    let staging = begin_reference_set(&store, &state, set_id);
    let owner = AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([82; 16]));
    let expected_sequential = marker_summary([(marker(1), ImageLabelOrdinal::FIRST)]);
    let expected_ordered =
        ordered_marker_asset_summary([(marker(1), ImageLabelOrdinal::FIRST, asset_id)]);
    let before_revision = state.assets().revision(&store).unwrap();

    assert!(state.assets().owner_head(&store, owner).unwrap().is_none());
    let completion = state
        .assets()
        .complete_reference_set(&store, staging, expected_sequential, expected_ordered)
        .unwrap();
    let AssetReferenceSetCompletion::Building(manifest) = completion else {
        panic!("open reference-set completion unexpectedly recovered sealed authority");
    };
    assert_eq!(manifest.set_id(), set_id);
    assert_eq!(manifest.marker_count(), 0);
    assert_eq!(manifest.entry_frontier(), 0);
    assert_ne!(manifest.sequential(), expected_sequential);
    assert_ne!(manifest.ordered_assets(), expected_ordered);
    assert_eq!(state.assets().revision(&store).unwrap(), before_revision);
    assert!(state.assets().owner_head(&store, owner).unwrap().is_none());
}

#[test]
fn completion_read_replays_only_exact_sealed_facts_through_a_fresh_handle() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let (asset_id, _) = publish_metadata(&store, &state);
    let set_id = AssetReferenceSetId::from_bytes([83; 16]);
    let marker_id = marker(1);
    let label = ImageLabelOrdinal::FIRST;
    let sequential = marker_summary([(marker_id, label)]);
    let ordered = ordered_marker_asset_summary([(marker_id, label, asset_id)]);
    let staging = begin_reference_set(&store, &state, set_id);
    let building = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    execute(
        &store,
        state.assets().append_reference_page(
            state.assets().revision(&store).unwrap(),
            AppendAssetReferencePage::new(
                building.build_proof(),
                Box::from([AssetReferencePageEntry::new(marker_id, label, asset_id)]),
            )
            .unwrap(),
        ),
    );
    let building = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    let seal = SealAssetReferenceSet::new(building.build_proof(), sequential, ordered).unwrap();
    let proof = seal.sealed_proof();
    execute(
        &store,
        state
            .assets()
            .seal_reference_set(state.assets().revision(&store).unwrap(), seal),
    );

    let fresh = BerylState::reacquire(&store).unwrap();
    let owner = AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([84; 16]));
    let before_revision = fresh.assets().revision(&store).unwrap();
    assert!(fresh.assets().owner_head(&store, owner).unwrap().is_none());
    assert!(matches!(
        fresh
            .assets()
            .staged_reference_set_manifest(&store, staging),
        Err(AssetReadError::ReferenceSetNotBuilding(actual)) if actual == set_id
    ));
    assert_eq!(
        fresh
            .assets()
            .complete_reference_set(&store, staging, sequential, ordered)
            .unwrap(),
        AssetReferenceSetCompletion::Sealed(proof)
    );
    assert_eq!(fresh.assets().revision(&store).unwrap(), before_revision);
    assert!(fresh.assets().owner_head(&store, owner).unwrap().is_none());

    assert!(matches!(
        fresh.assets().complete_reference_set(
            &store,
            staging,
            marker_summary(std::iter::empty()),
            ordered,
        ),
        Err(AssetReadError::CompletionMismatch(actual)) if actual == set_id
    ));
    assert!(matches!(
        fresh.assets().complete_reference_set(
            &store,
            staging,
            sequential,
            ordered_marker_asset_summary(std::iter::empty()),
        ),
        Err(AssetReadError::CompletionMismatch(actual)) if actual == set_id
    ));

    let missing_command = BeginAssetReferenceSet::new(AssetReferenceSetStagingAuthority::new(
        AssetReferenceSetId::from_bytes([85; 16]),
        [85; 32],
    ));
    let missing_staging = missing_command.staging_authority();
    assert!(matches!(
        fresh
            .assets()
            .complete_reference_set(&store, missing_staging, sequential, ordered),
        Err(AssetReadError::ReferenceSetMissing(actual))
            if actual == AssetReferenceSetId::from_bytes([85; 16])
    ));

    let wrong_staging =
        begin_reference_set(&store, &fresh, AssetReferenceSetId::from_bytes([86; 16]));
    let wrong_manifest = fresh
        .assets()
        .staged_reference_set_manifest(&store, wrong_staging)
        .unwrap();
    let wrong_build = wrong_manifest.build_proof();
    let wrong_seal = SealAssetReferenceSet::new(
        wrong_build,
        marker_summary(std::iter::empty()),
        wrong_build.ordered_assets(),
    )
    .unwrap();
    execute(
        &store,
        fresh
            .assets()
            .seal_reference_set(fresh.assets().revision(&store).unwrap(), wrong_seal),
    );
    assert!(matches!(
        fresh
            .assets()
            .complete_reference_set(&store, wrong_staging, sequential, ordered),
        Err(AssetReadError::CompletionMismatch(actual))
            if actual == AssetReferenceSetId::from_bytes([86; 16])
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn completion_read_rejects_reminted_authority_and_corrupt_manifest_fields() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let (asset_id, _) = publish_metadata(&store, &state);
    let set_id = AssetReferenceSetId::from_bytes([87; 16]);
    let proof = seal_one_entry_set(
        &store,
        &state,
        set_id,
        marker(1),
        ImageLabelOrdinal::FIRST,
        asset_id,
    );
    let reminted = BeginAssetReferenceSet::new(AssetReferenceSetStagingAuthority::new(
        set_id,
        [u8::MAX; 32],
    ));
    let reminted_authority = reminted.staging_authority();
    let remint = execute_outcome(
        &store,
        state
            .assets()
            .begin_reference_set(state.assets().revision(&store).unwrap(), reminted),
    );
    let CommandOutcome::NotCommitted { evidence: remint } = remint else {
        panic!("expected bare-id remint rejection, got {remint:?}");
    };
    assert!(matches!(
        asset_mutation_error(&remint),
        AssetMutationError::ReferenceSetAlreadyExists(actual) if *actual == set_id
    ));
    assert!(matches!(
        state.assets().complete_reference_set(
            &store,
            reminted_authority,
            proof.sequential(),
            proof.ordered_assets(),
        ),
        Err(AssetReadError::CompletionMismatch(actual)) if actual == set_id
    ));

    for corruption in [
        AssetReferenceSetManifestCorruption::Lifecycle,
        AssetReferenceSetManifestCorruption::Sequential,
        AssetReferenceSetManifestCorruption::OrderedAssets,
        AssetReferenceSetManifestCorruption::EntryFrontier,
        AssetReferenceSetManifestCorruption::AssetChain,
    ] {
        let directory = tempdir().unwrap();
        let (store, state) = support::open(directory.path());
        let (asset_id, _) = publish_metadata(&store, &state);
        let set_id = AssetReferenceSetId::from_bytes([88; 16]);
        let proof = seal_one_entry_set(
            &store,
            &state,
            set_id,
            marker(1),
            ImageLabelOrdinal::FIRST,
            asset_id,
        );
        execute(
            &store,
            state.assets().corrupt_reference_set_manifest_for_test(
                state.assets().revision(&store).unwrap(),
                set_id,
                corruption,
            ),
        );
        let fresh = BerylState::reacquire(&store).unwrap();
        let staging = AssetReferenceSetStagingAuthority::new(set_id, [88; 32]);
        assert!(matches!(
            fresh.assets().complete_reference_set(
                &store,
                staging,
                proof.sequential(),
                proof.ordered_assets(),
            ),
            Err(AssetReadError::CompletionEvidenceMismatch(actual)) if actual == set_id
        ));
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn schema_validation_rejects_manifest_without_completion_evidence() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let (asset_id, _) = publish_metadata(&store, &state);
    let set_id = AssetReferenceSetId::from_bytes([91; 16]);
    seal_one_entry_set(
        &store,
        &state,
        set_id,
        marker(1),
        ImageLabelOrdinal::FIRST,
        asset_id,
    );
    execute(
        &store,
        state
            .assets()
            .remove_reference_set_completion_evidence_for_test(
                state.assets().revision(&store).unwrap(),
                set_id,
            ),
    );
    store.close().unwrap();

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let error = match BerylState::register_with_schema_validation(&mut reopened) {
        Ok(_) => panic!("missing completion evidence must fail asset-domain schema validation"),
        Err(error) => error,
    };
    let BerylStateRegistrationError::Domain { domain, source } = error else {
        panic!("expected asset-domain registration failure");
    };
    assert_eq!(domain, "beryl-assets");
    let DomainRegistrationError::Validation { domain, source } = source else {
        panic!("expected asset-domain invariant rejection, got {source}");
    };
    assert_eq!(domain, "beryl-assets");
    assert_eq!(
        source.to_string(),
        "asset reference-set manifest has no completion evidence"
    );
}
