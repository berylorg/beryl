#![cfg(feature = "test-faults")]

use std::{fs, num::NonZeroUsize, sync::mpsc, time::Duration};

use beryl_home_store::{
    HomeDomainRequirements, HomeHealthState, HomeOpenCandidate, HomeOpenOptions,
    HomeOpenPublication, HomeSchemaVersion, ThemeWatchError, ThemeWatchHint, ThemeWatchLimits,
    test_faults::{FaultController, FaultPoint},
};

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

fn candidate(path: &std::path::Path, faults: &FaultController) -> HomeOpenPublication {
    HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap()
    .prepare_publication(HomeDomainRequirements::new())
    .unwrap()
}

#[test]
fn preparation_is_dormant_until_exact_publication_and_release() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = candidate(directory.path(), &faults);
    let reference = candidate.service_reference();
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    let prepared = candidate.prepare_theme_changes(limits()).unwrap();
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    assert_eq!(reference.health().state(), HomeHealthState::Opening);
    assert!(reference.home_revision().is_err());
    assert!(matches!(
        candidate.prepare_theme_changes(limits()),
        Err(ThemeWatchError::AlreadySubscribed)
    ));
    let generation = candidate.generation();
    let store = candidate.publish().unwrap();
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    let subscription = prepared.release().unwrap();
    assert_eq!(store.health().generation(), Some(generation));
    assert!(observation.wait_until_reached(Duration::from_secs(3)));
    observation.release();
    assert_eq!(
        subscription.recv_timeout(Duration::from_secs(3)).unwrap(),
        Some(ThemeWatchHint::Overflow)
    );
    let themes = directory.path().join("themes");
    fs::create_dir_all(&themes).unwrap();
    fs::write(themes.join("manifest.toml"), b"changed").unwrap();
    assert_eq!(
        subscription.recv_timeout(Duration::from_secs(3)).unwrap(),
        Some(ThemeWatchHint::ManifestChanged)
    );
    drop(subscription);
    store.close().unwrap();
}

#[test]
fn premature_release_and_abandonment_join_and_release_subscription_capacity() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = candidate(directory.path(), &faults);
    let prepared = candidate.prepare_theme_changes(limits()).unwrap();
    assert!(matches!(
        prepared.release(),
        Err(ThemeWatchError::Health(_))
    ));
    let prepared = candidate.prepare_theme_changes(limits()).unwrap();
    let (done, completed) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        drop(prepared);
        done.send(()).unwrap();
    });
    completed.recv_timeout(Duration::from_secs(3)).unwrap();
    worker.join().unwrap();
    drop(candidate.prepare_theme_changes(limits()).unwrap());
    candidate.close().unwrap();
    HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap()
    .close()
    .unwrap();
}

#[test]
fn failed_spawn_returns_capacity_without_observing_repository() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = candidate(directory.path(), &faults);
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    faults.fail_next(FaultPoint::BeforeThemeWatchSpawn);
    assert!(matches!(
        candidate.prepare_theme_changes(limits()),
        Err(ThemeWatchError::ShutDown)
    ));
    drop(candidate.prepare_theme_changes(limits()).unwrap());
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    observation.release();
    candidate.close().unwrap();
}

#[test]
fn candidate_retirement_cancels_retained_preparation_without_owning_home_lock() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = candidate(directory.path(), &faults);
    let prepared = candidate.prepare_theme_changes(limits()).unwrap();
    candidate.close().unwrap();
    let reopened = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    assert!(prepared.release().is_err());
    reopened.close().unwrap();
}

#[test]
fn recovered_generation_prepares_fresh_worker_and_rejects_old_release() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut initial = candidate(directory.path(), &faults);
    let old = initial.prepare_theme_changes(limits()).unwrap();
    let store = initial.publish().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovered = store.recover_same_home().unwrap();
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    let fresh = recovered.prepare_theme_changes(limits()).unwrap();
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    let store = recovered.publish().unwrap();
    assert!(old.release().is_err());
    let subscription = fresh.release().unwrap();
    assert!(observation.wait_until_reached(Duration::from_secs(3)));
    observation.release();
    assert_eq!(
        subscription.recv_timeout(Duration::from_secs(3)).unwrap(),
        Some(ThemeWatchHint::Overflow)
    );
    drop(subscription);
    store.close().unwrap();
}

#[test]
fn publication_failure_keeps_watcher_dormant_and_cleanup_joined() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = candidate(directory.path(), &faults);
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    let prepared = candidate.prepare_theme_changes(limits()).unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .is_err()
    );
    let candidate = candidate.publish().unwrap_err().into_parts().1;
    assert!(prepared.release().is_err());
    assert!(!observation.wait_until_reached(Duration::from_millis(60)));
    observation.release();
    candidate.close().unwrap();
}

#[test]
fn released_subscription_destruction_joins_in_progress_observation() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = candidate(directory.path(), &faults);
    let prepared = candidate.prepare_theme_changes(limits()).unwrap();
    let store = candidate.publish().unwrap();
    let observation = faults.block_next(FaultPoint::BeforeThemeWatchObservation);
    let subscription = prepared.release().unwrap();
    assert!(observation.wait_until_reached(Duration::from_secs(3)));
    let (done, completed) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        drop(subscription);
        done.send(()).unwrap();
    });
    assert!(completed.recv_timeout(Duration::from_millis(60)).is_err());
    observation.release();
    completed.recv_timeout(Duration::from_secs(3)).unwrap();
    worker.join().unwrap();
    drop(store.subscribe_theme_changes(limits()).unwrap());
    store.close().unwrap();
}
