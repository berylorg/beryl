#![cfg(feature = "test-faults")]

mod support;

use std::{fs, num::NonZeroU64};

use beryl_home_store::{
    test_faults::{FaultController, FaultPoint},
    CommandError, HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    SidecarByteLimit, SidecarError, SidecarNamespace,
};
use tempfile::tempdir;

use support::{committed, AlphaDomain, PutBytes};

fn limit() -> SidecarByteLimit {
    SidecarByteLimit::new(NonZeroU64::new(1024 * 1024).unwrap())
}

fn open(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

#[test]
fn sidecar_is_durable_before_its_first_metadata_reference_commits() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults);
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"durable image bytes",
            limit(),
        )
        .unwrap();
    let address = sidecar.address().clone();
    let path = sidecar.path().to_path_buf();
    assert_eq!(fs::read(&path).unwrap(), b"durable image bytes");

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.require_sidecar(sidecar).unwrap();
    command
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(1, address.digest().as_bytes().to_vec()),
        ))
        .unwrap();
    committed(store.execute(command));
    assert!(store.verify_sidecar(&address, limit()).is_ok());

    store.close().unwrap();
    let mut reopened = open(directory.path(), FaultController::new());
    reopened.register_domain::<AlphaDomain>().unwrap();
    assert!(reopened.verify_sidecar(&address, limit()).is_ok());
}

#[test]
fn identical_content_reuses_one_final_path_without_replacement() {
    let directory = tempdir().unwrap();
    let store = open(directory.path(), FaultController::new());
    let first = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"same bytes",
            limit(),
        )
        .unwrap();
    let second = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"same bytes",
            limit(),
        )
        .unwrap();
    assert_eq!(first.address(), second.address());
    assert_eq!(first.path(), second.path());
    assert_eq!(
        fs::read_dir(first.path().parent().unwrap())
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn content_mismatch_is_a_structural_failure_and_never_overwrites() {
    let directory = tempdir().unwrap();
    let store = open(directory.path(), FaultController::new());
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"original",
            limit(),
        )
        .unwrap();
    let address = sidecar.address().clone();
    let path = sidecar.path().to_path_buf();
    drop(sidecar);
    fs::write(&path, b"tampered").unwrap();

    assert!(matches!(
        store.verify_sidecar(&address, limit()),
        Err(SidecarError::ContentMismatch)
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    assert_eq!(fs::read(path).unwrap(), b"tampered");
}

#[test]
fn failed_temporary_flush_leaves_inert_bytes_and_gates_metadata_commands() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    faults.fail_next(FaultPoint::BeforeSidecarFileSync);

    assert!(matches!(
        store.admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"not published",
            limit(),
        ),
        Err(SidecarError::Storage { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Verifying);
    let mut command = HomeCommand::new(beryl_model::HomeRevision::new(1).unwrap());
    command
        .add(alpha.contribution(
            beryl_model::DomainRevision::new(1).unwrap(),
            PutBytes::<AlphaDomain>::new(1, b"must not publish".to_vec()),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::NotCommitted {
            evidence: CommandError::HealthGate(_)
        }
    ));

    let temporary_count = count_temporary_files(directory.path());
    assert_eq!(temporary_count, 1);
    store.verify_health().unwrap();
    assert_eq!(count_temporary_files(directory.path()), temporary_count);
}

#[test]
fn failure_after_atomic_rename_retains_unreferenced_final_bytes() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = open(directory.path(), faults.clone());
    faults.fail_next(FaultPoint::AfterSidecarRename);

    assert!(matches!(
        store.admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"renamed orphan",
            limit(),
        ),
        Err(SidecarError::Storage { .. })
    ));
    let final_files = final_sidecar_files(directory.path());
    assert_eq!(final_files.len(), 1);
    assert_eq!(fs::read(&final_files[0]).unwrap(), b"renamed orphan");
    store.verify_health().unwrap();
    assert!(final_files[0].exists());
}

#[test]
fn rename_and_directory_flush_failures_never_publish_a_metadata_token() {
    for point in [
        FaultPoint::BeforeSidecarRootDirectorySync,
        FaultPoint::BeforeSidecarRename,
    ] {
        let directory = tempdir().unwrap();
        let faults = FaultController::new();
        let store = open(directory.path(), faults.clone());
        faults.fail_next(point);
        assert!(matches!(
            store.admit_sidecar(
                SidecarNamespace::new("images").unwrap(),
                b"faulted publication",
                limit(),
            ),
            Err(SidecarError::Storage { .. })
        ));
        assert_eq!(store.health().state(), HomeHealthState::Verifying);
        store.verify_health().unwrap();
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
    }
}

#[test]
fn sidecar_token_from_an_obsolete_generation_cannot_authorize_metadata() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"old generation",
            limit(),
        )
        .unwrap();

    faults.fail_next(FaultPoint::BeforeCommit);
    let mut failed = HomeCommand::new(store.home_revision().unwrap());
    failed
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(7, b"fail".to_vec()),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(failed),
        beryl_home_store::CommandOutcome::NotCommitted { .. }
    ));
    faults.fail_next(FaultPoint::BeforeVerification);
    assert!(store.verify_health().is_err());
    store.recover_same_home().unwrap();
    let alpha = store.domain_handle::<AlphaDomain>().unwrap();

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.require_sidecar(sidecar).unwrap();
    command
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(8, b"no".to_vec()),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::NotCommitted {
            evidence: CommandError::ForeignSidecar
        }
    ));
}

fn count_temporary_files(home: &std::path::Path) -> usize {
    all_files(&home.join("sidecars"))
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".tmp-"))
        })
        .count()
}

fn final_sidecar_files(home: &std::path::Path) -> Vec<std::path::PathBuf> {
    all_files(&home.join("sidecars"))
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| !name.to_string_lossy().starts_with(".tmp-"))
        })
        .collect()
}

fn all_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    if !root.exists() {
        return Vec::new();
    }
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}
