use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, CursorReadLimits, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    BindingRevision, CasConversationToolProfile, CasItemId, CasLoadedSessionGeneration,
    CasLoadedThreadGeneration, CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId,
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicContentId, SyndicDraftId, SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::{
    ActivateBinding, AdmitStopOperation, AdvanceItemProjectionBuild, AdvanceTranscriptBuild,
    BindingState, CanonicalItemPresentation, CanonicalItemRecord, CasItemSource, CasLineageProof,
    CasRepresentedPrefixProof, CasTurnSource, ClaimCompactionDispatch, CompactionAdmissionRead,
    CompactionAttemptNonce, CompactionMarkerLifecycle, CompactionOperationId,
    CompactionOperationNonce, CompactionProviderEvent, CompactionProviderSequence,
    CompactionThreadStatus, CompleteTerminalHistory, ContentAppend, ContentBuild, ContentLifecycle,
    ContentManifestRecord, CreateThread, DraftEditHistoryPolicyV1, FinalizeNextTurnItem,
    FreezeNextTurnItem, GeneratedMediaResourceDisposition, ItemProjectionGeneration,
    LiveSourceEvent, NativeCasLineage, PreparedContent, ProjectionLifecycle,
    ProviderFrameOrdinalV1, ProviderFramePreparationPlan, ProviderFrameStageOutcome,
    ProviderItemBuildLifecycle, ProviderItemFrameV1, ProviderItemObservationV1, ProviderItemV1,
    ProviderLifecycleTimestampMsV1, ProviderSubmittedContentV1, ProviderUserMessageV1,
    PublishActiveCasTurn, PublishCompactionProviderEvent, PublishValidBinding, ResourceBacking,
    SealLifecycleContinuationContent, SealedProviderFrameReference, SettleLifecycleCompaction,
    SourceEventPayload, SourceEventSequence, StartItemProjectionBuild, StartTranscriptBuild,
    StopCause, StopCauseSet, StopOperationId, StopOperationNonce, StopOperationRecord,
    StopOperationTarget, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    TranscriptBuildPhase, TurnEndStatus, TurnItemOrdinal, TurnTerminalOutcome,
    empty_selected_path_digest, prepare_lifecycle_continuation_content, prepare_provider_frame,
    stage_provider_frame,
};

const CONVERGENCE_LIMIT: usize = 4_096;
static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

pub struct TestHome {
    path: PathBuf,
}

impl TestHome {
    pub fn new(name: &str) -> Self {
        loop {
            let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "beryl-syndic-phase65-stop-{name}-{}-{sequence}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("failed to create isolated stop test home: {error}"),
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

pub fn timestamp(value: u64) -> SyndicTimestamp {
    SyndicTimestamp::from_unix_millis(value)
}

pub fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([246; 16]),
        RootId::from_bytes([247; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-phase65-stop",
        )
        .unwrap(),
    )
}

fn tool_profile() -> CasConversationToolProfile {
    CasConversationToolProfile::v1([248; 32])
}

fn loaded_generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(65).unwrap(),
        CasLoadedThreadGeneration::new(1).unwrap(),
    )
}

pub fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean stop fixture command, got {outcome:?}"),
    }
}

fn assert_current(outcome: CommandOutcome) {
    match outcome {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean current command, got {outcome:?}"),
    }
}

fn publish_compaction_provider(
    store: &HomeStore,
    storage: &SyndicStorage,
    id: CompactionOperationId,
    event: CompactionProviderEvent,
    at: u64,
) {
    let operation = storage
        .compaction_operation(store, id, point_limit())
        .unwrap()
        .unwrap();
    let sequence = operation
        .provider_frontier()
        .map_or(CompactionProviderSequence::FIRST, |frontier| {
            frontier.checked_next().unwrap()
        });
    assert_current(
        store.execute_current(storage.current_publish_compaction_provider_event(
            PublishCompactionProviderEvent::new(
                id,
                operation.revision(),
                sequence,
                event,
                timestamp(at),
            ),
        )),
    );
}

fn stage_prepared_content(
    store: &HomeStore,
    storage: &SyndicStorage,
    prepared: &PreparedContent,
) -> ContentManifestRecord {
    let mut manifest = prepared.building_manifest();
    execute(
        store,
        storage.begin_content(
            storage.revision(store).unwrap(),
            ContentBuild::from_prepared(prepared),
        ),
    );
    loop {
        let Some(append) = ContentAppend::prepare(&manifest, prepared).unwrap() else {
            break;
        };
        manifest = append.next_manifest().clone();
        execute(
            store,
            storage.append_content(storage.revision(store).unwrap(), append),
        );
    }
    manifest
}

fn lifecycle_content(
    store: &HomeStore,
    storage: &SyndicStorage,
) -> syndic_storage::ContentReference {
    let prepared = prepare_lifecycle_continuation_content().unwrap();
    let manifest = stage_prepared_content(store, storage, &prepared);
    assert_current(
        store.execute_current(storage.current_seal_lifecycle_continuation_content(
            SealLifecycleContinuationContent::new(manifest),
        )),
    );
    storage
        .content_manifest(store, prepared.id(), point_limit())
        .unwrap()
        .unwrap()
        .sealed_reference()
        .unwrap()
}

#[path = "support/active_stop.rs"]
mod active_stop;
#[path = "support/live_turn.rs"]
mod live_turn;
#[path = "support/terminal_history.rs"]
mod terminal_history;
#[cfg(feature = "test-faults")]
#[path = "support/test_faults.rs"]
mod test_faults;

#[cfg(feature = "test-faults")]
pub use active_stop::active_stop_fixture_with_faults;
pub use active_stop::{ActiveStopFixture, active_stop_fixture, pending_stop_fixture};

use live_turn::establish_turn;
pub use live_turn::{admit_event, correlate_user_item};
pub use terminal_history::converge_and_release_terminal_history;
#[cfg(feature = "test-faults")]
pub use test_faults::{admit_current_draft_as_accepted, admit_queued_text};

pub struct PendingTurnFixture {
    pub _home: TestHome,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub thread: SyndicThreadId,
    pub turn: SyndicTurnId,
}

pub fn pending_turn_fixture(name: &str) -> PendingTurnFixture {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([111; 16]);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([112; 16]),
                execution_binding(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
    let current = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = current.binding().selected_path();
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap();
    execute(
        &store,
        storage.publish_valid_binding(
            storage.revision(&store).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                execution_binding(),
                CasThreadId::new("phase68-pending-compaction").unwrap(),
                represented,
                CasNativeTurnCount::ZERO,
                tool_profile(),
                lineage,
            ),
        ),
    );
    let CompactionAdmissionRead::Admissible(candidate) = storage
        .compaction_admission_read(&store, thread, point_limit())
        .unwrap()
    else {
        panic!("idle valid-binding fixture must admit compaction");
    };
    let admission = candidate.admission(
        CompactionOperationNonce::from_bytes([113; 16]),
        CompactionAttemptNonce::from_bytes([114; 16]),
        loaded_generation(),
        timestamp(1),
    );
    let operation_id = admission.operation_id();
    assert_current(store.execute_current(storage.current_admit_compaction_operation(admission)));
    let operation = storage
        .compaction_operation(&store, operation_id, point_limit())
        .unwrap()
        .unwrap();
    assert_current(
        store.execute_current(storage.current_claim_compaction_dispatch(
            ClaimCompactionDispatch::new(operation_id, operation.revision(), operation.attempt()),
        )),
    );
    publish_compaction_provider(
        &store,
        &storage,
        operation_id,
        CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Active),
        1,
    );
    publish_compaction_provider(
        &store,
        &storage,
        operation_id,
        CompactionProviderEvent::TurnStarted(CasTurnId::new("phase68-pending-compaction").unwrap()),
        1,
    );
    let marker = SyndicItemId::from_bytes([115; 16]);
    publish_compaction_provider(
        &store,
        &storage,
        operation_id,
        CompactionProviderEvent::Marker {
            item_id: marker,
            lifecycle: CompactionMarkerLifecycle::Started,
        },
        1,
    );
    publish_compaction_provider(
        &store,
        &storage,
        operation_id,
        CompactionProviderEvent::Marker {
            item_id: marker,
            lifecycle: CompactionMarkerLifecycle::Completed,
        },
        1,
    );
    publish_compaction_provider(
        &store,
        &storage,
        operation_id,
        CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Idle),
        1,
    );
    publish_compaction_provider(
        &store,
        &storage,
        operation_id,
        CompactionProviderEvent::Terminal(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        1,
    );
    let content = lifecycle_content(&store, &storage);
    let operation = storage
        .compaction_operation(&store, operation_id, point_limit())
        .unwrap()
        .unwrap();
    let settlement = SettleLifecycleCompaction::new(&operation, content, timestamp(1));
    let turn = settlement.turn_id();
    assert_current(store.execute_current(storage.current_settle_lifecycle_compaction(settlement)));
    PendingTurnFixture {
        _home: home,
        store,
        storage,
        thread,
        turn,
    }
}
