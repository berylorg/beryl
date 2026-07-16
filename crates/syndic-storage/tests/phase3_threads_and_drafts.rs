use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandCancellation, CommandError, CursorReadLimits, HomeCommand, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
};
use beryl_model::{SyndicDraftId, SyndicThreadId};
use syndic_storage::{
    ComposerAtom, ComposerContentAssembler, ComposerPayload, ContentAppend, ContentBuild,
    CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision, PreparedContent,
    SyndicCurrentDraft, SyndicMutationError, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    ThreadCreationStatus,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase3-{name}-{}-{sequence}",
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

fn ids(byte: u8) -> (SyndicThreadId, SyndicDraftId) {
    (
        SyndicThreadId::from_bytes([byte; 16]),
        SyndicDraftId::from_bytes([byte.wrapping_add(1); 16]),
    )
}

fn timestamp(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn payload(text: &str) -> ComposerPayload {
    ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap()
}

fn execute(
    store: &HomeStore,
    storage: SyndicStorage,
    contribution: beryl_home_store::MutationContribution,
) -> Result<beryl_home_store::CommitReceipt, CommandError> {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    let receipt = store.execute(command)?;
    assert!(
        storage
            .committed_revision(store, &receipt)
            .unwrap()
            .is_some()
    );
    Ok(receipt)
}

fn stage_content(store: &HomeStore, storage: SyndicStorage, content: &PreparedContent) {
    execute(
        store,
        storage,
        storage.begin_content(
            storage.revision(store).unwrap(),
            ContentBuild::from_prepared(content),
        ),
    )
    .unwrap();
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        let next = append.next_manifest().clone();
        execute(
            store,
            storage,
            storage.append_content(storage.revision(store).unwrap(), append),
        )
        .unwrap();
        manifest = next;
    }
}

fn read_payload(
    store: &HomeStore,
    storage: SyndicStorage,
    current: &SyndicCurrentDraft,
) -> ComposerPayload {
    let mut assembler = ComposerContentAssembler::new(current.draft().content()).unwrap();
    let mut after = None;
    loop {
        let page = storage
            .content_chunks(
                store,
                current.draft().content().id(),
                after,
                CursorReadLimits::new(16, 2_000_000).unwrap(),
            )
            .unwrap();
        for chunk in page.records() {
            assembler.push(chunk).unwrap();
            after = Some(chunk.ordinal());
        }
        if !page.has_more() {
            break;
        }
    }
    assembler.finish().unwrap()
}

fn assert_mutation_error(
    error: &CommandError,
    predicate: impl FnOnce(&SyndicMutationError) -> bool,
) {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected contributor validation error, got {error}");
    };
    let source = source
        .downcast_ref::<SyndicMutationError>()
        .expect("typed Syndic mutation error");
    assert!(predicate(source), "unexpected mutation error: {source}");
}

#[test]
fn ordinary_creation_is_atomic_reopenable_and_naturally_reconcilable() {
    let home = TestHome::new("ordinary");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread_id, draft_id) = ids(1);
    let creation = CreateThread::ordinary(thread_id, draft_id, timestamp(10));

    assert_eq!(
        storage
            .thread_creation_status(&store, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Absent
    );
    execute(
        &store,
        storage,
        storage.create_thread(storage.revision(&store).unwrap(), creation.clone()),
    )
    .unwrap();
    assert_eq!(
        storage
            .thread_creation_status(&store, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Exact
    );
    let current = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().id(), draft_id);
    assert!(read_payload(&store, storage, &current).atoms().is_empty());
    assert!(
        storage
            .current_binding(&store, thread_id, limit())
            .unwrap()
            .is_some()
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(&home);
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .thread_creation_status(&reopened, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Exact
    );
    reopened.close().unwrap();
}

#[test]
fn cancellation_before_admission_and_identity_collision_change_nothing() {
    let home = TestHome::new("cancel-collision");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread_id, draft_id) = ids(10);
    let creation = CreateThread::ordinary(thread_id, draft_id, timestamp(1));
    let before = storage.revision(&store).unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let mut command =
        HomeCommand::new(store.home_revision().unwrap()).with_cancellation(cancellation);
    command
        .add(storage.create_thread(before, creation.clone()))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        Err(CommandError::CancelledBeforeAdmission)
    ));
    assert_eq!(storage.revision(&store).unwrap(), before);
    assert_eq!(
        storage
            .thread_creation_status(&store, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Absent
    );

    execute(
        &store,
        storage,
        storage.create_thread(storage.revision(&store).unwrap(), creation.clone()),
    )
    .unwrap();
    let error = execute(
        &store,
        storage,
        storage.create_thread(storage.revision(&store).unwrap(), creation),
    )
    .unwrap_err();
    assert_mutation_error(&error, |error| {
        matches!(error, SyndicMutationError::IdentityCollision)
    });
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn dirty_only_update_preserves_immutable_draft_facts_and_reconciles_exactly() {
    let home = TestHome::new("update");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread_id, draft_id) = ids(20);
    let creation = CreateThread::ordinary(thread_id, draft_id, timestamp(5));
    execute(
        &store,
        storage,
        storage.create_thread(storage.revision(&store).unwrap(), creation),
    )
    .unwrap();
    let before = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    let domain_before_noop = storage.revision(&store).unwrap();
    let empty = PreparedContent::composer(&ComposerPayload::default()).unwrap();
    assert!(matches!(
        DraftPayloadUpdate::prepare(&before, &empty, timestamp(6)).unwrap(),
        DraftPayloadUpdateDecision::NoChange
    ));
    assert_eq!(storage.revision(&store).unwrap(), domain_before_noop);

    let durable = PreparedContent::composer(&payload("durable")).unwrap();
    stage_content(&store, storage, &durable);
    let update = match DraftPayloadUpdate::prepare(&before, &durable, timestamp(7)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => panic!("changed payload must be dirty"),
    };
    execute(
        &store,
        storage,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update.clone()),
    )
    .unwrap();
    let after = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    assert!(update.matches_committed(&after));
    assert_eq!(after.draft().thread_id(), before.draft().thread_id());
    assert_eq!(after.draft().parent(), before.draft().parent());
    assert_eq!(
        after.draft().context_owner_id(),
        before.draft().context_owner_id()
    );
    assert_eq!(
        after.draft().replacement_edit_intent(),
        before.draft().replacement_edit_intent()
    );
    assert_eq!(after.draft().created_at(), before.draft().created_at());
    assert_eq!(
        storage
            .history_summary(&store, thread_id, limit())
            .unwrap()
            .unwrap()
            .record()
            .last_activity_at(),
        timestamp(7)
    );

    let stale_content = PreparedContent::composer(&payload("stale")).unwrap();
    stage_content(&store, storage, &stale_content);
    let stale = match DraftPayloadUpdate::prepare(&after, &stale_content, timestamp(8)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    let maximum = payload(&"x".repeat(262_144));
    let maximum = PreparedContent::composer(&maximum).unwrap();
    stage_content(&store, storage, &maximum);
    let maximum_update = match DraftPayloadUpdate::prepare(&after, &maximum, timestamp(9)).unwrap()
    {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute(
        &store,
        storage,
        storage.update_draft_payload(storage.revision(&store).unwrap(), maximum_update.clone()),
    )
    .unwrap();
    let maximum_current = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    assert!(maximum_update.matches_committed(&maximum_current));

    let error = execute(
        &store,
        storage,
        storage.update_draft_payload(storage.revision(&store).unwrap(), stale),
    )
    .unwrap_err();
    assert_mutation_error(&error, |error| {
        matches!(error, SyndicMutationError::DraftRevisionConflict { .. })
    });
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
