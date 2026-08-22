#![allow(dead_code)]

pub mod exact_cas;
mod lifecycle;
pub mod populated;
pub mod semantic;

#[allow(unused_imports)]
pub use exact_cas::converge_and_release_terminal_history;

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    BindingRevision, CasConversationToolProfile, ContentRevision, DraftRevision,
    ProjectionRevision, SyndicContentId, SyndicDraftId, SyndicPathDigest, SyndicThreadId,
    SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{fixture_transcript_digest_seed, FixtureBatch, FixtureRecord};
use syndic_storage::{
    BindingHeadRecord, BindingState, ComposerPayload, ContentByteSpanRecord, ContentManifestRecord,
    ContentReference, DraftByThreadRecord, DraftRecord, DraftSubmissionIntent,
    HistorySummaryRecord, InputGateRecord, PreparedContent, ProjectionLifecycle, SelectedPathProof,
    SyndicStorage, SyndicTimestamp, ThreadAttributesRecord, ThreadCatalogSummaryRecord,
    ThreadExecutionRecord, ThreadLineageProof, ThreadRecord, ThreadUsageRecord,
    TranscriptBuildPhase, TranscriptBuildRecord, TranscriptGeneration, TranscriptPathTurnRecord,
    TranscriptViewHeadRecord, TurnDepth, TurnEndStatus, TurnIncompleteReason, TurnLifecycle,
    TurnStateRecord, TurnStateRevision, TurnTerminalOutcome,
};

use lifecycle::lifecycle_end_status;

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

pub fn commit(store: &HomeStore, storage: SyndicStorage, batch: FixtureBatch) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(storage.revision(store).unwrap(), batch))
        .unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        CommandOutcome::Committed {
            later_failure: Some(failure),
            ..
        } => panic!("fixture-batch command committed with a later failure: {failure:?}"),
        CommandOutcome::NotCommitted { evidence } => {
            panic!("fixture-batch command did not commit: {evidence:?}")
        }
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("fixture-batch command was indeterminate: {failure:?}")
        }
    }
}

pub fn seed_populated(store: &HomeStore, storage: SyndicStorage) {
    populated::seed_populated(store, storage);
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

pub fn canonical_empty_root_history_pair_for(
    draft_id: SyndicDraftId,
) -> syndic_storage::DraftRootHistoryPairV1 {
    let home = TestHome::new("canonical-root-history-reference");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_canonical_empty_thread(&store, storage, id(245), draft_id);
    storage
        .current_draft(
            &store,
            id(245),
            syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap()
        .draft()
        .root_history()
}

pub fn seed_canonical_empty_thread(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
) {
    let request = syndic_storage::CreateThread::ordinary(
        thread_id,
        draft_id,
        exact_cas::execution_binding(),
        timestamp(1),
        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.create_thread(storage.revision(store).unwrap(), request))
        .unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean canonical thread seed, got {outcome:?}"),
    }
}

pub fn seed_detached_canonical_draft_backing(
    store: &HomeStore,
    storage: SyndicStorage,
    staging_thread: SyndicThreadId,
    draft_id: SyndicDraftId,
) -> syndic_storage::DraftRootHistoryPairV1 {
    seed_canonical_empty_thread(store, storage, staging_thread, draft_id);
    let pair = storage
        .current_draft(
            store,
            staging_thread,
            syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap()
        .draft()
        .root_history();
    use syndic_storage::test_faults::FixtureDelete;
    let mut cleanup = FixtureBatch::new();
    for delete in [
        FixtureDelete::Thread(staging_thread),
        FixtureDelete::ThreadExecution(staging_thread),
        FixtureDelete::ThreadAttributes(staging_thread),
        FixtureDelete::ThreadUsage(staging_thread),
        FixtureDelete::ThreadCatalogSummary(staging_thread),
        FixtureDelete::Draft(draft_id),
        FixtureDelete::InputGate(staging_thread),
        FixtureDelete::ActivityQueryHead(staging_thread),
        FixtureDelete::TranscriptViewHead(staging_thread),
        FixtureDelete::TranscriptBuild {
            thread: staging_thread,
            generation: TranscriptGeneration::FIRST,
        },
        FixtureDelete::HistorySummary(staging_thread),
        FixtureDelete::Binding {
            thread: staging_thread,
            revision: BindingRevision::new(1).unwrap(),
        },
        FixtureDelete::DraftByThread(staging_thread),
        FixtureDelete::BindingHead(staging_thread),
    ] {
        cleanup.delete(delete).unwrap();
    }
    commit(store, storage, cleanup);
    pair
}

pub fn turn_end_status(outcome: TurnTerminalOutcome) -> TurnEndStatus {
    match outcome {
        TurnTerminalOutcome::Incomplete => {
            TurnEndStatus::incomplete(TurnIncompleteReason::ItemAuditFailed)
        }
        _ => TurnEndStatus::new(outcome, None).unwrap(),
    }
}

pub fn fixture_turn_state(
    turn: SyndicTurnId,
    revision: TurnStateRevision,
    lifecycle: TurnLifecycle,
    source_event_count: u64,
    item_count: u64,
    updated_at: SyndicTimestamp,
) -> TurnStateRecord {
    TurnStateRecord::new(
        turn,
        revision,
        lifecycle,
        source_event_count,
        item_count,
        lifecycle_end_status(lifecycle),
        updated_at,
    )
    .unwrap()
}

pub fn fixture_turn_state_with_finalization(
    turn: SyndicTurnId,
    revision: TurnStateRevision,
    lifecycle: TurnLifecycle,
    source_event_count: u64,
    item_count: u64,
    finalized_item_count: u64,
    updated_at: SyndicTimestamp,
) -> TurnStateRecord {
    TurnStateRecord::with_finalization_frontier(
        turn,
        revision,
        lifecycle,
        source_event_count,
        item_count,
        finalized_item_count,
        lifecycle_end_status(lifecycle),
        updated_at,
    )
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
pub fn fixture_turn_state_with_capture(
    turn: SyndicTurnId,
    revision: TurnStateRevision,
    lifecycle: TurnLifecycle,
    source_event_count: u64,
    item_count: u64,
    finalized_item_count: u64,
    open_item_count: u64,
    history_blocking_item_count: u64,
    updated_at: SyndicTimestamp,
) -> TurnStateRecord {
    TurnStateRecord::with_capture_frontiers(
        turn,
        revision,
        lifecycle,
        source_event_count,
        item_count,
        finalized_item_count,
        open_item_count,
        history_blocking_item_count,
        lifecycle_end_status(lifecycle),
        updated_at,
    )
    .unwrap()
}

pub const fn test_tool_profile() -> CasConversationToolProfile {
    CasConversationToolProfile::v1([0xa5; 32])
}

pub fn composer_content_records(
    payload: &ComposerPayload,
) -> (ContentReference, Vec<FixtureRecord>) {
    let content = PreparedContent::composer(payload).unwrap();
    prepared_content_records(&content)
}

pub fn empty_composer_content() -> ContentReference {
    composer_content_records(&ComposerPayload::default()).0
}

pub fn utf8_content_records(text: &str) -> (ContentReference, Vec<FixtureRecord>) {
    let content = PreparedContent::utf8(text).unwrap();
    prepared_content_records(&content)
}

pub fn empty_live_content_records(
    owner: beryl_model::SyndicItemId,
) -> (ContentReference, Vec<FixtureRecord>) {
    let (content, manifest) = syndic_storage::test_faults::fixture_empty_live_content(owner);
    (content, vec![FixtureRecord::ContentManifest(manifest)])
}

pub fn prepared_content_records(
    content: &PreparedContent,
) -> (ContentReference, Vec<FixtureRecord>) {
    let revision = ContentRevision::new(1).unwrap();
    prepared_content_records_with_manifest(content, content.sealed_manifest(revision))
}

pub fn stage_prepared_content(
    store: &HomeStore,
    storage: SyndicStorage,
    content: &PreparedContent,
) {
    let (_, records) = prepared_content_records(content);
    commit(store, storage, batch(records));
}

fn prepared_content_records_with_manifest(
    content: &PreparedContent,
    manifest: ContentManifestRecord,
) -> (ContentReference, Vec<FixtureRecord>) {
    let revision = manifest.revision();
    let mut records = Vec::with_capacity(
        1 + content.chunks().len() * 2 + content.text_spans().len() + content.pieces().len(),
    );
    records.push(FixtureRecord::ContentManifest(manifest));
    let mut encoded_start = 0;
    for chunk in content.chunks() {
        records.push(FixtureRecord::ContentChunk(chunk.clone()));
        let span = ContentByteSpanRecord::for_chunk(chunk, encoded_start).unwrap();
        encoded_start = span.end();
        records.push(FixtureRecord::ContentByteSpan(span));
    }
    records.extend(
        content
            .text_spans()
            .iter()
            .copied()
            .map(FixtureRecord::ContentTextSpan),
    );
    records.extend(
        content
            .pieces()
            .iter()
            .copied()
            .map(FixtureRecord::ContentPiece),
    );
    (content.reference(revision), records)
}

pub fn empty_thread_records(
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
) -> Vec<FixtureRecord> {
    thread_records(
        thread_id,
        draft_id,
        None,
        syndic_storage::empty_selected_path_digest(),
    )
}

pub fn thread_records(
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    tail: Option<beryl_model::SyndicTurnId>,
    digest: beryl_model::SyndicPathDigest,
) -> Vec<FixtureRecord> {
    thread_records_with_activity(thread_id, draft_id, tail, digest, timestamp(1))
}

pub fn thread_records_with_activity(
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    tail: Option<beryl_model::SyndicTurnId>,
    digest: beryl_model::SyndicPathDigest,
    last_activity_at: SyndicTimestamp,
) -> Vec<FixtureRecord> {
    let thread_revision = ThreadRevision::new(1).unwrap();
    let draft_revision = DraftRevision::new(1).unwrap();
    let binding_revision = BindingRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let selected = SelectedPathProof::new(tail, thread_revision, digest);
    let thread = ThreadRecord::new(
        thread_id,
        selected,
        draft_id,
        ThreadLineageProof::new(
            None,
            None,
            syndic_storage::ThreadLineageDepth::FIRST,
            syndic_storage::root_thread_lineage_digest(thread_id),
        ),
        syndic_storage::ThreadImageLabelFrontiers::empty(),
        None,
    );
    let (_, content_records) = composer_content_records(&ComposerPayload::default());
    let draft = DraftRecord::new(
        draft_id,
        thread_id,
        draft_revision,
        DraftSubmissionIntent::Ordinary,
        canonical_empty_root_history_pair_for(draft_id),
        timestamp(1),
        timestamp(1),
    );
    let binding = syndic_storage::BindingRecord::new(
        thread_id,
        binding_revision,
        selected,
        BindingState::unbound("fixture").unwrap(),
    );
    let execution = ThreadExecutionRecord::new(thread_id, exact_cas::execution_binding());
    let attributes = ThreadAttributesRecord::ordinary(thread_id);
    let usage = ThreadUsageRecord::empty(thread_id);
    let history = HistorySummaryRecord::new(
        thread_id,
        projection_revision,
        thread_revision,
        tail,
        digest,
        true,
        last_activity_at,
    );
    let catalog = ThreadCatalogSummaryRecord::initial(&thread, &execution, &attributes, &history);
    let mut records = vec![
        FixtureRecord::Thread(thread),
        FixtureRecord::ThreadExecution(execution),
        FixtureRecord::ThreadAttributes(attributes),
        FixtureRecord::ThreadUsage(usage),
        FixtureRecord::ThreadCatalogSummary(catalog),
        FixtureRecord::Draft(draft),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread_id,
            draft_id,
            draft_revision,
            thread_revision,
        )),
        FixtureRecord::InputGate(InputGateRecord::idle(thread_id)),
        FixtureRecord::ActivityQueryHead(syndic_storage::ActivityQueryHeadRecord::empty(thread_id)),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread_id,
            TranscriptGeneration::FIRST,
            projection_revision,
            0,
            tail,
            digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::HistorySummary(history),
        FixtureRecord::Binding(binding),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread_id,
            binding_revision,
            syndic_storage::BindingLifecycle::Unbound,
            digest,
        )),
    ];
    if tail.is_none() {
        records.push(FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
            thread_id,
            TranscriptGeneration::FIRST,
            projection_revision,
            thread_revision,
            None,
            digest,
            0,
            0,
            fixture_transcript_digest_seed(),
            true,
            TranscriptBuildPhase::Complete,
        )));
    }
    records.extend(content_records);
    records
}

pub fn item_free_transcript_build_records(
    thread_id: SyndicThreadId,
    thread_revision: ThreadRevision,
    path: &[(
        SyndicTurnId,
        SyndicPathDigest,
        TurnLifecycle,
        u64,
        SyndicTimestamp,
    )],
) -> Vec<FixtureRecord> {
    let &(tail, digest, _, _, _) = path.last().expect("fixture transcript path is nonempty");
    let path_turn_count = u64::try_from(path.len()).expect("fixture path length fits u64");
    let history_complete = path
        .iter()
        .all(|(_, _, lifecycle, _, _)| lifecycle.is_proven_terminal());
    let mut records = Vec::with_capacity(path.len() + 1);
    records.push(FixtureRecord::TranscriptBuild(TranscriptBuildRecord::new(
        thread_id,
        TranscriptGeneration::FIRST,
        ProjectionRevision::new(1).unwrap(),
        thread_revision,
        Some(tail),
        digest,
        path_turn_count,
        0,
        fixture_transcript_digest_seed(),
        history_complete,
        TranscriptBuildPhase::Complete,
    )));
    records.extend(path.iter().enumerate().map(
        |(index, (turn, digest, lifecycle, source_event_count, updated_at))| {
            FixtureRecord::TranscriptPathTurn(TranscriptPathTurnRecord::new(
                thread_id,
                TranscriptGeneration::FIRST,
                TurnDepth::new(u64::try_from(index).unwrap() + 1).unwrap(),
                *turn,
                *digest,
                TurnStateRevision::FIRST,
                *lifecycle,
                *source_event_count,
                0,
                0,
                *updated_at,
            ))
        },
    ));
    records
}

pub fn batch(records: impl IntoIterator<Item = FixtureRecord>) -> FixtureBatch {
    let mut batch = FixtureBatch::new();
    let mut manifests = std::collections::HashMap::<SyndicContentId, _>::new();
    let mut chunks = std::collections::HashMap::new();
    let mut byte_spans = std::collections::HashMap::new();
    let mut text_spans = std::collections::HashMap::new();
    let mut pieces = std::collections::HashMap::new();
    for record in records {
        match &record {
            FixtureRecord::ContentManifest(manifest) => {
                if let Some(existing) = manifests.insert(manifest.id(), manifest.clone()) {
                    assert_eq!(existing, *manifest, "conflicting fixture content manifests");
                    continue;
                }
            }
            FixtureRecord::ContentChunk(chunk) => {
                let key = (chunk.content_id(), chunk.ordinal());
                if let Some(existing) = chunks.insert(key, chunk.clone()) {
                    assert_eq!(existing, *chunk, "conflicting fixture content chunks");
                    continue;
                }
            }
            FixtureRecord::ContentByteSpan(span) => {
                let key = (span.content_id(), span.start());
                if let Some(existing) = byte_spans.insert(key, *span) {
                    assert_eq!(existing, *span, "conflicting fixture content byte spans");
                    continue;
                }
            }
            FixtureRecord::ContentTextSpan(span) => {
                let key = (span.content_id(), span.logical_start());
                if let Some(existing) = text_spans.insert(key, *span) {
                    assert_eq!(existing, *span, "conflicting fixture content text spans");
                    continue;
                }
            }
            FixtureRecord::ContentPiece(piece) => {
                let key = (piece.content_id(), piece.ordinal());
                if let Some(existing) = pieces.insert(key, *piece) {
                    assert_eq!(existing, *piece, "conflicting fixture content pieces");
                    continue;
                }
            }
            _ => {}
        }
        batch.put(record).unwrap();
    }
    batch
}
