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

fn recovery_candidate() -> (
    tempfile::TempDir,
    HomeRecoveryCandidate,
    SyndicStorage,
    AssetState,
    FaultController,
) {
    let (directory, initial, storage, assets, faults) = candidate();
    let store = initial.publish().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let recovery = store.recover_same_home().unwrap();
    (directory, recovery, storage, assets, faults)
}

#[test]
fn recovery_preparation_preserves_custody_and_revisions_through_transfer_and_abandonment() {
    for transfer in [false, true] {
        let (directory, mut candidate, _, _, _) = recovery_candidate();
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let assets = BerylState::reacquire_candidate(&candidate)
            .unwrap()
            .assets();
        let reference = candidate.service_reference();
        let generation = candidate.generation();
        let revisions = {
            let access = candidate.recovery_access().unwrap();
            (
                storage.revision_candidate(&access).unwrap(),
                assets.revision_candidate(&access).unwrap(),
            )
        };
        let prepared = PreparedMarkerServices::prepare_recovery(
            &mut candidate,
            storage.clone(),
            assets.clone(),
            limits(),
        )
        .unwrap();
        let retained = prepared.service.as_ref().unwrap().clone();
        {
            let state = lock_state(&retained.inner);
            assert_eq!(state.home_id, candidate.home_id());
            assert_eq!(state.home_generation, generation);
            assert_eq!(state.limits, limits());
            assert!(state.flights.is_empty());
            assert!(matches!(state.lifecycle, ServiceLifecycle::Active));
        }
        assert_eq!(reference.health().state(), HomeHealthState::Reopening);
        assert!(reference.home_revision().is_err());
        if transfer {
            let service = prepared.into_service();
            assert!(Arc::ptr_eq(&service.inner, &retained.inner));
            assert!(matches!(
                lock_state(&retained.inner).lifecycle,
                ServiceLifecycle::Active
            ));
            service.retire_home_generation();
        } else {
            drop(prepared);
        }
        assert!(matches!(
            lock_state(&retained.inner).lifecycle,
            ServiceLifecycle::Retired(_)
        ));
        {
            let access = candidate.recovery_access().unwrap();
            assert_eq!(
                (
                    storage.revision_candidate(&access).unwrap(),
                    assets.revision_candidate(&access).unwrap(),
                ),
                revisions
            );
        }
        assert_still_owned(&directory);
        drop((retained, storage, assets));
        candidate.abort().close().unwrap();
        assert_reopens(&directory);
    }
}

#[test]
fn recovery_preparation_rejects_stale_and_foreign_handles_without_losing_the_home() {
    for foreign in [false, true] {
        for wrong_assets in [false, true] {
            let (directory, mut candidate, old_storage, old_assets, _) = recovery_candidate();
            let (_other_directory, other_candidate, other_storage, other_assets, _) =
                self::candidate();
            let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
            let assets = BerylState::reacquire_candidate(&candidate)
                .unwrap()
                .assets();
            let (bad_storage, bad_assets) = if foreign {
                (other_storage, other_assets)
            } else {
                (old_storage, old_assets)
            };
            let result = if wrong_assets {
                PreparedMarkerServices::prepare_recovery(
                    &mut candidate,
                    storage,
                    bad_assets,
                    limits(),
                )
            } else {
                PreparedMarkerServices::prepare_recovery(
                    &mut candidate,
                    bad_storage,
                    assets,
                    limits(),
                )
            };
            if wrong_assets {
                assert!(matches!(result, Err(MarkerPreparationError::Asset(_))));
            } else {
                assert!(matches!(result, Err(MarkerPreparationError::Syndic(_))));
            }
            assert_eq!(
                candidate.service_reference().health().state(),
                HomeHealthState::Reopening
            );
            assert!(candidate.recovery_access().is_ok());
            assert_still_owned(&directory);
            drop(other_candidate);
            candidate.abort().close().unwrap();
            assert_reopens(&directory);
        }
    }
}

#[test]
fn recovery_preparation_read_failures_preserve_explicit_abort_custody() {
    for asset_failure in [false, true] {
        let (directory, mut candidate, _, _, faults) = recovery_candidate();
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        let assets = BerylState::reacquire_candidate(&candidate)
            .unwrap()
            .assets();
        let retry_storage = storage.clone();
        let retry_assets = assets.clone();
        let result = if asset_failure {
            let block = faults.block_next(FaultPoint::BeforeReadConfirmation);
            std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    PreparedMarkerServices::prepare_recovery(
                        &mut candidate,
                        storage,
                        assets,
                        limits(),
                    )
                });
                let reached = block.wait_until_reached(std::time::Duration::from_secs(5));
                if reached {
                    faults.fail_next(FaultPoint::BeforeReadConfirmation);
                }
                block.release();
                let result = worker.join().unwrap();
                assert!(reached);
                result
            })
        } else {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            PreparedMarkerServices::prepare_recovery(&mut candidate, storage, assets, limits())
        };
        if asset_failure {
            assert!(matches!(result, Err(MarkerPreparationError::Asset(_))));
        } else {
            assert!(matches!(result, Err(MarkerPreparationError::Syndic(_))));
        }
        assert_eq!(
            candidate.service_reference().health().state(),
            HomeHealthState::Failed
        );
        assert!(matches!(
            PreparedMarkerServices::prepare_recovery(
                &mut candidate,
                retry_storage,
                retry_assets,
                limits(),
            ),
            Err(MarkerPreparationError::Candidate(_))
        ));
        assert_still_owned(&directory);
        candidate.abort().close().unwrap();
        assert_reopens(&directory);
    }
}
