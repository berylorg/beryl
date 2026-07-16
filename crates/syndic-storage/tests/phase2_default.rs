use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::SyndicThreadId;
use syndic_storage::{SyndicPointReadLimit, SyndicStorage};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new() -> Self {
        let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-default-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn open(path: &Path) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap()
}

#[test]
fn normal_features_register_reopen_and_read_an_empty_domain() {
    let home = TestHome::new();
    let mut store = open(home.path());
    SyndicStorage::register(&mut store).unwrap();
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let missing = SyndicThreadId::from_bytes([1; 16]);
    let limit = SyndicPointReadLimit::new(1_024).unwrap();
    assert!(storage.thread(&reopened, missing, limit).unwrap().is_none());
    assert!(
        storage
            .current_binding(&reopened, missing, limit)
            .unwrap()
            .is_none()
    );
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}
