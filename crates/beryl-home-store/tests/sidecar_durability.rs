#![cfg(feature = "test-faults")]

mod support;

use std::{env, fs, num::NonZeroU64, path::PathBuf, process::Command};

use beryl_home_store::{
    CommandError, HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    SidecarByteLimit, SidecarError, SidecarNamespace,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::tempdir;

use support::{AlphaDomain, PutBytes, committed};

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
            store.domain_revision(&alpha).unwrap(),
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
    assert_eq!(store.health().state(), HomeHealthState::Failed);
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
    let store = store.recover_same_home().unwrap().publish();
    assert_eq!(count_temporary_files(directory.path()), temporary_count);
    store.close().unwrap();
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
    let store = store.recover_same_home().unwrap().publish();
    assert!(final_files[0].exists());
    store.close().unwrap();
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
        assert_eq!(store.health().state(), HomeHealthState::Failed);
        let store = store.recover_same_home().unwrap().publish();
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
        store.close().unwrap();
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
            store.domain_revision(&alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(7, b"fail".to_vec()),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(failed),
        beryl_home_store::CommandOutcome::NotCommitted { .. }
    ));
    assert!(store.home_revision().is_err());
    let candidate = store.recover_same_home().unwrap();
    let alpha = candidate.domain_handle::<AlphaDomain>().unwrap();
    let store = candidate.publish();

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.require_sidecar(sidecar).unwrap();
    command
        .add(alpha.contribution(
            store.domain_revision(&alpha).unwrap(),
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

const SIDECAR_CRASH_HOME_ENV: &str = "BERYL_SIDECAR_CRASH_HOME";
const SIDECAR_CRASH_POINT_ENV: &str = "BERYL_SIDECAR_CRASH_POINT";
const SIDECAR_CRASH_BYTES: &[u8] = b"sidecar crash-cut bytes";

#[test]
fn sidecar_crash_cut_helper() {
    let Some(home) = env::var_os(SIDECAR_CRASH_HOME_ENV).map(PathBuf::from) else {
        return;
    };
    let point = match env::var(SIDECAR_CRASH_POINT_ENV).unwrap().as_str() {
        "before-file-sync" => FaultPoint::BeforeSidecarFileSync,
        "before-rename" => FaultPoint::BeforeSidecarRename,
        "after-rename" => FaultPoint::AfterSidecarRename,
        value => panic!("unknown sidecar crash point {value}"),
    };
    let faults = FaultController::new();
    faults.abort_next(point);
    let store = open(&home, faults);
    let _ = store.admit_sidecar(
        SidecarNamespace::new("images").unwrap(),
        SIDECAR_CRASH_BYTES,
        limit(),
    );
    panic!("sidecar crash point did not abort the subprocess");
}

#[test]
fn subprocess_sidecar_crash_cuts_leave_inert_residue_and_later_converge() {
    assert_sidecar_crash_cut("before-file-sync", SidecarCrashResidue::Temporary);
    assert_sidecar_crash_cut("before-rename", SidecarCrashResidue::Temporary);
    assert_sidecar_crash_cut("after-rename", SidecarCrashResidue::Final);
}

#[derive(Clone, Copy)]
enum SidecarCrashResidue {
    Temporary,
    Final,
}

fn assert_sidecar_crash_cut(point: &str, expected_residue: SidecarCrashResidue) {
    let directory = tempdir().unwrap();
    let mut initial = open(directory.path(), FaultController::new());
    initial.register_domain::<AlphaDomain>().unwrap();
    initial.close().unwrap();

    let status = Command::new(env::current_exe().unwrap())
        .args(["--exact", "sidecar_crash_cut_helper", "--nocapture"])
        .env(SIDECAR_CRASH_HOME_ENV, directory.path())
        .env(SIDECAR_CRASH_POINT_ENV, point)
        .status()
        .unwrap();
    assert!(!status.success(), "sidecar helper unexpectedly succeeded");

    let mut reopened = open(directory.path(), FaultController::new());
    let alpha = reopened.register_domain::<AlphaDomain>().unwrap();
    assert_eq!(reopened.home_revision().unwrap().get(), 1);
    assert_eq!(reopened.domain_revision(&alpha).unwrap().get(), 1);

    let temporary_before = count_temporary_files(directory.path());
    let final_before = final_sidecar_files(directory.path());
    match expected_residue {
        SidecarCrashResidue::Temporary => {
            assert_eq!(temporary_before, 1);
            assert!(final_before.is_empty());
        }
        SidecarCrashResidue::Final => {
            assert_eq!(temporary_before, 0);
            assert_eq!(final_before.len(), 1);
            assert_eq!(fs::read(&final_before[0]).unwrap(), SIDECAR_CRASH_BYTES);
        }
    }

    let sidecar = reopened
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            SIDECAR_CRASH_BYTES,
            limit(),
        )
        .unwrap();
    assert_eq!(fs::read(sidecar.path()).unwrap(), SIDECAR_CRASH_BYTES);
    drop(sidecar);
    assert_eq!(count_temporary_files(directory.path()), temporary_before);
    assert_eq!(final_sidecar_files(directory.path()).len(), 1);
    reopened.close().unwrap();
}
