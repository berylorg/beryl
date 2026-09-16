use std::num::NonZeroUsize;

use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::BerylState;

use super::*;

fn candidate() -> (
    tempfile::TempDir,
    HomeOpenPublication,
    SyndicStorage,
    AssetState,
    FaultController,
) {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let candidate = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    (directory, candidate, storage, state.assets(), faults)
}

fn limits() -> DraftMarkerSealServiceLimits {
    DraftMarkerSealServiceLimits::new(NonZeroUsize::new(2).unwrap(), NonZeroUsize::new(1).unwrap())
        .unwrap()
}

fn assert_reopens(directory: &tempfile::TempDir) {
    HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
}

#[test]
fn preparation_preserves_candidate_identity_and_abandonment_retires_shared_state() {
    let (directory, candidate, storage, assets, _) = candidate();
    let reference = candidate.service_reference();
    let generation = candidate.generation();
    let prepared = PreparedMarkerServices::prepare(candidate, storage, assets, limits()).unwrap();
    assert_eq!(prepared.candidate.generation(), generation);
    assert_eq!(reference.health().state(), HomeHealthState::Opening);
    assert!(reference.home_revision().is_err());
    let shared = prepared.service.clone();
    let state = lock_state(&shared.inner);
    assert_eq!(state.home_id, reference.home_id());
    assert_eq!(state.home_generation, generation);
    assert_eq!(state.limits, limits());
    assert!(state.flights.is_empty());
    drop(state);
    drop(prepared);
    assert!(matches!(
        lock_state(&shared.inner).lifecycle,
        ServiceLifecycle::Retired(_)
    ));
    assert_reopens(&directory);
}

#[test]
fn foreign_handles_fail_preparation_and_release_candidate_custody() {
    for foreign_assets in [false, true] {
        let (directory, candidate, storage, assets, _) = candidate();
        let (_other_directory, other_candidate, other_storage, other_assets, _) = self::candidate();
        let result = if foreign_assets {
            PreparedMarkerServices::prepare(candidate, storage, other_assets, limits())
        } else {
            PreparedMarkerServices::prepare(candidate, other_storage, assets, limits())
        };
        if foreign_assets {
            assert!(matches!(result, Err(MarkerPreparationError::Asset(_))));
        } else {
            assert!(matches!(result, Err(MarkerPreparationError::Syndic(_))));
        }
        assert_eq!(other_candidate.health().state(), HomeHealthState::Opening);
        assert_reopens(&directory);
    }
}

#[test]
fn failed_candidate_read_confirmation_aborts_preparation() {
    let (directory, candidate, storage, assets, faults) = candidate();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(matches!(
        PreparedMarkerServices::prepare(candidate, storage, assets, limits()),
        Err(MarkerPreparationError::Syndic(_))
    ));
    assert_reopens(&directory);
}

#[test]
fn failed_asset_confirmation_aborts_after_successful_syndic_validation() {
    let (directory, candidate, storage, assets, faults) = candidate();
    let block = faults.block_next(FaultPoint::BeforeReadConfirmation);
    let worker = std::thread::spawn(move || {
        PreparedMarkerServices::prepare(candidate, storage, assets, limits())
    });
    assert!(block.wait_until_reached(std::time::Duration::from_secs(5)));
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    block.release();
    assert!(matches!(
        worker.join().unwrap(),
        Err(MarkerPreparationError::Asset(_))
    ));
    assert_reopens(&directory);
}

#[test]
fn handles_from_an_abandoned_candidate_cannot_authorize_a_reopened_candidate() {
    for stale_assets in [false, true] {
        let (directory, old_candidate, old_storage, old_assets, _) = candidate();
        drop(old_candidate);
        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let assets = BerylState::register(&mut candidate).unwrap().assets();
        let candidate = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap();
        let result = if stale_assets {
            PreparedMarkerServices::prepare(candidate, storage, old_assets, limits())
        } else {
            PreparedMarkerServices::prepare(candidate, old_storage, assets, limits())
        };
        if stale_assets {
            assert!(matches!(result, Err(MarkerPreparationError::Asset(_))));
        } else {
            assert!(matches!(result, Err(MarkerPreparationError::Syndic(_))));
        }
        assert_reopens(&directory);
    }
}
