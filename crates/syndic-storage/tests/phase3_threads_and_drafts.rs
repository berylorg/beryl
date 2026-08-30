use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, HomeCommand, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
use syndic_storage::{
    CreateThread, SyndicMutationError, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
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

fn execute(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> beryl_home_store::CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn assert_committed(
    store: &HomeStore,
    storage: &SyndicStorage,
    outcome: beryl_home_store::CommandOutcome,
) -> beryl_home_store::CommitReceipt {
    match outcome {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
            local_finalization: _,
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
            local_finalization: _,
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
            local_finalization: _,
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
        &storage,
        execute(
            &store,
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
            local_finalization: _,
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
        &storage,
        execute(
            &store,
            storage.create_thread(storage.revision(&store).unwrap(), creation.clone()),
        ),
    );
    let error = assert_not_committed(
        execute(
            &store,
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
