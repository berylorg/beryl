#![cfg(feature = "test-faults")]

mod support;

use std::path::Path;

use beryl_home_store::{
    HomeDomainRequirements, HomeHealthState, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, ReadError,
    test_faults::{FaultController, FaultPoint},
};
use support::AlphaDomain;

fn open(path: &Path, faults: &FaultController) -> HomeStore {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    candidate.register_domain::<AlphaDomain>().unwrap();
    candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<AlphaDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap()
}

#[test]
fn identity_agrees_across_ordinary_references_and_retains_no_home_lifecycle() {
    let directory = tempfile::tempdir().unwrap();
    let home = open(directory.path(), &FaultController::new());
    let identity = home.generation_identity().unwrap();
    assert_eq!(identity, home.generation_identity().unwrap());
    let reference = home.service_reference();
    assert_eq!(identity, reference.generation_identity().unwrap());
    drop(reference);
    let home_id = home.home_id();
    let generation = home.health().generation();
    home.close().unwrap();
    let reopened = open(directory.path(), &FaultController::new());
    assert_eq!(reopened.home_id(), home_id);
    assert_eq!(reopened.health().generation(), generation);
    assert_ne!(identity, reopened.generation_identity().unwrap());
    reopened.close().unwrap();
}

#[test]
fn identity_distinguishes_foreign_openings_with_equal_numeric_generations() {
    let directory = tempfile::tempdir().unwrap();
    let foreign_directory = tempfile::tempdir().unwrap();
    let home = open(directory.path(), &FaultController::new());
    let foreign = open(foreign_directory.path(), &FaultController::new());
    assert_eq!(home.health().generation(), foreign.health().generation());
    assert_ne!(
        home.generation_identity().unwrap(),
        foreign.generation_identity().unwrap()
    );
    home.close().unwrap();
    foreign.close().unwrap();
}

#[test]
fn identity_capture_refuses_failed_or_unpublished_recovery_and_changes_after_publication() {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let home = open(directory.path(), &faults);
    let identity = home.generation_identity().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    assert_eq!(home.health().state(), HomeHealthState::Failed);
    assert!(matches!(
        home.generation_identity(),
        Err(ReadError::HealthGate(_))
    ));
    let candidate = home.recover_same_home().unwrap();
    let failed = candidate.abort();
    assert!(matches!(
        failed.generation_identity(),
        Err(ReadError::HealthGate(_))
    ));
    let home = failed.recover_same_home().unwrap().publish().unwrap();
    assert_ne!(identity, home.generation_identity().unwrap());
    home.close().unwrap();
}
