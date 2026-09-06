#![cfg(all(feature = "test-faults", target_os = "windows"))]

use std::{
    fs,
    num::NonZeroU64,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    thread,
    time::Duration,
};

use beryl_home_store::{
    AdmittedSidecar, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    SidecarByteLimit, SidecarError, SidecarNamespace,
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

fn sidecar_paths(home: &Path, bytes: &[u8]) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let digest = hex::encode(<[u8; 32]>::from(Sha256::digest(bytes)));
    let root = home.join("sidecars");
    let namespace = root.join("images");
    let shard = namespace.join(&digest[..2]);
    let final_path = shard.join(digest);
    (root, namespace, shard, final_path)
}

fn barrier_points() -> [FaultPoint; 4] {
    [
        FaultPoint::BeforeSidecarRootDirectorySync,
        FaultPoint::BeforeSidecarNamespaceDirectorySync,
        FaultPoint::BeforeSidecarShardDirectorySync,
        FaultPoint::BeforeSidecarFinalDirectorySync,
    ]
}

fn admit_through_all_barriers(
    store: &Arc<HomeStore>,
    faults: &FaultController,
    bytes: &'static [u8],
) -> AdmittedSidecar {
    let blocks = barrier_points().map(|point| faults.block_next(point));
    let worker_store = Arc::clone(store);
    let worker = thread::spawn(move || {
        worker_store.admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit())
    });
    for block in &blocks {
        assert!(block.wait_until_reached(Duration::from_secs(10)));
        block.release();
    }
    worker.join().unwrap().unwrap()
}

fn temporary_files(home: &Path) -> Vec<PathBuf> {
    all_files(&home.join("sidecars"))
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".tmp-"))
        })
        .collect()
}

fn all_files(root: &Path) -> Vec<PathBuf> {
    if !root.exists() {
        return Vec::new();
    }
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

fn create_junction(link: &Path, target: &Path) {
    let output = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "junction creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fresh_publication_and_existing_reuse_visit_all_four_directory_barriers() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = Arc::new(open(directory.path(), faults.clone()));

    let fresh = admit_through_all_barriers(&store, &faults, b"all fresh barriers");
    drop(fresh);
    let reused = admit_through_all_barriers(&store, &faults, b"all fresh barriers");
    assert_eq!(fs::read(reused.path()).unwrap(), b"all fresh barriers");
}

#[test]
fn retry_after_post_rename_failure_repairs_the_final_barrier_before_token() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = Arc::new(open(directory.path(), faults.clone()));
    let bytes = b"retry final publication";
    faults.fail_next(FaultPoint::AfterSidecarRename);

    assert!(matches!(
        store.admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit()),
        Err(SidecarError::Storage { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    let store = Arc::try_unwrap(store).expect("sidecar caller released store");
    let store = Arc::new(store.recover_same_home().unwrap().publish());

    let blocks = barrier_points().map(|point| faults.block_next(point));
    let worker_store = Arc::clone(&store);
    let worker = thread::spawn(move || {
        worker_store.admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit())
    });
    for (index, block) in blocks.iter().enumerate() {
        assert!(block.wait_until_reached(Duration::from_secs(10)));
        if index == blocks.len() - 1 {
            assert!(!worker.is_finished(), "token returned before final barrier");
        }
        block.release();
    }
    let token = worker.join().unwrap().unwrap();
    assert_eq!(fs::read(token.path()).unwrap(), bytes);
    assert!(temporary_files(directory.path()).is_empty());
}

#[test]
fn exact_concurrent_collision_returns_two_tokens_and_retains_the_losing_temporary() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = Arc::new(open(directory.path(), faults.clone()));
    let first_cut = faults.block_next(FaultPoint::BeforeSidecarRename);
    let second_cut = faults.block_next(FaultPoint::BeforeSidecarRename);
    let published_cut = faults.block_next(FaultPoint::AfterSidecarRename);
    let first_final = faults.block_next(FaultPoint::BeforeSidecarFinalDirectorySync);
    let second_final = faults.block_next(FaultPoint::BeforeSidecarFinalDirectorySync);

    let first_store = Arc::clone(&store);
    let first = thread::spawn(move || {
        first_store.admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"concurrent collision",
            limit(),
        )
    });
    let second_store = Arc::clone(&store);
    let second = thread::spawn(move || {
        second_store.admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            b"concurrent collision",
            limit(),
        )
    });

    assert!(first_cut.wait_until_reached(Duration::from_secs(10)));
    assert!(second_cut.wait_until_reached(Duration::from_secs(10)));
    first_cut.release();
    assert!(published_cut.wait_until_reached(Duration::from_secs(10)));
    published_cut.release();
    second_cut.release();
    assert!(first_final.wait_until_reached(Duration::from_secs(10)));
    assert!(second_final.wait_until_reached(Duration::from_secs(10)));
    first_final.release();
    second_final.release();

    let first = first.join().unwrap().unwrap();
    let second = second.join().unwrap().unwrap();
    assert_eq!(first.path(), second.path());
    assert_eq!(fs::read(first.path()).unwrap(), b"concurrent collision");
    assert_eq!(temporary_files(directory.path()).len(), 1);
    assert_eq!(
        all_files(directory.path())
            .into_iter()
            .filter(|path| path.file_name().is_some_and(|name| name.len() == 64))
            .count(),
        1
    );
}

#[test]
fn elevated_exact_content_final_symlink_and_final_directory_are_structurally_rejected() {
    let symlink_fixture = tempdir().unwrap();
    let symlink_store = open(symlink_fixture.path(), FaultController::new());
    let symlink_bytes = b"exact bytes behind symlink";
    let (_, _, shard, final_path) = sidecar_paths(symlink_fixture.path(), symlink_bytes);
    fs::create_dir_all(&shard).unwrap();
    let external = symlink_fixture.path().join("external-bytes");
    fs::write(&external, symlink_bytes).unwrap();
    match std::os::windows::fs::symlink_file(&external, &final_path) {
        Ok(()) => {
            assert!(matches!(
                symlink_store.admit_sidecar(
                    SidecarNamespace::new("images").unwrap(),
                    symlink_bytes,
                    limit(),
                ),
                Err(SidecarError::InvalidLayout)
            ));
            assert_eq!(symlink_store.health().state(), HomeHealthState::Failed);
            assert!(
                fs::symlink_metadata(&final_path)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(fs::read(&external).unwrap(), symlink_bytes);
        }
        Err(error) if error.raw_os_error() == Some(1314) => {}
        Err(error) => panic!("create final file symlink: {error}"),
    }
    symlink_store.close().unwrap();

    let directory_fixture = tempdir().unwrap();
    let directory_store = open(directory_fixture.path(), FaultController::new());
    let directory_bytes = b"directory collision";
    let (_, _, shard, final_path) = sidecar_paths(directory_fixture.path(), directory_bytes);
    fs::create_dir_all(&shard).unwrap();
    fs::create_dir(&final_path).unwrap();
    assert!(matches!(
        directory_store.admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            directory_bytes,
            limit(),
        ),
        Err(SidecarError::InvalidLayout)
    ));
    assert_eq!(directory_store.health().state(), HomeHealthState::Failed);
    assert!(final_path.is_dir());
}

#[test]
fn sidecar_root_namespace_and_shard_junctions_are_rejected_without_touching_targets() {
    for level in 0..3 {
        let fixture = tempdir().unwrap();
        let home = fixture.path().join("home");
        let external = fixture.path().join("external");
        fs::create_dir(&external).unwrap();
        fs::write(external.join("sentinel"), b"unchanged").unwrap();
        let store = open(&home, FaultController::new());
        let bytes = b"ancestor junction";
        let (root, namespace, shard, _) = sidecar_paths(&home, bytes);
        let link = match level {
            0 => root,
            1 => {
                fs::create_dir(&root).unwrap();
                namespace
            }
            2 => {
                fs::create_dir(&root).unwrap();
                fs::create_dir(&namespace).unwrap();
                shard
            }
            _ => unreachable!(),
        };
        create_junction(&link, &external);

        assert!(matches!(
            store.admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit(),),
            Err(SidecarError::InvalidLayout)
        ));
        assert_eq!(store.health().state(), HomeHealthState::Failed);
        assert_eq!(fs::read(external.join("sentinel")).unwrap(), b"unchanged");
        store.close().unwrap();
        fs::remove_dir(link).unwrap();
    }
}

#[test]
fn identical_byte_replacement_after_successful_rename_converges_by_content() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let store = Arc::new(open(directory.path(), faults.clone()));
    let bytes = b"same bytes different object";
    let (_, _, shard, final_path) = sidecar_paths(directory.path(), bytes);
    let replacement = shard.join("replacement-object");
    let renamed = faults.block_next(FaultPoint::AfterSidecarRename);
    let worker_store = Arc::clone(&store);
    let worker = thread::spawn(move || {
        worker_store.admit_sidecar(SidecarNamespace::new("images").unwrap(), bytes, limit())
    });
    assert!(renamed.wait_until_reached(Duration::from_secs(10)));

    fs::write(&replacement, bytes).unwrap();
    fs::remove_file(&final_path).unwrap();
    fs::rename(&replacement, &final_path).unwrap();
    renamed.release();

    let sidecar = worker.join().unwrap().unwrap();
    assert_eq!(sidecar.address().length(), bytes.len() as u64);
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert_eq!(fs::read(final_path).unwrap(), bytes);
    assert!(temporary_files(directory.path()).is_empty());
}
