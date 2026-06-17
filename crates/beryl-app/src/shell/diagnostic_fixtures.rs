use std::path::PathBuf;

use beryl_backend::{HardStopCapabilities, ThreadInfo, ThreadSummary};
use beryl_model::{
    conversation::WorkspaceConversationState,
    semantic_graph::SemanticGraph,
    threaded_decision::ThreadedDecisionState,
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest, WorkspaceId},
};
use gpui::{Context, Window};
use serde_json::{Value, json};

use crate::{WorkspaceGraphRevision, WorkspaceUiState};

use super::{
    BackendAvailabilityRecord, BackendUnavailable, BackendUnavailableState,
    ConversationSurfaceState, LoadedWorkspaceState, ShellState, ShellView,
    backend_availability::BackendUnavailableKind, token_usage_snapshot,
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

        let thread = scroll_smoke_thread_info(&execution_target);
        let selected_thread_id = thread.summary().id;
        let known_threads = vec![thread.summary()];
        let turn_count = thread.turns.len();
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
        self.notify_transcript_panel(cx);
        cx.notify();

        json!({
            "fixture": "scroll_smoke_transcript",
            "selectedThreadId": selected_thread_id,
            "turnCount": turn_count,
            "presentationRows": 0,
            "published": published.is_some(),
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
            "Beryl seeded an in-memory transcript for a diagnostic scroll smoke.".to_string(),
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
    let turns = (0..SCROLL_SMOKE_TURN_COUNT)
        .map(scroll_smoke_turn_value)
        .collect::<Vec<_>>();
    let mut value = serde_json::to_value(summary)
        .expect("diagnostic thread summary should serialize to an object");
    let object = value
        .as_object_mut()
        .expect("diagnostic thread summary should serialize to an object");
    object.insert("status".to_string(), json!({ "type": "idle" }));
    object.insert("turns".to_string(), json!(turns));
    serde_json::from_value(value).expect("diagnostic scroll smoke thread should deserialize")
}

fn scroll_smoke_turn_value(index: usize) -> Value {
    json!({
        "id": format!("diagnostic-scroll-smoke-turn-{index:02}"),
        "status": "completed",
        "itemsView": "full",
        "items": [
            {
                "type": "userMessage",
                "id": format!("diagnostic-user-{index:02}"),
                "content": [
                    {
                        "type": "text",
                        "text": scroll_smoke_markdown(index, "user"),
                    }
                ],
            },
            {
                "type": "agentMessage",
                "id": format!("diagnostic-agent-{index:02}"),
                "text": scroll_smoke_markdown(index, "agent"),
                "phase": "final_answer",
            }
        ],
    })
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
