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
    .unwrap()
    .close()
    .unwrap();
}

fn assert_still_owned(directory: &tempfile::TempDir) {
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .is_err()
    );
}

#[test]
fn preparation_preserves_candidate_identity_and_abandonment_retires_shared_state() {
    let (directory, mut candidate, storage, assets, _) = candidate();
    let reference = candidate.service_reference();
    let generation = candidate.generation();
    let prepared =
        PreparedMarkerServices::prepare(&mut candidate, storage, assets, limits()).unwrap();
    assert_eq!(candidate.generation(), generation);
    assert_eq!(reference.health().state(), HomeHealthState::Opening);
    assert!(reference.home_revision().is_err());
    let shared = prepared.service.as_ref().unwrap().clone();
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
    assert_eq!(candidate.health().state(), HomeHealthState::Opening);
    assert_eq!(
        candidate.recovery_access().unwrap().generation(),
        generation
    );
    assert_still_owned(&directory);
    drop(candidate);
    assert_reopens(&directory);
}

#[test]
fn abandoned_component_can_be_prepared_again_on_the_same_owned_candidate() {
    let (directory, mut candidate, storage, assets, _) = candidate();
    let revisions = {
        let access = candidate.recovery_access().unwrap();
        (
            storage.revision_candidate(&access).unwrap(),
            assets.revision_candidate(&access).unwrap(),
        )
    };
    let prepared =
        PreparedMarkerServices::prepare(&mut candidate, storage.clone(), assets.clone(), limits())
            .unwrap();
    let old = prepared.service.as_ref().unwrap().clone();
    drop(prepared);
    let fresh =
        PreparedMarkerServices::prepare(&mut candidate, storage.clone(), assets.clone(), limits())
            .unwrap();
    assert!(!Arc::ptr_eq(
        &old.inner,
        &fresh.service.as_ref().unwrap().inner
    ));
    assert!(matches!(
        lock_state(&old.inner).lifecycle,
        ServiceLifecycle::Retired(_)
    ));
    {
        let access = candidate.recovery_access().unwrap();
        assert_eq!(
            (
                storage.revision_candidate(&access).unwrap(),
                assets.revision_candidate(&access).unwrap(),
            ),
            revisions,
        );
    }
    drop(fresh);
    assert_eq!(candidate.health().state(), HomeHealthState::Opening);
    assert_still_owned(&directory);
    drop(candidate);
    assert_reopens(&directory);
}

#[test]
fn consuming_transfer_preserves_service_until_the_graph_retires_it() {
    let (directory, mut candidate, storage, assets, _) = candidate();
    let prepared =
        PreparedMarkerServices::prepare(&mut candidate, storage, assets, limits()).unwrap();
    let retained = prepared.service.as_ref().unwrap().clone();
    let service = prepared.into_service();
    assert!(Arc::ptr_eq(&retained.inner, &service.inner));
    assert!(matches!(
        lock_state(&retained.inner).lifecycle,
        ServiceLifecycle::Active
    ));
    assert_eq!(candidate.health().state(), HomeHealthState::Opening);
    assert_still_owned(&directory);
    service.retire_home_generation();
    assert!(matches!(
        lock_state(&retained.inner).lifecycle,
        ServiceLifecycle::Retired(_)
    ));
    drop(service);
    drop(candidate);
    assert_reopens(&directory);
}

#[test]
fn foreign_handles_fail_preparation_without_consuming_candidate_custody() {
    for foreign_assets in [false, true] {
        let (directory, mut candidate, storage, assets, _) = candidate();
        let (_other_directory, other_candidate, other_storage, other_assets, _) = self::candidate();
        let result = if foreign_assets {
            PreparedMarkerServices::prepare(&mut candidate, storage, other_assets, limits())
        } else {
            PreparedMarkerServices::prepare(&mut candidate, other_storage, assets, limits())
        };
        if foreign_assets {
            assert!(matches!(result, Err(MarkerPreparationError::Asset(_))));
        } else {
            assert!(matches!(result, Err(MarkerPreparationError::Syndic(_))));
        }
        assert_eq!(other_candidate.health().state(), HomeHealthState::Opening);
        assert_eq!(candidate.health().state(), HomeHealthState::Opening);
        assert!(candidate.recovery_access().is_ok());
        assert_still_owned(&directory);
        drop(candidate);
        assert_reopens(&directory);
    }
}

#[test]
fn failed_candidate_read_confirmation_aborts_preparation() {
    let (directory, mut candidate, storage, assets, faults) = candidate();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(matches!(
        PreparedMarkerServices::prepare(&mut candidate, storage, assets, limits()),
        Err(MarkerPreparationError::Syndic(_))
    ));
    assert_eq!(candidate.health().state(), HomeHealthState::Failed);
    assert_still_owned(&directory);
    drop(candidate);
    assert_reopens(&directory);
}

#[test]
fn failed_asset_confirmation_aborts_after_successful_syndic_validation() {
    let (directory, mut candidate, storage, assets, faults) = candidate();
    let block = faults.block_next(FaultPoint::BeforeReadConfirmation);
    std::thread::scope(|scope| {
        let worker = scope
            .spawn(|| PreparedMarkerServices::prepare(&mut candidate, storage, assets, limits()));
        let reached = block.wait_until_reached(std::time::Duration::from_secs(5));
        if reached {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
        }
        block.release();
        let result = worker.join().unwrap();
        assert!(reached);
        assert!(matches!(result, Err(MarkerPreparationError::Asset(_))));
    });
    assert_eq!(candidate.health().state(), HomeHealthState::Failed);
    assert_still_owned(&directory);
    drop(candidate);
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
        let mut candidate = candidate
            .prepare_publication(
                BerylState::required_domains()
                    .unwrap()
                    .merge(SyndicStorage::required_domains().unwrap())
                    .unwrap(),
            )
            .unwrap();
        let result = if stale_assets {
            PreparedMarkerServices::prepare(&mut candidate, storage, old_assets, limits())
        } else {
            PreparedMarkerServices::prepare(&mut candidate, old_storage, assets, limits())
        };
        if stale_assets {
            assert!(matches!(result, Err(MarkerPreparationError::Asset(_))));
        } else {
            assert!(matches!(result, Err(MarkerPreparationError::Syndic(_))));
        }
        assert_eq!(candidate.health().state(), HomeHealthState::Opening);
        assert!(candidate.recovery_access().is_ok());
        assert_still_owned(&directory);
        drop(candidate);
        assert_reopens(&directory);
    }
}
