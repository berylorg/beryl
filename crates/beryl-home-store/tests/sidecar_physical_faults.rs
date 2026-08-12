#![cfg(feature = "test-faults")]

use std::{error::Error, fs, io, num::NonZeroU64, path::Path};

use beryl_home_store::{
    HomeDurabilityTier, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    SidecarByteLimit, SidecarError, SidecarNamespace, SidecarStage,
    test_faults::{FaultController, FaultPoint},
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

fn limit() -> SidecarByteLimit {
    SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap())
}

fn open(path: &Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn open_with_tier(
    path: &Path,
    faults: FaultController,
    durability_tier: HomeDurabilityTier,
) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)
            .with_durability_tier_for_tests(durability_tier),
        faults,
    )
    .unwrap()
}

fn assert_io_kind(source: &(dyn Error + Send + Sync + 'static), expected: io::ErrorKind) {
    let source = source
        .downcast_ref::<io::Error>()
        .expect("deterministic sidecar fault must remain an io::Error");
    assert_eq!(source.kind(), expected);
}

#[test]
fn directory_sync_tier_treatment_distinguishes_physical_unsupported_and_faults() {
    let bytes = b"directory sync tier result";

    let directory = tempdir().unwrap();
    let store = open_with_tier(
        directory.path(),
        FaultController::new(),
        HomeDurabilityTier::BestEffort,
    );
    store.fail_next_sidecar_directory_sync_for_tests(io::ErrorKind::Unsupported);
    assert!(
        store
            .admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit())
            .is_ok()
    );
    assert_eq!(store.health().state(), HomeHealthState::Healthy);

    let directory = tempdir().unwrap();
    let store = open_with_tier(
        directory.path(),
        FaultController::new(),
        HomeDurabilityTier::BestEffort,
    );
    store.fail_next_sidecar_directory_sync_for_tests(io::ErrorKind::PermissionDenied);
    let error = store
        .admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit())
        .unwrap_err();
    assert_storage_error_kind(error, io::ErrorKind::PermissionDenied);

    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = open_with_tier(
        directory.path(),
        faults.clone(),
        HomeDurabilityTier::BestEffort,
    );
    faults.fail_next_with_kind(
        FaultPoint::BeforeSidecarRootDirectorySync,
        io::ErrorKind::Unsupported,
    );
    let error = store
        .admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit())
        .unwrap_err();
    assert_storage_error_kind(error, io::ErrorKind::Unsupported);

    let directory = tempdir().unwrap();
    let store = open_with_tier(
        directory.path(),
        FaultController::new(),
        HomeDurabilityTier::Full,
    );
    store.fail_next_sidecar_directory_sync_for_tests(io::ErrorKind::Unsupported);
    let error = store
        .admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit())
        .unwrap_err();
    assert_storage_error_kind(error, io::ErrorKind::Unsupported);
}

fn assert_storage_error_kind(error: SidecarError, expected: io::ErrorKind) {
    match error {
        SidecarError::Storage { stage, source } => {
            assert_eq!(stage, SidecarStage::FlushDirectory);
            assert_io_kind(source.as_ref(), expected);
        }
        other => panic!("unexpected sidecar error: {other:?}"),
    }
}

#[test]
fn truncating_a_final_sidecar_fails_structural_verification() {
    let directory = tempdir().unwrap();
    let store = open(directory.path(), FaultController::new());
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"truncation target",
            limit(),
        )
        .unwrap();
    let address = sidecar.address().clone();
    let path = sidecar.path().to_path_buf();
    drop(sidecar);

    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(address.length() - 1).unwrap();
    file.sync_all().unwrap();
    drop(file);

    assert!(matches!(
        store.verify_sidecar(&address, limit()),
        Err(SidecarError::ContentMismatch)
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert_eq!(fs::metadata(path).unwrap().len(), address.length() - 1);
}

#[test]
fn fault_targets_the_final_post_rename_containing_directory_sync() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(directory.path(), faults.clone());
    let bytes = b"final directory barrier";
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    let digest_hex = hex::encode(digest);
    let shard = directory
        .path()
        .join("sidecars")
        .join("images")
        .join(&digest_hex[..2]);
    fs::create_dir_all(&shard).unwrap();

    faults.fail_next_with_kind(
        FaultPoint::BeforeSidecarFinalDirectorySync,
        io::ErrorKind::PermissionDenied,
    );
    let error = store
        .admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit())
        .unwrap_err();
    match error {
        SidecarError::Storage { stage, source } => {
            assert_eq!(stage, SidecarStage::FlushDirectory);
            assert_io_kind(source.as_ref(), io::ErrorKind::PermissionDenied);
        }
        other => panic!("unexpected sidecar error: {other:?}"),
    }

    let final_path = shard.join(digest_hex);
    assert_eq!(fs::read(final_path).unwrap(), bytes);
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    let store = store.recover_same_home().unwrap().publish();
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}

#[test]
fn final_sidecar_verification_fault_surfaces_before_reading_the_final_file() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(directory.path(), faults.clone());
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"verify me",
            limit(),
        )
        .unwrap();
    let address = sidecar.address().clone();
    drop(sidecar);

    faults.fail_next_with_kind(
        FaultPoint::BeforeSidecarVerification,
        io::ErrorKind::NotFound,
    );
    let error = store.verify_sidecar(&address, limit()).unwrap_err();
    match error {
        SidecarError::Storage { stage, source } => {
            assert_eq!(stage, SidecarStage::OpenFinal);
            assert_io_kind(source.as_ref(), io::ErrorKind::NotFound);
        }
        other => panic!("unexpected sidecar error: {other:?}"),
    }
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    let store = store.recover_same_home().unwrap().publish();
    assert!(store.verify_sidecar(&address, limit()).is_ok());
}
