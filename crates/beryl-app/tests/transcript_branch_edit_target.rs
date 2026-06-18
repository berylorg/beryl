#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

pub use beryl_app::{BerylWorkspacePersistence, WorkspacePersistenceError};

mod syndic_transcript {
    pub(crate) use crate::syndic_transcript_core::*;
}

#[path = "../src/branch_bootstrap_core.rs"]
mod branch_bootstrap_core;
#[path = "../src/shell/resident_branch_edit.rs"]
mod resident_branch_edit;
#[path = "../src/shell/resident_branch_worker.rs"]
mod resident_branch_worker;
#[path = "../src/shell/syndic_ingestion.rs"]
mod syndic_ingestion;
#[path = "../src/shell/token_usage_snapshot.rs"]
mod token_usage_snapshot;
mod transcript_images {
    use beryl_backend::UserInput;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct TranscriptImageMarker;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct TranscriptImageMarkerSpec;

    #[derive(Default)]
    pub(super) struct TranscriptImagePathResolver;

    pub(super) struct TranscriptImageParts {
        display_text: String,
    }

    impl TranscriptImageParts {
        pub(super) fn display_text(&self) -> &str {
            self.display_text.as_str()
        }

        pub(super) fn into_image_markers(self) -> Vec<TranscriptImageMarkerSpec> {
            Vec::new()
        }
    }

    pub(super) fn transcript_image_parts_for_backend_records(
        records: &[UserInput],
        _: &TranscriptImagePathResolver,
    ) -> TranscriptImageParts {
        let display_text = records
            .iter()
            .filter_map(|record| match record {
                UserInput::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        TranscriptImageParts { display_text }
    }

    pub(super) fn transcript_image_markers_from_specs(
        _: u64,
        _: Vec<TranscriptImageMarkerSpec>,
    ) -> Vec<TranscriptImageMarker> {
        Vec::new()
    }

    pub(super) fn transcript_image_marker_specs_from_markers(
        _: &[TranscriptImageMarker],
    ) -> Vec<TranscriptImageMarkerSpec> {
        Vec::new()
    }
}
#[path = "../src/shell/turn_input.rs"]
mod turn_input;

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::Duration,
};

use beryl_backend::{
    ApprovalRequest, DynamicToolCallRequest, DynamicToolCallResponse, ThreadForkOptions,
    ThreadForkResponse, ThreadInfo, ThreadItem, ThreadRollbackResponse, ThreadStatus,
    ThreadSummary, TurnInfo, TurnItemsView, TurnStartOptions, TurnStartResponse,
    TurnStatus as BackendTurnStatus, TurnStreamEvent, UserInput, UserMessageItem,
};
use beryl_model::workspace::{BerylWorkspaceId, WorkspaceId};
use serde_json::json;
use syndic_storage::{
    ByteRange, CanonicalItemKind, CanonicalItemRecord, CanonicalItemVisibility,
    CasProjectionBindingId, CasProjectionBindingStatus, ConversationId, ConversationRecord,
    ExternalSourceMetadata, HistoryIncompleteReason, HistoryState, ItemId, ProjectionPayload,
    ProjectionRecord, ProjectionRecordId, ProjectionRecordKind, ProjectionStatus, ProviderRevision,
    SourceEventId, SourceEventPayload, SourceEventRecord, SourceEventVisibility, StoreOpenOptions,
    SyndicSourceProvenance, SyndicStore, SyndicWriteBatch, ThreadViewId, TranscriptNarrativeKind,
    TranscriptPageAnchor, TranscriptPageDirection, TranscriptViewPosition, TranscriptViewRecord,
    TranscriptViewRecordId, TurnId, TurnKind, TurnRecord, TurnStatus,
};
use syndic_transcript::{
    ProjectionRecordId as ResidentProjectionRecordId, ProviderRevision as ResidentProviderRevision,
    ResidentActionTargetProvenance, ResidentBranchActionTarget, ResidentContextMenuContentKind,
    ResidentEditActionTarget, ResidentPresentationRecordId, SyndicItemId as ResidentSyndicItemId,
    SyndicSourceProvenance as ResidentSyndicSourceProvenance, SyndicTurnId as ResidentSyndicTurnId,
    TranscriptViewId as ResidentTranscriptViewId,
    TranscriptViewPosition as ResidentTranscriptViewPosition,
};

fn source(thread_id: &str, turn_id: Option<&str>) -> ExternalSourceMetadata {
    ExternalSourceMetadata {
        provider: "codex-app-server".to_string(),
        runtime_target: Some("host-windows".to_string()),
        external_thread_id: Some(thread_id.to_string()),
        external_turn_id: turn_id.map(str::to_string),
        external_item_id: None,
        external_event_id: None,
    }
}

fn storage_provenance(
    view_id: &ThreadViewId,
    turn_id: &TurnId,
    item_id: &ItemId,
    event_id: &SourceEventId,
    projection_id: &ProjectionRecordId,
    position: u64,
    text: &str,
) -> SyndicSourceProvenance {
    let range = ByteRange::new(0, text.len() as u64);
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(TranscriptViewPosition(position)),
        turn_id: Some(turn_id.clone()),
        item_id: Some(item_id.clone()),
        source_event_id: Some(event_id.clone()),
        projection_id: Some(projection_id.clone()),
        resource_id: None,
        source_range: Some(range),
        resource_range: None,
        copy_source_range: Some(range),
    }
}

fn seed_store(history_state: HistoryState) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir should be created");
    seed_store_at(dir.path(), history_state);
    dir
}

fn seed_store_at(storage_dir: &Path, history_state: HistoryState) {
    let store =
        SyndicStore::open(storage_dir, StoreOpenOptions::default()).expect("store should open");
    let conversation_id = ConversationId::from("conversation-1");
    let view_id = ThreadViewId::from("thread-1");
    let mut batch = SyndicWriteBatch::new().put_conversation(ConversationRecord {
        id: conversation_id.clone(),
        view_id: view_id.clone(),
        parent_view_id: None,
        branch_source_turn_id: None,
        title: Some("captured".to_string()),
        created_at_ms: 1,
        updated_at_ms: 4,
        current_revision: ProviderRevision(1),
        source: Some(source("cas-thread-1", None)),
        history_state,
    });

    for (index, text) in ["first", "second", "third"].into_iter().enumerate() {
        let number = index + 1;
        let turn_id = TurnId::from(format!("turn-{number}"));
        let event_id = SourceEventId::from(format!("event-{number}"));
        let item_id = ItemId::from(format!("item-{number}"));
        let projection_id = ProjectionRecordId::from(format!("projection-{number}"));
        let provenance = storage_provenance(
            &view_id,
            &turn_id,
            &item_id,
            &event_id,
            &projection_id,
            index as u64,
            text,
        );
        batch = batch
            .put_turn(TurnRecord {
                id: turn_id.clone(),
                conversation_id: conversation_id.clone(),
                view_id: view_id.clone(),
                parent_turn_id: None,
                kind: TurnKind::User,
                status: TurnStatus::Completed,
                source: Some(source("cas-thread-1", Some(&format!("cas-turn-{number}")))),
                created_at_ms: number as u64,
                started_at_ms: Some(number as u64),
                completed_at_ms: Some(number as u64),
                terminal_error: None,
                projection_revision: ProviderRevision(1),
            })
            .put_source_event(SourceEventRecord {
                id: event_id.clone(),
                turn_id: turn_id.clone(),
                sequence: 0,
                captured_at_ms: number as u64,
                source: source("cas-thread-1", Some(&format!("cas-turn-{number}"))),
                visibility: SourceEventVisibility::TranscriptVisible,
                payload: SourceEventPayload {
                    kind: "acceptedUserInput".to_string(),
                    body: json!({
                        "fragmentId": number,
                        "text": text,
                        "backendInput": [UserInput::text(text)],
                    }),
                },
            })
            .put_item(CanonicalItemRecord {
                id: item_id.clone(),
                turn_id: turn_id.clone(),
                source_event_id: event_id,
                kind: CanonicalItemKind::UserInput,
                visibility: CanonicalItemVisibility::Transcript,
                source: Some(source("cas-thread-1", Some(&format!("cas-turn-{number}")))),
                payload: json!({ "text": text }),
            })
            .put_projection(ProjectionRecord {
                id: projection_id.clone(),
                view_id: view_id.clone(),
                turn_id: turn_id.clone(),
                item_id: item_id.clone(),
                revision: ProviderRevision(1),
                kind: ProjectionRecordKind::TextChunk,
                status: ProjectionStatus::Current,
                payload: ProjectionPayload::Text {
                    text: text.to_string(),
                },
                provenance: provenance.clone(),
            })
            .put_view_record(TranscriptViewRecord {
                id: TranscriptViewRecordId::from(format!("view-record-{number}")),
                view_id: view_id.clone(),
                position: TranscriptViewPosition(index as u64),
                projection_id,
                narrative_kind: TranscriptNarrativeKind::UserInput,
                provenance,
            });
    }

    store.commit(batch).expect("records should commit");
}

fn target_provenance() -> ResidentActionTargetProvenance {
    ResidentActionTargetProvenance {
        presentation_revision: 1,
        record_id: ResidentPresentationRecordId("record-2".to_string()),
        source: ResidentSyndicSourceProvenance {
            view_id: ResidentTranscriptViewId("thread-1".to_string()),
            position: Some(ResidentTranscriptViewPosition(1)),
            turn_id: Some(ResidentSyndicTurnId("turn-2".to_string())),
            item_id: Some(ResidentSyndicItemId("item-2".to_string())),
            projection_id: Some(ResidentProjectionRecordId("projection-2".to_string())),
            resource_id: None,
            source_range: Some(0..6),
            resource_range: None,
            copy_source_range: Some(0..6),
        },
        projection_id: ResidentProjectionRecordId("projection-2".to_string()),
        projection_revision: ResidentProviderRevision(1),
        content_kind: ResidentContextMenuContentKind::TextChunk,
        source_range: Some(0..6),
        resource_range: None,
    }
}

#[test]
fn resident_branch_and_edit_proofs_read_complete_syndic_history() {
    let dir = seed_store(HistoryState::Complete);
    let branch_target = ResidentBranchActionTarget {
        provenance: target_provenance(),
    };
    let edit_target = ResidentEditActionTarget {
        provenance: target_provenance(),
    };

    let branch = resident_branch_edit::prove_resident_branch_target(dir.path(), &branch_target)
        .expect("branch target should prove from complete Syndic history");
    assert_eq!(branch.source_view_id, ThreadViewId::from("thread-1"));
    assert_eq!(branch.target_turn_id, TurnId::from("turn-2"));
    assert_eq!(branch.source_thread_id, "cas-thread-1");
    assert_eq!(branch.source_turn_id, "cas-turn-2");
    assert_eq!(branch.rollback_turns_after_target, 1);
    assert_eq!(branch.title_seed, "second");
    let materialization = resident_branch_edit::materialize_resident_branch_prefix(
        dir.path(),
        "workspace-1",
        &branch,
        "host-windows",
        "cas-thread-branch",
    )
    .expect("branch prefix should materialize into a new Syndic view");
    let branch_view_id = ThreadViewId::from("view:workspace-1:cas:cas-thread-branch");
    assert_eq!(
        materialization.conversation_id,
        ConversationId::from("conversation:workspace-1:cas:cas-thread-branch")
    );
    assert_eq!(materialization.view_id, branch_view_id);
    assert_eq!(materialization.copied_view_records, 2);
    let store = SyndicStore::open(dir.path(), StoreOpenOptions::default())
        .expect("store should reopen after branch materialization");
    let branch_conversation = store
        .conversation_by_external_thread(
            "codex-app-server",
            Some("host-windows"),
            "cas-thread-branch",
        )
        .expect("branch conversation lookup should succeed")
        .expect("branch conversation should be indexed by CAS thread");
    assert_eq!(branch_conversation.view_id, branch_view_id);
    assert!(matches!(
        branch_conversation.history_state,
        HistoryState::Complete
    ));
    let branch_summary = store
        .conversation_view_summary(&branch_view_id)
        .expect("branch summary should read")
        .expect("branch summary should exist");
    assert!(
        branch_summary.title_candidates.is_empty(),
        "CAS branch metadata must not become a Syndic title candidate"
    );
    let branch_page = store
        .read_transcript_page(
            &branch_view_id,
            TranscriptPageAnchor::Start,
            TranscriptPageDirection::Forward,
            10,
            None,
        )
        .expect("branch view should read");
    assert_eq!(
        branch_page
            .records
            .iter()
            .map(|record| record.position.0)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        branch_page.records[1].provenance.turn_id,
        Some(TurnId::from("turn-2"))
    );
    let branch_projection = store
        .projection(&branch_page.records[1].projection_id)
        .expect("branch projection lookup should succeed")
        .expect("branch projection should exist");
    assert_eq!(branch_projection.provenance.view_id, branch_view_id);
    assert_eq!(
        branch_projection.provenance.position,
        Some(TranscriptViewPosition(1))
    );
    let branch_binding = store
        .cas_projection_binding(&CasProjectionBindingId::from(format!(
            "binding:{branch_view_id}"
        )))
        .expect("branch binding lookup should succeed")
        .expect("branch materialization should bind the CAS projection");
    assert!(matches!(
        branch_binding.status,
        CasProjectionBindingStatus::Valid {
            ref cas_thread_id,
            ..
        } if cas_thread_id == "cas-thread-branch"
    ));
    drop(store);

    let edit = resident_branch_edit::prove_resident_edit_target(dir.path(), &edit_target)
        .expect("edit target should prove from complete Syndic history");
    assert_eq!(edit.source_thread_id, "cas-thread-1");
    assert_eq!(edit.source_turn_id, "cas-turn-2");
    assert_eq!(edit.rollback_turns_including_target, 2);
    assert_eq!(edit.display_text, "second");
    assert_eq!(edit.backend_input, vec![UserInput::text("second")]);
    assert_eq!(edit.detached_view_records.len(), 2);
    assert_eq!(edit.detached_view_records[0].position.0, 1);
    assert_eq!(edit.detached_view_records[0].id.as_str(), "view-record-2");
    assert_eq!(edit.detached_view_records[1].position.0, 2);
    assert_eq!(edit.detached_view_records[1].id.as_str(), "view-record-3");

    resident_branch_edit::detach_resident_edit_tail(dir.path(), &edit)
        .expect("edit tail should detach from the selected view");
    let store = SyndicStore::open(dir.path(), StoreOpenOptions::default())
        .expect("store should reopen after edit detachment");
    let page = store
        .read_transcript_page(
            &ThreadViewId::from("thread-1"),
            TranscriptPageAnchor::Start,
            TranscriptPageDirection::Forward,
            10,
            None,
        )
        .expect("selected view should read after detachment");
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.position.0)
            .collect::<Vec<_>>(),
        vec![0]
    );
    assert!(
        store
            .projection(&ProjectionRecordId::from("projection-2"))
            .expect("projection lookup should succeed")
            .is_some()
    );
    let binding = store
        .cas_projection_binding(&CasProjectionBindingId::from("binding:thread-1"))
        .expect("binding lookup should succeed")
        .expect("edit detachment should mark a binding");
    assert!(matches!(
        binding.status,
        CasProjectionBindingStatus::Stale {
            old_cas_thread_id: Some(ref thread_id),
            ..
        } if thread_id == "cas-thread-1"
    ));
}

#[test]
fn resident_branch_proof_rejects_incomplete_history() {
    let dir = seed_store(HistoryState::Incomplete {
        reason: HistoryIncompleteReason::NotCaptured,
        detail: Some("older turns were not captured".to_string()),
    });
    let branch_target = ResidentBranchActionTarget {
        provenance: target_provenance(),
    };

    let error = resident_branch_edit::prove_resident_branch_target(dir.path(), &branch_target)
        .expect_err("incomplete history must not prove a branch target");

    assert!(matches!(
        error,
        resident_branch_edit::ResidentBranchEditProofError::IncompleteHistory(detail)
            if detail.contains("older turns were not captured")
    ));
}

#[test]
fn resident_branch_worker_materializes_prefix_then_runs_cas_branch_bootstrap() {
    let (_root, persistence, workspace_id, execution_target, storage_dir, proof) =
        seeded_workspace_branch();
    let mut backend = FakeResidentBranchBackend::new(thread_fork_response(
        "cas-thread-branch",
        r"C:\work\captured",
        Some("Copied parent title"),
    ))
    .with_read_metadata(thread_summary(
        "cas-thread-branch",
        r"C:\work\captured",
        Some("cas-thread-1"),
        Some("Durable branch"),
        false,
    ));

    let outcome = resident_branch_worker::run_resident_branch_backend_result(
        &mut backend,
        persistence,
        &workspace_id,
        &execution_target,
        storage_dir.clone(),
        &proof,
        Some("Parent title"),
        Duration::from_secs(1),
    )
    .expect("resident branch worker should complete");

    let resident_branch_worker::ResidentBranchOutcome::Created {
        source_thread_id,
        source_turn_id,
        thread_summary,
        bootstrap_turn_id,
        ..
    } = outcome
    else {
        panic!("expected resident branch to be created");
    };
    assert_eq!(source_thread_id.as_str(), "cas-thread-1");
    assert_eq!(source_turn_id.as_str(), "cas-turn-2");
    assert_eq!(thread_summary.id, "cas-thread-branch");
    assert_eq!(bootstrap_turn_id.as_str(), "bootstrap-turn");
    assert_eq!(
        backend.fork_calls,
        vec![(
            "cas-thread-1".to_string(),
            ThreadForkOptions::metadata_only()
        )]
    );
    assert_eq!(
        backend.rollback_calls,
        vec![("cas-thread-branch".to_string(), 1)]
    );
    assert_eq!(backend.start_calls.len(), 1);
    assert_eq!(backend.start_calls[0].thread_id, "cas-thread-branch");
    assert!(backend.start_calls[0].text.contains("Parent title"));
    assert!(
        backend.start_calls[0]
            .options
            .developer_instructions_context()
            .is_none()
    );
    assert_eq!(backend.read_metadata_calls, vec!["cas-thread-branch"]);

    let store = SyndicStore::open(&storage_dir, StoreOpenOptions::default())
        .expect("store should reopen after branch worker");
    let conversation = store
        .conversation_by_external_thread(
            "codex-app-server",
            Some("host-windows"),
            "cas-thread-branch",
        )
        .expect("branch conversation lookup should succeed")
        .expect("branch worker should bind branch conversation");
    assert!(matches!(conversation.history_state, HistoryState::Complete));
    let summary = store
        .conversation_view_summary(&conversation.view_id)
        .expect("branch summary should read")
        .expect("branch summary should exist");
    assert!(
        summary.title_candidates.is_empty(),
        "CAS branch metadata name must not become a Syndic title candidate"
    );
    let page = store
        .read_transcript_page(
            &conversation.view_id,
            TranscriptPageAnchor::Start,
            TranscriptPageDirection::Forward,
            10,
            None,
        )
        .expect("branch transcript should read");
    assert_eq!(
        page.records
            .iter()
            .map(|record| record.position.0)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    let projections = store
        .read_projection_records(
            &conversation.view_id,
            &page
                .records
                .iter()
                .map(|record| record.projection_id.clone())
                .collect::<Vec<_>>(),
            None,
        )
        .expect("branch projections should read");
    assert!(projections.iter().any(|record| {
        matches!(
            &record.payload,
            ProjectionPayload::Text { text } if text == "Branched from [Parent title](beryl_threadid://cas-thread-1), no response required."
        )
    }));
    assert!(projections.iter().any(|record| {
        matches!(
            &record.payload,
            ProjectionPayload::Text { text } if text == "bootstrap acknowledged"
        )
    }));
    assert!(store.list_recovery_markers(8).unwrap().is_empty());
}

#[test]
fn resident_branch_worker_marks_syndic_incomplete_when_bootstrap_completion_is_missed() {
    let (_root, persistence, workspace_id, execution_target, storage_dir, proof) =
        seeded_workspace_branch();
    let mut backend = FakeResidentBranchBackend::new(thread_fork_response(
        "cas-thread-branch",
        r"C:\work\captured",
        Some("Copied parent title"),
    ))
    .with_stream_event(TurnStreamEvent::ThreadStatusChanged {
        thread_id: "cas-thread-branch".to_string(),
        status: ThreadStatus::Idle,
    });

    let error = match resident_branch_worker::run_resident_branch_backend_result(
        &mut backend,
        persistence,
        &workspace_id,
        &execution_target,
        storage_dir.clone(),
        &proof,
        Some("Parent title"),
        Duration::from_secs(1),
    ) {
        Ok(_) => panic!("missed bootstrap completion should fail branch publication"),
        Err(error) => error,
    };

    assert!(error.contains("idle before Beryl observed"));
    assert_eq!(
        backend.rollback_calls,
        vec![("cas-thread-branch".to_string(), 1)]
    );
    assert_eq!(backend.start_calls.len(), 1);
    assert!(backend.read_metadata_calls.is_empty());

    let store = SyndicStore::open(&storage_dir, StoreOpenOptions::default())
        .expect("store should reopen after failed branch worker");
    let conversation = store
        .conversation_by_external_thread(
            "codex-app-server",
            Some("host-windows"),
            "cas-thread-branch",
        )
        .expect("branch conversation lookup should succeed")
        .expect("branch materialization should still be indexed");
    assert!(matches!(
        conversation.history_state,
        HistoryState::Incomplete {
            reason: HistoryIncompleteReason::StreamLost,
            ..
        }
    ));
    let marker = store
        .list_recovery_markers(8)
        .expect("recovery markers should read")
        .pop()
        .expect("stream loss should leave a recovery marker");
    let turn = store
        .turn(
            &marker
                .turn_id
                .expect("recovery marker should name the turn"),
        )
        .expect("turn lookup should succeed")
        .expect("bootstrap turn should remain in Syndic");
    assert!(matches!(
        turn.status,
        syndic_storage::TurnStatus::Incomplete {
            reason: HistoryIncompleteReason::StreamLost,
            ..
        }
    ));
}

fn seeded_workspace_branch() -> (
    tempfile::TempDir,
    BerylWorkspacePersistence,
    BerylWorkspaceId,
    WorkspaceId,
    PathBuf,
    resident_branch_edit::ResidentBranchProof,
) {
    let root = tempfile::tempdir().expect("workspace root should be created");
    let persistence = BerylWorkspacePersistence::new(root.path());
    let workspace_id = BerylWorkspaceId::untitled(1);
    let execution_target = WorkspaceId::host_windows(r"C:\work\captured");
    let storage_dir = persistence.workspace_syndic_storage_dir(&workspace_id);
    seed_store_at(&storage_dir, HistoryState::Complete);
    let branch_target = ResidentBranchActionTarget {
        provenance: target_provenance(),
    };
    let proof = resident_branch_edit::prove_resident_branch_target(&storage_dir, &branch_target)
        .expect("branch target should prove from complete Syndic history");
    (
        root,
        persistence,
        workspace_id,
        execution_target,
        storage_dir,
        proof,
    )
}

#[derive(Clone, Debug)]
struct BranchStartCall {
    thread_id: String,
    text: String,
    options: TurnStartOptions,
}

struct FakeResidentBranchBackend {
    fork_response: ThreadForkResponse,
    read_metadata: VecDeque<ThreadSummary>,
    stream_events: VecDeque<TurnStreamEvent>,
    fork_calls: Vec<(String, ThreadForkOptions)>,
    rollback_calls: Vec<(String, u32)>,
    start_calls: Vec<BranchStartCall>,
    read_metadata_calls: Vec<String>,
    approval_denials: Vec<ApprovalRequest>,
    dynamic_tool_responses: Vec<(DynamicToolCallRequest, DynamicToolCallResponse)>,
}

impl FakeResidentBranchBackend {
    fn new(fork_response: ThreadForkResponse) -> Self {
        Self {
            fork_response,
            read_metadata: VecDeque::new(),
            stream_events: VecDeque::new(),
            fork_calls: Vec::new(),
            rollback_calls: Vec::new(),
            start_calls: Vec::new(),
            read_metadata_calls: Vec::new(),
            approval_denials: Vec::new(),
            dynamic_tool_responses: Vec::new(),
        }
    }

    fn with_read_metadata(mut self, summary: ThreadSummary) -> Self {
        self.read_metadata.push_back(summary);
        self
    }

    fn with_stream_event(mut self, event: TurnStreamEvent) -> Self {
        self.stream_events.push_back(event);
        self
    }
}

impl resident_branch_worker::ResidentBranchBackend for FakeResidentBranchBackend {
    fn fork_thread_with_options(
        &mut self,
        thread_id: &str,
        options: ThreadForkOptions,
        _: Duration,
    ) -> Result<ThreadForkResponse, Self::Error> {
        self.fork_calls.push((thread_id.to_string(), options));
        Ok(self.fork_response.clone())
    }

    fn rollback_thread(
        &mut self,
        thread_id: &str,
        num_turns: u32,
        _: Duration,
    ) -> Result<ThreadRollbackResponse, Self::Error> {
        self.rollback_calls.push((thread_id.to_string(), num_turns));
        Ok(ThreadRollbackResponse {
            thread: thread_info(
                thread_id,
                r"C:\work\captured",
                Some("cas-thread-1"),
                None,
                ThreadStatus::Idle,
                Vec::new(),
            ),
        })
    }
}

impl branch_bootstrap_core::BranchBootstrapBackend for FakeResidentBranchBackend {
    type Error = String;

    fn start_turn_with_options(
        &mut self,
        thread_id: &str,
        text: &str,
        options: TurnStartOptions,
        _: Duration,
    ) -> Result<TurnStartResponse, Self::Error> {
        self.start_calls.push(BranchStartCall {
            thread_id: thread_id.to_string(),
            text: text.to_string(),
            options,
        });
        Ok(TurnStartResponse {
            turn: TurnInfo {
                id: "bootstrap-turn".to_string(),
                status: BackendTurnStatus::InProgress,
                items_view: TurnItemsView::Full,
                items: Vec::new(),
                error: None,
            },
        })
    }

    fn read_thread_metadata(
        &mut self,
        thread_id: &str,
        _: Duration,
    ) -> Result<ThreadSummary, Self::Error> {
        self.read_metadata_calls.push(thread_id.to_string());
        self.read_metadata
            .pop_front()
            .ok_or_else(|| "read metadata was not configured".to_string())
    }

    fn next_turn_stream_event(
        &mut self,
        _: Duration,
    ) -> Result<Option<TurnStreamEvent>, Self::Error> {
        if let Some(event) = self.stream_events.pop_front() {
            return Ok(Some(event));
        }
        let call = self
            .start_calls
            .last()
            .expect("bootstrap turn should be started before stream polling");
        Ok(Some(TurnStreamEvent::TurnCompleted {
            thread_id: call.thread_id.clone(),
            turn: completed_bootstrap_turn("bootstrap-turn", &call.text),
        }))
    }

    fn deny_approval_request(&mut self, request: &ApprovalRequest) -> Result<(), Self::Error> {
        self.approval_denials.push(request.clone());
        Ok(())
    }

    fn respond_dynamic_tool_call(
        &mut self,
        request: &DynamicToolCallRequest,
        response: &DynamicToolCallResponse,
    ) -> Result<(), Self::Error> {
        self.dynamic_tool_responses
            .push((request.clone(), response.clone()));
        Ok(())
    }
}

fn thread_fork_response(id: &str, cwd: &str, name: Option<&str>) -> ThreadForkResponse {
    ThreadForkResponse {
        thread: thread_info(
            id,
            cwd,
            Some("cas-thread-1"),
            name,
            ThreadStatus::Idle,
            Vec::new(),
        ),
        model: None,
        model_provider: None,
        reasoning_effort: None,
    }
}

fn thread_info(
    id: &str,
    cwd: &str,
    forked_from_id: Option<&str>,
    name: Option<&str>,
    status: ThreadStatus,
    turns: Vec<TurnInfo>,
) -> ThreadInfo {
    serde_json::from_value(json!({
        "id": id,
        "forkedFromId": forked_from_id,
        "cwd": cwd,
        "preview": "",
        "name": name,
        "createdAt": 10,
        "updatedAt": 20,
        "modelProvider": "openai",
        "ephemeral": false,
        "status": status,
        "turns": turns,
    }))
    .expect("thread info fixture should deserialize")
}

fn thread_summary(
    id: &str,
    cwd: &str,
    forked_from_id: Option<&str>,
    name: Option<&str>,
    ephemeral: bool,
) -> ThreadSummary {
    ThreadSummary {
        id: id.to_string(),
        forked_from_id: forked_from_id.map(str::to_string),
        cwd: PathBuf::from(cwd),
        preview: String::new(),
        name: name.map(str::to_string),
        agent_nickname: None,
        path: None,
        created_at: 10,
        updated_at: 20,
        model_provider: "openai".to_string(),
        ephemeral,
    }
}

fn completed_bootstrap_turn(turn_id: &str, bootstrap_message: &str) -> TurnInfo {
    TurnInfo {
        id: turn_id.to_string(),
        status: BackendTurnStatus::Completed,
        items_view: TurnItemsView::Full,
        items: vec![
            ThreadItem::UserMessage(UserMessageItem {
                id: "bootstrap-user-message".to_string(),
                content: vec![UserInput::Text {
                    text: bootstrap_message.to_string(),
                }],
            }),
            ThreadItem::AgentMessage(beryl_backend::AgentMessageItem {
                id: "bootstrap-agent-message".to_string(),
                text: "bootstrap acknowledged".to_string(),
                phase: Some(beryl_backend::ProtocolPhase::FinalAnswer),
            }),
        ],
        error: None,
    }
}
