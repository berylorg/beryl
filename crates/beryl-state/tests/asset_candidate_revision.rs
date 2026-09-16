use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeHealthState, HomeOpenCandidate, HomeOpenOptions,
    HomeOpenPublication, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::AssetReferenceSetId;
use beryl_state::{AssetReferenceSetStagingAuthority, BeginAssetReferenceSet, BerylState};

fn candidate(
    directory: &tempfile::TempDir,
    faults: FaultController,
) -> (HomeOpenPublication, BerylState) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    (
        candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap(),
        state,
    )
}

#[test]
fn candidate_asset_revision_preserves_provenance_and_persisted_revision() {
    let directory = tempfile::tempdir().unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (mut candidate, state) = candidate(&directory, faults.clone());
    let (_foreign, foreign_state) = self::candidate(&foreign_directory, FaultController::new());
    let assets = state.assets();
    let reference = candidate.service_reference();
    let generation = candidate.generation();
    let access = candidate.recovery_access().unwrap();
    let initial = assets.revision_candidate(&access).unwrap();
    assert!(foreign_state.assets().revision_candidate(&access).is_err());
    assert_eq!(reference.health().state(), HomeHealthState::Opening);
    assert!(assets.revision(&reference).is_err());
    let store = candidate.publish().unwrap();
    assert_eq!(store.health().generation(), Some(generation));
    assert_eq!(assets.revision(&store).unwrap(), initial);

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(assets.begin_reference_set(
            initial,
            BeginAssetReferenceSet::new(AssetReferenceSetStagingAuthority::new(
                AssetReferenceSetId::from_bytes([1; 16]),
                [2; 32],
            )),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let populated = assets.revision(&store).unwrap();
    assert_ne!(populated, initial);

    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&recovered)
        .unwrap()
        .assets();
    let access = recovered.recovery_access().unwrap();
    assert_ne!(access.generation(), generation);
    assert!(assets.revision_candidate(&access).is_err());
    assert!(foreign_state.assets().revision_candidate(&access).is_err());
    assert_eq!(fresh.revision_candidate(&access).unwrap(), populated);
    let store = recovered.publish().unwrap();
    assert_eq!(fresh.revision(&store).unwrap(), populated);
}

#[test]
fn candidate_asset_confirmation_failure_prevents_publication() {
    for recovering in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let faults = FaultController::new();
        let (mut candidate, state) = candidate(&directory, faults.clone());
        if recovering {
            let store = candidate.publish().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
            let mut recovered = store.recover_same_home().unwrap();
            let assets = BerylState::reacquire_candidate(&recovered)
                .unwrap()
                .assets();
            let access = recovered.recovery_access().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(assets.revision_candidate(&access).is_err());
            assert!(assets.revision_candidate(&access).is_err());
            assert!(recovered.publish().is_err());
        } else {
            let access = candidate.recovery_access().unwrap();
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(state.assets().revision_candidate(&access).is_err());
            assert!(state.assets().revision_candidate(&access).is_err());
            assert!(candidate.publish().is_err());
        }
    }
}
