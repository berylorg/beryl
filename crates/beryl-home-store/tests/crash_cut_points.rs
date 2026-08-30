#![cfg(feature = "test-faults")]

mod support;

use std::{env, path::PathBuf, process::Command};

use beryl_home_store::{
    test_faults::{FaultController, FaultPoint},
    HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore, PointReadLimit,
};
use tempfile::tempdir;

use support::{AlphaDomain, BytesRecord, PutBytes};

const HOME_ENV: &str = "BERYL_PHASE5_CRASH_HOME";
const POINT_ENV: &str = "BERYL_PHASE5_CRASH_POINT";

fn open(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

#[test]
fn crash_cut_helper() {
    let Some(home) = env::var_os(HOME_ENV).map(PathBuf::from) else {
        return;
    };
    let point = match env::var(POINT_ENV).unwrap().as_str() {
        "before-commit" => FaultPoint::BeforeCommit,
        "after-commit" => FaultPoint::AfterCommitBeforePersist,
        "after-persist" => FaultPoint::AfterPersist,
        value => panic!("unknown crash point {value}"),
    };
    let faults = FaultController::new();
    faults.abort_next(point);
    let mut store = open(&home, faults);
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(alpha.contribution(
            store.domain_revision(&alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(77, b"crash cut".to_vec()),
        ))
        .unwrap();
    let _ = store.execute(command);
    panic!("fault point did not abort the subprocess");
}

#[test]
fn subprocess_crash_cuts_preserve_atomic_old_or_new_state() {
    assert_crash_cut("before-commit", ExpectedState::Old);
    assert_crash_cut("after-commit", ExpectedState::Either);
    assert_crash_cut("after-persist", ExpectedState::New);
}

#[derive(Clone, Copy)]
enum ExpectedState {
    Old,
    New,
    Either,
}

fn assert_crash_cut(point: &str, expected: ExpectedState) {
    let directory = tempdir().unwrap();
    let mut initial = open(directory.path(), FaultController::new());
    initial.register_domain::<AlphaDomain>().unwrap();
    initial.close().unwrap();

    let status = Command::new(env::current_exe().unwrap())
        .args(["--exact", "crash_cut_helper", "--nocapture"])
        .env(HOME_ENV, directory.path())
        .env(POINT_ENV, point)
        .status()
        .unwrap();
    assert!(!status.success(), "crash helper unexpectedly succeeded");

    let mut reopened = open(directory.path(), FaultController::new());
    let alpha = reopened.register_domain::<AlphaDomain>().unwrap();
    let home_revision = reopened.home_revision().unwrap().get();
    let domain_revision = reopened.domain_revision(&alpha).unwrap().get();
    let value = reopened
        .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            &alpha,
            &77,
            PointReadLimit::new(1_028).unwrap(),
        )
        .unwrap();
    let old = home_revision == 1 && domain_revision == 1 && value.is_none();
    let new = home_revision == 2 && domain_revision == 2 && value.as_deref() == Some(b"crash cut");
    match expected {
        ExpectedState::Old => assert!(old, "before-commit cut must preserve old state"),
        ExpectedState::New => assert!(new, "post-persist cut must preserve new state"),
        ExpectedState::Either => assert!(old || new, "recovery exposed a partial batch"),
    }
}
