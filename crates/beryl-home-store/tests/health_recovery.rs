#![cfg(feature = "test-faults")]

#[path = "support/fjall.rs"]
mod fjall_support;
mod support;

use std::{fs, process::Command, thread, time::Duration};

use beryl_home_store::{
    HomeHealthSnapshot, HomeHealthState, HomeOpenOptions, HomeRecoveryError, HomeSchemaVersion,
    HomeStore, ReadError,
    test_faults::{FaultController, FaultPoint},
};
use fjall::{Database, PersistMode};
use tempfile::tempdir;

use support::AlphaDomain;

fn open(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

#[test]
fn opening_snapshot_has_no_generation() {
    let opening = HomeHealthSnapshot::opening();
    assert_eq!(opening.state(), HomeHealthState::Opening);
    assert_eq!(opening.generation(), None);
}

#[test]
fn candidate_abort_retains_failed_authority_and_allows_a_fresh_retry() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let stale = store.register_domain::<AlphaDomain>().unwrap();
    let home_id = store.home_id();
    let original_generation = store.health().generation().unwrap();
    let original_tier = store.durability_tier();

    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(matches!(
        store.home_revision(),
        Err(ReadError::Storage { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    let candidate = store.recover_same_home().unwrap();
    assert_eq!(candidate.home_id(), home_id);
    assert_eq!(candidate.generation().get(), original_generation.get() + 1);
    assert_eq!(candidate.durability_tier(), original_tier);
    let aborted = candidate.domain_handle::<AlphaDomain>().unwrap();
    let failed = candidate.abort();
    assert_eq!(failed.health().state(), HomeHealthState::Failed);

    let candidate = failed.recover_same_home().unwrap();
    let current = candidate.domain_handle::<AlphaDomain>().unwrap();
    let recovered = candidate.publish();
    assert_eq!(recovered.health().state(), HomeHealthState::Healthy);
    assert!(recovered.domain_revision(&stale).is_err());
    assert!(recovered.domain_revision(&aborted).is_err());
    assert_eq!(recovered.domain_revision(&current).unwrap().get(), 1);
    recovered.close().unwrap();
}

#[test]
fn recovery_is_rejected_outside_failed_authority() {
    let directory = tempdir().unwrap();
    let store = open(directory.path(), FaultController::new());
    let failure = store.recover_same_home().unwrap_err();
    assert_eq!(
        failure.into_store().health().state(),
        HomeHealthState::Healthy
    );
}

fn fail_store(store: HomeStore, faults: &FaultController) -> HomeStore {
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(matches!(
        store.home_revision(),
        Err(ReadError::Storage { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    store
}

#[test]
fn recovery_rejects_current_state_file_without_fresh_fallback() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = fail_store(open(directory.path(), faults.clone()), &faults);
    let state = directory.path().join("state");
    let saved_state = directory.path().join("saved-state");
    let block = faults.block_next(FaultPoint::BeforeReopen);
    let worker = thread::spawn(move || store.recover_same_home());
    assert!(block.wait_until_reached(Duration::from_secs(10)));

    fs::rename(&state, &saved_state).unwrap();
    fs::write(&state, b"state-file-collision").unwrap();
    block.release();

    let failure = worker
        .join()
        .unwrap()
        .expect_err("candidate must remain unpublished");
    assert!(matches!(failure.error(), HomeRecoveryError::Layout { .. }));
    assert_eq!(fs::read(&state).unwrap(), b"state-file-collision");
    assert!(saved_state.join("version").is_file());
    let failed = failure.into_store();
    assert_eq!(failed.health().state(), HomeHealthState::Failed);
    failed.close().unwrap();
}

#[test]
fn recovery_rejects_missing_current_state_without_fresh_fallback() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = fail_store(open(directory.path(), faults.clone()), &faults);
    let state = directory.path().join("state");
    let saved_state = directory.path().join("saved-state");
    let block = faults.block_next(FaultPoint::BeforeReopen);
    let worker = thread::spawn(move || store.recover_same_home());
    assert!(block.wait_until_reached(Duration::from_secs(10)));

    fs::rename(&state, &saved_state).unwrap();
    block.release();

    let failure = worker
        .join()
        .unwrap()
        .expect_err("candidate must remain unpublished");
    assert!(matches!(failure.error(), HomeRecoveryError::Layout { .. }));
    assert!(!state.exists());
    assert!(saved_state.join("version").is_file());
    let failed = failure.into_store();
    assert_eq!(failed.health().state(), HomeHealthState::Failed);
    failed.close().unwrap();
}

#[test]
fn recovery_rejects_current_state_header_schema_mismatch() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = fail_store(open(directory.path(), faults.clone()), &faults);
    let state = directory.path().join("state");
    let block = faults.block_next(FaultPoint::BeforeReopen);
    let worker = thread::spawn(move || store.recover_same_home());
    assert!(block.wait_until_reached(Duration::from_secs(10)));

    let database = Database::recover(fjall_support::config(&state)).unwrap();
    let header = database.open_keyspace("_beryl_home").unwrap();
    let mut mismatched_header = [0_u8; 30];
    mismatched_header[..8].copy_from_slice(b"BRYLHOME");
    mismatched_header[8..10].copy_from_slice(&1_u16.to_be_bytes());
    mismatched_header[10..14].copy_from_slice(&2_u32.to_be_bytes());
    header.insert(b"header", mismatched_header).unwrap();
    database.persist(PersistMode::SyncAll).unwrap();
    drop(header);
    drop(database);
    block.release();

    let failure = worker
        .join()
        .unwrap()
        .expect_err("candidate must remain unpublished");
    assert!(matches!(failure.error(), HomeRecoveryError::HomeMismatch));
    let failed = failure.into_store();
    assert_eq!(failed.health().state(), HomeHealthState::Failed);
    failed.close().unwrap();
}

#[test]
fn recovery_rejects_current_state_reparse_point() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = fail_store(open(directory.path(), faults.clone()), &faults);
    let state = directory.path().join("state");
    let saved_state = directory.path().join("saved-state");
    let external = directory.path().join("external-state");
    let block = faults.block_next(FaultPoint::BeforeReopen);
    let worker = thread::spawn(move || store.recover_same_home());
    assert!(block.wait_until_reached(Duration::from_secs(10)));

    fs::rename(&state, &saved_state).unwrap();
    fs::create_dir(&external).unwrap();
    let sentinel = external.join("sentinel");
    fs::write(&sentinel, b"external state remains untouched").unwrap();
    let output = Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(&state)
        .arg(&external)
        .output()
        .expect("run built-in junction command");
    assert!(
        output.status.success(),
        "junction creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    block.release();

    let failure = worker
        .join()
        .unwrap()
        .expect_err("candidate must remain unpublished");
    assert!(matches!(failure.error(), HomeRecoveryError::Layout { .. }));
    assert!(state.is_dir());
    assert_eq!(
        fs::read(&sentinel).unwrap(),
        b"external state remains untouched"
    );
    assert!(!external.join("version").exists());
    assert!(saved_state.join("version").is_file());
    let failed = failure.into_store();
    assert_eq!(failed.health().state(), HomeHealthState::Failed);
    failed.close().unwrap();
    fs::remove_dir(&state).unwrap();
}
