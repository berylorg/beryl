use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
#[cfg(feature = "test-faults")]
use beryl_model::SyndicItemId;
use beryl_model::{
    BindingRevision, CasConversationToolProfile, CasLoadedSessionGeneration,
    CasLoadedThreadGeneration, CasNativeTurnCount, CasProcessGeneration, CasThreadId,
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
#[cfg(feature = "test-faults")]
use syndic_storage::{
    AcceptGeneratedThreadTitle, GeneratedThreadTitle, SourceEventPayload, ThreadAttributesRevision,
    TurnEndStatus, TurnTerminalOutcome,
};
use syndic_storage::{
    CasLineageProof, CasRepresentedPrefixProof, CreateThread, DraftEditHistoryPolicyV1,
    NativeCasLineage, ProviderControlOrdinal, PublishThreadUsage, PublishUnboundBinding,
    PublishValidBinding, SyndicConnectionGeneration, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp, ThreadTokenUsageBreakdown, ThreadUsageObservation, ThreadUsageRevision,
    empty_selected_path_digest,
};

#[cfg(feature = "test-faults")]
use crate::support::{
    converge_and_release_terminal_history,
    exact_cas::{admit_event, correlate_user_item, establish_turn, submit_current_draft},
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome {
    path: PathBuf,
}

impl TestHome {
    fn new(name: &str) -> Self {
        loop {
            let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "beryl-phase74-{name}-{}-{sequence}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test home {path:?}: {error}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn open(path: &Path) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap()
}

fn id(byte: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([byte; 16])
}

fn draft_id(byte: u8) -> SyndicDraftId {
    SyndicDraftId::from_bytes([byte; 16])
}

fn timestamp(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([246; 16]),
        RootId::from_bytes([247; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-phase74-thread-properties",
        )
        .unwrap(),
    )
}

fn tool_profile() -> CasConversationToolProfile {
    CasConversationToolProfile::v1([248; 32])
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn history_policy() -> DraftEditHistoryPolicyV1 {
    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap()
}

fn execute(
    store: &beryl_home_store::HomeStore,
    contribution: beryl_home_store::MutationContribution,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean thread-property command, got {outcome:?}"),
    }
}

fn execute_rejected(
    store: &beryl_home_store::HomeStore,
    contribution: beryl_home_store::MutationContribution,
) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::NotCommitted { .. } => {}
        CommandOutcome::Indeterminate { reconciliation, .. } => {
            reconciliation.install();
            panic!("expected rejected thread-property command, got Indeterminate");
        }
        outcome => panic!("expected rejected thread-property command, got {outcome:?}"),
    }
}

#[test]
#[cfg(feature = "test-faults")]
fn accepted_generated_title_survives_later_thread_revision_and_reopen() {
    let home = TestHome::new("phase74-title-historical-source");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(40);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft_id(41),
                execution_binding(),
                timestamp(1),
                history_policy(),
            ),
        ),
    );
    let item = SyndicItemId::from_bytes([42; 16]);
    let first_turn = submit_current_draft(
        &store,
        storage,
        thread,
        draft_id(43),
        item,
        "first prompt",
        timestamp(2),
    );
    let canonical = storage
        .canonical_item(&store, item, limit())
        .unwrap()
        .unwrap();
    let source_content = canonical.presentation_content().unwrap();
    let source_thread = storage.thread(&store, thread, limit()).unwrap().unwrap();
    let title = GeneratedThreadTitle::new(
        "Accepted generated title",
        first_turn,
        source_content,
        source_thread.selected_path_digest(),
        source_thread.revision(),
        timestamp(3),
    )
    .unwrap();
    execute(
        &store,
        storage.accept_generated_thread_title(
            storage.revision(&store).unwrap(),
            AcceptGeneratedThreadTitle::new(thread, ThreadAttributesRevision::FIRST, title.clone()),
        ),
    );
    execute_rejected(
        &store,
        storage.accept_generated_thread_title(
            storage.revision(&store).unwrap(),
            AcceptGeneratedThreadTitle::new(
                thread,
                ThreadAttributesRevision::new(2).unwrap(),
                title,
            ),
        ),
    );

    let source = establish_turn(&store, storage, thread, first_turn, timestamp(4));
    admit_event(
        &store,
        storage,
        thread,
        first_turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    correlate_user_item(
        &store,
        storage,
        thread,
        first_turn,
        item,
        &source,
        timestamp(5),
    );
    admit_event(
        &store,
        storage,
        thread,
        first_turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        timestamp(6),
    );
    converge_and_release_terminal_history(&store, storage, thread, first_turn);
    let accepted_source_revision = storage
        .thread_attributes(&store, thread, limit())
        .unwrap()
        .unwrap()
        .generated_title()
        .unwrap()
        .source_thread_revision();
    submit_current_draft(
        &store,
        storage,
        thread,
        draft_id(45),
        SyndicItemId::from_bytes([44; 16]),
        "later prompt",
        timestamp(7),
    );
    assert!(
        storage
            .thread(&store, thread, limit())
            .unwrap()
            .unwrap()
            .revision()
            > accepted_source_revision
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .thread_attributes(&reopened, thread, limit())
            .unwrap()
            .unwrap()
            .generated_title()
            .unwrap()
            .text(),
        "Accepted generated title"
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

fn loaded_generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(7).unwrap(),
        CasLoadedThreadGeneration::new(9).unwrap(),
    )
}

fn usage_observation(
    binding_revision: BindingRevision,
    cas_thread: CasThreadId,
    ordinal: u64,
) -> ThreadUsageObservation {
    ThreadUsageObservation::new(
        ThreadTokenUsageBreakdown::new(1, 2, 3, 4, 10),
        ThreadTokenUsageBreakdown::new(5, 6, 7, 8, 26),
        Some(128_000),
        timestamp(4),
        execution_binding(),
        binding_revision,
        cas_thread,
        loaded_generation(),
        SyndicConnectionGeneration::FIRST,
        ProviderControlOrdinal::new(ordinal).unwrap(),
    )
    .unwrap()
}

#[test]
fn usage_survives_binding_advance_and_reopen_while_wrong_routes_are_rejected() {
    let home = TestHome::new("phase74-usage-historical-binding");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(50);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft_id(51),
                execution_binding(),
                timestamp(1),
                history_policy(),
            ),
        ),
    );
    let other_thread = id(52);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                other_thread,
                draft_id(53),
                execution_binding(),
                timestamp(1),
                history_policy(),
            ),
        ),
    );
    let current = storage
        .current_binding(&store, thread, limit())
        .unwrap()
        .unwrap();
    let selected = current.binding().selected_path();
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let cas_thread = CasThreadId::new("phase74-usage-route").unwrap();
    execute(
        &store,
        storage.publish_valid_binding(
            storage.revision(&store).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                execution_binding(),
                cas_thread.clone(),
                represented,
                CasNativeTurnCount::ZERO,
                tool_profile(),
                CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
            ),
        ),
    );
    let valid = storage
        .current_binding(&store, thread, limit())
        .unwrap()
        .unwrap();
    let valid_revision = valid.binding().revision();

    let wrong = usage_observation(
        valid_revision,
        CasThreadId::new("phase74-wrong-route").unwrap(),
        1,
    );
    execute_rejected(
        &store,
        storage.publish_thread_usage(
            storage.revision(&store).unwrap(),
            PublishThreadUsage::new(thread, ThreadUsageRevision::FIRST, wrong),
        ),
    );
    let observation = usage_observation(valid_revision, cas_thread, 1);
    execute(
        &store,
        storage.publish_thread_usage(
            storage.revision(&store).unwrap(),
            PublishThreadUsage::new(thread, ThreadUsageRevision::FIRST, observation.clone()),
        ),
    );
    let accepted_usage_revision = ThreadUsageRevision::new(2).unwrap();
    execute_rejected(
        &store,
        storage.publish_thread_usage(
            storage.revision(&store).unwrap(),
            PublishThreadUsage::new(
                thread,
                accepted_usage_revision,
                usage_observation(
                    valid_revision,
                    CasThreadId::new("phase74-usage-route").unwrap(),
                    1,
                ),
            ),
        ),
    );
    execute_rejected(
        &store,
        storage.publish_thread_usage(
            storage.revision(&store).unwrap(),
            PublishThreadUsage::new(
                thread,
                accepted_usage_revision,
                usage_observation(
                    BindingRevision::new(1).unwrap(),
                    CasThreadId::new("phase74-usage-route").unwrap(),
                    2,
                ),
            ),
        ),
    );
    execute_rejected(
        &store,
        storage.publish_thread_usage(
            storage.revision(&store).unwrap(),
            PublishThreadUsage::new(
                other_thread,
                ThreadUsageRevision::FIRST,
                usage_observation(
                    valid_revision,
                    CasThreadId::new("phase74-usage-route").unwrap(),
                    2,
                ),
            ),
        ),
    );
    execute(
        &store,
        storage.publish_unbound_binding(
            storage.revision(&store).unwrap(),
            PublishUnboundBinding::new(thread, valid_revision, selected, "route advanced").unwrap(),
        ),
    );
    assert!(
        storage
            .current_binding(&store, thread, limit())
            .unwrap()
            .unwrap()
            .binding()
            .revision()
            > valid_revision
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .thread_usage(&reopened, thread, limit())
            .unwrap()
            .unwrap()
            .observation(),
        Some(&observation)
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}
