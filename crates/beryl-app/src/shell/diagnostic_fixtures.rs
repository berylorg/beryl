use std::path::{Path, PathBuf};

use beryl_backend::{HardStopCapabilities, ThreadInfo, ThreadSummary};
use beryl_model::{
    conversation::WorkspaceConversationState,
    semantic_graph::SemanticGraph,
    threaded_decision::ThreadedDecisionState,
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest, WorkspaceId},
};
use gpui::{Context, Window};
use serde_json::{Value, json};
use syndic_storage::{
    ByteRange, CanonicalItemKind, CanonicalItemRecord, CanonicalItemVisibility, ConversationId,
    ConversationRecord, ExternalSourceMetadata, HistoryState, ItemId, ProjectionPayload,
    ProjectionRecord, ProjectionRecordId, ProjectionRecordKind, ProjectionStatus, ProviderRevision,
    SourceEventId, SourceEventPayload, SourceEventRecord, SourceEventVisibility, StoreOpenOptions,
    SyndicSourceProvenance, SyndicStore, SyndicWriteBatch, ThreadViewId, TranscriptNarrativeKind,
    TranscriptViewPosition, TranscriptViewRecord, TranscriptViewRecordId, TurnId, TurnKind,
    TurnRecord, TurnStatus,
};

use crate::{WorkspaceGraphRevision, WorkspaceUiState};

use super::{
    BackendAvailabilityRecord, BackendUnavailable, BackendUnavailableState,
    ConversationSurfaceState, LoadedWorkspaceState, ShellState, ShellView,
    backend_availability::BackendUnavailableKind,
    thread_activation::prepare_storage_backed_transcript_activation, token_usage_snapshot,
    workspace_members::apply_primary_execution_target_selection, workspace_picker,
};

const SCROLL_SMOKE_WORKSPACE_ID: &str = "diagnostic_scroll_smoke";
const SCROLL_SMOKE_THREAD_ID: &str = "diagnostic-scroll-smoke-thread";
const SCROLL_SMOKE_TURN_COUNT: usize = 48;

impl ShellView {
    pub(super) fn handle_seed_scroll_smoke_transcript_tool_result(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Value {
        self.cancel_pending_scroll_smoke_startup_work();

        let attempt = self.next_attempt;
        self.next_attempt = self.next_attempt.saturating_add(1);

        let execution_target = scroll_smoke_execution_target();
        let mut workspace_state = WorkspaceConversationState::default();
        let _ = apply_primary_execution_target_selection(&mut workspace_state, &execution_target);
        let workspace_ui_state = WorkspaceUiState::default();
        let workspace_id = BerylWorkspaceId::new(SCROLL_SMOKE_WORKSPACE_ID)
            .expect("diagnostic fixture workspace id is valid");
        let manifest = BerylWorkspaceManifest::named(
            workspace_id.clone(),
            "Diagnostic Scroll Smoke",
            token_usage_snapshot::current_unix_millis(),
        );
        let mut member_paths = workspace_picker::WorkspacePickerMemberPaths::new();
        member_paths.insert(
            workspace_id.clone(),
            workspace_picker::explicit_member_path_strings(&workspace_state),
        );

        let mut loaded_workspace = LoadedWorkspaceState::new(
            manifest.clone(),
            vec![manifest],
            member_paths,
            workspace_state,
            workspace_ui_state,
            ThreadedDecisionState::default(),
            None,
        );
        let availability = diagnostic_fixture_backend_availability(
            &mut loaded_workspace,
            execution_target.clone(),
            attempt,
        );

        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return json!({
                "fixture": "scroll_smoke_transcript",
                "selectedThreadId": SCROLL_SMOKE_THREAD_ID,
                "turnCount": SCROLL_SMOKE_TURN_COUNT,
                "presentationRows": 0,
                "published": false,
                "error": "Beryl home storage is unavailable for the diagnostic scroll smoke fixture.",
            });
        };
        let storage_dir = persistence.workspace_syndic_storage_dir(&workspace_id);
        if let Err(error) = seed_scroll_smoke_syndic_store(&storage_dir, &execution_target) {
            return json!({
                "fixture": "scroll_smoke_transcript",
                "selectedThreadId": SCROLL_SMOKE_THREAD_ID,
                "turnCount": SCROLL_SMOKE_TURN_COUNT,
                "presentationRows": 0,
                "published": false,
                "error": error,
            });
        }

        let thread = scroll_smoke_thread_info(&execution_target);
        let selected_thread_id = thread.summary().id;
        let known_threads = vec![thread.summary()];
        let turn_count = SCROLL_SMOKE_TURN_COUNT;
        let prepared_transcript =
            prepare_storage_backed_transcript_activation(storage_dir, &selected_thread_id);
        let (surface, published) = ConversationSurfaceState::seeded(
            workspace_id,
            execution_target.clone(),
            &loaded_workspace.workspace_state,
            &loaded_workspace.workspace_ui_state,
            known_threads,
            HardStopCapabilities::default(),
            Some(thread),
            Some(selected_thread_id.clone()),
            None,
            Some(prepared_transcript),
            None,
            SemanticGraph::default(),
            WorkspaceGraphRevision::default(),
            None,
        );

        self.state = ShellState::BackendUnavailable(BackendUnavailableState {
            attempt,
            loaded_workspace,
            execution_target,
            availability,
            surface,
        });
        window.set_window_title("Beryl - Diagnostic Scroll Smoke");
        let mut published_thread_id = None;
        if let Some(publication) = published {
            published_thread_id = Some(self.finish_published_thread_activation(publication, cx));
        }
        self.notify_transcript_panel(cx);
        cx.notify();
        let presentation_rows = self
            .transcript_panel
            .read(cx)
            .status_facts()
            .resident_presentation_record_count;

        json!({
            "fixture": "scroll_smoke_transcript",
            "selectedThreadId": selected_thread_id,
            "turnCount": turn_count,
            "presentationRows": presentation_rows,
            "published": published_thread_id.as_deref() == Some(selected_thread_id.as_str()),
        })
    }

    fn cancel_pending_scroll_smoke_startup_work(&mut self) {
        self.cancel_workspace_open();
        self.discovery_receiver = None;
        self.workspace_receiver = None;
        self.workspace_picker_action_receiver = None;
        self.thread_activation_receiver = None;
        self.turn_receiver = None;
    }
}

fn diagnostic_fixture_backend_availability(
    loaded_workspace: &mut LoadedWorkspaceState,
    execution_target: WorkspaceId,
    attempt: u32,
) -> BackendAvailabilityRecord {
    loaded_workspace.record_backend_unavailable(
        execution_target,
        attempt,
        BackendUnavailable::new(
            BackendUnavailableKind::ProbeFailed,
            None,
            "Diagnostic backend intentionally unavailable",
            "Beryl seeded a storage-backed Syndic transcript for a diagnostic scroll smoke."
                .to_string(),
            "No managed backend is started for this diagnostic fixture.".to_string(),
            vec![
                "Run the diagnostic scroll smoke against the seeded transcript.".to_string(),
                "Close the diagnostic child when the smoke finishes.".to_string(),
            ],
        ),
    )
}

fn scroll_smoke_execution_target() -> WorkspaceId {
    if cfg!(target_os = "windows") {
        WorkspaceId::host_windows(r"C:\beryl-diagnostic-scroll-smoke")
    } else {
        WorkspaceId::from_parts(
            beryl_model::workspace::RuntimeMode::HostWindows,
            PathBuf::from("/beryl-diagnostic-scroll-smoke"),
        )
    }
}

fn seed_scroll_smoke_syndic_store(
    storage_dir: &Path,
    execution_target: &WorkspaceId,
) -> Result<(), String> {
    std::fs::create_dir_all(storage_dir).map_err(|error| {
        format!(
            "Diagnostic scroll smoke Syndic storage directory could not be created at {}: {error}",
            storage_dir.display()
        )
    })?;
    let store = SyndicStore::open(storage_dir, StoreOpenOptions::default()).map_err(|error| {
        format!(
            "Diagnostic scroll smoke Syndic storage could not be opened at {}: {error:?}",
            storage_dir.display()
        )
    })?;

    let conversation_id = ConversationId::from("diagnostic-scroll-smoke-conversation");
    let view_id = ThreadViewId::from(SCROLL_SMOKE_THREAD_ID);
    let runtime_target = execution_target.display_label();
    let current_revision = ProviderRevision((SCROLL_SMOKE_TURN_COUNT * 2) as u64);
    let source = scroll_smoke_source(
        &runtime_target,
        Some(SCROLL_SMOKE_THREAD_ID),
        None,
        None,
        None,
    );
    let mut batch = SyndicWriteBatch::new().put_conversation(ConversationRecord {
        id: conversation_id.clone(),
        view_id: view_id.clone(),
        title: Some("Diagnostic Scroll Smoke".to_string()),
        created_at_ms: 1,
        updated_at_ms: current_revision.0,
        current_revision,
        source: Some(source),
        history_state: HistoryState::Complete,
    });

    for index in 0..SCROLL_SMOKE_TURN_COUNT {
        let turn_id = TurnId::from(format!("diagnostic-scroll-smoke-turn-{index:02}"));
        let created_at_ms = 10 + (index as u64 * 10);
        batch = batch.put_turn(TurnRecord {
            id: turn_id.clone(),
            conversation_id: conversation_id.clone(),
            view_id: view_id.clone(),
            parent_turn_id: None,
            kind: TurnKind::User,
            status: TurnStatus::Completed,
            source: Some(scroll_smoke_source(
                &runtime_target,
                Some(SCROLL_SMOKE_THREAD_ID),
                Some(turn_id.as_str()),
                None,
                None,
            )),
            created_at_ms,
            started_at_ms: Some(created_at_ms),
            completed_at_ms: Some(created_at_ms + 2),
            terminal_error: None,
            projection_revision: ProviderRevision(((index + 1) * 2) as u64),
        });

        batch = put_scroll_smoke_text_record(
            batch,
            &runtime_target,
            &view_id,
            &turn_id,
            index,
            0,
            TranscriptNarrativeKind::UserInput,
            CanonicalItemKind::UserInput,
            "acceptedUserInput",
            "user",
        );
        batch = put_scroll_smoke_text_record(
            batch,
            &runtime_target,
            &view_id,
            &turn_id,
            index,
            1,
            TranscriptNarrativeKind::AssistantFinalAnswer,
            CanonicalItemKind::AssistantMessage,
            "assistantMessage",
            "agent",
        );
    }

    store.commit(batch).map(|_| ()).map_err(|error| {
        format!(
            "Diagnostic scroll smoke Syndic records could not be committed at {}: {error:?}",
            storage_dir.display()
        )
    })
}

fn scroll_smoke_thread_info(execution_target: &WorkspaceId) -> ThreadInfo {
    let summary = ThreadSummary {
        id: SCROLL_SMOKE_THREAD_ID.to_string(),
        forked_from_id: None,
        cwd: execution_target.canonical_path().to_path_buf(),
        preview: "Diagnostic scroll smoke".to_string(),
        name: Some("Diagnostic Scroll Smoke".to_string()),
        agent_nickname: None,
        path: None,
        created_at: 1,
        updated_at: 2,
        model_provider: "diagnostic".to_string(),
        ephemeral: true,
    };
    let mut value = serde_json::to_value(summary)
        .expect("diagnostic thread summary should serialize to an object");
    let object = value
        .as_object_mut()
        .expect("diagnostic thread summary should serialize to an object");
    object.insert("status".to_string(), json!({ "type": "idle" }));
    object.insert("turns".to_string(), json!([]));
    serde_json::from_value(value).expect("diagnostic scroll smoke thread should deserialize")
}

fn put_scroll_smoke_text_record(
    batch: SyndicWriteBatch,
    runtime_target: &str,
    view_id: &ThreadViewId,
    turn_id: &TurnId,
    turn_index: usize,
    record_index: usize,
    narrative_kind: TranscriptNarrativeKind,
    item_kind: CanonicalItemKind,
    event_kind: &str,
    role: &str,
) -> SyndicWriteBatch {
    let position = (turn_index * 2 + record_index) as u64;
    let text = scroll_smoke_markdown(turn_index, role);
    let event_id = SourceEventId::from(format!(
        "diagnostic-scroll-smoke-event-{role}-{turn_index:02}"
    ));
    let item_id = ItemId::from(format!(
        "diagnostic-scroll-smoke-item-{role}-{turn_index:02}"
    ));
    let projection_id = ProjectionRecordId::from(format!(
        "diagnostic-scroll-smoke-projection-{role}-{turn_index:02}"
    ));
    let source = scroll_smoke_source(
        runtime_target,
        Some(SCROLL_SMOKE_THREAD_ID),
        Some(turn_id.as_str()),
        Some(item_id.as_str()),
        Some(event_id.as_str()),
    );
    let provenance = scroll_smoke_text_provenance(
        view_id,
        position,
        turn_id,
        &item_id,
        &event_id,
        &projection_id,
        &text,
    );

    batch
        .put_source_event(SourceEventRecord {
            id: event_id.clone(),
            turn_id: turn_id.clone(),
            sequence: record_index as u64,
            captured_at_ms: 10 + position,
            source: source.clone(),
            visibility: SourceEventVisibility::TranscriptVisible,
            payload: SourceEventPayload {
                kind: event_kind.to_string(),
                body: json!({ "text": text.as_str() }),
            },
        })
        .put_item(CanonicalItemRecord {
            id: item_id.clone(),
            turn_id: turn_id.clone(),
            source_event_id: event_id,
            kind: item_kind,
            visibility: CanonicalItemVisibility::Transcript,
            source: Some(source),
            payload: json!({ "text": text.as_str() }),
        })
        .put_projection(ProjectionRecord {
            id: projection_id.clone(),
            view_id: view_id.clone(),
            turn_id: turn_id.clone(),
            item_id,
            revision: ProviderRevision(position + 1),
            kind: ProjectionRecordKind::TextChunk,
            status: ProjectionStatus::Current,
            payload: ProjectionPayload::Text { text },
            provenance: provenance.clone(),
        })
        .put_view_record(TranscriptViewRecord {
            id: TranscriptViewRecordId::from(format!(
                "diagnostic-scroll-smoke-view-record-{role}-{turn_index:02}"
            )),
            view_id: view_id.clone(),
            position: TranscriptViewPosition(position),
            projection_id,
            narrative_kind,
            provenance,
        })
}

fn scroll_smoke_source(
    runtime_target: &str,
    external_thread_id: Option<&str>,
    external_turn_id: Option<&str>,
    external_item_id: Option<&str>,
    external_event_id: Option<&str>,
) -> ExternalSourceMetadata {
    ExternalSourceMetadata {
        provider: "diagnostic-scroll-smoke".to_string(),
        runtime_target: Some(runtime_target.to_string()),
        external_thread_id: external_thread_id.map(str::to_string),
        external_turn_id: external_turn_id.map(str::to_string),
        external_item_id: external_item_id.map(str::to_string),
        external_event_id: external_event_id.map(str::to_string),
    }
}

fn scroll_smoke_text_provenance(
    view_id: &ThreadViewId,
    position: u64,
    turn_id: &TurnId,
    item_id: &ItemId,
    source_event_id: &SourceEventId,
    projection_id: &ProjectionRecordId,
    text: &str,
) -> SyndicSourceProvenance {
    let range = ByteRange::new(0, text.len() as u64);
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(TranscriptViewPosition(position)),
        turn_id: Some(turn_id.clone()),
        item_id: Some(item_id.clone()),
        source_event_id: Some(source_event_id.clone()),
        projection_id: Some(projection_id.clone()),
        resource_id: None,
        source_range: Some(range),
        resource_range: None,
        copy_source_range: Some(range),
    }
}

fn scroll_smoke_markdown(index: usize, role: &str) -> String {
    (0..8)
        .map(|line| {
            format!(
                "Diagnostic scroll smoke {role} turn {index:02} line {line:02}: stable wrapped text for measured transcript scrolling."
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
