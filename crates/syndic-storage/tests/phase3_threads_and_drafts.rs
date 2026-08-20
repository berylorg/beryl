use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, CursorReadLimits, HomeCommand,
    HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
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

fn execution() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([91; 16]),
        RootId::from_bytes([92; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-phase3",
        )
        .unwrap(),
    )
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
    _storage: SyndicStorage,
    contribution: beryl_home_store::MutationContribution,
) -> beryl_home_store::CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn assert_committed(
    store: &HomeStore,
    storage: SyndicStorage,
    outcome: beryl_home_store::CommandOutcome,
) -> beryl_home_store::CommitReceipt {
    match outcome {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
        } => {
            assert!(
                storage
                    .committed_revision(store, &receipt)
                    .unwrap()
                    .is_some()
            );
            receipt
        }
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(failure),
        } => panic!(
            "expected clean thread-and-draft command outcome, got committed receipt {receipt:?} with later failure {failure:?}"
        ),
        CommandOutcome::NotCommitted { evidence } => panic!(
            "expected clean thread-and-draft command outcome, got definitive non-commit {evidence:?}"
        ),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!(
                "expected clean thread-and-draft command outcome, got indeterminate outcome {failure:?}"
            )
        }
    }
}

fn assert_not_committed(outcome: CommandOutcome, operation: &str) -> CommandError {
    match outcome {
        CommandOutcome::NotCommitted { evidence } => evidence,
        CommandOutcome::Committed {
            receipt,
            later_failure,
        } => panic!(
            "expected {operation} to be rejected, got committed receipt {receipt:?} with later failure {later_failure:?}"
        ),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected {operation} to be rejected, got indeterminate outcome {failure:?}")
        }
    }
}

fn stage_content(store: &HomeStore, storage: SyndicStorage, content: &PreparedContent) {
    assert_committed(
        store,
        storage,
        execute(
            store,
            storage,
            storage.begin_content(
                storage.revision(store).unwrap(),
                ContentBuild::from_prepared(content),
            ),
        ),
    );
    let mut manifest = content.building_manifest();
    while let Some(append) = ContentAppend::prepare(&manifest, content).unwrap() {
        let next = append.next_manifest().clone();
        assert_committed(
            store,
            storage,
            execute(
                store,
                storage,
                storage.append_content(storage.revision(store).unwrap(), append),
            ),
        );
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
    let creation = CreateThread::ordinary(
        thread_id,
        draft_id,
        execution(),
        timestamp(10),
        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
    );

    assert_eq!(
        storage
            .thread_creation_status(&store, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Absent
    );
    assert_committed(
        &store,
        storage,
        execute(
            &store,
            storage,
            storage.create_thread(storage.revision(&store).unwrap(), creation.clone()),
        ),
    );
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
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
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
    let creation = CreateThread::ordinary(
        thread_id,
        draft_id,
        execution(),
        timestamp(1),
        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
    );
    let before = storage.revision(&store).unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let mut command =
        HomeCommand::new(store.home_revision().unwrap()).with_cancellation(cancellation);
    command
        .add(storage.create_thread(before, creation.clone()))
        .unwrap();
    match store.execute(command) {
        CommandOutcome::NotCommitted {
            evidence: CommandError::CancelledBeforeAdmission,
        } => {}
        CommandOutcome::NotCommitted { evidence } => {
            panic!("expected cancelled-before-admission rejection, got {evidence:?}")
        }
        CommandOutcome::Committed {
            receipt,
            later_failure,
        } => panic!(
            "expected cancelled-before-admission rejection, got committed receipt {receipt:?} with later failure {later_failure:?}"
        ),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!(
                "expected cancelled-before-admission rejection, got indeterminate outcome {failure:?}"
            )
        }
    }
    assert_eq!(storage.revision(&store).unwrap(), before);
    assert_eq!(
        storage
            .thread_creation_status(&store, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Absent
    );

    assert_committed(
        &store,
        storage,
        execute(
            &store,
            storage,
            storage.create_thread(storage.revision(&store).unwrap(), creation.clone()),
        ),
    );
    let error = assert_not_committed(
        execute(
            &store,
            storage,
            storage.create_thread(storage.revision(&store).unwrap(), creation),
        ),
        "duplicate thread creation",
    );
    assert_mutation_error(&error, |error| {
        matches!(error, SyndicMutationError::IdentityCollision)
    });
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn dirty_only_update_preserves_immutable_draft_facts_and_reconciles_exactly() {
    let home = TestHome::new("update");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread_id, draft_id) = ids(20);
    let creation = CreateThread::ordinary(
        thread_id,
        draft_id,
        execution(),
        timestamp(5),
        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
    );
    assert_committed(
        &store,
        storage,
        execute(
            &store,
            storage,
            storage.create_thread(storage.revision(&store).unwrap(), creation),
        ),
    );
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
    assert_committed(
        &store,
        storage,
        execute(
            &store,
            storage,
            storage.update_draft_payload(storage.revision(&store).unwrap(), update.clone()),
        ),
    );
    let after = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    assert!(update.matches_committed(&after));
    assert_eq!(after.draft().thread_id(), before.draft().thread_id());
    assert_eq!(
        after.draft().submission_intent(),
        before.draft().submission_intent()
    );
    assert_eq!(after.draft().created_at(), before.draft().created_at());
    assert_eq!(
        storage
            .history_summary(&store, thread_id, limit())
            .unwrap()
            .unwrap()
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
    assert_committed(
        &store,
        storage,
        execute(
            &store,
            storage,
            storage.update_draft_payload(storage.revision(&store).unwrap(), maximum_update.clone()),
        ),
    );
    let maximum_current = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    assert!(maximum_update.matches_committed(&maximum_current));

    let error = assert_not_committed(
        execute(
            &store,
            storage,
            storage.update_draft_payload(storage.revision(&store).unwrap(), stale),
        ),
        "stale draft payload update",
    );
    assert_mutation_error(&error, |error| {
        matches!(error, SyndicMutationError::DraftRevisionConflict { .. })
    });
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
