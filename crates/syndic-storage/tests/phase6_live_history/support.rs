use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{SyndicDraftId, SyndicThreadId};
use syndic_storage::{
    ContentAppend, ContentBuild, PreparedContent, SyndicStorage, SyndicTimestamp,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

pub struct TestHome {
    path: PathBuf,
}

impl TestHome {
    pub fn new(name: &str) -> Self {
        let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase6-{name}-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
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

pub fn stage_prepared_content(
    store: &HomeStore,
    storage: SyndicStorage,
    content: &PreparedContent,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.begin_content(
            storage.revision(store).unwrap(),
            ContentBuild::from_prepared(content),
        ))
        .unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean content-build fixture command, got {outcome:?}"),
    }

    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        let next = append.next_manifest().clone();
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.append_content(storage.revision(store).unwrap(), append))
            .unwrap();
        match store.execute(command) {
            beryl_home_store::CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean content-append fixture command, got {outcome:?}"),
        }
        manifest = next;
    }
}
