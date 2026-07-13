use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use beryl_home_store::{HomeOpenError, HomeOpenOptions, HomeSchemaVersion, HomeStore};

const MAPPED_REMOTE_HOME: &str = "BERYL_HOME_MAPPED_REMOTE_FIXTURE";

fn open(path: impl Into<PathBuf>) -> Result<HomeStore, HomeOpenError> {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT))
}

fn assert_busy(path: &Path) {
    assert!(matches!(
        open(path).expect_err("alias must reach the live lock"),
        HomeOpenError::Busy { .. }
    ));
}

#[test]
fn mapped_remote_fixture() {
    let Some(path) = env::var_os(MAPPED_REMOTE_HOME) else {
        return;
    };
    let error = open(PathBuf::from(path)).expect_err("mapped remote home must fail closed");
    assert!(matches!(
        error,
        HomeOpenError::LockUnsupported {
            capability: beryl_home_store::HomeLockCapability::LocalStorage,
            ..
        }
    ));
}

#[test]
fn case_and_extended_spelling_reach_one_home() {
    let directory = tempfile::tempdir().expect("temp directory");
    let home = directory.path().join("case-home");
    fs::create_dir_all(&home).expect("create home");
    let case_alias = directory.path().join("CASE-HOME");
    let extended_alias = PathBuf::from(format!(r"\\?\{}", home.display()));

    let original = open(&home).expect("open original spelling");
    let home_id = original.home_id();
    let object_id = original.canonical_identity();
    assert_busy(&case_alias);
    assert_busy(&extended_alias);
    original.close().expect("close original spelling");

    let via_case = open(&case_alias).expect("open case alias");
    assert_eq!(home_id, via_case.home_id());
    assert_eq!(object_id, via_case.canonical_identity());
    via_case.close().expect("close case alias");

    let via_extended = open(&extended_alias).expect("open extended alias");
    assert_eq!(home_id, via_extended.home_id());
    assert_eq!(object_id, via_extended.canonical_identity());
}

#[test]
fn directory_symlink_reaches_one_opened_object_and_lock() {
    let directory = tempfile::tempdir().expect("temp directory");
    let home = directory.path().join("real-home");
    let alias = directory.path().join("symlink-home");
    fs::create_dir_all(&home).expect("create real home");
    std::os::windows::fs::symlink_dir(&home, &alias).expect("create directory symlink");

    assert_alias_identity_and_lock(&home, &alias);
    fs::remove_dir(&alias).expect("remove directory symlink");
}

#[test]
fn directory_junction_reaches_one_opened_object_and_lock() {
    let directory = tempfile::tempdir().expect("temp directory");
    let home = directory.path().join("real-home");
    let alias = directory.path().join("junction-home");
    fs::create_dir_all(&home).expect("create real home");

    create_junction(&alias, &home);

    assert_alias_identity_and_lock(&home, &alias);
    fs::remove_dir(&alias).expect("remove directory junction");
}

#[test]
fn junction_cannot_replace_the_reserved_database_directory() {
    let directory = tempfile::tempdir().expect("temp directory");
    let home = directory.path().join("home");
    let external = directory.path().join("external-state");
    let database_alias = home.join("state");
    fs::create_dir_all(&home).expect("create home");
    fs::create_dir_all(&external).expect("create external state");
    fs::write(external.join("sentinel"), b"unchanged").expect("write sentinel");
    create_junction(&database_alias, &external);

    let error = open(&home).expect_err("database reparse point must fail");
    assert!(matches!(
        error,
        HomeOpenError::Open {
            stage: beryl_home_store::HomeOpenStage::AdmitPhysicalLayout,
            ..
        }
    ));
    assert_eq!(
        fs::read(external.join("sentinel")).expect("sentinel remains"),
        b"unchanged"
    );
    fs::remove_dir(&database_alias).expect("remove database junction");
}

fn assert_alias_identity_and_lock(home: &Path, alias: &Path) {
    let original = open(home).expect("open original home");
    let home_id = original.home_id();
    let object_id = original.canonical_identity();
    let canonical_path = original.canonical_path().to_path_buf();
    assert_busy(alias);
    original.close().expect("close original home");

    let via_alias = open(alias).expect("open alias after release");
    assert_eq!(home_id, via_alias.home_id());
    assert_eq!(object_id, via_alias.canonical_identity());
    assert_eq!(canonical_path, via_alias.canonical_path());
    via_alias.close().expect("close alias");
}

fn create_junction(alias: &Path, target: &Path) {
    let output = Command::new("cmd.exe")
        .arg("/D")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(alias)
        .arg(target)
        .output()
        .expect("run built-in junction command");
    assert!(
        output.status.success(),
        "junction creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
