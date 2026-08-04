#[path = "support/fjall.rs"]
mod fjall_support;

use std::{fs, path::PathBuf};

use beryl_home_store::{
    HomeLockCapability, HomeOpenError, HomeOpenOptions, HomeOpenStage, HomeSchemaVersion,
    HomeStore, HomeUnreadableStage,
};
use fjall::{Database, PersistMode};

fn open(path: impl Into<PathBuf>) -> Result<HomeStore, HomeOpenError> {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT))
}

#[test]
fn fresh_home_persists_identity_and_canonical_facts() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("home");

    let first = open(&configured).expect("fresh home opens");
    let home_id = first.home_id();
    let object_id = first.canonical_identity();
    let canonical_path = first.canonical_path().to_path_buf();
    let database_path = first.database_path().to_path_buf();
    assert!(canonical_path.is_absolute());
    assert!(database_path.join("version").is_file());
    first.close().expect("orderly close");

    let reopened = open(&configured).expect("existing home reopens");
    assert_eq!(home_id, reopened.home_id());
    assert_eq!(object_id, reopened.canonical_identity());
    assert_eq!(canonical_path, reopened.canonical_path());
    assert_eq!(database_path, reopened.database_path());
    reopened.close().expect("orderly close");
}

#[test]
fn stale_lock_file_is_reused_without_deletion() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("home");
    fs::create_dir_all(&configured).expect("create home");
    let lock_path = configured.join("home.lock");
    fs::write(&lock_path, b"stale diagnostic bytes").expect("create stale lock file");

    let store = open(&configured).expect("stale file is not live ownership");
    assert!(lock_path.is_file());
    store.close().expect("orderly close");
    assert!(lock_path.is_file());
}

#[test]
fn unwritable_lock_file_fails_before_database_creation() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("home");
    fs::create_dir_all(&configured).expect("create home");
    let lock_path = configured.join("home.lock");
    fs::write(&lock_path, b"").expect("create lock file");
    let original_permissions = fs::metadata(&lock_path)
        .expect("lock metadata")
        .permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&lock_path, permissions).expect("make lock file read-only");

    let error = open(&configured).expect_err("unwritable ownership file must fail");
    assert!(matches!(
        error,
        HomeOpenError::Open {
            stage: HomeOpenStage::OpenLockFile,
            ..
        }
    ));
    assert!(!configured.join("state").exists());

    fs::set_permissions(&lock_path, original_permissions).expect("restore lock permissions");
}

#[test]
fn unsupported_schema_is_typed_and_non_destructive() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("home");
    let first = open(&configured).expect("fresh home opens");
    let home_id = first.home_id();
    first.close().expect("orderly close");

    let newer = HomeSchemaVersion::new(2).expect("nonzero schema");
    let error = HomeStore::open(HomeOpenOptions::new(&configured, newer))
        .expect_err("schema mismatch must fail");
    assert!(matches!(
        error,
        HomeOpenError::UnsupportedSchema {
            supported,
            found: HomeSchemaVersion::CURRENT,
            ..
        } if supported == newer
    ));

    let reopened = open(&configured).expect("mismatch did not replace the home");
    assert_eq!(home_id, reopened.home_id());
}

#[test]
fn relative_paths_are_rejected_before_filesystem_mutation() {
    let relative = PathBuf::from("relative-beryl-home");
    let error = open(&relative).expect_err("relative path must fail");
    assert!(matches!(
        error,
        HomeOpenError::Open {
            stage: HomeOpenStage::ValidateConfiguredPath,
            ..
        }
    ));
}

#[test]
fn generic_unc_paths_fail_closed_as_unsupported() {
    let unc = PathBuf::from(r"\\server.invalid\share\beryl-home");
    let error = open(&unc).expect_err("generic UNC must fail closed");
    assert!(matches!(
        error,
        HomeOpenError::LockUnsupported {
            capability: HomeLockCapability::LocalStorage,
            ..
        }
    ));
}

#[test]
fn nonempty_state_without_version_is_never_initialized_over() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("home");
    let database = configured.join("state");
    fs::create_dir_all(&database).expect("create state directory");
    let sentinel = database.join("existing-state.bin");
    fs::write(&sentinel, b"must survive").expect("write sentinel");

    let error = open(&configured).expect_err("unrecognized state must fail");
    assert!(matches!(
        error,
        HomeOpenError::Unreadable {
            stage: HomeUnreadableStage::MissingDatabaseVersion,
            ..
        }
    ));
    assert_eq!(
        fs::read(&sentinel).expect("sentinel remains"),
        b"must survive"
    );
    assert!(!database.join("version").exists());
}

#[test]
fn existing_fjall_database_without_beryl_header_is_unreadable() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("home");
    let database_path = configured.join("state");
    let database = Database::create(fjall_support::config(&database_path))
        .expect("create plain Fjall database");
    database
        .persist(PersistMode::SyncAll)
        .expect("persist plain database");
    drop(database);

    let error = open(&configured).expect_err("foreign database must not be adopted");
    assert!(matches!(
        error,
        HomeOpenError::Unreadable {
            stage: HomeUnreadableStage::MissingHeaderKeyspace,
            ..
        }
    ));
}

#[test]
fn fjall_database_directory_cannot_be_used_as_a_beryl_home() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("foreign-database");
    let database =
        Database::create(fjall_support::config(&configured)).expect("create Fjall database");
    database
        .persist(PersistMode::SyncAll)
        .expect("persist Fjall database");
    drop(database);

    let error = open(&configured).expect_err("nested database layout must fail");
    assert!(matches!(
        error,
        HomeOpenError::Open {
            stage: HomeOpenStage::AdmitPhysicalLayout,
            ..
        }
    ));
    assert!(!configured.join("state").exists());
    assert!(!configured.join("home.lock").exists());
}

#[test]
fn missing_version_from_a_prior_home_is_not_recreated() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("home");
    let store = open(&configured).expect("fresh home opens");
    let database_path = store.database_path().to_path_buf();
    store.close().expect("orderly close");

    fs::remove_file(database_path.join("version")).expect("remove version marker");
    let error = open(&configured).expect_err("damaged database must fail");
    assert!(matches!(
        error,
        HomeOpenError::Unreadable {
            stage: HomeUnreadableStage::MissingDatabaseVersion,
            ..
        }
    ));
    assert!(!database_path.join("version").exists());
}

#[test]
fn invalid_fjall_version_is_preserved_as_unreadable_state() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("home");
    let store = open(&configured).expect("fresh home opens");
    let marker = store.database_path().join("version");
    store.close().expect("orderly close");
    fs::write(&marker, b"not-a-fjall-version").expect("corrupt version marker");

    let error = open(&configured).expect_err("invalid engine version must fail");
    assert!(matches!(
        error,
        HomeOpenError::Unreadable {
            stage: HomeUnreadableStage::RecoverDatabase,
            ..
        }
    ));
    assert_eq!(
        fs::read(&marker).expect("invalid marker remains"),
        b"not-a-fjall-version"
    );
}

#[test]
fn malformed_home_header_is_not_replaced() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("home");
    let store = open(&configured).expect("fresh home opens");
    let database_path = store.database_path().to_path_buf();
    store.close().expect("orderly close");

    let database = Database::recover(fjall_support::config(&database_path))
        .expect("open database directly for corruption fixture");
    let header = database
        .open_keyspace("_beryl_home")
        .expect("open header keyspace");
    header
        .insert("header", b"malformed")
        .expect("write malformed header");
    database
        .persist(PersistMode::SyncAll)
        .expect("persist malformed header");
    drop(header);
    drop(database);

    let error = open(&configured).expect_err("malformed header must fail");
    assert!(matches!(
        error,
        HomeOpenError::Unreadable {
            stage: HomeUnreadableStage::DecodeHeader,
            ..
        }
    ));

    let database =
        Database::recover(fjall_support::config(&database_path)).expect("reopen fixture database");
    let header = database
        .open_keyspace("_beryl_home")
        .expect("reopen header keyspace");
    let snapshot = database.snapshot().expect("snapshot fixture database");
    let point = snapshot
        .point(&header, b"header")
        .expect("select malformed header")
        .expect("malformed header remains");
    let pair = point.acquire().expect("read malformed header");
    assert_eq!(pair.value(), b"malformed");
}

#[test]
fn reserved_state_file_is_a_typed_layout_collision() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("home");
    fs::create_dir_all(&configured).expect("create home");
    fs::write(configured.join("state"), b"collision").expect("create collision");

    let error = open(&configured).expect_err("reserved state file must fail");
    assert!(matches!(
        error,
        HomeOpenError::Open {
            stage: HomeOpenStage::AdmitPhysicalLayout,
            ..
        }
    ));
}

#[test]
fn precreated_empty_state_directory_is_a_valid_fresh_home() {
    let directory = tempfile::tempdir().expect("temp directory");
    let configured = directory.path().join("home");
    fs::create_dir_all(configured.join("state")).expect("create empty state directory");

    let store = open(&configured).expect("empty reserved directory is fresh");
    assert!(store.database_path().join("version").is_file());
}
