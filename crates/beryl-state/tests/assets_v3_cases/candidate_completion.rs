use super::*;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_state::AssetReferenceSetManifestCorruption;

#[test]
fn candidate_completion_retains_original_authority_through_build_and_seal() {
    let directory = tempdir().unwrap();
    let foreign_directory = tempdir().unwrap();
    let faults = FaultController::new();
    let (store, state) = support::state_fixture::open_with_faults(directory.path(), faults.clone());
    let (_foreign_store, foreign_state) = support::open(foreign_directory.path());
    let (asset_id, _) = publish_metadata(&store, &state);
    let set_id = AssetReferenceSetId::from_bytes([131; 16]);
    let authority = begin_reference_set(&store, &state, set_id);
    let sequential = marker_summary([(marker(1), ImageLabelOrdinal::FIRST)]);
    let ordered = ordered_marker_asset_summary([(marker(1), ImageLabelOrdinal::FIRST, asset_id)]);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&recovered).unwrap();
    let reference = recovered.service_reference();
    let access = recovered.recovery_access().unwrap();
    assert!(
        fresh
            .assets()
            .staged_reference_set_manifest(&reference, authority)
            .is_err()
    );
    for invalid in [state.assets(), foreign_state.assets()] {
        assert!(
            invalid
                .staged_reference_set_manifest_candidate(&access, authority)
                .is_err()
        );
        assert!(
            invalid
                .complete_reference_set_candidate(&access, authority, sequential, ordered)
                .is_err()
        );
    }
    let initial_revision = fresh.assets().revision_candidate(&access).unwrap();
    let building = fresh
        .assets()
        .staged_reference_set_manifest_candidate(&access, authority)
        .unwrap();
    assert_eq!(
        fresh
            .assets()
            .complete_reference_set_candidate(&access, authority, sequential, ordered)
            .unwrap(),
        AssetReferenceSetCompletion::Building(building.clone())
    );
    assert_eq!(
        fresh.assets().revision_candidate(&access).unwrap(),
        initial_revision
    );
    let substituted = AssetReferenceSetStagingAuthority::new(set_id, [255; 32]);
    assert!(
        matches!(fresh.assets().staged_reference_set_manifest_candidate(&access, substituted), Err(AssetReadError::CompletionMismatch(actual)) if actual == set_id)
    );
    assert!(
        matches!(fresh.assets().complete_reference_set_candidate(&access, substituted, sequential, ordered), Err(AssetReadError::CompletionMismatch(actual)) if actual == set_id)
    );
    let missing_id = AssetReferenceSetId::from_bytes([132; 16]);
    assert!(
        matches!(fresh.assets().complete_reference_set_candidate(&access, AssetReferenceSetStagingAuthority::new(missing_id, [132; 32]), sequential, ordered), Err(AssetReadError::ReferenceSetMissing(actual)) if actual == missing_id)
    );
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(
            fresh.assets().append_reference_page(
                initial_revision,
                AppendAssetReferencePage::new(
                    building.build_proof(),
                    Box::from([AssetReferencePageEntry::new(
                        marker(1),
                        ImageLabelOrdinal::FIRST,
                        asset_id,
                    )]),
                )
                .unwrap(),
            ),
        )
        .unwrap();
    assert!(matches!(
        access.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let building = fresh
        .assets()
        .staged_reference_set_manifest_candidate(&access, authority)
        .unwrap();
    assert_eq!(building.entry_frontier(), 1);
    let seal = SealAssetReferenceSet::new(building.build_proof(), sequential, ordered).unwrap();
    let proof = seal.sealed_proof();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add(
            fresh
                .assets()
                .seal_reference_set(fresh.assets().revision_candidate(&access).unwrap(), seal),
        )
        .unwrap();
    assert!(matches!(
        access.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert!(
        matches!(fresh.assets().staged_reference_set_manifest_candidate(&access, authority), Err(AssetReadError::ReferenceSetNotBuilding(actual)) if actual == set_id)
    );
    assert_eq!(
        fresh
            .assets()
            .complete_reference_set_candidate(&access, authority, sequential, ordered)
            .unwrap(),
        AssetReferenceSetCompletion::Sealed(proof)
    );
    assert!(
        matches!(fresh.assets().complete_reference_set_candidate(&access, authority, marker_summary(std::iter::empty()), ordered), Err(AssetReadError::CompletionMismatch(actual)) if actual == set_id)
    );
    assert!(
        matches!(fresh.assets().complete_reference_set_candidate(&access, authority, sequential, ordered_marker_asset_summary(std::iter::empty())), Err(AssetReadError::CompletionMismatch(actual)) if actual == set_id)
    );
    let published = recovered.publish().unwrap();
    assert_eq!(
        fresh
            .assets()
            .complete_reference_set(&published, authority, sequential, ordered)
            .unwrap(),
        AssetReferenceSetCompletion::Sealed(proof)
    );
}

#[test]
fn candidate_completion_refuses_corrupt_sealed_facts() {
    for corruption in [
        AssetReferenceSetManifestCorruption::Lifecycle,
        AssetReferenceSetManifestCorruption::Sequential,
        AssetReferenceSetManifestCorruption::OrderedAssets,
        AssetReferenceSetManifestCorruption::EntryFrontier,
        AssetReferenceSetManifestCorruption::AssetChain,
    ] {
        let directory = tempdir().unwrap();
        let faults = FaultController::new();
        let (store, state) =
            support::state_fixture::open_with_faults(directory.path(), faults.clone());
        let (asset_id, _) = publish_metadata(&store, &state);
        let set_id = AssetReferenceSetId::from_bytes([133; 16]);
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
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovered = store.recover_same_home().unwrap();
        let fresh = BerylState::reacquire_candidate(&recovered).unwrap();
        let access = recovered.recovery_access().unwrap();
        assert!(
            matches!(fresh.assets().complete_reference_set_candidate(&access, AssetReferenceSetStagingAuthority::new(set_id, [133; 32]), proof.sequential(), proof.ordered_assets()), Err(AssetReadError::CompletionEvidenceMismatch(actual)) if actual == set_id)
        );
    }
}

#[test]
fn candidate_build_and_completion_read_failures_block_publication() {
    for completion in [false, true] {
        let directory = tempdir().unwrap();
        let faults = FaultController::new();
        let (store, state) =
            support::state_fixture::open_with_faults(directory.path(), faults.clone());
        let authority =
            begin_reference_set(&store, &state, AssetReferenceSetId::from_bytes([134; 16]));
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovered = store.recover_same_home().unwrap();
        let fresh = BerylState::reacquire_candidate(&recovered).unwrap();
        let access = recovered.recovery_access().unwrap();
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        if completion {
            assert!(
                fresh
                    .assets()
                    .complete_reference_set_candidate(
                        &access,
                        authority,
                        marker_summary(std::iter::empty()),
                        ordered_marker_asset_summary(std::iter::empty())
                    )
                    .is_err()
            );
        } else {
            assert!(
                fresh
                    .assets()
                    .staged_reference_set_manifest_candidate(&access, authority)
                    .is_err()
            );
        }
        assert!(fresh.assets().revision_candidate(&access).is_err());
        assert!(recovered.publish().is_err());
    }
}
