#![cfg(feature = "test-faults")]

mod support;

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

use beryl_home_store::{
    HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore, PointReadLimit,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::tempdir;

use support::{AlphaDomain, BytesRecord, PutBytes};

const HOME_ENV: &str = "BERYL_PHASE9_FORCE_HOME";
const POINT_ENV: &str = "BERYL_PHASE9_FORCE_POINT";
const READY_ENV: &str = "BERYL_PHASE9_FORCE_READY";

fn open(path: &Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn point_from_name(name: &str) -> FaultPoint {
    match name {
        "before-commit" => FaultPoint::BeforeCommit,
        "post-commit-pre-sync-all" => FaultPoint::AfterCommitBeforePersist,
        "post-sync-all" => FaultPoint::AfterPersist,
        value => panic!("unknown parent-forced cut point {value}"),
    }
}

#[test]
fn parent_forced_cut_helper() {
    let Some(home) = env::var_os(HOME_ENV).map(PathBuf::from) else {
        return;
    };
    let ready = PathBuf::from(env::var_os(READY_ENV).expect("ready marker path is required"));
    let point = point_from_name(&env::var(POINT_ENV).expect("cut-point name is required"));
    let faults = FaultController::new();
    let block = faults.block_next(point);

    let _marker = thread::spawn(move || {
        assert!(block.wait_until_reached(Duration::from_secs(30)));
        fs::write(ready, b"reached").unwrap();
        loop {
            thread::park();
        }
    });

    let mut store = open(&home, faults);
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(77, b"parent-forced cut".to_vec()),
        ))
        .unwrap();
    let _ = store.execute(command);
    panic!("parent-forced boundary unexpectedly released");
}

#[derive(Clone, Copy)]
enum ExpectedState {
    Old,
    New,
    Either,
}

#[test]
fn parent_forced_termination_preserves_atomic_state_at_controlled_boundaries() {
    assert_parent_forced_cut("before-commit", ExpectedState::Old);
    assert_parent_forced_cut("post-commit-pre-sync-all", ExpectedState::Either);
    assert_parent_forced_cut("post-sync-all", ExpectedState::New);
}

fn assert_parent_forced_cut(point: &str, expected: ExpectedState) {
    let fixture = tempdir().unwrap();
    let home = fixture.path().join("home");
    let ready = fixture.path().join("cut-reached");

    let mut initial = open(&home, FaultController::new());
    initial.register_domain::<AlphaDomain>().unwrap();
    initial.close().unwrap();

    let mut child = Command::new(env::current_exe().unwrap())
        .args(["--exact", "parent_forced_cut_helper", "--nocapture"])
        .env(HOME_ENV, &home)
        .env(POINT_ENV, point)
        .env(READY_ENV, &ready)
        .spawn()
        .unwrap();
    wait_for_boundary(&mut child, &ready);
    child.kill().expect("parent must terminate blocked helper");
    let status = child.wait().unwrap();
    assert!(
        !status.success(),
        "terminated helper unexpectedly succeeded"
    );

    let mut reopened = open(&home, FaultController::new());
    let alpha = reopened.register_domain::<AlphaDomain>().unwrap();
    let home_revision = reopened.home_revision().unwrap().get();
    let domain_revision = reopened.domain_revision(alpha).unwrap().get();
    let value = reopened
        .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            alpha,
            &77,
            PointReadLimit::new(1_028).unwrap(),
        )
        .unwrap();
    let old = home_revision == 1 && domain_revision == 1 && value.is_none();
    let new = home_revision == 2
        && domain_revision == 2
        && value.as_deref() == Some(b"parent-forced cut".as_slice());
    match expected {
        ExpectedState::Old => assert!(old, "before-commit cut must preserve old state"),
        ExpectedState::New => assert!(new, "post-SyncAll cut must preserve new state"),
        ExpectedState::Either => assert!(
            old || new,
            "recovery exposed a partial batch: home_revision={home_revision}, domain_revision={domain_revision}, value={value:?}"
        ),
    }
}

fn wait_for_boundary(child: &mut Child, ready: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ready.exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("cut helper exited before reaching its boundary: {status}");
        }
        if Instant::now() >= deadline {
            child.kill().expect("timed-out helper must be terminated");
            let _ = child.wait();
            panic!("cut helper did not reach its boundary before timeout");
        }
        thread::sleep(Duration::from_millis(10));
    }
}
