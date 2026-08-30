#![cfg(feature = "test-faults")]

use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    WholeHomeScrubTrigger,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
use syndic_storage::{
    CreateThread, DraftEditHistoryPolicyV1, DraftImageLabelProtectionHeadV1, ImageLabelFrontier,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, ThreadCreationStatus,
    test_faults::{
        FixtureBatch, FixtureDelete, FixtureRecord, PhysicalCorruption, PhysicalFamily,
        inject_physical_corruption,
    },
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase211-{name}-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn open(home: &TestHome) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap()
}

fn execution() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([211; 16]),
        RootId::from_bytes([212; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-phase211",
        )
        .unwrap(),
    )
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn creation() -> CreateThread {
    CreateThread::ordinary(
        SyndicThreadId::from_bytes([213; 16]),
        SyndicDraftId::from_bytes([214; 16]),
        execution(),
        SyndicTimestamp::from_unix_millis(1),
        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
    )
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected a clean phase211 command, got {outcome:?}"),
    }
}

fn fixture_delete(store: &HomeStore, storage: SyndicStorage, key: FixtureDelete) {
    let mut batch = FixtureBatch::new();
    batch.delete(key).unwrap();
    execute(
        store,
        storage.fixture_contribution(storage.revision(store).unwrap(), batch),
    );
}

#[test]
fn creation_installs_and_reopens_the_independent_protection_head() {
    let home = TestHome::new("creation-restart");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let creation = creation();
    let thread = creation.thread_id();

    execute(
        &store,
        storage.create_thread(storage.revision(&store).unwrap(), creation.clone()),
    );
    let head = storage
        .draft_image_label_protection_head(&store, thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.thread_id(), thread);
    assert_eq!(head.revision(), 1);
    assert_eq!(head.protected_maximum(), ImageLabelFrontier::EMPTY);
    assert_eq!(
        storage
            .thread_creation_status(&store, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Exact
    );
    store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(&home);
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .draft_image_label_protection_head(&reopened, thread, limit())
            .unwrap()
            .unwrap(),
        head
    );
    reopened.close().unwrap();
}

#[test]
fn missing_mismatched_or_malformed_protection_state_is_not_exact_creation() {
    let home = TestHome::new("corruption");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let creation = creation();
    let thread = creation.thread_id();
    execute(
        &store,
        storage.create_thread(storage.revision(&store).unwrap(), creation.clone()),
    );
    fixture_delete(
        &store,
        storage.clone(),
        FixtureDelete::DraftImageLabelProtectionHead(thread),
    );
    assert_eq!(
        storage
            .thread_creation_status(&store, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Collision
    );
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::DraftImageLabelProtectionHead(
            DraftImageLabelProtectionHeadV1::new(thread, 2, ImageLabelFrontier::EMPTY).unwrap(),
        ))
        .unwrap();
    execute(
        &store,
        storage.fixture_contribution(storage.revision(&store).unwrap(), batch),
    );
    assert_eq!(
        storage
            .thread_creation_status(&store, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Collision
    );

    let home = TestHome::new("physical-corruption");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let physical_thread = SyndicThreadId::from_bytes([1; 16]);
    inject_physical_corruption(
        &store,
        storage.clone(),
        PhysicalFamily::DraftImageLabelProtectionHeads,
        PhysicalCorruption::MalformedCodecPayload,
    )
    .unwrap();
    assert!(
        storage
            .draft_image_label_protection_head(&store, physical_thread, limit())
            .is_err()
    );
}
