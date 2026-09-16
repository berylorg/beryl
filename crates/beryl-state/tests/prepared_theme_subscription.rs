use std::{num::NonZeroUsize, time::Duration};

use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeOpenPublication, HomeSchemaVersion, ThemeWatchError,
    ThemeWatchLimits,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{BerylState, ThemeChangeHint, ThemeChangeSubscriptionError, ThemeService};

fn limits() -> ThemeWatchLimits {
    ThemeWatchLimits::new(
        Duration::from_millis(15),
        NonZeroUsize::new(4).unwrap(),
        NonZeroUsize::new(8).unwrap(),
        4096,
        NonZeroUsize::new(128).unwrap(),
    )
    .unwrap()
}

fn candidate(
    directory: &tempfile::TempDir,
    faults: &FaultController,
) -> (HomeOpenPublication, ThemeService) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    (
        candidate
            .prepare_publication(BerylState::required_domains().unwrap())
            .unwrap(),
        state.themes(),
    )
}

#[test]
fn publication_transfers_dormant_activity_to_typed_subscription() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (mut candidate, service) = candidate(&directory, &faults);
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    let prepared = service
        .prepare_initial_changes(&mut candidate, limits())
        .unwrap();
    assert_eq!(service.diagnostics().active_subscriptions(), 1);
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    let store = candidate.publish().unwrap();
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    let subscription = prepared.release().unwrap();
    assert!(observation.wait_until_reached(Duration::from_secs(3)));
    observation.release();
    assert_eq!(service.diagnostics().active_subscriptions(), 1);
    assert_eq!(
        subscription.recv_timeout(Duration::from_secs(3)).unwrap(),
        Some(ThemeChangeHint::Overflow)
    );
    subscription.shutdown();
    assert_eq!(service.diagnostics().active_subscriptions(), 0);
    store.close().unwrap();
}

#[test]
fn failed_preparation_release_and_abandonment_return_activity_and_capacity() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (mut candidate, service) = candidate(&directory, &faults);
    faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
    assert!(matches!(
        service.prepare_initial_changes(&mut candidate, limits()),
        Err(ThemeChangeSubscriptionError::Watcher(
            ThemeWatchError::ShutDown
        ))
    ));
    assert_eq!(service.diagnostics().active_subscriptions(), 0);
    let prepared = service
        .prepare_initial_changes(&mut candidate, limits())
        .unwrap();
    assert!(matches!(
        prepared.release(),
        Err(ThemeChangeSubscriptionError::Watcher(
            ThemeWatchError::Health(_)
        ))
    ));
    assert_eq!(service.diagnostics().active_subscriptions(), 0);
    drop(
        service
            .prepare_initial_changes(&mut candidate, limits())
            .unwrap(),
    );
    assert_eq!(service.diagnostics().active_subscriptions(), 0);
    drop(
        service
            .prepare_initial_changes(&mut candidate, limits())
            .unwrap(),
    );
    candidate.close().unwrap();
}

#[test]
fn foreign_and_stale_services_cannot_prepare_and_old_preparation_cannot_release() {
    let directory = tempfile::tempdir().unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (mut candidate, service) = candidate(&directory, &faults);
    let (foreign, foreign_service) = self::candidate(&foreign_directory, &FaultController::new());
    assert!(matches!(
        foreign_service.prepare_initial_changes(&mut candidate, limits()),
        Err(ThemeChangeSubscriptionError::Freshness(_))
    ));
    let old = service
        .prepare_initial_changes(&mut candidate, limits())
        .unwrap();
    let store = candidate.publish().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    assert!(matches!(
        service.prepare_recovered_changes(&mut recovered, limits()),
        Err(ThemeChangeSubscriptionError::Freshness(_))
    ));
    assert!(matches!(
        foreign_service.prepare_recovered_changes(&mut recovered, limits()),
        Err(ThemeChangeSubscriptionError::Freshness(_))
    ));
    let fresh = BerylState::reacquire_candidate(&recovered)
        .unwrap()
        .themes();
    let prepared = fresh
        .prepare_recovered_changes(&mut recovered, limits())
        .unwrap();
    let store = recovered.publish().unwrap();
    assert!(old.release().is_err());
    assert_eq!(service.diagnostics().active_subscriptions(), 0);
    let subscription = prepared.release().unwrap();
    assert_eq!(
        subscription.recv_timeout(Duration::from_secs(3)).unwrap(),
        Some(ThemeChangeHint::Overflow)
    );
    drop(subscription);
    assert_eq!(fresh.diagnostics().active_subscriptions(), 0);
    store.close().unwrap();
    foreign.close().unwrap();
}

#[test]
fn activity_remains_owned_until_worker_destruction_finishes() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let (mut candidate, service) = candidate(&directory, &faults);
    let observer = service.clone();
    let prepared = service
        .prepare_initial_changes(&mut candidate, limits())
        .unwrap();
    drop(service);
    let store = candidate.publish().unwrap();
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    let subscription = prepared.release().unwrap();
    assert!(observation.wait_until_reached(Duration::from_secs(3)));
    let (done, completed) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        drop(subscription);
        done.send(()).unwrap();
    });
    assert!(completed.recv_timeout(Duration::from_millis(60)).is_err());
    assert_eq!(observer.diagnostics().active_subscriptions(), 1);
    observation.release();
    completed.recv_timeout(Duration::from_secs(3)).unwrap();
    worker.join().unwrap();
    assert_eq!(observer.diagnostics().active_subscriptions(), 0);
    store.close().unwrap();
}
