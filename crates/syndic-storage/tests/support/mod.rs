#![allow(dead_code)]

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, DraftRootHistoryPairV1, SyndicPointReadLimit,
    SyndicStorage, SyndicTimestamp,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

pub struct TestHome {
    path: PathBuf,
}

impl TestHome {
    pub fn new(name: &str) -> Self {
        loop {
            let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "beryl-syndic-{name}-{}-{sequence}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("failed to create isolated test home {path:?}: {error}"),
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub fn open(path: &Path) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap()
}

pub fn id(byte: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([byte; 16])
}

pub fn draft_id(byte: u8) -> SyndicDraftId {
    SyndicDraftId::from_bytes([byte; 16])
}

pub fn timestamp(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

pub fn canonical_empty_root_history_pair(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    created_at: SyndicTimestamp,
) -> DraftRootHistoryPairV1 {
    let policy = DraftEditHistoryPolicyV1::new(65_536, 1).unwrap();
    let execution = ExecutionBinding::new(
        RuntimeId::from_bytes([246; 16]),
        RootId::from_bytes([247; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-test-root-history",
        )
        .unwrap(),
    );
    let request = CreateThread::ordinary(thread_id, draft_id, execution, created_at, policy);
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.create_thread(storage.revision(store).unwrap(), request))
        .unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean canonical root-history creation, got {outcome:?}"),
    }
    let current = storage
        .current_draft(store, thread_id, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap();
    current.draft().root_history()
}

#[cfg(feature = "test-faults")]
pub fn inject_fault_records(
    store: &HomeStore,
    storage: SyndicStorage,
    batch: syndic_storage::test_faults::FixtureBatch,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(storage.revision(store).unwrap(), batch))
        .unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean target-state fault injection, got {outcome:?}"),
    }
}
