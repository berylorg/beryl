#![cfg(all(feature = "test-faults", target_os = "windows"))]

mod support;

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    thread,
    time::Duration,
};

use beryl_home_store::{
    test_faults::{FaultController, FaultPoint},
    HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore, PointReadLimit,
};
use tempfile::tempdir;

use support::{committed, AlphaDomain, BytesRecord, PutBytes};

const RENAME_CHILD_HOME: &str = "BERYL_PHASE13_RENAME_HOME";
const RENAME_CHILD_TARGET: &str = "BERYL_PHASE13_RENAME_TARGET";

fn open(path: &Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn put(store: &mut HomeStore, domain: beryl_home_store::DomainHandle<AlphaDomain>, value: &[u8]) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<AlphaDomain>::new(1, value.to_vec()),
        ))
        .unwrap();
    committed(store.execute(command));
}

fn read(store: &HomeStore, domain: beryl_home_store::DomainHandle<AlphaDomain>) -> Vec<u8> {
    store
        .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &1,
            PointReadLimit::new(1_028).unwrap(),
        )
        .unwrap()
        .unwrap()
}

fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&source_path, &target_path);
        } else {
            fs::copy(source_path, target_path).unwrap();
        }
    }
}

#[test]
fn state_rename_child() {
    let Ok(home) = env::var(RENAME_CHILD_HOME) else {
        return;
    };
    let target = PathBuf::from(env::var_os(RENAME_CHILD_TARGET).unwrap());
    let state = PathBuf::from(home).join("state");
    assert!(
        fs::rename(&state, &target).is_err(),
        "the retained state object must deny rename in another process"
    );
    assert!(state.is_dir());
    assert!(!target.exists());
}

#[test]
fn recovery_retains_the_exact_state_directory_until_orderly_close() {
    let directory = tempdir().unwrap();
    let home = directory.path();
    let state = home.join("state");
    let old_copy = home.join("old-state-copy");
    let blocked_target = home.join("blocked-state");
    let newer_state = home.join("newer-state");

    let mut initial = open(home, FaultController::new());
    let alpha = initial.register_domain::<AlphaDomain>().unwrap();
    let home_id = initial.home_id();
    put(&mut initial, alpha, b"old durable value");
    initial.close().unwrap();
    copy_tree(&state, &old_copy);

    let faults = FaultController::new();
    let mut current = open(home, faults.clone());
    let alpha = current.register_domain::<AlphaDomain>().unwrap();
    assert_eq!(current.home_id(), home_id);
    put(&mut current, alpha, b"new durable value");

    faults.fail_next(FaultPoint::BeforeCommit);
    let mut failed = HomeCommand::new(current.home_revision().unwrap());
    failed
        .add(alpha.contribution(
            current.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(2, b"surface failure".to_vec()),
        ))
        .unwrap();
    assert!(matches!(
        current.execute(failed),
        beryl_home_store::CommandOutcome::NotCommitted { .. }
    ));
    faults.fail_next(FaultPoint::BeforeVerification);
    assert!(current.verify_health().is_err());

    let block = faults.block_next(FaultPoint::BeforeReopen);
    let current = Arc::new(current);
    let recovering = Arc::clone(&current);
    let worker = thread::spawn(move || recovering.recover_same_home());
    assert!(block.wait_until_reached(Duration::from_secs(10)));

    let status = Command::new(env::current_exe().unwrap())
        .args(["--exact", "state_rename_child", "--nocapture"])
        .env(RENAME_CHILD_HOME, home)
        .env(RENAME_CHILD_TARGET, &blocked_target)
        .status()
        .unwrap();
    assert!(status.success(), "state rename child failed: {status}");

    block.release();
    worker.join().unwrap().unwrap();
    let alpha = current.domain_handle::<AlphaDomain>().unwrap();
    assert_eq!(read(&current, alpha), b"new durable value");

    let current = Arc::try_unwrap(current).expect("recovery worker released store");
    current.close().unwrap();
    fs::rename(&state, &newer_state).expect("state rename succeeds after orderly close");
    fs::rename(&old_copy, &state).unwrap();

    let mut restored_old = open(home, FaultController::new());
    assert_eq!(restored_old.home_id(), home_id);
    let alpha = restored_old.register_domain::<AlphaDomain>().unwrap();
    assert_eq!(read(&restored_old, alpha), b"old durable value");
    restored_old.close().unwrap();
    assert!(newer_state.is_dir());
}
