use std::{
    cell::Cell,
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    ops::Range,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

use beryl_backend::{
    DynamicToolCallRequest, HardStopCapabilities, ManagedBackendClientConnector,
    ManagedBackendProbeReport, ManagedBackendServer, ManagedBackendStartupProgress,
    ManagedBackendStartupStage, ThreadInfo, ThreadSessionMetadata, ThreadStatus, ThreadSummary,
    TurnStartOptions, list_wsl_distros,
};
use beryl_model::conversation::{
    ConversationThreadId, ConversationThreadTokenUsageSnapshot, RegisteredConversationThread,
    WorkspaceConversationState,
};
use beryl_model::semantic_graph::{SemanticGraph, SemanticNodeId, SoftLinkId, ThreadRefId};
use beryl_model::workspace::{
    BerylWorkspaceId, BerylWorkspaceManifest, RuntimeMode, WorkspaceId, WorkspaceMemberId,
};
use gpui::{
    App, Application, AsyncApp, Bounds, ClipboardItem, Context, Entity, Image, KeyBinding,
    KeyDownEvent, KeyUpEvent, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathPromptOptions,
    Pixels, Point, PromptButton, PromptLevel, ScrollHandle, ScrollWheelEvent, Task, WeakEntity,
    Window, WindowBounds, WindowOptions, prelude::*, px, rgb, size,
};
use gpui_settings_window::{
    SettingsWindowEvent, SettingsWindowHandle, SettingsWindowOpenDisposition, open_settings_window,
};
#[cfg(target_os = "windows")]
use raw_window_handle::RawWindowHandle;
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{debug, warn};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use crate::diagnostic_child_control::{
    DiagnosticStartTurnArguments, DiagnosticStopTurnArguments, DiagnosticThreadListArguments,
};
use crate::diagnostic_child_dynamic_tools::{
    diagnostic_child_failure_response, dispatch_beryl_diagnostic_child_dynamic_tool_call,
    is_beryl_diagnostic_child_dynamic_tool,
};
use crate::diagnostic_child_protocol::{
    CLOSE_POPUPS_COMMAND, DiagnosticChildCommand, DiagnosticProtocolRequest,
    DiagnosticProtocolResponse, READ_UI_STATE_COMMAND, SCROLL_TRANSCRIPT_COMMAND,
    SWITCH_THREAD_COMMAND, SWITCH_WORKSPACE_COMMAND,
};
use crate::diagnostic_child_supervisor::DiagnosticChildSupervisor;
use crate::diagnostic_child_target::{
    DiagnosticTargetShellRequest, spawn_diagnostic_target_stdio_server,
};
use crate::diagnostic_dynamic_tools::{
    DEFAULT_MEDIA_EVENT_LIMIT, DEFAULT_VISIBLE_MEDIA_LIMIT, DiagnosticToolSnapshot,
    MAX_MEDIA_EVENT_LIMIT, MAX_VISIBLE_MEDIA_LIMIT, ManagedBackendProcessDiagnostic,
    MemoryDiagnosticSnapshot, MemoryDiagnosticUiCorrelation, PreviewStateDiagnostic,
    ProcessDiagnosticSnapshot, RendererDiagnosticSnapshot, RuntimeTargetDiagnostic,
    bounded_diagnostic_string, dispatch_beryl_diagnostic_dynamic_tool_call, media_events_result,
    renderer_snapshot_with_shell_window, visible_media_result,
};
use crate::gui_control_dynamic_tools::{
    ActivityPanelUiState, BackgroundWorkUiState, CancellableTurnUiState, ClosePopupsResult,
    DEFAULT_UI_VISIBLE_ROW_LIMIT, GuiControlToolRequest, PopupUiState,
    SETTINGS_WINDOW_POPUP_CLOSE_REASON, ScrollTranscriptArguments, ScrollTranscriptCommand,
    ScrollTranscriptResult, SwitchThreadArguments, SwitchThreadResult, SwitchWorkspaceArguments,
    SwitchWorkspaceResult, TranscriptScrollPositionDiagnostic, TranscriptUiState, TurnUiState,
    UiRangeDiagnostic, UiStateSnapshot, VisibleTranscriptRowDiagnostic, bounded_control_string,
    close_popups_tool_response, gui_control_failure_response, is_beryl_gui_control_dynamic_tool,
    parse_beryl_gui_control_dynamic_tool_request, parse_gui_control_tool_request,
    scroll_transcript_tool_response, switch_thread_tool_response, ui_state_tool_response,
};
use crate::member_thread_inventory::{
    MemberThreadInventoryMemberKey, MemberThreadInventoryMemberKind, MemberThreadInventoryState,
    resolved_thread_title,
};
use crate::memory_diagnostics::{self, MemoryMilestone, RetainedStateSnapshot};
use crate::text_input::{
    SharedTextInputCopy, SharedTextInputCut, SharedTextInputEnter, SharedTextInputPaste,
    SingleLineInput, TextInputAtomClipboardPolicy, TextInputEnterKey, TextInputEvent,
    TextInputOptions, TextInputRetainedCounts, TextInputRichPastePolicy, TextInputSelectionAtom,
    TextInputSelectionExport,
};
use crate::{AppBootstrap, WorkspaceActivityPanelMode, WorkspaceGraphRevision, WorkspaceUiState};

const COMPOSER_KEY_CONTEXT: &str = "ConversationComposer";
const APP_SHUTDOWN_OPEN_WORKER_GRACE_TIMEOUT: Duration = Duration::from_secs(5);
const APP_SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(25);
const BACKEND_LIVENESS_POLL_INTERVAL: Duration = Duration::from_millis(500);
const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(16);
const READY_IDLE_POLL_INTERVAL: Duration = Duration::from_millis(250);
const SHELL_WORKER_POLL_MAX_EVENTS_PER_FRAME: usize = 64;
const SHELL_WORKER_POLL_MAX_FRAME_TIME: Duration = Duration::from_millis(4);
const TURN_UPDATE_POLL_MAX_EVENTS_PER_FRAME: usize = 64;
const TURN_UPDATE_POLL_MAX_FRAME_TIME: Duration = Duration::from_millis(4);
const SHORT_TEXT_INPUT_UNDO_BYTE_LIMIT: usize = 256 * 1024;
const COMPOSER_TEXT_INPUT_UNDO_BYTE_LIMIT: usize = 2 * 1024 * 1024;
const PENDING_NEW_THREAD_LABEL_SCOPE_BINDINGS_MAX: usize = 64;
const KNOWN_THREADS_MAX: usize = 2048;
const KNOWN_THREADS_PAYLOAD_BYTE_LIMIT: usize = 4 * 1024 * 1024;
const KNOWN_THREAD_PREVIEW_MAX_BYTES: usize = 4 * 1024;
const KNOWN_THREAD_NAME_MAX_BYTES: usize = 512;
const KNOWN_THREAD_AGENT_NICKNAME_MAX_BYTES: usize = 512;
const KNOWN_THREAD_MODEL_PROVIDER_MAX_BYTES: usize = 256;

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn runtime_target_diagnostic(target: &WorkspaceId) -> RuntimeTargetDiagnostic {
    RuntimeTargetDiagnostic {
        runtime: bounded_diagnostic_string(target.runtime_mode().display_name()),
        canonical_path: bounded_diagnostic_string(target.canonical_path().display().to_string()),
        display_label: bounded_diagnostic_string(target.display_label()),
    }
}

fn known_thread_payload_bytes(threads: &[ThreadSummary]) -> usize {
    threads.iter().map(known_thread_summary_payload_bytes).sum()
}

fn known_thread_summary_payload_bytes(thread: &ThreadSummary) -> usize {
    thread
        .id
        .len()
        .saturating_add(thread.forked_from_id.as_ref().map_or(0, String::len))
        .saturating_add(thread.cwd.to_string_lossy().len())
        .saturating_add(thread.preview.len())
        .saturating_add(thread.name.as_ref().map_or(0, String::len))
        .saturating_add(thread.agent_nickname.as_ref().map_or(0, String::len))
        .saturating_add(
            thread
                .path
                .as_ref()
                .map_or(0, |path| path.to_string_lossy().len()),
        )
        .saturating_add(thread.model_provider.len())
}

fn bounded_known_thread_summary(mut thread: ThreadSummary) -> ThreadSummary {
    truncate_string_to_byte_limit(&mut thread.preview, KNOWN_THREAD_PREVIEW_MAX_BYTES);
    if let Some(name) = thread.name.as_mut() {
        truncate_string_to_byte_limit(name, KNOWN_THREAD_NAME_MAX_BYTES);
    }
    if let Some(agent_nickname) = thread.agent_nickname.as_mut() {
        truncate_string_to_byte_limit(agent_nickname, KNOWN_THREAD_AGENT_NICKNAME_MAX_BYTES);
    }
    truncate_string_to_byte_limit(
        &mut thread.model_provider,
        KNOWN_THREAD_MODEL_PROVIDER_MAX_BYTES,
    );
    thread
}

fn truncate_string_to_byte_limit(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    if max_bytes == 0 {
        text.clear();
        return;
    }

    const SUFFIX: &str = "...";
    let suffix = if max_bytes >= SUFFIX.len() {
        SUFFIX
    } else {
        ""
    };
    let mut end = max_bytes.saturating_sub(suffix.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str(suffix);
}

fn bounded_known_threads(
    known_threads: Vec<ThreadSummary>,
    pinned_thread_ids: impl IntoIterator<Item = String>,
) -> Vec<ThreadSummary> {
    let pinned_thread_ids = pinned_thread_ids.into_iter().collect::<HashSet<_>>();
    let mut retained = Vec::new();
    let mut retained_bytes = 0usize;

    for thread in known_threads.into_iter().map(bounded_known_thread_summary) {
        let thread_bytes = known_thread_summary_payload_bytes(&thread);
        let pinned = pinned_thread_ids.contains(&thread.id);
        let within_budget = retained.len() < KNOWN_THREADS_MAX
            && retained_bytes.saturating_add(thread_bytes) <= KNOWN_THREADS_PAYLOAD_BYTE_LIMIT;
        if pinned || within_budget {
            retained_bytes = retained_bytes.saturating_add(thread_bytes);
            retained.push(thread);
        }
    }

    retained
}

fn clamp_ui_range(range: Range<usize>, item_count: usize) -> Range<usize> {
    let start = range.start.min(item_count);
    let end = range.end.min(item_count).max(start);
    start..end
}

fn ui_range_diagnostic(range: &Range<usize>) -> UiRangeDiagnostic {
    UiRangeDiagnostic {
        start: range.start,
        end: range.end,
    }
}

fn transcript_scroll_position_diagnostic(
    position: ListScrollPosition,
) -> TranscriptScrollPositionDiagnostic {
    match position {
        ListScrollPosition::Bottom => TranscriptScrollPositionDiagnostic {
            kind: "bottom".to_string(),
            item_index: None,
            offset_px: None,
        },
        ListScrollPosition::Content(offset) => TranscriptScrollPositionDiagnostic {
            kind: "content".to_string(),
            item_index: Some(offset.item_ix),
            offset_px: Some(f64::from(f32::from(offset.offset_in_item))),
        },
        ListScrollPosition::VirtualTail {
            offset_from_content_end,
        } => TranscriptScrollPositionDiagnostic {
            kind: "virtual_tail".to_string(),
            item_index: None,
            offset_px: Some(f64::from(f32::from(offset_from_content_end))),
        },
    }
}

fn selected_thread_state_diagnostic_label(surface: &ConversationSurfaceState) -> &'static str {
    if surface.selected_thread_id().is_none() {
        return "pending_new_thread";
    }
    if surface.pending_thread_activation_label().is_some() {
        return "pending_activation";
    }
    if surface.selected_thread_context_compaction_id().is_some() {
        return "compacting";
    }
    if surface.execution_details.working_turn_index().is_some() {
        return "working";
    }

    match surface.selected_thread_status.as_ref() {
        Some(ThreadStatus::Idle) => "idle",
        Some(ThreadStatus::Active { .. }) => "working",
        Some(ThreadStatus::NotLoaded) => "not_loaded",
        Some(ThreadStatus::SystemError) => "system_error",
        None => "unknown",
    }
}

fn thread_status_diagnostic_label(status: &ThreadStatus) -> &'static str {
    match status {
        ThreadStatus::NotLoaded => "not_loaded",
        ThreadStatus::Idle => "idle",
        ThreadStatus::SystemError => "system_error",
        ThreadStatus::Active { .. } if status.waiting_on_user_input() => "waiting_on_user_input",
        ThreadStatus::Active { .. } => "active",
    }
}

fn cancellable_turn_ui_state(turn: &CancellableActiveTurn) -> CancellableTurnUiState {
    let kind = match turn.kind {
        CancellableActiveTurnKind::Ordinary => "ordinary",
        CancellableActiveTurnKind::ContextCompaction => "context_compaction",
    };
    CancellableTurnUiState {
        thread_id: bounded_control_string(turn.thread_id.clone()),
        turn_id: bounded_control_string(turn.turn_id.clone()),
        kind: kind.to_string(),
    }
}

fn diagnostic_expected_turn_mismatch(
    expected: &DiagnosticStopTurnArguments,
    current: Option<&CancellableActiveTurn>,
) -> Option<String> {
    let current = current?;
    (!expected.matches(&current.thread_id, &current.turn_id)).then(|| {
        format!(
            "Expected selected child turn {}/{} but current selected child turn is {}/{}.",
            expected.expected_thread_id,
            expected.expected_turn_id,
            current.thread_id,
            current.turn_id
        )
    })
}

fn visible_transcript_rows(
    surface: &ConversationSurfaceState,
    visible_range: Range<usize>,
    limit: usize,
) -> (Vec<VisibleTranscriptRowDiagnostic>, bool) {
    let mut rows = Vec::new();
    for index in visible_range.clone().take(limit) {
        if let Some(row) = surface.transcript_presentation().turn_at(index) {
            rows.push(VisibleTranscriptRowDiagnostic {
                row_index: row.index,
                row_identity: bounded_control_string(row.identity.as_str().to_string()),
                source_turn_index: row.source_turn_index,
                item_count: row.turn.item_count(),
                text_chars: row.turn.text_char_count(),
                released_history_placeholder: row.turn.is_released_history_placeholder(),
            });
        }
    }
    let truncated = visible_range.len() > rows.len();
    (rows, truncated)
}

fn workspace_picker_action_keyboard_activation_key(key: &str) -> bool {
    matches!(key, "enter" | "space" | " ")
}

gpui::actions!(
    beryl_shell,
    [
        SubmitComposer,
        JumpTranscriptTurnUp,
        JumpTranscriptTurnDown,
        BrowseComposerHistoryPrevious,
        BrowseComposerHistoryNext
    ]
);

mod account_rate_limits;
mod checklist_sidebar_panel_state;
mod checklist_sidebar_projection;
mod checklist_sidebar_visibility;
mod checklist_thread_menu;
mod column_selector;
mod composer_clipboard;
mod composer_draft;
mod composer_history;
mod composer_image_assets;
mod composer_image_delivery;
mod composer_image_label_scan;
mod composer_image_labels;
mod composer_submission;
mod composer_submit;
mod context_compaction;
mod discovery;
mod execution_detail;
mod graph;
mod graph_link_menu;
mod graph_link_menu_state;
mod graph_node_action_policy;
mod graph_node_delete;
mod graph_thread_start;
mod graph_worker;
mod hard_stop;
mod hard_stop_targets;
mod image_preview_popup;
mod layout;
mod lifecycle;
mod lifecycle_continuation;
mod lifecycle_yield;
mod member_thread_inventory;
mod notification_policy;
mod notification_policy_adapter;
mod notifications;
mod pending_turn_input;
mod platform_attention;
mod render;
mod semantic_thread_start;
mod settings;
mod status_line;
mod status_operation;
mod status_operation_state;
mod surface_notice;
mod thread_activation;
mod thread_history_worker;
mod thread_selection;
mod thread_selector;
mod thread_title;
mod token_usage_snapshot;
mod tool_activity;
mod tool_activity_nickname;
mod transcript_anchor;
mod transcript_branch_core;
mod transcript_branch_menu;
mod transcript_branch_menu_state;
mod transcript_branch_worker;
mod transcript_edit_commit;
mod transcript_edit_commit_worker;
mod transcript_edit_menu_state;
mod transcript_edit_mode;
mod transcript_edit_mode_state;
mod transcript_history;
mod transcript_image_menu_actions;
mod transcript_image_preview;
mod transcript_image_sources;
#[allow(dead_code)]
mod transcript_markdown;
#[allow(dead_code)]
mod transcript_media;
mod transcript_media_runs;
mod transcript_presentation;
mod transcript_projection;
#[allow(dead_code)]
mod transcript_quote;
mod transcript_quote_popup;
mod transcript_scroll;
#[allow(dead_code)]
mod transcript_selection;
mod transcript_stream_invalidation;
mod turn_steering;
mod turn_stop;
mod turn_worker;
#[allow(dead_code)]
mod virtual_list;
mod workspace_members;
mod workspace_persistence_worker;
mod workspace_picker;
mod workspace_picker_actions;
mod workspace_rename_policy;
mod workspace_title;

use account_rate_limits::{
    AccountRateLimitsOutcome, AccountRateLimitsUpdate, spawn_account_rate_limits_worker,
};
use checklist_sidebar_projection::{
    ChecklistSidebarProjection, ChecklistSidebarProjectionCache, ChecklistSidebarRow,
};
use checklist_sidebar_visibility::ChecklistSidebarVisibilityState;
use checklist_thread_menu::ChecklistThreadStartMenuState;
use column_selector::{
    ColumnSelectorKeyboardIntent, ColumnSelectorScrollState, ColumnSelectorSurface,
};
use composer_clipboard::{
    ComposerClipboardAtom, ComposerClipboardImage, ComposerClipboardLabelScope,
    ComposerClipboardPastePlan, ComposerClipboardPayload, ComposerClipboardPayloadError,
    ComposerClipboardStore,
};
use composer_draft::{
    AcceptedComposerDraft, ComposerDraft, ComposerDraftImageAdmissionError, ComposerDraftImageAtom,
    ComposerDraftImageData, composer_image_copy_text, composer_image_label_from_atom_id,
    composer_image_marker, first_clipboard_image,
};
use composer_history::{ComposerHistoryBrowseResult, ComposerHistoryScope, ComposerHistoryState};
use composer_image_assets::{ComposerImageAssetUpdate, spawn_composer_image_asset_worker};
use composer_image_delivery::{
    ComposerImageDeliveryUpdate, PreparedComposerDraft, spawn_composer_image_delivery_worker,
};
use composer_image_label_scan::{
    ComposerImageLabelScanOutcome, ComposerImageLabelScanUpdate,
    spawn_composer_image_label_scan_worker,
};
use composer_image_labels::{ComposerImageLabelState, ComposerImagePasteReadiness};
use composer_submission::prepared_composer_draft_fragment;
use composer_submit::accepted_composer_draft;
use discovery::WorkspaceOpenCancellation;
use execution_detail::{
    ActiveTurnIdentity, ExecutionDetailState, ExecutionItem, TranscriptImagePathResolver,
    TurnExecutionStatus, UserInputFragment,
};
use graph::{
    GraphColumnKey, GraphCommitApplication, GraphMutationCommitUpdate, GraphMutationUpdate,
    GraphOptimisticMutation, GraphOverlayState, OptimisticGraphMutationId,
};
use graph_thread_start::GraphThreadStartTask;
use graph_worker::{
    GraphReloadUpdate, GraphUpdate, GraphWorkerTask, spawn_graph_reload_worker,
    spawn_thread_ref_link_worker,
};
use hard_stop::HardStopUpdate;
use hard_stop_targets::HardStopTargetProjection;
use lifecycle_continuation::{
    PhaseContinueRequest, pending_turn_queue_should_wait_for_compaction, phase_continue_request,
};
use lifecycle_yield::{LifecycleYieldState, TerminalLifecycleYield};
use member_thread_inventory::MemberThreadInventoryUpdate;
use notification_policy::{
    BerylWindowFocusState, NotificationCandidateKind, NotificationPlaybackRequest,
    NotificationPolicyDecision,
};
use notification_policy_adapter::terminal_parent_turn_notification_decision;
use notifications::{
    LifecycleNotificationCandidate, LifecycleNotificationKind, NotificationSoundPlayer,
    TurnCompletionSoundCandidate,
};
use pending_turn_input::{
    PendingActiveTurnSteeringQueue, PendingActiveTurnSteeringSubmissionPlan, PendingTurnInputQueue,
    PendingTurnInputSubmissionPlan,
};
use platform_attention::PlatformAttentionMonitor;
use settings::{SharedAppearanceSettings, SharedGuiPreferences};
use status_line::{
    CancellableActiveTurn, CancellableActiveTurnKind, StatusLineState, ThreadTurnDefaults,
};
use status_operation::{StatusOperationUpdate, spawn_context_compaction_worker};
use status_operation_state::{StatusLineOperationState, StatusModelListCache};
use surface_notice::{
    SurfaceNotice, SurfaceNoticeQueue, local_turn_failure_notice,
    selected_backend_turn_error_notice,
};
use thread_history_worker::{
    ThreadHistoryPageOutcome, ThreadHistoryPageUpdate, spawn_older_thread_history_page_worker,
};
use thread_selection::{
    ThreadSelectionRequest, exact_thread_selection_request, graph_thread_ref_availability,
};
use thread_selector::{
    ThreadSelectorActivationTarget, ThreadSelectorColumnKey, ThreadSelectorState,
};
use thread_title::{
    ThreadTitleCancellation, ThreadTitleResult, ThreadTitleTask, ThreadTitleTaskOutcome,
    spawn_thread_title_worker,
};
use tool_activity::ToolActivityProjection;
use tool_activity_nickname::{
    ToolActivityNicknameOutcome, ToolActivityNicknamePoll, ToolActivityNicknameResolutionTarget,
    ToolActivityNicknameResolver,
};
use transcript_anchor::{
    TranscriptSubmitAnchor, TranscriptSubmitAnchorSnapshot, release_forced_submit_anchor,
    transcript_list_item_count,
};
use transcript_branch_worker::TranscriptBranchUpdate;
use transcript_edit_commit_worker::TranscriptEditCommitUpdate;
use transcript_history::{
    LoadedTranscriptHistoryPage, TranscriptHistoryPageRequest, TranscriptHistoryWindow,
};
use transcript_presentation::{TranscriptActivityCaret, TranscriptPresentationState};
use transcript_scroll::{
    LiveTranscriptRows, TranscriptTurnJumpDirection, sync_live_transcript_rows,
    transcript_turn_jump_target,
};
use transcript_stream_invalidation::TranscriptStreamInvalidations;
use turn_steering::{
    SteeringInputFragment, TurnSteeringOutcome, TurnSteeringUpdate, spawn_turn_steering_worker,
};
use turn_stop::TurnStopUpdate;
use turn_worker::{
    AcceptedLifecycleYield, ShellDynamicToolRequest, ThreadActivationUpdate, TurnWorkerOutcome,
    TurnWorkerUpdate, shell_dynamic_tool_request_channel, spawn_thread_activation_worker,
    spawn_turn_worker,
};
use virtual_list::{ListAlignment, ListOffset, ListScrollEvent, ListScrollPosition, ListState};
use workspace_members::{
    WorkspaceMemberAttachRequest, apply_primary_execution_target_selection,
    apply_workspace_member_attachment, apply_workspace_member_detach,
    apply_workspace_member_primary_selection, reconcile_workspace_member_availability,
    resolve_new_thread_execution_target, resolve_workspace_member_attach_request,
};
use workspace_persistence_worker::{
    WorkspacePersistenceFlush, WorkspacePersistenceQueue, spawn_workspace_persistence_worker,
};
use workspace_picker_actions::{
    WorkspacePickerActionUpdate, WorkspacePickerDeletionOutcome, WorkspacePickerOpenedWorkspace,
    spawn_create_workspace_for_target_worker, spawn_create_workspace_worker,
    spawn_delete_workspace_worker, spawn_switch_workspace_worker,
};
use workspace_rename_policy::{WorkspaceRenameBlockers, workspace_rename_disabled_reason};
use workspace_title::{
    WorkspaceTitleCandidate, WorkspaceTitleResult, WorkspaceTitleUpdate,
    spawn_workspace_manual_title_worker, spawn_workspace_title_worker,
};

#[derive(Clone)]
struct ConfiguredAppState {
    home_dir: crate::BerylHomeDir,
    startup_persistence: crate::StartupPersistence,
    workspace_persistence: crate::BerylWorkspacePersistence,
}

impl ConfiguredAppState {
    fn new(home_dir: crate::BerylHomeDir) -> Self {
        Self {
            startup_persistence: home_dir.startup_persistence(),
            workspace_persistence: home_dir.workspace_persistence(),
            home_dir,
        }
    }

    fn home_display(&self) -> String {
        self.home_dir.root_dir().display().to_string()
    }
}

pub(crate) fn run_app(bootstrap: AppBootstrap) {
    run_app_with_diagnostic_target(bootstrap, None);
}

pub(crate) fn run_diagnostic_target_stdio(bootstrap: AppBootstrap) {
    let diagnostic_target_receiver = Some(spawn_diagnostic_target_stdio_server());
    run_app_with_diagnostic_target(bootstrap, diagnostic_target_receiver);
}

fn run_app_with_diagnostic_target(
    bootstrap: AppBootstrap,
    mut diagnostic_target_receiver: Option<Receiver<DiagnosticTargetShellRequest>>,
) {
    memory_diagnostics::configure(bootstrap.memory_milestones_enabled());
    MemoryMilestone::new("app_startup").log();

    let initial_title = bootstrap.window_title();
    let app_state = bootstrap
        .beryl_home_dir()
        .map(ConfiguredAppState::new)
        .map_err(|error| error.to_string());
    MemoryMilestone::new("app_state_resolved").log();

    let application = Application::new();
    MemoryMilestone::new("gpui_application_created").log();

    application.run(move |cx: &mut App| {
        MemoryMilestone::new("gpui_run_closure_start").log();
        crate::text_input::bind_keys(cx);
        render::transcript::bind_keys(cx);
        cx.bind_keys([
            KeyBinding::new("enter", SubmitComposer, Some(COMPOSER_KEY_CONTEXT)),
            KeyBinding::new("ctrl-up", JumpTranscriptTurnUp, Some(COMPOSER_KEY_CONTEXT)),
            KeyBinding::new(
                "ctrl-down",
                JumpTranscriptTurnDown,
                Some(COMPOSER_KEY_CONTEXT),
            ),
            KeyBinding::new(
                "alt-up",
                BrowseComposerHistoryPrevious,
                Some(COMPOSER_KEY_CONTEXT),
            ),
            KeyBinding::new(
                "alt-down",
                BrowseComposerHistoryNext,
                Some(COMPOSER_KEY_CONTEXT),
            ),
        ]);
        let bounds = Bounds::centered(None, size(px(1040.0), px(760.0)), cx);
        let appearance_store = app_state
            .as_ref()
            .ok()
            .map(|state| state.home_dir.appearance_settings_store());
        let preferences_store = app_state
            .as_ref()
            .ok()
            .map(|state| state.home_dir.gui_preferences_store());
        let appearance_settings = std::sync::Arc::new(std::sync::Mutex::new(
            appearance_store
                .as_ref()
                .map(settings::load_initial_appearance_settings)
                .unwrap_or_default(),
        ));
        let gui_preferences = std::sync::Arc::new(std::sync::Mutex::new(
            preferences_store
                .as_ref()
                .map(settings::load_initial_gui_preferences)
                .unwrap_or_default(),
        ));
        let settings_state = match (appearance_store, preferences_store) {
            (Some(appearance_store), Some(preferences_store)) => {
                settings::SettingsState::new_with_stores(
                    appearance_settings.clone(),
                    appearance_store,
                    gui_preferences.clone(),
                    preferences_store,
                )
            }
            _ => settings::SettingsState::new_without_stores(
                appearance_settings.clone(),
                gui_preferences.clone(),
                app_state
                    .as_ref()
                    .err()
                    .map(|error| format!("Beryl home directory is unavailable: {error}"))
                    .unwrap_or_else(|| {
                        "Beryl settings storage is unavailable for the configured home directory."
                            .to_string()
                    }),
            ),
        };
        let settings_window = open_settings_window(
            cx,
            settings_state.model(),
            settings_state.window_options(),
            SettingsWindowOpenDisposition::Hidden,
        )
        .expect("beryl settings window should open");
        MemoryMilestone::new("settings_window_created").log();

        MemoryMilestone::new("main_window_open_start").log();
        let diagnostic_target_receiver = diagnostic_target_receiver.take();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(
                    px(layout::WINDOW_MIN_WIDTH),
                    px(layout::WINDOW_MIN_HEIGHT),
                )),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(initial_title.clone().into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |window, cx| {
                let shell = cx.new(|cx| {
                    ShellView::new(
                        window,
                        bootstrap.clone(),
                        app_state.clone(),
                        settings_window,
                        settings_state,
                        appearance_settings.clone(),
                        gui_preferences.clone(),
                        diagnostic_target_receiver,
                        cx,
                    )
                });
                let weak_shell = shell.downgrade();
                window.on_window_should_close(cx, move |window, cx| {
                    let _ = weak_shell.update(cx, |shell, cx| {
                        shell.begin_application_shutdown(window, cx);
                    });
                    false
                });
                shell
            },
        )
        .expect("beryl shell window should open");
        MemoryMilestone::new("main_window_opened").log();
        cx.activate(true);
        MemoryMilestone::new("app_activated").log();
    });
}

struct ShellView {
    bootstrap: AppBootstrap,
    app_state: Result<ConfiguredAppState, String>,
    settings_window: SettingsWindowHandle,
    settings_state: settings::SettingsState,
    notification_sound_path_prompt: NotificationSoundPathPromptState,
    notification_sound_player: NotificationSoundPlayer,
    platform_attention_monitor: PlatformAttentionMonitor,
    appearance_settings: SharedAppearanceSettings,
    #[allow(dead_code)]
    gui_preferences: SharedGuiPreferences,
    state: ShellState,
    backend_servers: HashMap<WorkspaceId, ManagedBackendServer>,
    workspace_open_cancellation: Option<WorkspaceOpenCancellation>,
    discovery_receiver: Option<Receiver<DiscoveryUpdate>>,
    workspace_receiver: Option<Receiver<WorkspaceUpdate>>,
    graph_receiver: Option<GraphWorkerTask>,
    graph_thread_start_receiver: Option<GraphThreadStartTask>,
    transcript_branch_receiver: Option<Receiver<TranscriptBranchUpdate>>,
    transcript_edit_commit_receiver: Option<Receiver<TranscriptEditCommitUpdate>>,
    member_thread_inventory_receiver: Option<Receiver<MemberThreadInventoryUpdate>>,
    thread_activation_receiver: Option<Receiver<ThreadActivationUpdate>>,
    thread_history_page_receiver: Option<Receiver<ThreadHistoryPageUpdate>>,
    composer_image_label_scan_receiver: Option<Receiver<ComposerImageLabelScanUpdate>>,
    composer_image_asset_receiver: Option<Receiver<ComposerImageAssetUpdate>>,
    turn_receiver: Option<Receiver<TurnWorkerUpdate>>,
    shell_tool_receiver: Option<Receiver<ShellDynamicToolRequest>>,
    diagnostic_target_receiver: Option<Receiver<DiagnosticTargetShellRequest>>,
    diagnostic_child_supervisor: Arc<Mutex<DiagnosticChildSupervisor>>,
    transcript_edit_replacement_turn: Option<TranscriptEditReplacementTurnState>,
    turn_steering_receivers: Vec<TurnSteeringTask>,
    composer_image_delivery_receiver: Option<Receiver<ComposerImageDeliveryUpdate>>,
    thread_title_receivers: Vec<ThreadTitleTask>,
    status_operation_receiver: Option<Receiver<StatusOperationUpdate>>,
    pending_lifecycle_phase_continue: Option<PhaseContinueRequest>,
    account_rate_limits_receiver: Option<Receiver<AccountRateLimitsUpdate>>,
    turn_stop_receiver: Option<Receiver<TurnStopUpdate>>,
    hard_stop_receiver: Option<Receiver<HardStopUpdate>>,
    tool_activity_nickname_resolver: ToolActivityNicknameResolver,
    workspace_picker_action_receiver: Option<Receiver<WorkspacePickerActionUpdate>>,
    workspace_runtime_selector_distro_receiver:
        Option<Receiver<WorkspaceRuntimeSelectorDistroUpdate>>,
    workspace_title_receiver: Option<Receiver<WorkspaceTitleUpdate>>,
    application_shutdown_receiver: Option<Receiver<ApplicationShutdownUpdate>>,
    workspace_persistence_queue: WorkspacePersistenceQueue,
    workspace_member_attach_pending_workspace_id: Option<BerylWorkspaceId>,
    pending_workspace_title_candidate: Option<WorkspaceTitleCandidate>,
    workspace_persistence_pending_last_poll: bool,
    status_model_cache: StatusModelListCache,
    last_backend_liveness_poll_at: Option<Instant>,
    frame_poll_scheduled: bool,
    ready_idle_poll_scheduled: bool,
    host_path_input: Entity<SingleLineInput>,
    wsl_distro_input: Entity<SingleLineInput>,
    wsl_path_input: Entity<SingleLineInput>,
    workspace_picker_filter_input: Entity<SingleLineInput>,
    workspace_rename_input: Entity<SingleLineInput>,
    conversation_input: Entity<SingleLineInput>,
    surface_notice_text_input: Entity<SingleLineInput>,
    surface_notice_text_input_notice_id: Cell<Option<u64>>,
    composer_draft: ComposerDraft,
    composer_clipboard: ComposerClipboardStore,
    pending_composer_image_asset_paste: Option<PendingComposerImageAssetPaste>,
    composer_image_popup: Option<ComposerImagePopupState>,
    transcript_panel: Entity<render::transcript::TranscriptPanel>,
    checklist_sidebar_panel: Entity<render::checklist_sidebar::ChecklistSidebarPanel>,
    startup_scroll_handle: ScrollHandle,
    scrollbar_activity: HashMap<ScrollbarRegion, ScrollbarActivity>,
    next_attempt: u32,
}

#[derive(Clone)]
struct ComposerImagePopupState {
    atom_id: String,
    label: String,
    position: Point<Pixels>,
    bounds: Option<Bounds<Pixels>>,
    mode: ComposerImagePopupMode,
    preview_image: Option<Arc<Image>>,
    preview_image_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingComposerImageAssetPaste {
    workspace_id: BerylWorkspaceId,
    display_text_snapshot: String,
    replacement_range: Range<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TextInputRetainedAggregate {
    count: usize,
    current_text_bytes: usize,
    current_atom_count: usize,
    current_atom_bytes: usize,
    undo_snapshot_count: usize,
    redo_snapshot_count: usize,
    undo_bytes: usize,
    redo_bytes: usize,
    widget_layout_lines: usize,
    widget_visual_lines: usize,
    widget_visible_text_bytes: usize,
}

impl TextInputRetainedAggregate {
    fn add(&mut self, counts: TextInputRetainedCounts) {
        self.count = self.count.saturating_add(1);
        self.current_text_bytes = self
            .current_text_bytes
            .saturating_add(counts.current_text_bytes);
        self.current_atom_count = self
            .current_atom_count
            .saturating_add(counts.current_atom_count);
        self.current_atom_bytes = self
            .current_atom_bytes
            .saturating_add(counts.current_atom_id_bytes)
            .saturating_add(counts.current_atom_display_bytes)
            .saturating_add(counts.current_atom_copy_text_bytes);
        self.undo_snapshot_count = self
            .undo_snapshot_count
            .saturating_add(counts.undo_snapshot_count);
        self.redo_snapshot_count = self
            .redo_snapshot_count
            .saturating_add(counts.redo_snapshot_count);
        self.undo_bytes = self
            .undo_bytes
            .saturating_add(counts.undo_text_bytes)
            .saturating_add(counts.undo_atom_bytes);
        self.redo_bytes = self
            .redo_bytes
            .saturating_add(counts.redo_text_bytes)
            .saturating_add(counts.redo_atom_bytes);
        self.widget_layout_lines = self
            .widget_layout_lines
            .saturating_add(counts.widget_layout_line_count.unwrap_or_default());
        self.widget_visual_lines = self
            .widget_visual_lines
            .saturating_add(counts.widget_visual_line_count.unwrap_or_default());
        self.widget_visible_text_bytes = self
            .widget_visible_text_bytes
            .saturating_add(counts.widget_visible_text_bytes.unwrap_or_default());
    }

    fn payload_bytes_lower_bound(self) -> usize {
        self.current_text_bytes
            .saturating_add(self.current_atom_bytes)
            .saturating_add(self.undo_bytes)
            .saturating_add(self.redo_bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComposerImagePopupMode {
    Menu,
    Preview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComposerHistoryDirection {
    Previous,
    Next,
}

struct TurnSteeringTask {
    thread_id: String,
    fragments: Vec<SteeringInputFragment>,
    receiver: Receiver<TurnSteeringUpdate>,
}

const MAX_CONCURRENT_TURN_STEERING_TASKS: usize = 4;

fn validate_pending_active_turn_steering_first_fragment(
    fragment: &SteeringInputFragment,
) -> Result<(), pending_turn_input::PendingInputAdmissionError> {
    let retained_bytes = fragment.retained_payload_bytes_lower_bound();
    if retained_bytes > pending_turn_input::PENDING_ACTIVE_TURN_STEERING_MAX_PAYLOAD_BYTES {
        return Err(
            pending_turn_input::PendingInputAdmissionError::TooManyBytes {
                max_bytes: pending_turn_input::PENDING_ACTIVE_TURN_STEERING_MAX_PAYLOAD_BYTES,
            },
        );
    }
    Ok(())
}

impl Drop for ShellView {
    fn drop(&mut self) {
        self.cancel_thread_title_workers();
        self.cancel_workspace_open();
    }
}

fn spawn_managed_backend_shutdown(server: ManagedBackendServer, reason: &'static str) {
    thread::spawn(move || {
        if let Err(error) = shutdown_managed_backend_server(server, reason) {
            warn!(
                reason,
                error = %error,
                "managed backend shutdown failed"
            );
        }
    });
}

fn shutdown_managed_backend_server(
    mut server: ManagedBackendServer,
    reason: &'static str,
) -> Result<(), String> {
    server
        .shutdown()
        .map_err(|error| format!("{reason}: {error}"))
}

fn shutdown_queued_workspace_open_result(
    receiver: Receiver<WorkspaceUpdate>,
    reason: &'static str,
) {
    loop {
        match receiver.try_recv() {
            Ok(WorkspaceUpdate::Finished(Ok(opened))) => {
                let OpenedWorkspace { server, .. } = opened;
                spawn_managed_backend_shutdown(server, reason);
                break;
            }
            Ok(WorkspaceUpdate::Finished(Err(_))) => break,
            Ok(
                WorkspaceUpdate::Detail(_)
                | WorkspaceUpdate::ResolvedExecutionTarget(_)
                | WorkspaceUpdate::Progress(_),
            ) => {}
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }
}

fn spawn_application_shutdown_worker(
    active_servers: Vec<ManagedBackendServer>,
    workspace_receiver: Option<Receiver<WorkspaceUpdate>>,
    workspace_persistence_flush: WorkspacePersistenceFlush,
    pending_open_timeout: Duration,
) -> Receiver<ApplicationShutdownUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut errors = Vec::new();
        for server in active_servers {
            if let Err(error) = shutdown_managed_backend_server(server, "application shutdown") {
                errors.push(error);
            }
        }

        if let Some(receiver) = workspace_receiver
            && let Err(error) =
                wait_for_pending_workspace_open_shutdown(receiver, pending_open_timeout)
        {
            errors.push(error);
        }

        if let Err(error) = workspace_persistence_flush.wait(pending_open_timeout) {
            errors.push(error);
        }

        let result = if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        };
        let _ = sender.send(ApplicationShutdownUpdate::Finished(result));
    });
    receiver
}

fn wait_for_pending_workspace_open_shutdown(
    receiver: Receiver<WorkspaceUpdate>,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(format!(
                "timed out after {timeout:?} waiting for pending workspace open shutdown"
            ));
        }

        let wait_for = (deadline - now).min(APP_SHUTDOWN_POLL_INTERVAL);
        match receiver.recv_timeout(wait_for) {
            Ok(WorkspaceUpdate::Finished(Ok(opened))) => {
                let OpenedWorkspace { server, .. } = opened;
                return shutdown_managed_backend_server(
                    server,
                    "application shutdown after pending workspace open",
                );
            }
            Ok(WorkspaceUpdate::Finished(Err(_))) => return Ok(()),
            Ok(
                WorkspaceUpdate::Detail(_)
                | WorkspaceUpdate::ResolvedExecutionTarget(_)
                | WorkspaceUpdate::Progress(_),
            ) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ScrollbarRegion {
    Startup,
    WorkspacePicker,
    WorkspaceMembers,
    Transcript,
    ToolActivity,
    Composer,
    GraphColumns,
    GraphColumn(GraphColumnKey),
    ThreadSelectorColumns,
    ThreadSelectorColumn(ThreadSelectorColumnKey),
}

struct ScrollbarActivity {
    generation: u64,
    last_activity_at: Option<Instant>,
    transition: Option<ScrollbarTransition>,
    animation_task: Option<Task<()>>,
}

struct ScrollbarTransition {
    started_at: Instant,
    from_opacity: f32,
    to_opacity: f32,
}

impl Default for ScrollbarActivity {
    fn default() -> Self {
        Self {
            generation: 0,
            last_activity_at: None,
            transition: None,
            animation_task: None,
        }
    }
}

impl ScrollbarActivity {
    fn record_activity(&mut self) -> u64 {
        let now = Instant::now();
        let current_opacity = self.opacity(now);
        self.generation = self.generation.saturating_add(1);
        self.last_activity_at = Some(now);
        self.transition = if current_opacity >= (1.0 - f32::EPSILON) {
            None
        } else {
            Some(ScrollbarTransition {
                started_at: now,
                from_opacity: current_opacity,
                to_opacity: 1.0,
            })
        };
        self.animation_task = None;
        self.generation
    }

    fn opacity(&self, now: Instant) -> f32 {
        if let Some(transition) = &self.transition {
            transition.opacity(now)
        } else if self.last_activity_at.is_some() {
            1.0
        } else {
            0.0
        }
    }

    fn is_animating(&self, now: Instant) -> bool {
        self.transition
            .as_ref()
            .is_some_and(|transition| transition.is_active(now))
    }
}

impl ScrollbarTransition {
    fn duration(&self) -> Duration {
        let delta = (self.to_opacity - self.from_opacity).abs();
        if delta <= f32::EPSILON {
            return Duration::ZERO;
        }

        let duration = SCROLLBAR_FADE_DURATION.mul_f32(delta);
        if duration.is_zero() {
            Duration::from_millis(1)
        } else {
            duration
        }
    }

    fn progress(&self, now: Instant) -> f32 {
        let duration = self.duration();
        if duration.is_zero() {
            return 1.0;
        }

        let elapsed = now.saturating_duration_since(self.started_at);
        (elapsed.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    fn opacity(&self, now: Instant) -> f32 {
        let progress = self.progress(now);
        let eased_progress = progress * progress * (3.0 - (2.0 * progress));
        self.from_opacity + ((self.to_opacity - self.from_opacity) * eased_progress)
    }

    fn is_active(&self, now: Instant) -> bool {
        self.progress(now) < 1.0
    }

    fn remaining_duration(&self, now: Instant) -> Option<Duration> {
        let duration = self.duration();
        if duration.is_zero() {
            return None;
        }

        let elapsed = now.saturating_duration_since(self.started_at);
        duration.checked_sub(elapsed)
    }
}

#[allow(dead_code)]
enum ShellState {
    Discovering(DiscoveringState),
    Picker(PickerState),
    Opening(OpeningState),
    WorkspaceIdle(IdleWorkspaceState),
    WorkspaceLoaded(LoadedWorkspaceState),
    Ready(ReadyState),
    Blocked(BlockedState),
}

struct DiscoveringState {
    detail: String,
}

#[allow(dead_code)]
struct PickerState {
    model: PickerModel,
    notice: Option<String>,
}

#[allow(dead_code)]
struct PickerModel {
    choices: Vec<WorkspaceChoice>,
    host_available: bool,
    host_issue: Option<String>,
    available_wsl_distros: Vec<String>,
    unavailable_wsl: Vec<(String, String)>,
    selected_wsl_distro: Option<String>,
    metadata_warning: Option<String>,
    wsl_listing_error: Option<String>,
}

#[allow(dead_code)]
struct WorkspaceChoice {
    workspace: WorkspaceId,
    thread_count: usize,
    latest_preview: Option<String>,
    latest_updated_at: Option<i64>,
    remembered_rank: Option<usize>,
    last_opened: bool,
}

#[derive(Clone)]
struct IdleWorkspaceState {
    loaded_workspace: LoadedWorkspaceState,
}

#[derive(Clone)]
struct LoadedWorkspaceState {
    workspace: BerylWorkspaceManifest,
    known_workspaces: Vec<BerylWorkspaceManifest>,
    workspace_picker_member_paths: workspace_picker::WorkspacePickerMemberPaths,
    workspace_state: WorkspaceConversationState,
    workspace_ui_state: WorkspaceUiState,
    startup_warning: Option<String>,
    implicit_home_path_resolution: Option<ImplicitHomePathResolution>,
    workspace_members_notice: Option<String>,
    workspace_picker_notice: Option<String>,
    workspace_picker: workspace_picker::WorkspacePickerState,
    workspace_runtime_selector_distro_list: workspace_picker::RuntimeSelectorDistroList,
    workspace_members: workspace_members::WorkspaceMembersState,
    workspace_picker_scroll_handle: ScrollHandle,
    workspace_members_scroll_handle: ScrollHandle,
}

struct OpeningState {
    attempt: u32,
    loaded_workspace: LoadedWorkspaceState,
    preserved_surface: Option<ConversationSurfaceState>,
    target: RetryTarget,
    intent: WorkspaceOpenIntent,
    workspace_label: String,
    detail: String,
    progress: Option<ManagedBackendStartupProgress>,
    previous_failure: Option<FailureSummary>,
}

struct ReadyState {
    attempt: u32,
    loaded_workspace: LoadedWorkspaceState,
    execution_target: WorkspaceId,
    process_id: Option<u32>,
    report: ManagedBackendProbeReport,
    cleared_failure: Option<FailureSummary>,
    surface: ConversationSurfaceState,
}

#[derive(Clone)]
struct FailureSummary {
    stage: Option<ManagedBackendStartupStage>,
    title: &'static str,
    summary: String,
}

#[derive(Clone)]
struct BlockedState {
    attempt: u32,
    loaded_workspace: Option<LoadedWorkspaceState>,
    target: RetryTarget,
    intent: WorkspaceOpenIntent,
    workspace_label: String,
    stage: Option<ManagedBackendStartupStage>,
    title: &'static str,
    summary: String,
    detail: String,
    next_steps: Vec<String>,
    disconnect: bool,
    surface: Option<ConversationSurfaceState>,
}

#[derive(Clone)]
struct PendingThreadActivation {
    label: String,
}

enum ThreadActivationStart {
    Started,
    AlreadySelected,
    Rejected { kind: &'static str, message: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyDiagnosticTargetArguments {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DiagnosticTargetLimitArguments {
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DiagnosticTargetMediaEventsArguments {
    limit: Option<usize>,
    after_sequence: Option<u64>,
}

impl DiagnosticTargetLimitArguments {
    fn limit_or_default(self, default: usize, max: usize) -> usize {
        self.limit.unwrap_or(default).min(max)
    }
}

impl DiagnosticTargetMediaEventsArguments {
    fn limit_or_default(&self, default: usize, max: usize) -> usize {
        self.limit.unwrap_or(default).min(max)
    }
}

fn parse_diagnostic_target_arguments<T>(arguments: &Value) -> Result<T, (&'static str, String)>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone())
        .map_err(|source| ("invalid_arguments", source.to_string()))
}

struct CompletedTurnTitleCandidate {
    user_input: String,
    assistant_text: String,
}

#[derive(Default)]
struct AppliedStreamEvent {
    title_candidate: Option<CompletedTurnTitleCandidate>,
    turn_completion_sound: Option<TurnCompletionSoundCandidate>,
    lifecycle_yield: Option<TerminalLifecycleYield>,
}

struct ActiveTurnSteeringTarget {
    thread_id: String,
    turn_index: usize,
    turn_id: Option<String>,
}

struct TranscriptEditReplacementTurnState {
    workspace_id: BerylWorkspaceId,
    execution_target: WorkspaceId,
    thread_id: String,
    accepted_draft: AcceptedComposerDraft,
    composer_cleared: bool,
    turn_started: bool,
}

#[derive(Clone)]
struct ConversationSurfaceState {
    known_threads: Vec<ThreadSummary>,
    selected_thread: Option<usize>,
    selected_thread_status: Option<ThreadStatus>,
    execution_details: ExecutionDetailState,
    transcript_presentation: TranscriptPresentationState,
    tool_activity: ToolActivityProjection,
    hard_stop_targets: HardStopTargetProjection,
    lifecycle_yields: LifecycleYieldState,
    tool_activity_panel_mode: WorkspaceActivityPanelMode,
    tool_activity_panel_height: Pixels,
    status_line: StatusLineState,
    status_line_operations: StatusLineOperationState,
    transcript_submit_anchor: Option<TranscriptSubmitAnchor>,
    loaded_history_anchor_pending: bool,
    transcript_user_scrolled: bool,
    transcript_history_window: TranscriptHistoryWindow,
    transcript_reset_generation: u64,
    transcript_content_release_generation: u64,
    transcript_content_release_row_identities: Vec<String>,
    invalidated_stream_turns: TranscriptStreamInvalidations,
    pending_thread_activation: Option<PendingThreadActivation>,
    context_compaction_thread_id: Option<String>,
    composer_image_labels: ComposerImageLabelState,
    pending_new_thread_label_scope_id: u64,
    next_pending_new_thread_label_scope_id: u64,
    pending_new_thread_label_scope_bindings: HashMap<u64, String>,
    composer_history: ComposerHistoryState,
    pending_turn_input_queue: Option<PendingTurnInputQueue>,
    pending_active_turn_steering_queue:
        Option<PendingActiveTurnSteeringQueue<SteeringInputFragment>>,
    notices: SurfaceNoticeQueue,
    transcript_list_state: ListState,
    graph_overlay: GraphOverlayState,
    thread_selector: ThreadSelectorState,
    graph_thread_link_menu: graph_link_menu::GraphThreadLinkMenuState,
    transcript_branch_menu: transcript_branch_menu_state::TranscriptBranchMenuState,
    transcript_edit_mode: Option<transcript_edit_mode_state::TranscriptEditModeState>,
    checklist_thread_start_menu: ChecklistThreadStartMenuState,
    checklist_sidebar_projection: ChecklistSidebarProjectionCache,
    member_thread_inventory: MemberThreadInventoryState,
    graph_column_selector_scroll: ColumnSelectorScrollState<GraphColumnKey>,
    thread_column_selector_scroll: ColumnSelectorScrollState<ThreadSelectorColumnKey>,
    tool_activity_scroll_handle: ScrollHandle,
    composer_scroll_handle: ScrollHandle,
    composer_reveal_snapshot: Cell<Option<ComposerRevealSnapshot>>,
    graph_overlay_panel_height: Pixels,
    checklist_sidebar_visibility: ChecklistSidebarVisibilityState,
    checklist_sidebar_ratio: f32,
    layout_bounds: Option<Bounds<Pixels>>,
    split_bounds: Option<Bounds<Pixels>>,
    divider_drag: Option<DividerDragState>,
    graph_overlay_drag: Option<GraphOverlayDragState>,
    tool_activity_panel_drag: Option<ToolActivityPanelDragState>,
}

#[derive(Clone, Copy)]
struct DividerDragState {
    pointer_offset: Pixels,
}

#[derive(Clone, Copy)]
struct GraphOverlayDragState {
    pointer_offset: Pixels,
}

#[derive(Clone, Copy)]
struct ToolActivityPanelDragState {
    panel_bottom: Pixels,
    pointer_offset: Pixels,
    min_height: Pixels,
    max_height: Pixels,
}

#[derive(Clone, Copy, PartialEq)]
struct ComposerRevealSnapshot {
    text_hash: u64,
    cursor_offset: usize,
    text_width: Pixels,
    input_content_height: Pixels,
    visible_input_height: Pixels,
}

#[derive(Clone)]
enum RetryTarget {
    Startup,
    WorkspacePrimary,
    Workspace(WorkspaceId),
    HostPath(String),
    WslPath { distro_name: String, path: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkspaceOpenIntent {
    None,
    UseAsPrimaryMember,
    ThreadSelectorActivation,
}

enum DiscoveryUpdate {
    Progress(String),
    Finished(Result<DiscoveryOutcome, String>),
}

struct DiscoveryOutcome {
    startup: crate::ResolvedStartupState,
    workspace_picker_member_paths: workspace_picker::WorkspacePickerMemberPaths,
    workspace_state: WorkspaceConversationState,
    workspace_ui_state: WorkspaceUiState,
}

enum WorkspaceUpdate {
    Detail(String),
    ResolvedExecutionTarget(WorkspaceId),
    Progress(ManagedBackendStartupProgress),
    Finished(Result<OpenedWorkspace, OpenWorkspaceFailure>),
}

enum WorkspaceRuntimeSelectorDistroUpdate {
    Finished(Result<Vec<String>, String>),
}

enum ApplicationShutdownUpdate {
    Finished(Result<(), String>),
}

enum WorkspaceMemberPathPromptResult {
    Selected(PathBuf),
    Cancelled,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ImplicitHomePathResolution {
    runtime: RuntimeMode,
    status: ImplicitHomePathResolutionStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ImplicitHomePathResolutionStatus {
    Pending,
    Resolved(PathBuf),
    Failed(String),
}

fn os_window_focus_state(window: &Window) -> Result<BerylWindowFocusState, String> {
    #[cfg(target_os = "windows")]
    {
        windows_os_window_focus_state(window)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(if window.is_window_active() {
            BerylWindowFocusState::Focused
        } else {
            BerylWindowFocusState::Unfocused
        })
    }
}

#[cfg(target_os = "windows")]
fn windows_os_window_focus_state(window: &Window) -> Result<BerylWindowFocusState, String> {
    let handle = raw_window_handle::HasWindowHandle::window_handle(window)
        .map_err(|error| error.to_string())?;
    let hwnd = match handle.as_raw() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get(),
        raw => {
            return Err(format!(
                "expected Win32 window handle for Beryl window, got {raw:?}"
            ));
        }
    };
    let foreground_hwnd = unsafe { GetForegroundWindow() }.0 as isize;
    Ok(if foreground_hwnd == hwnd {
        BerylWindowFocusState::Focused
    } else {
        BerylWindowFocusState::Unfocused
    })
}

#[derive(Default)]
struct NotificationSoundPathPromptState {
    next_token: u64,
    active_token: Option<u64>,
}

impl NotificationSoundPathPromptState {
    fn begin(&mut self) -> Option<u64> {
        if self.active_token.is_some() {
            return None;
        }

        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(1);
        self.active_token = Some(token);
        Some(token)
    }

    fn cancel_active(&mut self) {
        self.active_token = None;
        self.next_token = self.next_token.wrapping_add(1);
    }

    fn finish(&mut self, token: u64) -> bool {
        if self.active_token != Some(token) {
            return false;
        }

        self.active_token = None;
        true
    }
}

enum NotificationSoundPathPromptResult {
    Selected(PathBuf),
    Cancelled,
    Failed(String),
}

struct OpenedWorkspace {
    execution_target: WorkspaceId,
    server: ManagedBackendServer,
    report: ManagedBackendProbeReport,
    hard_stop_capabilities: HardStopCapabilities,
    known_threads: Vec<ThreadSummary>,
    selected_thread_id: Option<String>,
    selected_thread_history: Option<ThreadInfo>,
    selected_thread_history_window: Option<TranscriptHistoryWindow>,
    selected_thread_image_resolver: TranscriptImagePathResolver,
    selected_thread_session_metadata: Option<ThreadSessionMetadata>,
    surface_notice: Option<SurfaceNotice>,
    graph: SemanticGraph,
    graph_revision: WorkspaceGraphRevision,
    graph_warning: Option<String>,
}

struct OpenWorkspaceFailure {
    stage: Option<ManagedBackendStartupStage>,
    title: &'static str,
    summary: String,
    detail: String,
    next_steps: Vec<String>,
}

const DEFAULT_CHECKLIST_SIDEBAR_RATIO: f32 = 0.34;
const GRAPH_OVERLAY_TOGGLE_KEYSTROKE: &str = "ctrl-shift-g";
const SCROLLBAR_FADE_DELAY: Duration = Duration::from_secs(2);
const SCROLLBAR_FADE_DURATION: Duration = Duration::from_millis(180);

fn column_selector_scrollbar_region(surface: ColumnSelectorSurface) -> ScrollbarRegion {
    match surface {
        ColumnSelectorSurface::GraphOverlay => ScrollbarRegion::GraphColumns,
        ColumnSelectorSurface::ThreadSelector => ScrollbarRegion::ThreadSelectorColumns,
    }
}

fn composer_history_text_input_atom(atom: &ComposerDraftImageAtom) -> TextInputSelectionAtom {
    TextInputSelectionAtom::new(
        atom.atom_id().to_string(),
        atom.range(),
        composer_image_marker(atom.label()),
        composer_image_copy_text(atom.label()),
    )
}

fn rgba_from_role_color(color: Option<crate::ParsedHexColor>, fallback: gpui::Rgba) -> gpui::Rgba {
    color
        .map(|color| {
            rgb(((color.red() as u32) << 16) | ((color.green() as u32) << 8) | color.blue() as u32)
        })
        .unwrap_or(fallback)
}

fn chrome_color(value: &str, fallback: gpui::Rgba) -> gpui::Rgba {
    rgba_from_role_color(crate::ParsedHexColor::parse(value), fallback)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ChromeButtonTheme {
    pub normal: ChromeButtonStateTheme,
    pub hover: ChromeButtonStateTheme,
    pub active: ChromeButtonStateTheme,
    pub disabled: ChromeButtonStateTheme,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ChromeButtonStateTheme {
    pub background: gpui::Rgba,
    pub border: gpui::Rgba,
    pub foreground: gpui::Rgba,
}

impl ChromeButtonTheme {
    fn primary() -> Self {
        Self {
            normal: ChromeButtonStateTheme::new(rgb(0x1d4ed8), rgb(0x3b82f6), rgb(0xeff6ff)),
            hover: ChromeButtonStateTheme::new(rgb(0x2563eb), rgb(0x60a5fa), rgb(0xffffff)),
            active: ChromeButtonStateTheme::new(rgb(0x1e40af), rgb(0x3b82f6), rgb(0xffffff)),
            disabled: ChromeButtonStateTheme::new(rgb(0x334155), rgb(0x475569), rgb(0x94a3b8)),
        }
    }

    fn secondary() -> Self {
        Self {
            normal: ChromeButtonStateTheme::new(rgb(0x1e293b), rgb(0x475569), rgb(0xe2e8f0)),
            hover: ChromeButtonStateTheme::new(rgb(0x334155), rgb(0x64748b), rgb(0xf8fafc)),
            active: ChromeButtonStateTheme::new(rgb(0x0f172a), rgb(0x475569), rgb(0xf8fafc)),
            disabled: ChromeButtonStateTheme::new(rgb(0x111827), rgb(0x334155), rgb(0x64748b)),
        }
    }
}

impl ChromeButtonStateTheme {
    fn new(background: gpui::Rgba, border: gpui::Rgba, foreground: gpui::Rgba) -> Self {
        Self {
            background,
            border,
            foreground,
        }
    }
}

fn chrome_button_theme(
    settings: &crate::AppearanceButtonSettings,
    fallback: ChromeButtonTheme,
) -> ChromeButtonTheme {
    ChromeButtonTheme {
        normal: chrome_button_state_theme(&settings.normal, fallback.normal),
        hover: chrome_button_state_theme(&settings.hover, fallback.hover),
        active: chrome_button_state_theme(&settings.active, fallback.active),
        disabled: chrome_button_state_theme(&settings.disabled, fallback.disabled),
    }
}

fn chrome_button_state_theme(
    settings: &crate::AppearanceButtonStateSettings,
    fallback: ChromeButtonStateTheme,
) -> ChromeButtonStateTheme {
    ChromeButtonStateTheme {
        background: chrome_color(&settings.background, fallback.background),
        border: chrome_color(&settings.border, fallback.border),
        foreground: chrome_color(&settings.foreground, fallback.foreground),
    }
}

impl LoadedWorkspaceState {
    fn new(
        workspace: BerylWorkspaceManifest,
        known_workspaces: Vec<BerylWorkspaceManifest>,
        workspace_picker_member_paths: workspace_picker::WorkspacePickerMemberPaths,
        workspace_state: WorkspaceConversationState,
        workspace_ui_state: WorkspaceUiState,
        startup_warning: Option<String>,
    ) -> Self {
        Self {
            workspace,
            known_workspaces,
            workspace_picker_member_paths,
            workspace_state,
            workspace_ui_state,
            startup_warning,
            implicit_home_path_resolution: None,
            workspace_members_notice: None,
            workspace_picker_notice: None,
            workspace_picker: workspace_picker::WorkspacePickerState::default(),
            workspace_runtime_selector_distro_list:
                workspace_picker::RuntimeSelectorDistroList::default(),
            workspace_members: workspace_members::WorkspaceMembersState::default(),
            workspace_picker_scroll_handle: ScrollHandle::new(),
            workspace_members_scroll_handle: ScrollHandle::new(),
        }
    }

    fn selected_runtime(&self) -> Option<&beryl_model::workspace::RuntimeMode> {
        self.workspace_state.selected_runtime()
    }

    fn explicit_members(&self) -> &[beryl_model::workspace::WorkspaceMember] {
        self.workspace_state.explicit_members()
    }

    fn implicit_home_path_display_text(&self) -> String {
        let Some(runtime) = self.selected_implicit_home_runtime() else {
            return String::new();
        };

        match self.implicit_home_path_resolution.as_ref() {
            Some(resolution) if resolution.runtime == *runtime => match &resolution.status {
                ImplicitHomePathResolutionStatus::Resolved(path) => path.display().to_string(),
                ImplicitHomePathResolutionStatus::Failed(error) => {
                    format!("Could not resolve home directory: {error}")
                }
                ImplicitHomePathResolutionStatus::Pending => {
                    "Resolving home directory...".to_string()
                }
            },
            _ => "Resolving home directory...".to_string(),
        }
    }

    fn begin_implicit_home_path_resolution(&mut self, runtime: RuntimeMode) {
        self.implicit_home_path_resolution = Some(ImplicitHomePathResolution {
            runtime,
            status: ImplicitHomePathResolutionStatus::Pending,
        });
    }

    fn finish_implicit_home_path_resolution(
        &mut self,
        runtime: &RuntimeMode,
        result: Result<PathBuf, String>,
    ) -> bool {
        if !self
            .selected_implicit_home_runtime()
            .is_some_and(|selected| selected == runtime)
        {
            return false;
        }

        let restored_target = result
            .as_ref()
            .ok()
            .map(|path| WorkspaceId::from_parts(runtime.clone(), path.clone()));
        self.implicit_home_path_resolution = Some(ImplicitHomePathResolution {
            runtime: runtime.clone(),
            status: match result {
                Ok(path) => ImplicitHomePathResolutionStatus::Resolved(path),
                Err(error) => ImplicitHomePathResolutionStatus::Failed(error),
            },
        });
        restored_target.is_some_and(|target| self.restore_implicit_home_threads_for_target(&target))
    }

    fn set_resolved_implicit_home_path_from_target(
        &mut self,
        execution_target: &WorkspaceId,
    ) -> bool {
        if !self
            .selected_implicit_home_runtime()
            .is_some_and(|runtime| runtime == execution_target.runtime_mode())
        {
            return false;
        }

        self.implicit_home_path_resolution = Some(ImplicitHomePathResolution {
            runtime: execution_target.runtime_mode().clone(),
            status: ImplicitHomePathResolutionStatus::Resolved(
                execution_target.canonical_path().to_path_buf(),
            ),
        });
        self.restore_implicit_home_threads_for_target(execution_target)
    }

    fn resolved_implicit_home_execution_target(&self) -> Option<WorkspaceId> {
        let runtime = self.selected_implicit_home_runtime()?;
        let Some(resolution) = self.implicit_home_path_resolution.as_ref() else {
            return None;
        };
        if resolution.runtime != *runtime {
            return None;
        }
        let ImplicitHomePathResolutionStatus::Resolved(path) = &resolution.status else {
            return None;
        };

        Some(WorkspaceId::from_parts(runtime.clone(), path.clone()))
    }

    fn restore_resolved_implicit_home_threads(&mut self) -> bool {
        self.resolved_implicit_home_execution_target()
            .is_some_and(|target| self.restore_implicit_home_threads_for_target(&target))
    }

    fn restore_implicit_home_threads_for_target(&mut self, execution_target: &WorkspaceId) -> bool {
        self.workspace_state
            .restore_implicit_home_threads_for_execution_target(execution_target)
    }

    fn clear_implicit_home_path_resolution(&mut self) {
        self.implicit_home_path_resolution = None;
    }

    fn implicit_home_path_resolution_needed(&self) -> Option<RuntimeMode> {
        let runtime = self.selected_implicit_home_runtime()?.clone();
        match self.implicit_home_path_resolution.as_ref() {
            Some(resolution)
                if resolution.runtime == runtime
                    && matches!(
                        &resolution.status,
                        ImplicitHomePathResolutionStatus::Pending
                            | ImplicitHomePathResolutionStatus::Resolved(_)
                            | ImplicitHomePathResolutionStatus::Failed(_)
                    ) =>
            {
                None
            }
            _ => Some(runtime),
        }
    }

    fn selected_implicit_home_runtime(&self) -> Option<&RuntimeMode> {
        if !self.workspace_state.has_available_explicit_members() {
            self.selected_runtime()
        } else {
            None
        }
    }

    fn replace_manifest(&mut self, manifest: BerylWorkspaceManifest) {
        let workspace_id = manifest.id().clone();
        self.workspace = manifest.clone();
        if let Some(existing) = self
            .known_workspaces
            .iter_mut()
            .find(|workspace| workspace.id() == &workspace_id)
        {
            *existing = manifest;
        }
    }

    fn replace_manifest_for_rename(
        &mut self,
        old_workspace_id: &BerylWorkspaceId,
        manifest: BerylWorkspaceManifest,
    ) {
        let new_workspace_id = manifest.id().clone();
        self.workspace = manifest.clone();
        if let Some(existing) = self
            .known_workspaces
            .iter_mut()
            .find(|workspace| workspace.id() == old_workspace_id)
        {
            *existing = manifest;
        } else if let Some(existing) = self
            .known_workspaces
            .iter_mut()
            .find(|workspace| workspace.id() == &new_workspace_id)
        {
            *existing = manifest;
        } else {
            self.known_workspaces.insert(0, manifest);
        }

        if old_workspace_id != &new_workspace_id {
            let member_paths = self
                .workspace_picker_member_paths
                .remove(old_workspace_id)
                .unwrap_or_else(|| {
                    workspace_picker::explicit_member_path_strings(&self.workspace_state)
                });
            self.workspace_picker_member_paths
                .insert(new_workspace_id, member_paths);
        } else {
            self.refresh_active_workspace_picker_member_paths();
        }
    }

    fn refresh_active_workspace_picker_member_paths(&mut self) {
        let workspace_id = self.workspace.id().clone();
        self.workspace_picker_member_paths.insert(
            workspace_id,
            workspace_picker::explicit_member_path_strings(&self.workspace_state),
        );
    }

    fn workspace_picker_scroll_handle(&self) -> ScrollHandle {
        self.workspace_picker_scroll_handle.clone()
    }

    fn reset_workspace_picker_scroll(&mut self) {
        self.workspace_picker_scroll_handle = ScrollHandle::new();
    }

    fn workspace_members_scroll_handle(&self) -> ScrollHandle {
        self.workspace_members_scroll_handle.clone()
    }

    fn reset_workspace_members_scroll(&mut self) {
        self.workspace_members_scroll_handle = ScrollHandle::new();
    }

    fn workspace_members_notice(&self) -> Option<&str> {
        self.workspace_members_notice.as_deref()
    }

    fn set_workspace_members_notice(&mut self, notice: impl Into<String>) {
        self.workspace_members_notice = Some(notice.into());
    }

    fn clear_workspace_members_notice(&mut self) {
        self.workspace_members_notice = None;
    }

    fn workspace_picker_notice(&self) -> Option<&str> {
        self.workspace_picker_notice.as_deref()
    }

    fn set_workspace_picker_notice(&mut self, notice: impl Into<String>) {
        self.workspace_picker_notice = Some(notice.into());
    }

    fn clear_workspace_picker_notice(&mut self) {
        self.workspace_picker_notice = None;
    }

    fn runtime_selector_distro_list(&self) -> &workspace_picker::RuntimeSelectorDistroList {
        &self.workspace_runtime_selector_distro_list
    }

    fn begin_runtime_selector_distro_refresh(&mut self) -> bool {
        if !self.workspace_runtime_selector_distro_list.should_refresh() {
            return false;
        }

        self.workspace_runtime_selector_distro_list.begin_loading()
    }

    fn finish_runtime_selector_distro_refresh(&mut self, result: Result<Vec<String>, String>) {
        self.workspace_runtime_selector_distro_list
            .finish_loading(result);
    }
}

impl IdleWorkspaceState {
    fn new(loaded_workspace: LoadedWorkspaceState) -> Self {
        Self { loaded_workspace }
    }
}

impl ConversationSurfaceState {
    fn seeded(
        workspace_id: BerylWorkspaceId,
        workspace_state: &WorkspaceConversationState,
        workspace_ui_state: &WorkspaceUiState,
        known_threads: Vec<ThreadSummary>,
        hard_stop_capabilities: HardStopCapabilities,
        selected_thread_history: Option<ThreadInfo>,
        selected_thread_history_window: Option<TranscriptHistoryWindow>,
        selected_thread_image_resolver: TranscriptImagePathResolver,
        selected_thread_id: Option<String>,
        selected_thread_session_metadata: Option<ThreadSessionMetadata>,
        notice: Option<SurfaceNotice>,
        graph: SemanticGraph,
        graph_revision: WorkspaceGraphRevision,
        graph_warning: Option<String>,
    ) -> Self {
        let known_threads =
            bounded_known_threads(known_threads, selected_thread_id.iter().cloned());
        let mut state = Self {
            known_threads,
            selected_thread: None,
            selected_thread_status: None,
            execution_details: ExecutionDetailState::default(),
            transcript_presentation: TranscriptPresentationState::default(),
            tool_activity: ToolActivityProjection::default(),
            hard_stop_targets: {
                let mut projection = HardStopTargetProjection::default();
                projection.set_capabilities(hard_stop_capabilities);
                projection
            },
            lifecycle_yields: LifecycleYieldState::default(),
            tool_activity_panel_mode: workspace_ui_state.tool_activity_panel_mode(),
            tool_activity_panel_height: px(workspace_ui_state.tool_activity_panel_height_px()),
            status_line: StatusLineState::default(),
            status_line_operations: StatusLineOperationState::default(),
            transcript_submit_anchor: None,
            loaded_history_anchor_pending: false,
            transcript_user_scrolled: false,
            transcript_history_window: TranscriptHistoryWindow::default(),
            transcript_reset_generation: 0,
            transcript_content_release_generation: 0,
            transcript_content_release_row_identities: Vec::new(),
            invalidated_stream_turns: TranscriptStreamInvalidations::default(),
            pending_thread_activation: None,
            context_compaction_thread_id: None,
            composer_image_labels: ComposerImageLabelState::default(),
            pending_new_thread_label_scope_id: 0,
            next_pending_new_thread_label_scope_id: 1,
            pending_new_thread_label_scope_bindings: HashMap::new(),
            composer_history: ComposerHistoryState::default(),
            pending_turn_input_queue: None,
            pending_active_turn_steering_queue: None,
            notices: SurfaceNoticeQueue::from_initial(notice),
            transcript_list_state: ListState::new(0, ListAlignment::Bottom, px(320.0)),
            graph_overlay: GraphOverlayState::new(graph, graph_revision, graph_warning),
            thread_selector: ThreadSelectorState::default(),
            graph_thread_link_menu: graph_link_menu::GraphThreadLinkMenuState::default(),
            transcript_branch_menu:
                transcript_branch_menu_state::TranscriptBranchMenuState::default(),
            transcript_edit_mode: None,
            checklist_thread_start_menu: ChecklistThreadStartMenuState::default(),
            checklist_sidebar_projection: ChecklistSidebarProjectionCache::default(),
            member_thread_inventory: MemberThreadInventoryState::new(workspace_id, workspace_state),
            graph_column_selector_scroll: ColumnSelectorScrollState::new(),
            thread_column_selector_scroll: ColumnSelectorScrollState::new(),
            tool_activity_scroll_handle: ScrollHandle::new(),
            composer_scroll_handle: ScrollHandle::new(),
            composer_reveal_snapshot: Cell::new(None),
            graph_overlay_panel_height: Pixels::ZERO,
            checklist_sidebar_visibility: ChecklistSidebarVisibilityState::default(),
            checklist_sidebar_ratio: DEFAULT_CHECKLIST_SIDEBAR_RATIO,
            layout_bounds: None,
            split_bounds: None,
            divider_drag: None,
            graph_overlay_drag: None,
            tool_activity_panel_drag: None,
        };

        if let Some(thread) = selected_thread_history {
            state.load_thread_history_window(
                &thread,
                selected_thread_history_window.unwrap_or_default(),
                &selected_thread_image_resolver,
            );
            if let Some(metadata) = selected_thread_session_metadata {
                state.set_thread_session_metadata(metadata);
            }
        } else if let Some(thread_id) = selected_thread_id {
            state.select_thread_by_id(&thread_id);
        } else {
            state.selected_thread = None;
            state.selected_thread_status = None;
        }

        state.apply_known_thread_agent_labels();
        state.hydrate_token_usage_snapshots(workspace_state);
        state.reconcile_graph_scroll_handles();
        state.refresh_checklist_sidebar_projection();
        state
    }

    fn selected_thread(&self) -> Option<&ThreadSummary> {
        self.selected_thread
            .and_then(|index| self.known_threads.get(index))
    }

    fn selected_thread_id(&self) -> Option<&str> {
        self.selected_thread().map(|thread| thread.id.as_str())
    }

    fn record_lifecycle_yield(&mut self, yielded: AcceptedLifecycleYield) -> bool {
        self.lifecycle_yields
            .record(yielded.thread_id, yielded.turn_id, yielded.outcome)
    }

    fn composer_clipboard_label_scope(&self) -> ComposerClipboardLabelScope {
        ComposerClipboardLabelScope::for_selected_thread(
            self.selected_thread_id(),
            self.pending_new_thread_label_scope_id,
        )
    }

    fn composer_history_scope(&self) -> ComposerHistoryScope {
        self.selected_thread_id()
            .map(|thread_id| ComposerHistoryScope::Thread(thread_id.to_string()))
            .unwrap_or(ComposerHistoryScope::PendingNewThread(
                self.pending_new_thread_label_scope_id,
            ))
    }

    fn record_accepted_composer_history(&mut self, draft: &AcceptedComposerDraft) {
        self.composer_history.record_accepted(
            self.composer_history_scope(),
            draft.with_durable_image_references(),
        );
    }

    fn browse_composer_history_previous(
        &mut self,
        current_draft: ComposerDraft,
    ) -> Option<ComposerHistoryBrowseResult> {
        self.composer_history
            .browse_previous(self.composer_history_scope(), current_draft)
    }

    fn browse_composer_history_next(&mut self) -> Option<ComposerHistoryBrowseResult> {
        self.composer_history
            .browse_next(self.composer_history_scope())
    }

    fn is_composer_clipboard_label_scope_current(
        &self,
        scope: &ComposerClipboardLabelScope,
    ) -> bool {
        match (scope, self.selected_thread_id()) {
            (ComposerClipboardLabelScope::Thread(source), Some(current)) => source == current,
            (ComposerClipboardLabelScope::PendingNewThread(source), None) => {
                *source == self.pending_new_thread_label_scope_id
            }
            (ComposerClipboardLabelScope::PendingNewThread(source), Some(current)) => self
                .pending_new_thread_label_scope_bindings
                .get(source)
                .is_some_and(|thread_id| thread_id == current),
            _ => false,
        }
    }

    fn allocate_composer_image_label(&mut self) -> String {
        let selected_thread_id = self.selected_thread_id().map(str::to_string);
        self.composer_image_labels
            .allocate(selected_thread_id.as_deref())
    }

    fn composer_image_paste_readiness(&self) -> ComposerImagePasteReadiness {
        self.composer_image_labels
            .paste_readiness(self.selected_thread_id())
    }

    fn selected_thread_needing_composer_image_label_scan(&self) -> Option<String> {
        self.composer_image_labels
            .selected_thread_needing_history_scan(self.selected_thread_id())
    }

    fn observe_composer_image_labels_in_fragment(&mut self, fragment: &UserInputFragment) {
        let selected_thread_id = self.selected_thread_id().map(str::to_string);
        self.composer_image_labels
            .observe_backend_input(selected_thread_id.as_deref(), fragment.backend_input());
    }

    fn observe_composer_image_labels_in_thread_fragment(
        &mut self,
        thread_id: &str,
        fragment: &UserInputFragment,
    ) {
        self.composer_image_labels
            .observe_thread_backend_input(thread_id, fragment.backend_input());
    }

    fn bind_pending_new_thread_image_labels_to_thread(&mut self, thread_id: &str) {
        self.composer_image_labels
            .bind_pending_new_thread_to_thread(thread_id);
        self.pending_new_thread_label_scope_bindings.insert(
            self.pending_new_thread_label_scope_id,
            thread_id.to_string(),
        );
        self.prune_pending_new_thread_label_scope_bindings();
        self.composer_history.bind_pending_new_thread_to_thread(
            self.pending_new_thread_label_scope_id,
            thread_id.to_string(),
        );
    }

    fn prune_pending_new_thread_label_scope_bindings(&mut self) {
        if self.pending_new_thread_label_scope_bindings.len()
            <= PENDING_NEW_THREAD_LABEL_SCOPE_BINDINGS_MAX
        {
            return;
        }

        let current_scope = self.pending_new_thread_label_scope_id;
        let mut removable_scopes = self
            .pending_new_thread_label_scope_bindings
            .keys()
            .copied()
            .filter(|scope_id| *scope_id != current_scope)
            .collect::<Vec<_>>();
        removable_scopes.sort_unstable();
        for scope_id in removable_scopes {
            if self.pending_new_thread_label_scope_bindings.len()
                <= PENDING_NEW_THREAD_LABEL_SCOPE_BINDINGS_MAX
            {
                break;
            }
            self.pending_new_thread_label_scope_bindings
                .remove(&scope_id);
        }
    }

    fn finish_composer_image_label_scan(
        &mut self,
        thread_id: &str,
        observations: composer_image_labels::ComposerImageLabelObservations,
    ) {
        self.composer_image_labels
            .finish_thread_history_scan(thread_id, observations);
    }

    fn fail_composer_image_label_scan(&mut self, thread_id: &str, message: impl Into<String>) {
        self.composer_image_labels
            .fail_thread_history_scan(thread_id, message);
    }

    fn earliest_known_user_input_fragment_text(&self) -> Option<&str> {
        self.execution_details
            .turns()
            .iter()
            .find_map(|turn| turn.first_user_input_fragment_text())
    }

    fn selected_thread_display_label(
        &self,
        workspace_state: &WorkspaceConversationState,
        execution_target: &WorkspaceId,
    ) -> Option<String> {
        let thread = self.selected_thread()?;
        let thread_id = ConversationThreadId::new(thread.id.clone());
        let backend_name = normalized_thread_name(thread.name.as_deref());
        Some(resolved_thread_title(
            workspace_state,
            &thread_id,
            execution_target,
            &thread.preview,
            backend_name.as_deref(),
            thread.created_at,
            thread.updated_at,
        ))
    }

    fn apply_known_thread_agent_labels(&mut self) -> bool {
        let selected_thread_id = self.selected_thread_id().map(str::to_string);
        self.tool_activity.apply_thread_summary_agent_labels(
            self.known_threads
                .iter()
                .filter(|thread| selected_thread_id.as_deref() != Some(thread.id.as_str())),
        )
    }

    fn transcript_presentation(&self) -> &TranscriptPresentationState {
        &self.transcript_presentation
    }

    fn retained_state_snapshot(&self) -> RetainedStateSnapshot {
        let transcript = self.execution_details.retained_counts();
        let presentation = self.transcript_presentation.retained_counts();
        let history = self.transcript_history_window.retained_counts();
        let activity = self.tool_activity.retained_counts();
        let graph = self.graph_overlay.retained_counts();
        let inventory = self.member_thread_inventory.snapshot().retained_counts();
        let composer_history = self.composer_history.retained_counts();
        let pending_turn_input_fragments = self
            .pending_turn_input_queue
            .as_ref()
            .map(PendingTurnInputQueue::fragment_count)
            .unwrap_or_default();
        let pending_turn_input_bytes = self
            .pending_turn_input_queue
            .as_ref()
            .map(PendingTurnInputQueue::payload_bytes_lower_bound)
            .unwrap_or_default();
        let pending_steering_fragments = self
            .pending_active_turn_steering_queue
            .as_ref()
            .map(PendingActiveTurnSteeringQueue::fragment_count)
            .unwrap_or_default();
        let pending_steering_bytes = self
            .pending_active_turn_steering_queue
            .as_ref()
            .map(|queue| {
                queue
                    .fragments()
                    .iter()
                    .map(SteeringInputFragment::retained_payload_bytes_lower_bound)
                    .sum::<usize>()
            })
            .unwrap_or_default();
        let known_thread_payload_bytes = known_thread_payload_bytes(&self.known_threads);
        RetainedStateSnapshot {
            retained_payload_bytes_lower_bound: Some(
                transcript
                    .payload_bytes
                    .saturating_add(presentation.text_bytes)
                    .saturating_add(presentation.identity_bytes)
                    .saturating_add(presentation.anchor_bytes)
                    .saturating_add(history.metadata_bytes)
                    .saturating_add(activity.payload_bytes)
                    .saturating_add(graph.payload_bytes)
                    .saturating_add(inventory.payload_bytes)
                    .saturating_add(composer_history.display_text_bytes)
                    .saturating_add(composer_history.part_text_bytes)
                    .saturating_add(composer_history.image_bytes)
                    .saturating_add(composer_history.atom_bytes)
                    .saturating_add(pending_turn_input_bytes)
                    .saturating_add(pending_steering_bytes)
                    .saturating_add(known_thread_payload_bytes),
            ),
            loaded_transcript_turns: Some(transcript.turns),
            loaded_transcript_items: Some(transcript.items),
            loaded_transcript_text_bytes: Some(transcript.text_bytes),
            transcript_user_fragments: Some(transcript.user_fragments),
            transcript_user_fragment_text_bytes: Some(transcript.user_fragment_text_bytes),
            transcript_backend_input_records: Some(transcript.backend_input_records),
            transcript_backend_input_bytes: Some(transcript.backend_input_bytes),
            transcript_image_marker_bytes: Some(transcript.image_marker_bytes),
            transcript_narrative_entries: Some(transcript.narrative_entries),
            released_transcript_placeholders: Some(transcript.released_placeholders),
            active_turn_payload_bytes: Some(transcript.active_turn_payload_bytes),
            transcript_agent_text_bytes: Some(transcript.agent_text_bytes),
            transcript_reasoning_summary_bytes: Some(transcript.reasoning_summary_bytes),
            transcript_reasoning_content_bytes: Some(transcript.reasoning_content_bytes),
            transcript_command_text_bytes: Some(transcript.command_text_bytes),
            transcript_command_output_bytes: Some(transcript.command_output_bytes),
            transcript_file_change_path_bytes: Some(transcript.file_change_path_bytes),
            transcript_file_change_output_bytes: Some(transcript.file_change_output_bytes),
            transcript_generated_image_inline_bytes: Some(transcript.generated_image_inline_bytes),
            transcript_generated_image_metadata_bytes: Some(
                transcript.generated_image_metadata_bytes,
            ),
            transcript_error_bytes: Some(transcript.error_bytes),
            transcript_identity_bytes: Some(transcript.identity_bytes),
            presentation_rows: Some(presentation.rows),
            presentation_items: Some(presentation.items),
            presentation_text_bytes: Some(presentation.text_bytes),
            presentation_identity_bytes: Some(presentation.identity_bytes),
            presentation_anchor_bytes: Some(presentation.anchor_bytes),
            presentation_placeholder_rows: Some(presentation.placeholder_rows),
            history_pages: Some(history.pages),
            history_resident_pages: Some(history.resident_pages),
            history_released_pages: Some(history.released_pages),
            history_loading_pages: Some(history.loading_pages),
            history_pinned_pages: Some(history.pinned_pages),
            history_turn_ids: Some(history.turn_ids),
            history_turn_id_bytes: Some(history.turn_id_bytes),
            history_cursor_bytes: Some(history.cursor_bytes),
            history_metadata_bytes: Some(history.metadata_bytes),
            activity_records: Some(activity.records),
            activity_rows: Some(activity.rows),
            activity_visible_thread_indexes: Some(activity.visible_thread_indexes),
            activity_label_count: Some(activity.label_count),
            activity_label_bytes: Some(activity.label_payload_bytes),
            activity_reasoning_summary_parts: Some(activity.reasoning_summary_parts),
            activity_reasoning_summary_bytes: Some(activity.reasoning_summary_bytes),
            activity_subagent_metadata_count: Some(activity.subagent_metadata_count),
            activity_subagent_metadata_bytes: Some(activity.subagent_metadata_bytes),
            activity_parent_thread_links: Some(activity.parent_thread_links),
            activity_parent_thread_link_bytes: Some(activity.parent_thread_link_bytes),
            activity_visible_thread_index_maps: Some(activity.visible_thread_index_maps),
            activity_visible_thread_index_key_bytes: Some(activity.visible_thread_index_key_bytes),
            activity_visible_thread_index_bytes: Some(activity.visible_thread_index_bytes),
            activity_record_payload_bytes: Some(activity.record_payload_bytes),
            activity_row_payload_bytes: Some(activity.row_payload_bytes),
            graph_nodes: Some(graph.visible_nodes),
            graph_soft_links: Some(graph.visible_soft_links),
            graph_thread_refs: Some(graph.visible_thread_refs),
            graph_committed_nodes: Some(graph.committed_nodes),
            graph_committed_soft_links: Some(graph.committed_soft_links),
            graph_committed_thread_refs: Some(graph.committed_thread_refs),
            graph_columns: Some(graph.columns),
            graph_pending_optimistic_mutations: Some(graph.pending_optimistic_mutations),
            graph_queued_commits: Some(graph.queued_commits),
            inventory_groups: Some(inventory.groups),
            inventory_threads: Some(inventory.threads),
            known_threads: Some(self.known_threads.len()),
            composer_history_lanes: Some(composer_history.lanes),
            composer_history_entries: Some(composer_history.entries),
            composer_history_text_bytes: Some(
                composer_history
                    .display_text_bytes
                    .saturating_add(composer_history.part_text_bytes),
            ),
            composer_history_images: Some(composer_history.image_count),
            composer_history_image_bytes: Some(composer_history.image_bytes),
            composer_history_atoms: Some(composer_history.atom_count),
            composer_history_atom_bytes: Some(composer_history.atom_bytes),
            pending_turn_input_fragments: Some(pending_turn_input_fragments),
            pending_turn_input_bytes: Some(pending_turn_input_bytes),
            pending_steering_fragments: Some(pending_steering_fragments),
            pending_steering_bytes: Some(pending_steering_bytes),
            ..RetainedStateSnapshot::default()
        }
    }

    fn transcript_activity_caret(&self) -> Option<TranscriptActivityCaret> {
        self.transcript_presentation
            .activity_caret_for_source_turn(self.execution_details.working_turn_index())
    }

    #[allow(dead_code)]
    fn tool_activity_rows(&self) -> Vec<&tool_activity::ToolActivityRow> {
        self.tool_activity
            .rows_for_selected_thread(self.selected_thread_id())
    }

    fn tool_activity_row_count(&self) -> usize {
        self.tool_activity
            .row_count_for_selected_thread(self.selected_thread_id())
    }

    fn tool_activity_subagent_metadata_targets(
        &self,
    ) -> Vec<tool_activity::ToolActivitySubagentMetadataTarget> {
        self.tool_activity.subagent_metadata_resolution_targets()
    }

    fn tool_activity_row_window(
        &self,
        range: Range<usize>,
    ) -> Vec<(usize, &tool_activity::ToolActivityRow)> {
        self.tool_activity
            .rows_for_selected_thread_window(self.selected_thread_id(), range)
    }

    fn tool_activity_panel_mode(&self) -> WorkspaceActivityPanelMode {
        self.tool_activity_panel_mode
    }

    fn tool_activity_panel_height(&self) -> Pixels {
        self.tool_activity_panel_height
    }

    fn tool_activity_panel_height_for_layout(&self, composer_height: Pixels) -> Pixels {
        let main_region_height = self
            .layout_bounds
            .map(|bounds| bounds.size.height)
            .unwrap_or_else(|| px(layout::WINDOW_MIN_HEIGHT));
        layout::tool_activity_panel_height(
            main_region_height,
            composer_height,
            self.tool_activity_panel_height,
        )
    }

    fn tool_activity_panel_visible(&self) -> bool {
        self.tool_activity_panel_mode.panel_visible(
            self.execution_details.working_turn_index().is_some(),
            self.selected_thread_context_compaction_id().is_some(),
        )
    }

    fn tool_activity_scroll_handle(&self) -> ScrollHandle {
        self.tool_activity_scroll_handle.clone()
    }

    fn cycle_tool_activity_panel_mode(&mut self) {
        self.tool_activity_panel_mode = self.tool_activity_panel_mode.next();
    }

    fn workspace_ui_state(&self) -> WorkspaceUiState {
        WorkspaceUiState::new(
            self.tool_activity_panel_mode,
            f32::from(self.tool_activity_panel_height()),
        )
    }

    #[allow(dead_code)]
    fn clear_tool_activity(&mut self) -> bool {
        self.tool_activity.clear_all() | self.hard_stop_targets.clear_all()
    }

    fn finish_running_tool_activity_for_thread(
        &mut self,
        thread_id: &str,
        status: tool_activity::ToolActivityRowStatus,
    ) -> bool {
        self.tool_activity
            .finish_running_for_thread(thread_id, status)
    }

    fn finish_running_tool_activity_for_thread_ok(&mut self, thread_id: &str) -> bool {
        self.finish_running_tool_activity_for_thread(
            thread_id,
            tool_activity::ToolActivityRowStatus::FinishedOk,
        )
    }

    fn finish_running_tool_activity_for_thread_error(&mut self, thread_id: &str) -> bool {
        self.finish_running_tool_activity_for_thread(
            thread_id,
            tool_activity::ToolActivityRowStatus::FinishedError,
        )
    }

    fn status_line_projection(&self) -> status_line::StatusLineProjection {
        let cancellable_active_turn = self.selected_cancellable_active_turn();
        let hard_stop_targets = self
            .hard_stop_targets
            .selected_turn_targets(cancellable_active_turn.as_ref());
        self.status_line.projection_with_turn_operations(
            self.selected_thread_id(),
            self.status_line_model_reasoning_available(),
            self.status_line_context_operation_available(),
            self.execution_details.last_turn_state().label(),
            cancellable_active_turn,
            hard_stop_targets,
        )
    }

    fn status_line_model_reasoning_available(&self) -> bool {
        status_line::status_line_model_reasoning_available(
            self.selected_thread_id(),
            self.selected_thread_status.as_ref(),
        )
    }

    fn status_line_context_operation_available(&self) -> bool {
        status_line::status_line_context_operation_available(
            self.selected_thread_id(),
            self.selected_thread_status.as_ref(),
        )
    }

    fn set_thread_session_metadata(&mut self, metadata: ThreadSessionMetadata) {
        let selected_thread_id = self.selected_thread_id().map(str::to_string);
        self.status_line
            .set_session_metadata_for_thread(selected_thread_id.as_deref(), metadata);
    }

    fn pending_turn_start_options(&self, selected_thread_id: Option<&str>) -> TurnStartOptions {
        self.status_line
            .pending_turn_start_options(selected_thread_id)
    }

    fn effective_turn_context_defaults(
        &self,
        selected_thread_id: Option<&str>,
    ) -> ThreadTurnDefaults {
        self.status_line
            .effective_turn_context_defaults(selected_thread_id)
    }

    fn promote_pending_turn_defaults(&mut self, thread_id: &str) -> bool {
        self.status_line.promote_pending_turn_defaults(thread_id)
    }

    fn set_effective_new_thread_defaults(&mut self, defaults: Option<ThreadTurnDefaults>) -> bool {
        self.status_line.set_effective_new_thread_defaults(defaults)
    }

    fn bind_pending_new_thread_defaults_to_thread(&mut self, thread_id: &str) -> bool {
        self.status_line
            .bind_pending_new_thread_defaults_to_thread(thread_id)
    }

    fn apply_token_usage_update(
        &mut self,
        thread_id: String,
        turn_id: String,
        token_usage: beryl_backend::ThreadTokenUsage,
    ) -> bool {
        let known_thread = self
            .known_threads
            .iter()
            .any(|thread| thread.id == thread_id);
        self.status_line
            .apply_token_usage(known_thread, thread_id, turn_id, token_usage)
    }

    fn apply_account_rate_limits_update(
        &mut self,
        rate_limits: beryl_backend::RateLimitSnapshot,
    ) -> bool {
        self.status_line.apply_account_rate_limits(rate_limits)
    }

    fn replace_account_rate_limits(
        &mut self,
        rate_limits: beryl_backend::AccountRateLimitsResponse,
    ) -> bool {
        self.status_line.replace_account_rate_limits(rate_limits)
    }

    fn hydrate_token_usage_snapshots(
        &mut self,
        workspace_state: &WorkspaceConversationState,
    ) -> bool {
        let known_thread_ids = self
            .known_threads
            .iter()
            .map(|thread| thread.id.as_str())
            .collect::<Vec<_>>();
        self.status_line
            .hydrate_token_usage_snapshots(workspace_state, |thread_id| {
                known_thread_ids.contains(&thread_id)
            })
    }

    fn hydrate_selected_thread_token_usage_snapshot(
        &mut self,
        workspace_state: &WorkspaceConversationState,
    ) -> bool {
        let Some(thread_id) = self.selected_thread_id().map(str::to_string) else {
            return false;
        };
        let thread_id = ConversationThreadId::new(thread_id);
        let Some(thread) = workspace_state.thread_registration(&thread_id) else {
            return false;
        };
        self.hydrate_thread_token_usage_snapshot(thread)
    }

    fn hydrate_thread_token_usage_snapshot(
        &mut self,
        thread: &RegisteredConversationThread,
    ) -> bool {
        let Some(snapshot) = thread.token_usage_snapshot() else {
            return false;
        };
        let thread_id = thread.thread_id().as_str().to_string();
        let known_thread = self.known_threads.iter().any(|known| known.id == thread_id);
        self.status_line
            .apply_token_usage_snapshot(known_thread, thread_id, snapshot)
    }

    fn apply_thread_name_update(
        &mut self,
        workspace_state: &WorkspaceConversationState,
        thread_id: &ConversationThreadId,
        thread_name: Option<&str>,
    ) -> bool {
        let thread_name = normalized_thread_name(thread_name);
        let mut known_thread_changed = false;
        for thread in &mut self.known_threads {
            if thread.id == thread_id.as_str() && thread.name != thread_name {
                thread.name.clone_from(&thread_name);
                known_thread_changed = true;
            }
        }

        let inventory_changed = self.member_thread_inventory.update_thread_backend_name(
            workspace_state,
            thread_id,
            thread_name.as_deref(),
        );
        if inventory_changed {
            self.reconcile_thread_selector_state();
        }

        known_thread_changed || inventory_changed
    }

    fn transcript_list_state(&self) -> ListState {
        self.transcript_list_state.clone()
    }

    fn transcript_submit_anchor_snapshot(&self) -> Option<TranscriptSubmitAnchorSnapshot> {
        self.transcript_submit_anchor
            .as_ref()
            .map(TranscriptSubmitAnchor::snapshot)
    }

    fn loaded_history_anchor_pending(&self) -> bool {
        self.loaded_history_anchor_pending
    }

    fn older_history_loading(&self) -> bool {
        self.transcript_history_window.is_loading_older()
    }

    fn transcript_content_release_generation(&self) -> u64 {
        self.transcript_content_release_generation
    }

    fn transcript_reset_generation(&self) -> u64 {
        self.transcript_reset_generation
    }

    fn transcript_content_release_row_identities(&self) -> &[String] {
        &self.transcript_content_release_row_identities
    }

    fn begin_loading_thread_history_page(
        &mut self,
        visible_range: &std::ops::Range<usize>,
    ) -> Option<(String, TranscriptHistoryPageRequest)> {
        let thread_id = self.selected_thread_id()?.to_string();
        let source_visible_range = self
            .transcript_presentation
            .source_range_for_presentation_range(visible_range);
        let request = self
            .transcript_history_window
            .begin_loading_page_for_visible_range(&source_visible_range)?;
        Some((thread_id, request))
    }

    fn finish_loading_thread_history_page(
        &mut self,
        thread_id: &str,
        request: TranscriptHistoryPageRequest,
        page: LoadedTranscriptHistoryPage,
        image_resolver: &TranscriptImagePathResolver,
    ) -> usize {
        if self.selected_thread_id() != Some(thread_id) {
            self.transcript_history_window.fail_loading_older();
            return 0;
        }

        match request {
            TranscriptHistoryPageRequest::Older { .. } => {
                self.composer_image_labels
                    .observe_thread_turns(thread_id, &page.turns);
                let prepended = self
                    .execution_details
                    .prepend_thread_history_page_with_image_resolver(
                        thread_id,
                        page.turns.clone(),
                        image_resolver,
                    );
                self.transcript_history_window
                    .finish_loading_older_with_turn_ids(&page, prepended.turn_ids);
                if prepended.added_count > 0 {
                    let visible_added = self.transcript_presentation.prepend_from_turns(
                        &self.execution_details.turns()[..prepended.added_count],
                    );
                    self.shift_transcript_anchor(visible_added);
                    if visible_added > 0 {
                        self.transcript_list_state.splice(0..0, visible_added);
                    }
                    self.release_cold_history_pages_around_current_view();
                }
                prepended.added_count
            }
            TranscriptHistoryPageRequest::Released { page_id, .. } => {
                self.composer_image_labels
                    .observe_thread_turns(thread_id, &page.turns);
                let Some(restored) = self
                    .transcript_history_window
                    .finish_loading_released_page(page_id, &page)
                else {
                    return 0;
                };
                let replacements = self
                    .execution_details
                    .restore_history_page_with_image_resolver(
                        thread_id,
                        restored.range.start,
                        &restored.turn_ids,
                        page.turns,
                        image_resolver,
                    );
                let restored_count = replacements.len();
                for replacement in replacements {
                    self.transcript_presentation
                        .replace_turn(replacement.index, replacement.turn);
                }
                self.release_cold_history_pages_around_current_view();
                restored_count
            }
        }
    }

    fn finish_loading_older_history_failure(&mut self) {
        self.transcript_history_window.fail_loading_older();
    }

    fn release_cold_history_pages(&mut self, visible_range: &std::ops::Range<usize>) -> bool {
        let source_visible_range = self
            .transcript_presentation
            .source_range_for_presentation_range(visible_range);
        let releases = self
            .transcript_history_window
            .release_cold_pages(&source_visible_range);
        if releases.is_empty() {
            return false;
        }

        for release in releases {
            let replacements = self
                .execution_details
                .release_history_range(release.range.clone());
            let mut released_row_identities = Vec::new();
            for replacement in replacements {
                let presentation_index = self
                    .transcript_presentation
                    .presentation_index_for_source_turn(replacement.index);
                if let Some(row_identity) = presentation_index
                    .and_then(|presentation_index| {
                        self.transcript_presentation
                            .row_identity(presentation_index)
                    })
                    .map(|identity| identity.as_str().to_string())
                {
                    released_row_identities.push(row_identity);
                }
                let placeholder_height = presentation_index.and_then(|presentation_index| {
                    self.transcript_list_state
                        .measured_item_size(presentation_index)
                        .map(|size| size.height)
                });
                self.transcript_presentation.replace_turn_with_placeholder(
                    replacement.index,
                    replacement.turn,
                    placeholder_height,
                );
            }
            if !released_row_identities.is_empty() {
                self.note_transcript_content_release(released_row_identities);
            }
        }

        true
    }

    fn note_transcript_content_release(&mut self, row_identities: Vec<String>) {
        self.transcript_content_release_generation =
            self.transcript_content_release_generation.saturating_add(1);
        self.transcript_content_release_row_identities = row_identities;
        self.reconcile_transcript_branch_menu_target();
        self.reconcile_transcript_edit_mode();
    }

    fn release_cold_history_pages_around_current_view(&mut self) -> bool {
        let visible_range = self.transcript_list_state.visible_range();
        self.release_cold_history_pages(&visible_range)
    }

    fn set_transcript_user_scrolled(&mut self, is_scrolled: bool) -> bool {
        if self.transcript_user_scrolled == is_scrolled {
            return false;
        }
        self.transcript_user_scrolled = is_scrolled;
        true
    }

    fn transcript_width(&self) -> Pixels {
        let total_width = self
            .split_bounds
            .or(self.layout_bounds)
            .map(|bounds| bounds.size.width)
            .unwrap_or_else(|| px(layout::WINDOW_MIN_WIDTH));
        layout::split_layout(
            total_width,
            self.checklist_sidebar_ratio,
            self.checklist_sidebar_visible(),
        )
        .left_width
    }

    fn graph_overlay(&self) -> &GraphOverlayState {
        &self.graph_overlay
    }

    fn reserve_optimistic_graph_mutation_id(&mut self) -> OptimisticGraphMutationId {
        self.graph_overlay.reserve_optimistic_mutation_id()
    }

    fn graph_thread_link_menu(&self) -> &graph_link_menu::GraphThreadLinkMenuState {
        &self.graph_thread_link_menu
    }

    fn thread_selector(&self) -> &ThreadSelectorState {
        &self.thread_selector
    }

    fn thread_selector_mut(&mut self) -> &mut ThreadSelectorState {
        &mut self.thread_selector
    }

    fn graph_thread_link_menu_mut(&mut self) -> &mut graph_link_menu::GraphThreadLinkMenuState {
        &mut self.graph_thread_link_menu
    }

    fn checklist_thread_start_menu(&self) -> &ChecklistThreadStartMenuState {
        &self.checklist_thread_start_menu
    }

    fn checklist_thread_start_menu_mut(&mut self) -> &mut ChecklistThreadStartMenuState {
        &mut self.checklist_thread_start_menu
    }

    fn member_thread_inventory(&self) -> &MemberThreadInventoryState {
        &self.member_thread_inventory
    }

    fn member_thread_inventory_mut(&mut self) -> &mut MemberThreadInventoryState {
        &mut self.member_thread_inventory
    }

    fn workspace_rename_blockers(&self) -> WorkspaceRenameBlockers {
        WorkspaceRenameBlockers {
            graph_work: self.graph_overlay.mutation_pending()
                || self.graph_thread_link_menu.delete_hold_active(),
            transcript_work: self.pending_thread_activation.is_some()
                || self.context_compaction_thread_id.is_some()
                || self.pending_turn_input_queue.is_some()
                || self.pending_active_turn_steering_queue.is_some()
                || self.execution_details.working_turn_index().is_some(),
            inventory_work: self.member_thread_inventory.refreshing()
                || self.member_thread_inventory.needs_refresh(),
            status_work: self.status_line_operations.stop_request_in_flight()
                || self.status_line_operations.hard_stop_hold_active(),
            ..WorkspaceRenameBlockers::default()
        }
    }

    fn graph_columns_scroll_handle(&self) -> ScrollHandle {
        self.graph_column_selector_scroll.horizontal_handle()
    }

    fn thread_selector_columns_scroll_handle(&self) -> ScrollHandle {
        self.thread_column_selector_scroll.horizontal_handle()
    }

    fn checklist_sidebar_projection(&self) -> Option<&ChecklistSidebarProjection> {
        self.checklist_sidebar_projection.projection()
    }

    fn checklist_sidebar_row(&self, index: usize) -> Option<ChecklistSidebarRow> {
        self.checklist_sidebar_projection
            .projection()?
            .row(self.graph_overlay.graph(), index)
    }

    fn checklist_sidebar_viewport_height_hint(&self) -> Pixels {
        self.layout_bounds
            .map(|bounds| (bounds.size.height - px(96.0)).max(px(0.0)))
            .unwrap_or_else(|| px(layout::WINDOW_MIN_HEIGHT - 96.0))
    }

    fn composer_scroll_handle(&self) -> ScrollHandle {
        self.composer_scroll_handle.clone()
    }

    fn release_transcript_submit_anchor(&mut self) -> bool {
        release_forced_submit_anchor(&mut self.transcript_submit_anchor)
    }

    fn shift_transcript_anchor(&mut self, amount: usize) {
        if let Some(anchor) = self.transcript_submit_anchor.as_mut() {
            anchor.shift_turn_index(amount);
        }
    }

    fn install_loaded_history_transcript_anchor(&mut self) -> bool {
        if !self.loaded_history_anchor_pending {
            return false;
        }
        self.loaded_history_anchor_pending = false;
        if self.transcript_submit_anchor.is_some() {
            return false;
        }

        let previous_turn_count = self.transcript_presentation.len();
        let Some((turn_index, fragment_index, user_input)) = self.latest_user_prompt_anchor()
        else {
            return false;
        };

        self.transcript_submit_anchor = Some(TranscriptSubmitAnchor::passive(
            turn_index,
            fragment_index,
            user_input,
        ));
        self.transcript_user_scrolled = false;
        self.sync_live_transcript_rows(previous_turn_count);
        if previous_turn_count > 0 {
            self.transcript_list_state
                .scroll_to_reveal_item_end(previous_turn_count - 1);
        }
        true
    }

    fn latest_user_prompt_anchor(&self) -> Option<(usize, usize, String)> {
        self.transcript_presentation.latest_user_prompt_anchor()
    }

    fn should_reveal_composer_cursor(
        &self,
        text: &str,
        cursor_offset: usize,
        text_width: Pixels,
        input_content_height: Pixels,
        visible_input_height: Pixels,
    ) -> bool {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let snapshot = ComposerRevealSnapshot {
            text_hash: hasher.finish(),
            cursor_offset,
            text_width,
            input_content_height,
            visible_input_height,
        };
        if self.composer_reveal_snapshot.get() == Some(snapshot) {
            return false;
        }

        self.composer_reveal_snapshot.set(Some(snapshot));
        true
    }

    fn graph_column_scroll_handle(&self, column_index: usize) -> Option<ScrollHandle> {
        self.graph_column_selector_scroll
            .column_handle(column_index)
    }

    fn thread_selector_column_scroll_handle(&self, column_index: usize) -> Option<ScrollHandle> {
        self.thread_column_selector_scroll
            .column_handle(column_index)
    }

    fn checklist_sidebar_visible(&self) -> bool {
        self.checklist_sidebar_visibility.visible()
    }

    fn selected_checklist_node_id(&self) -> Option<&SemanticNodeId> {
        let selected_node_id = self.graph_overlay.selected_node_id()?;
        let selected_node = self.graph_overlay.graph().node(selected_node_id)?;
        selected_node
            .facets()
            .has_checklist()
            .then_some(selected_node_id)
    }

    fn notice(&self) -> Option<&SurfaceNotice> {
        self.notices.active()
    }

    fn pending_thread_activation_label(&self) -> Option<&str> {
        self.pending_thread_activation
            .as_ref()
            .map(|pending| pending.label.as_str())
    }

    fn checklist_sidebar_ratio(&self) -> f32 {
        self.checklist_sidebar_ratio
    }

    fn snapshot(&self) -> Self {
        Self {
            known_threads: self.known_threads.clone(),
            selected_thread: self.selected_thread,
            selected_thread_status: self.selected_thread_status.clone(),
            execution_details: self.execution_details.clone(),
            transcript_presentation: self.transcript_presentation.clone(),
            tool_activity: self.tool_activity.clone(),
            hard_stop_targets: self.hard_stop_targets.clone(),
            lifecycle_yields: self.lifecycle_yields.clone(),
            tool_activity_panel_mode: self.tool_activity_panel_mode,
            tool_activity_panel_height: self.tool_activity_panel_height,
            status_line: self.status_line.clone(),
            status_line_operations: self.status_line_operations.clone(),
            transcript_submit_anchor: self.transcript_submit_anchor.clone(),
            loaded_history_anchor_pending: self.loaded_history_anchor_pending,
            transcript_user_scrolled: self.transcript_user_scrolled,
            transcript_history_window: self.transcript_history_window.clone(),
            transcript_reset_generation: self.transcript_reset_generation,
            transcript_content_release_generation: self.transcript_content_release_generation,
            transcript_content_release_row_identities: self
                .transcript_content_release_row_identities
                .clone(),
            invalidated_stream_turns: self.invalidated_stream_turns.clone(),
            pending_thread_activation: self.pending_thread_activation.clone(),
            context_compaction_thread_id: self.context_compaction_thread_id.clone(),
            composer_image_labels: self.composer_image_labels.clone(),
            pending_new_thread_label_scope_id: self.pending_new_thread_label_scope_id,
            next_pending_new_thread_label_scope_id: self.next_pending_new_thread_label_scope_id,
            pending_new_thread_label_scope_bindings: self
                .pending_new_thread_label_scope_bindings
                .clone(),
            composer_history: self.composer_history.clone(),
            pending_turn_input_queue: self.pending_turn_input_queue.clone(),
            pending_active_turn_steering_queue: self.pending_active_turn_steering_queue.clone(),
            notices: self.notices.clone(),
            transcript_list_state: self.transcript_list_state.clone(),
            graph_overlay: self.graph_overlay.clone(),
            thread_selector: self.thread_selector.clone(),
            graph_thread_link_menu: self.graph_thread_link_menu.clone(),
            transcript_branch_menu: self.transcript_branch_menu.clone(),
            transcript_edit_mode: self.transcript_edit_mode.clone(),
            checklist_thread_start_menu: self.checklist_thread_start_menu.clone(),
            checklist_sidebar_projection: self.checklist_sidebar_projection.clone(),
            member_thread_inventory: self.member_thread_inventory.clone(),
            graph_column_selector_scroll: self.graph_column_selector_scroll.clone(),
            thread_column_selector_scroll: self.thread_column_selector_scroll.clone(),
            tool_activity_scroll_handle: self.tool_activity_scroll_handle.clone(),
            composer_scroll_handle: self.composer_scroll_handle.clone(),
            composer_reveal_snapshot: Cell::new(self.composer_reveal_snapshot.get()),
            graph_overlay_panel_height: self.graph_overlay_panel_height,
            checklist_sidebar_visibility: self.checklist_sidebar_visibility.clone(),
            checklist_sidebar_ratio: self.checklist_sidebar_ratio,
            layout_bounds: None,
            split_bounds: None,
            divider_drag: None,
            graph_overlay_drag: None,
            tool_activity_panel_drag: None,
        }
    }

    fn snapshot_for_backend_reopen(&self) -> Self {
        let mut snapshot = self.snapshot();
        snapshot
            .member_thread_inventory
            .prepare_for_backend_reopen();
        snapshot.tool_activity.clear_all();
        snapshot.hard_stop_targets.clear_all();
        snapshot.lifecycle_yields.clear_all();
        snapshot
            .status_line_operations
            .clear_stop_requests_for_backend_exit();
        snapshot.cancel_transcript_edit_mode();
        snapshot
    }

    fn refresh_after_backend_reopen(
        &mut self,
        workspace_state: &WorkspaceConversationState,
        known_threads: Vec<ThreadSummary>,
        hard_stop_capabilities: HardStopCapabilities,
        selected_thread_history: Option<ThreadInfo>,
        selected_thread_history_window: Option<TranscriptHistoryWindow>,
        selected_thread_image_resolver: TranscriptImagePathResolver,
        selected_thread_id: Option<String>,
        selected_thread_session_metadata: Option<ThreadSessionMetadata>,
        notice: Option<SurfaceNotice>,
        graph: SemanticGraph,
        graph_revision: WorkspaceGraphRevision,
        graph_warning: Option<String>,
    ) {
        let previous_selected_thread = self.selected_thread().cloned();
        let known_thread_pin = selected_thread_id.clone().or_else(|| {
            previous_selected_thread
                .as_ref()
                .map(|thread| thread.id.clone())
        });
        let known_threads = bounded_known_threads(known_threads, known_thread_pin.iter().cloned());
        match selected_thread_history {
            Some(thread) => {
                self.known_threads = known_threads;
                self.load_thread_history_window(
                    &thread,
                    selected_thread_history_window.unwrap_or_default(),
                    &selected_thread_image_resolver,
                );
                if let Some(metadata) = selected_thread_session_metadata {
                    self.set_thread_session_metadata(metadata);
                }
            }
            None => {
                self.known_threads = known_threads;
                self.status_line.clear_session_metadata();
                self.selected_thread_status = None;
                if let Some(thread_id) = selected_thread_id {
                    self.select_thread_by_id(&thread_id);
                    if self.selected_thread.is_none()
                        && previous_selected_thread
                            .as_ref()
                            .is_some_and(|thread| thread.id == thread_id)
                    {
                        self.known_threads
                            .insert(0, previous_selected_thread.unwrap());
                        self.selected_thread = Some(0);
                    }
                } else if let Some(previous_selected_thread) = previous_selected_thread {
                    self.selected_thread = self
                        .known_threads
                        .iter()
                        .position(|thread| thread.id == previous_selected_thread.id);
                    if self.selected_thread.is_none() {
                        self.known_threads.insert(0, previous_selected_thread);
                        self.selected_thread = Some(0);
                    }
                } else {
                    self.selected_thread = None;
                    self.selected_thread_status = None;
                }
            }
        }

        if let Some(notice) = notice {
            self.set_notice(notice);
        }
        self.tool_activity.clear_all();
        self.hard_stop_targets.clear_all();
        self.hard_stop_targets
            .set_capabilities(hard_stop_capabilities);
        self.apply_known_thread_agent_labels();
        self.hydrate_token_usage_snapshots(workspace_state);
        self.pending_thread_activation = None;
        self.transcript_branch_menu.close();
        self.cancel_transcript_edit_mode();
        self.finish_graph_mutation(graph, graph_revision, graph_warning);
        self.member_thread_inventory.prepare_for_backend_reopen();
        self.reconcile_graph_scroll_handles();
        self.reconcile_thread_selector_state();
        self.reconcile_checklist_sidebar_visibility();
        self.sync_thread_selector_active_thread();
    }

    fn toggle_checklist_sidebar(&mut self) {
        let selected_checklist_id = self.selected_checklist_node_id().cloned();
        self.checklist_sidebar_visibility
            .toggle(selected_checklist_id.as_ref());
        self.divider_drag = None;
        if !self.checklist_sidebar_visible() {
            self.checklist_thread_start_menu.close();
        }
    }

    fn toggle_graph_overlay(&mut self) -> bool {
        self.graph_overlay_drag = None;
        self.graph_thread_link_menu.close();
        self.transcript_branch_menu.close();
        let visible = self.graph_overlay.toggle_visibility();
        if visible {
            self.close_thread_selector();
        }
        visible
    }

    fn toggle_thread_selector(&mut self) -> bool {
        let snapshot = self.member_thread_inventory.snapshot().clone();
        let active_thread_id = self
            .selected_thread_id()
            .map(|thread_id| ConversationThreadId::new(thread_id.to_string()));
        let opened = self.thread_selector.toggle(&snapshot, active_thread_id);
        if opened {
            self.graph_overlay_drag = None;
            self.graph_overlay.close();
            self.graph_thread_link_menu.close();
            self.transcript_branch_menu.close();
            self.checklist_thread_start_menu.close();
            self.member_thread_inventory.mark_refresh_needed();
            self.thread_column_selector_scroll
                .reconcile(self.thread_selector.columns());
        } else {
            self.thread_column_selector_scroll.clear();
        }
        opened
    }

    fn close_thread_selector(&mut self) -> bool {
        let was_open = self.thread_selector.is_open();
        self.thread_selector.close();
        self.thread_column_selector_scroll.clear();
        was_open
    }

    fn select_thread_selector_member(
        &mut self,
        column_index: usize,
        member_key: MemberThreadInventoryMemberKey,
    ) -> bool {
        let changed = self.thread_selector.select_member(column_index, member_key);
        if changed {
            self.thread_column_selector_scroll
                .reconcile(self.thread_selector.columns());
        }
        changed
    }

    fn select_thread_selector_thread(
        &mut self,
        column_index: usize,
        thread_id: ConversationThreadId,
    ) -> bool {
        let changed = self.thread_selector.select_thread(column_index, thread_id);
        if changed {
            self.thread_column_selector_scroll
                .reconcile(self.thread_selector.columns());
        }
        changed
    }

    fn thread_selector_activation_target(&self) -> Option<ThreadSelectorActivationTarget> {
        self.thread_selector.selected_activation_target()
    }

    fn sync_thread_selector_active_thread(&mut self) {
        let active_thread_id = self
            .selected_thread_id()
            .map(|thread_id| ConversationThreadId::new(thread_id.to_string()));
        self.thread_selector.mark_active_thread(active_thread_id);
    }

    fn select_graph_node(&mut self, column_index: usize, node_id: &SemanticNodeId) -> bool {
        let changed = self.graph_overlay.select_node(column_index, node_id);
        if changed {
            self.reconcile_graph_scroll_handles();
        }
        let visibility_changed = changed && self.reconcile_checklist_sidebar_visibility();
        changed || visibility_changed
    }

    fn select_graph_soft_link(
        &mut self,
        column_index: usize,
        link_id: &SoftLinkId,
        target_node_id: &SemanticNodeId,
    ) -> bool {
        let changed = self
            .graph_overlay
            .select_soft_link(column_index, link_id, target_node_id);
        if changed {
            self.reconcile_graph_scroll_handles();
        }
        let visibility_changed = changed && self.reconcile_checklist_sidebar_visibility();
        changed || visibility_changed
    }

    fn toggle_graph_node_expansion(
        &mut self,
        column_index: usize,
        node_id: &SemanticNodeId,
        depth: usize,
    ) -> bool {
        self.graph_overlay
            .toggle_node_expansion(column_index, node_id, depth)
    }

    fn begin_graph_mutation(&mut self, status_message: impl Into<String>) {
        self.graph_overlay.begin_mutation(status_message);
    }

    fn begin_optimistic_graph_mutation(
        &mut self,
        mutation: GraphOptimisticMutation,
    ) -> Result<(), String> {
        self.graph_overlay
            .begin_optimistic_mutation(mutation)
            .map_err(|error| error.to_string())?;
        self.reconcile_graph_scroll_handles();
        self.reconcile_checklist_sidebar_visibility();
        Ok(())
    }

    fn finish_graph_mutation(
        &mut self,
        graph: SemanticGraph,
        revision: WorkspaceGraphRevision,
        warning: Option<String>,
    ) {
        self.graph_overlay.finish_mutation(graph, revision, warning);
        self.close_stale_graph_node_action_menu();
        self.reconcile_graph_scroll_handles();
        self.reconcile_checklist_sidebar_visibility();
    }

    fn finish_graph_mutation_commit_update(
        &mut self,
        update: GraphMutationCommitUpdate,
    ) -> Result<GraphCommitApplication, String> {
        let application = self
            .graph_overlay
            .finish_mutation_commit_update(update)
            .map_err(|error| error.to_string())?;
        if matches!(application, GraphCommitApplication::Applied { .. }) {
            self.close_stale_graph_node_action_menu();
            self.reconcile_graph_scroll_handles();
            self.reconcile_checklist_sidebar_visibility();
        }
        Ok(application)
    }

    fn fail_graph_mutation(&mut self, error: impl Into<String>) {
        self.graph_overlay.fail_mutation(error);
        self.graph_thread_link_menu_mut().clear_delete_in_flight();
    }

    fn fail_optimistic_graph_mutation(
        &mut self,
        mutation_id: Option<OptimisticGraphMutationId>,
        error: impl Into<String>,
    ) -> Result<(), String> {
        self.graph_overlay
            .fail_optimistic_mutation(mutation_id, error)
            .map_err(|error| error.to_string())?;
        self.graph_thread_link_menu_mut().clear_delete_in_flight();
        self.close_stale_graph_node_action_menu();
        self.reconcile_graph_scroll_handles();
        self.reconcile_checklist_sidebar_visibility();
        Ok(())
    }

    fn report_graph_mutation_failure(&mut self, error: impl Into<String>) {
        let (title, detail) = graph_node_action_policy::graph_mutation_failure_notice(error);
        self.fail_graph_mutation(detail.clone());
        self.set_notice(SurfaceNotice::new(title, detail));
    }

    fn report_optimistic_graph_mutation_failure(
        &mut self,
        mutation_id: Option<OptimisticGraphMutationId>,
        error: impl Into<String>,
    ) {
        let (title, detail) = graph_node_action_policy::graph_mutation_failure_notice(error);
        if let Err(rollback_error) =
            self.fail_optimistic_graph_mutation(mutation_id, detail.clone())
        {
            self.fail_graph_mutation(rollback_error.clone());
            self.set_notice(SurfaceNotice::new(title, rollback_error));
            return;
        }
        self.set_notice(SurfaceNotice::new(title, detail));
    }

    fn close_stale_graph_node_action_menu(&mut self) -> bool {
        let Some(node_id) = self
            .graph_thread_link_menu()
            .active()
            .map(|open| open.node_id().clone())
        else {
            return false;
        };
        if self.graph_overlay.graph().node(&node_id).is_some() {
            return false;
        }
        self.graph_thread_link_menu_mut().close();
        true
    }

    fn reconcile_graph_scroll_handles(&mut self) {
        self.graph_column_selector_scroll
            .reconcile(self.graph_overlay.columns());
    }

    fn reconcile_thread_selector_state(&mut self) {
        let snapshot = self.member_thread_inventory.snapshot();
        self.thread_selector.reconcile_snapshot(snapshot);
        self.thread_column_selector_scroll
            .reconcile(self.thread_selector.columns());
    }

    fn reconcile_checklist_sidebar_visibility(&mut self) -> bool {
        let selected_checklist_id = self.selected_checklist_node_id().cloned();
        let changed = self
            .checklist_sidebar_visibility
            .reconcile_selection(selected_checklist_id.as_ref());
        let projection_changed = self.refresh_checklist_sidebar_projection();
        if !self.checklist_sidebar_visible() {
            self.checklist_thread_start_menu.close();
        }
        changed || projection_changed
    }

    fn refresh_checklist_sidebar_projection(&mut self) -> bool {
        let selected_checklist_id = self.selected_checklist_node_id().cloned();
        let refresh = self
            .checklist_sidebar_projection
            .refresh(self.graph_overlay.graph(), selected_checklist_id.as_ref());
        refresh.changed()
    }

    fn start_new_thread(&mut self) {
        self.selected_thread = None;
        self.selected_thread_status = None;
        self.sync_thread_selector_active_thread();
        self.execution_details.reset();
        self.transcript_presentation.clear();
        self.hard_stop_targets.clear_all();
        self.status_line.clear_session_metadata();
        self.status_line.clear_pending_new_thread_defaults();
        self.transcript_submit_anchor = None;
        self.loaded_history_anchor_pending = false;
        self.transcript_user_scrolled = false;
        self.transcript_history_window = TranscriptHistoryWindow::default();
        self.transcript_reset_generation = self.transcript_reset_generation.saturating_add(1);
        self.transcript_content_release_generation = 0;
        self.transcript_content_release_row_identities.clear();
        self.invalidated_stream_turns.clear();
        self.pending_thread_activation = None;
        self.context_compaction_thread_id = None;
        self.transcript_branch_menu.close();
        self.cancel_transcript_edit_mode();
        self.composer_image_labels.reset_pending_new_thread();
        self.pending_new_thread_label_scope_id = self.next_pending_new_thread_label_scope_id;
        self.next_pending_new_thread_label_scope_id = self
            .next_pending_new_thread_label_scope_id
            .saturating_add(1);
        self.pending_turn_input_queue = None;
        self.pending_active_turn_steering_queue = None;
        self.notices.clear_all();
        self.transcript_list_state.reset(0);
    }

    fn begin_thread_activation(&mut self, label: impl Into<String>) {
        self.pending_thread_activation = Some(PendingThreadActivation {
            label: label.into(),
        });
        self.notices.clear_all();
        self.transcript_branch_menu.close();
        self.cancel_transcript_edit_mode();
    }

    fn clear_pending_thread_activation(&mut self) {
        self.pending_thread_activation = None;
    }

    fn load_thread_history(&mut self, thread: &ThreadInfo) {
        self.load_thread_history_window(
            thread,
            TranscriptHistoryWindow::default(),
            &TranscriptImagePathResolver::default(),
        );
    }

    fn load_thread_history_window(
        &mut self,
        thread: &ThreadInfo,
        history_window: TranscriptHistoryWindow,
        image_resolver: &TranscriptImagePathResolver,
    ) {
        let load_started = Instant::now();
        let thread_id = thread.summary().id;
        self.upsert_selected_thread(thread.summary());
        self.selected_thread_status = Some(thread.status.clone());
        self.sync_thread_selector_active_thread();
        self.composer_image_labels.observe_thread_history(thread);
        self.composer_image_labels
            .prepare_thread_history_scan(&thread.summary().id, history_window.has_older_pages());
        let execution_detail_started = Instant::now();
        self.execution_details
            .load_thread_history_with_image_resolver(thread, image_resolver);
        let execution_detail_elapsed = execution_detail_started.elapsed();
        self.hard_stop_targets.clear_all();
        let presentation_started = Instant::now();
        self.transcript_presentation
            .replace_from_turns(self.execution_details.turns());
        let presentation_elapsed = presentation_started.elapsed();
        if memory_diagnostics::enabled() {
            let turn_count = self.execution_details.turns().len();
            let item_count = self
                .execution_details
                .turns()
                .iter()
                .map(|turn| turn.items.len())
                .sum::<usize>();
            let generated_image_count = self
                .execution_details
                .turns()
                .iter()
                .flat_map(|turn| turn.items.iter())
                .filter(|item| matches!(item, ExecutionItem::GeneratedImage(_)))
                .count();
            MemoryMilestone::new("transcript_projection_update")
                .thread_id(thread_id.as_str())
                .history_counts(turn_count, item_count, generated_image_count)
                .retained_state_if_enabled(|| self.retained_state_snapshot())
                .log();
        }
        self.status_line.clear_session_metadata();
        self.transcript_submit_anchor = None;
        self.loaded_history_anchor_pending = self.latest_user_prompt_anchor().is_some();
        self.transcript_user_scrolled = false;
        self.transcript_history_window = history_window;
        self.transcript_reset_generation = self.transcript_reset_generation.saturating_add(1);
        self.transcript_content_release_generation = 0;
        self.transcript_content_release_row_identities.clear();
        self.invalidated_stream_turns.clear();
        self.pending_thread_activation = None;
        self.context_compaction_thread_id = None;
        self.transcript_branch_menu.close();
        self.cancel_transcript_edit_mode();
        self.pending_turn_input_queue = None;
        self.pending_active_turn_steering_queue = None;
        self.notices.clear_all();
        self.transcript_list_state
            .reset(self.transcript_list_item_count());
        debug!(
            thread_id = thread_id.as_str(),
            execution_detail_load_history_ms = elapsed_ms(execution_detail_elapsed),
            presentation_replace_from_turns_ms = elapsed_ms(presentation_elapsed),
            surface_load_thread_history_window_ms = elapsed_ms(load_started.elapsed()),
            "loaded thread history window into conversation surface"
        );
    }

    fn begin_turn(&mut self, user_input: UserInputFragment) {
        let thread_id = self.selected_thread_id().map(str::to_string);
        self.begin_turn_for_thread_id(thread_id, user_input);
    }

    fn begin_turn_for_thread(&mut self, thread_id: &str, user_input: UserInputFragment) {
        self.begin_turn_for_thread_id(Some(thread_id.to_string()), user_input);
    }

    fn begin_turn_for_thread_id(
        &mut self,
        thread_id: Option<String>,
        user_input: UserInputFragment,
    ) {
        let before = self.transcript_presentation.len();
        let anchor_text = user_input.text.clone();
        self.observe_composer_image_labels_in_fragment(&user_input);
        let turn_index = self
            .execution_details
            .begin_turn_with_thread_fragments(thread_id.clone(), vec![user_input]);
        let presentation_index = self
            .execution_details
            .turns()
            .get(turn_index)
            .and_then(|turn| {
                self.transcript_presentation
                    .append_turn(turn_index, turn.clone())
            });
        self.transcript_submit_anchor =
            presentation_index.map(|index| TranscriptSubmitAnchor::new(index, 0, anchor_text));
        self.loaded_history_anchor_pending = false;
        self.transcript_user_scrolled = false;
        self.notices.clear_all();
        self.transcript_branch_menu.close();
        self.cancel_transcript_edit_mode();
        if self.selected_thread_id().is_some() && thread_id.as_deref() == self.selected_thread_id()
        {
            self.selected_thread_status = Some(ThreadStatus::Active {
                active_flags: Vec::new(),
            });
        }
        self.sync_live_transcript_rows(before);
    }

    fn selected_active_turn_steering_target(&self) -> Option<ActiveTurnSteeringTarget> {
        if self.selected_thread_context_compaction_id().is_some() {
            return None;
        }

        let thread_id = self.selected_thread_id()?.to_string();
        let ActiveTurnIdentity {
            turn_index,
            thread_id: active_thread_id,
            turn_id,
        } = self.execution_details.active_turn_identity()?;
        if active_thread_id
            .as_deref()
            .is_some_and(|id| id != thread_id)
        {
            return None;
        }

        Some(ActiveTurnSteeringTarget {
            thread_id,
            turn_index,
            turn_id,
        })
    }

    fn selected_cancellable_active_turn(&self) -> Option<CancellableActiveTurn> {
        let selected_thread_id = self.selected_thread_id()?;
        if let Some(target) = self
            .status_line
            .context_compaction_cancellation_target(Some(selected_thread_id))
        {
            return Some(target);
        }
        if self.selected_thread_context_compaction_id().is_some() {
            return None;
        }

        let ActiveTurnIdentity {
            thread_id, turn_id, ..
        } = self.execution_details.active_turn_identity()?;
        if thread_id.as_deref() != Some(selected_thread_id) {
            return None;
        }
        Some(CancellableActiveTurn::ordinary(
            selected_thread_id,
            turn_id?,
        ))
    }

    fn append_active_turn_steering_fragment(
        &mut self,
        target: &ActiveTurnSteeringTarget,
        user_input: UserInputFragment,
    ) -> Option<SteeringInputFragment> {
        let before = self.transcript_presentation.len();
        let anchor_text = user_input.text.clone();
        let steering_fragment =
            SteeringInputFragment::from_user_input_fragment(target.turn_index, &user_input);
        self.observe_composer_image_labels_in_thread_fragment(&target.thread_id, &user_input);
        let fragment_index = self
            .execution_details
            .turns()
            .get(target.turn_index)?
            .user_input_fragments()
            .len();
        self.execution_details
            .append_user_input_fragment(target.turn_index, user_input)?;
        let presentation_index = self
            .execution_details
            .turns()
            .get(target.turn_index)
            .and_then(|turn| {
                self.transcript_presentation
                    .replace_turn(target.turn_index, turn.clone())
                    .or_else(|| {
                        self.transcript_presentation
                            .append_turn(target.turn_index, turn.clone())
                    })
            });
        self.transcript_submit_anchor = presentation_index
            .map(|index| TranscriptSubmitAnchor::new(index, fragment_index, anchor_text));
        self.loaded_history_anchor_pending = false;
        self.transcript_user_scrolled = false;
        self.notices.clear_all();
        self.sync_live_transcript_rows(before);
        Some(steering_fragment)
    }

    fn queue_pending_active_turn_steering_fragment(
        &mut self,
        thread_id: String,
        turn_index: usize,
        fragment: SteeringInputFragment,
    ) -> bool {
        match PendingActiveTurnSteeringQueue::submission_plan(
            self.pending_active_turn_steering_queue.as_ref(),
            &thread_id,
            turn_index,
        ) {
            Some(PendingActiveTurnSteeringSubmissionPlan::AppendToQueue) => {
                if let Some(queue) = self.pending_active_turn_steering_queue.as_mut() {
                    if let Err(error) = queue.try_append(
                        fragment,
                        SteeringInputFragment::retained_payload_bytes_lower_bound,
                    ) {
                        self.report_pending_input_admission_error(error);
                        return false;
                    }
                }
                true
            }
            Some(PendingActiveTurnSteeringSubmissionPlan::StartQueue) => {
                if let Err(error) = validate_pending_active_turn_steering_first_fragment(&fragment)
                {
                    self.report_pending_input_admission_error(error);
                    return false;
                }
                self.pending_active_turn_steering_queue = Some(
                    PendingActiveTurnSteeringQueue::new(thread_id, turn_index, fragment),
                );
                true
            }
            None => false,
        }
    }

    fn pending_active_turn_steering_admission(
        &self,
        thread_id: &str,
        turn_index: usize,
        fragment: &SteeringInputFragment,
    ) -> Result<bool, pending_turn_input::PendingInputAdmissionError> {
        match PendingActiveTurnSteeringQueue::<SteeringInputFragment>::submission_plan(
            self.pending_active_turn_steering_queue.as_ref(),
            thread_id,
            turn_index,
        ) {
            Some(PendingActiveTurnSteeringSubmissionPlan::AppendToQueue) => {
                let Some(queue) = self.pending_active_turn_steering_queue.as_ref() else {
                    return Ok(false);
                };
                queue.validate_append(
                    fragment,
                    SteeringInputFragment::retained_payload_bytes_lower_bound,
                )?;
                Ok(true)
            }
            Some(PendingActiveTurnSteeringSubmissionPlan::StartQueue) => {
                validate_pending_active_turn_steering_first_fragment(fragment)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    fn take_pending_active_turn_steering_fragments(
        &mut self,
        thread_id: &str,
        turn_index: usize,
    ) -> Option<Vec<SteeringInputFragment>> {
        if !self
            .pending_active_turn_steering_queue
            .as_ref()
            .is_some_and(|queue| queue.is_for_turn(thread_id, turn_index))
        {
            return None;
        }

        Some(
            self.pending_active_turn_steering_queue
                .take()?
                .into_fragments(),
        )
    }

    fn take_pending_active_turn_steering_for_started_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
    ) -> Option<Vec<SteeringInputFragment>> {
        let active = self.execution_details.active_turn_identity()?;
        if active.thread_id.as_deref() != Some(thread_id)
            || active.turn_id.as_deref() != Some(turn_id)
        {
            return None;
        }
        self.take_pending_active_turn_steering_fragments(thread_id, active.turn_index)
    }

    fn move_steering_fragments_to_pending_turn(
        &mut self,
        thread_id: String,
        execution_target: WorkspaceId,
        automatic_title_generation_allowed: bool,
        turn_options: TurnStartOptions,
        fragments: Vec<SteeringInputFragment>,
    ) -> bool {
        if fragments.is_empty() {
            return false;
        }

        let user_inputs = fragments
            .iter()
            .cloned()
            .map(SteeringInputFragment::into_user_input_fragment)
            .collect::<Vec<_>>();
        match pending_turn_input::validate_pending_turn_input_fragments(
            self.pending_turn_input_queue.as_ref(),
            &thread_id,
            &execution_target,
            automatic_title_generation_allowed,
            &turn_options,
            self.execution_details.turns().len(),
            &user_inputs,
        ) {
            Ok(true) => {}
            Ok(false) => return false,
            Err(error) => {
                self.report_pending_input_admission_error(error);
                return false;
            }
        }

        let mut queued = true;
        for user_input in user_inputs {
            queued &= self.queue_pending_turn_fragment(
                thread_id.clone(),
                execution_target.clone(),
                automatic_title_generation_allowed,
                turn_options.clone(),
                user_input,
            );
        }
        debug_assert!(queued, "pending turn input was validated before queueing");
        if !queued {
            return false;
        }

        let removals = fragments
            .iter()
            .map(|fragment| {
                (
                    fragment.turn_index,
                    fragment.fragment_id,
                    fragment.text.as_str(),
                )
            })
            .collect::<Vec<_>>();
        let affected_turns = self
            .execution_details
            .remove_user_input_fragments(&removals);
        for turn_index in affected_turns {
            if let Some(turn) = self.execution_details.turns().get(turn_index) {
                self.transcript_presentation
                    .replace_turn(turn_index, turn.clone());
            }
        }

        true
    }

    fn queue_pending_turn_fragment(
        &mut self,
        thread_id: String,
        execution_target: WorkspaceId,
        automatic_title_generation_allowed: bool,
        turn_options: TurnStartOptions,
        user_input: UserInputFragment,
    ) -> bool {
        let before = self.transcript_presentation.len();
        let anchor_text = user_input.text.clone();
        self.observe_composer_image_labels_in_thread_fragment(&thread_id, &user_input);
        let queued = match PendingTurnInputQueue::submission_plan(
            self.pending_turn_input_queue.as_ref(),
            &thread_id,
        ) {
            Some(PendingTurnInputSubmissionPlan::AppendToQueue {
                turn_index,
                fragment_index,
            }) => {
                if let Some(queue) = self.pending_turn_input_queue.as_mut() {
                    match queue.try_append(user_input.clone()) {
                        Ok(index) => debug_assert_eq!(index, fragment_index),
                        Err(error) => {
                            self.report_pending_input_admission_error(error);
                            return false;
                        }
                    }
                }
                self.execution_details
                    .append_user_input_fragment(turn_index, user_input);
                Some((turn_index, fragment_index))
            }
            Some(PendingTurnInputSubmissionPlan::StartQueue) => {
                let queue = match PendingTurnInputQueue::try_new(
                    thread_id,
                    execution_target,
                    automatic_title_generation_allowed,
                    turn_options,
                    self.execution_details.turns().len(),
                    user_input.clone(),
                ) {
                    Ok(queue) => queue,
                    Err(error) => {
                        self.report_pending_input_admission_error(error);
                        return false;
                    }
                };
                let turn_index = self
                    .execution_details
                    .begin_pending_turn_with_fragments(vec![user_input.clone()]);
                debug_assert_eq!(queue.turn_index(), turn_index);
                self.pending_turn_input_queue = Some(queue);
                Some((turn_index, 0))
            }
            None => None,
        };

        let Some((turn_index, fragment_index)) = queued else {
            return false;
        };

        let presentation_index = self
            .execution_details
            .turns()
            .get(turn_index)
            .and_then(|turn| {
                self.transcript_presentation
                    .replace_turn(turn_index, turn.clone())
                    .or_else(|| {
                        self.transcript_presentation
                            .append_turn(turn_index, turn.clone())
                    })
            });
        self.transcript_submit_anchor = presentation_index
            .map(|index| TranscriptSubmitAnchor::new(index, fragment_index, anchor_text));
        self.loaded_history_anchor_pending = false;
        self.transcript_user_scrolled = false;
        self.notices.clear_all();
        self.sync_live_transcript_rows(before);
        true
    }

    fn report_pending_input_admission_error(
        &mut self,
        error: pending_turn_input::PendingInputAdmissionError,
    ) {
        self.set_notice(SurfaceNotice::new("Input queue full", error.user_message()));
    }

    fn selected_thread_context_compaction_id(&self) -> Option<&str> {
        let selected_thread_id = self.selected_thread_id()?;
        self.context_compaction_thread_id
            .as_deref()
            .filter(|thread_id| *thread_id == selected_thread_id)
    }

    fn context_compaction_thread_id(&self) -> Option<&str> {
        self.context_compaction_thread_id.as_deref()
    }

    fn observe_context_compaction_event(&mut self, event: &beryl_backend::TurnStreamEvent) -> bool {
        let Some(thread_id) = self.context_compaction_thread_id.as_deref() else {
            return false;
        };
        let Some(turn_id) = context_compaction::context_compaction_turn_id(thread_id, event) else {
            return false;
        };
        self.status_line
            .set_context_compaction_turn_id(thread_id, turn_id)
    }

    fn take_pending_turn_input_queue_for_thread(
        &mut self,
        thread_id: &str,
    ) -> Option<PendingTurnInputQueue> {
        if !self
            .pending_turn_input_queue
            .as_ref()
            .is_some_and(|queue| queue.is_for_thread(thread_id))
        {
            return None;
        }

        let queue = self.pending_turn_input_queue.take()?;
        if self
            .execution_details
            .activate_pending_turn(queue.turn_index())
            && self.selected_thread_id() == Some(thread_id)
        {
            self.selected_thread_status = Some(ThreadStatus::Active {
                active_flags: Vec::new(),
            });
            self.cancel_transcript_edit_mode();
        }
        Some(queue)
    }

    fn invalidate_stream_turns(
        &mut self,
        thread_id: &str,
        turn_ids: impl IntoIterator<Item = String>,
    ) {
        self.invalidated_stream_turns
            .invalidate_turns(thread_id, turn_ids);
    }

    fn stream_event_targets_invalidated_turn(
        &self,
        event: &beryl_backend::TurnStreamEvent,
    ) -> bool {
        self.invalidated_stream_turns
            .event_targets_invalidated_turn(event)
    }

    fn fail_pending_turn_input_queue_for_thread(
        &mut self,
        thread_id: &str,
        message: impl Into<String>,
    ) -> bool {
        let Some(_queue) = self.take_pending_turn_input_queue_for_thread(thread_id) else {
            return false;
        };
        let _ = self.finish_turn_failure(message);
        true
    }

    fn apply_stream_event(
        &mut self,
        event: beryl_backend::TurnStreamEvent,
        execution_target: Option<&WorkspaceId>,
    ) -> AppliedStreamEvent {
        if self.stream_event_targets_invalidated_turn(&event) {
            return AppliedStreamEvent::default();
        }

        let terminal_turn = match &event {
            beryl_backend::TurnStreamEvent::TurnCompleted { thread_id, turn }
                if turn.is_terminal() =>
            {
                Some((thread_id.clone(), turn.id.clone()))
            }
            _ => None,
        };
        let selected_thread_id = self.selected_thread_id().map(str::to_string);
        self.tool_activity
            .set_selected_thread_id(selected_thread_id.as_deref());
        let tool_activity_agent_label = event.activity().and_then(|activity| {
            (selected_thread_id.as_deref() == Some(activity.thread_id.as_str()))
                .then(|| "Main".to_string())
        });
        self.hard_stop_targets.apply_stream_event(&event);
        self.tool_activity.apply_stream_event_with_execution_target(
            &event,
            tool_activity_agent_label,
            execution_target,
        );

        let selected_thread_completed_turn = match &event {
            beryl_backend::TurnStreamEvent::TurnCompleted { thread_id, turn }
                if selected_thread_id.as_deref() == Some(thread_id.as_str())
                    && turn.is_terminal() =>
            {
                Some(thread_id.clone())
            }
            _ => None,
        };
        let turn_error_notice = match &event {
            beryl_backend::TurnStreamEvent::TurnCompleted { thread_id, turn } => {
                selected_backend_turn_error_notice(self.selected_thread_id(), thread_id, turn)
            }
            _ => None,
        };
        if let beryl_backend::TurnStreamEvent::ThreadStatusChanged { thread_id, status } = &event
            && self.selected_thread_id() == Some(thread_id.as_str())
        {
            self.selected_thread_status = Some(status.clone());
            if !matches!(status, ThreadStatus::Idle) {
                self.cancel_transcript_edit_mode();
            }
        }
        if let Some((thread_id, turn_id)) = terminal_turn.as_ref() {
            self.status_line_operations
                .finish_turn_stop_request_for_target(thread_id, turn_id);
        }

        let before = self.transcript_presentation.len();
        let Some(turn_index) = self.execution_details.apply_stream_event(event) else {
            return AppliedStreamEvent::default();
        };
        let lifecycle_yield = terminal_turn.as_ref().and_then(|(thread_id, turn_id)| {
            self.lifecycle_yields
                .apply_terminal_turn(thread_id, turn_id)
        });
        if let Some(thread_id) = selected_thread_completed_turn {
            self.mark_selected_turn_finished_idle(&thread_id);
        }
        if let Some(notice) = turn_error_notice {
            self.set_notice(notice);
        }
        if let Some(turn) = self.execution_details.turns().get(turn_index) {
            self.transcript_presentation
                .replace_turn(turn_index, turn.clone());
        }
        self.sync_live_transcript_rows(before);
        let suppresses_ordinary_end_turn_sound = lifecycle_yield
            .as_ref()
            .is_some_and(TerminalLifecycleYield::suppresses_ordinary_end_turn_sound);
        AppliedStreamEvent {
            title_candidate: self.completed_turn_title_candidate(turn_index),
            turn_completion_sound: terminal_turn.and_then(|(thread_id, turn_id)| {
                (!suppresses_ordinary_end_turn_sound)
                    .then(|| TurnCompletionSoundCandidate::new(Some(thread_id), Some(turn_id)))
            }),
            lifecycle_yield,
        }
    }

    fn finish_turn_failure(
        &mut self,
        message: impl Into<String>,
    ) -> Option<TurnCompletionSoundCandidate> {
        let message = message.into();
        let active_turn = self.execution_details.active_turn_identity();
        if let Some(ActiveTurnIdentity {
            thread_id: Some(thread_id),
            turn_id: Some(turn_id),
            ..
        }) = active_turn.as_ref()
        {
            self.status_line_operations
                .finish_turn_stop_request_for_target(thread_id, turn_id);
            self.hard_stop_targets.finish_turn(thread_id, turn_id);
            self.lifecycle_yields.clear_turn(thread_id, turn_id);
        } else if let Some(thread_id) = self.selected_thread_id().map(str::to_string) {
            self.hard_stop_targets.finish_thread(&thread_id);
        }
        if let Some(thread_id) = self.selected_thread_id().map(str::to_string) {
            self.finish_running_tool_activity_for_thread_error(&thread_id);
        }
        let before = self.transcript_presentation.len();
        let Some(turn_index) = self.execution_details.finish_turn_failure(message.clone()) else {
            return None;
        };
        self.set_notice(local_turn_failure_notice(message));
        if let Some(thread_id) = self.selected_thread_id().map(str::to_string) {
            self.mark_selected_turn_finished_idle(&thread_id);
        }
        if let Some(turn) = self.execution_details.turns().get(turn_index) {
            self.transcript_presentation
                .replace_turn(turn_index, turn.clone());
        }
        self.sync_live_transcript_rows(before);
        active_turn
            .map(|active| TurnCompletionSoundCandidate::new(active.thread_id, active.turn_id))
    }

    fn select_thread_by_id(&mut self, thread_id: &str) {
        let previous_thread_id = self.selected_thread_id().map(str::to_string);
        self.selected_thread = self
            .known_threads
            .iter()
            .position(|thread| thread.id == thread_id);
        if previous_thread_id.as_deref() != self.selected_thread_id() {
            self.selected_thread_status = None;
            self.cancel_transcript_edit_mode();
        }
        self.sync_thread_selector_active_thread();
    }

    fn upsert_selected_thread(&mut self, thread: ThreadSummary) {
        let thread = bounded_known_thread_summary(thread);
        let previous_thread_id = self.selected_thread_id().map(str::to_string);
        if let Some(index) = self
            .known_threads
            .iter()
            .position(|known| known.id == thread.id)
        {
            self.known_threads[index] = thread;
            self.selected_thread = Some(index);
            if previous_thread_id.as_deref() != self.selected_thread_id() {
                self.selected_thread_status = None;
                self.cancel_transcript_edit_mode();
            }
            self.prune_known_threads();
            self.apply_known_thread_agent_labels();
            return;
        }

        self.known_threads.insert(0, thread);
        self.selected_thread = Some(0);
        if previous_thread_id.as_deref() != self.selected_thread_id() {
            self.selected_thread_status = None;
            self.cancel_transcript_edit_mode();
        }
        self.prune_known_threads();
        self.apply_known_thread_agent_labels();
    }

    fn replace_known_threads(&mut self, known_threads: Vec<ThreadSummary>, active_thread_id: &str) {
        self.known_threads = bounded_known_threads(known_threads, [active_thread_id.to_string()]);
        self.select_thread_by_id(active_thread_id);
        if self.selected_thread.is_none() {
            self.selected_thread = (!self.known_threads.is_empty()).then_some(0);
        }
        self.apply_known_thread_agent_labels();
    }

    fn prune_known_threads(&mut self) {
        let selected_thread_id = self.selected_thread_id().map(str::to_string);
        let pinned_thread_ids = selected_thread_id.iter().cloned().collect::<Vec<_>>();
        let previous_selected_thread_id = selected_thread_id;
        self.known_threads =
            bounded_known_threads(std::mem::take(&mut self.known_threads), pinned_thread_ids);
        self.selected_thread = previous_selected_thread_id.and_then(|thread_id| {
            self.known_threads
                .iter()
                .position(|thread| thread.id == thread_id)
        });
    }

    fn mark_selected_turn_finished_idle(&mut self, active_thread_id: &str) -> bool {
        if self.selected_thread_id() != Some(active_thread_id)
            || self.execution_details.working_turn_index().is_some()
        {
            return false;
        }

        let should_mark_idle = match self.selected_thread_status.as_ref() {
            None => true,
            Some(ThreadStatus::Idle) => false,
            Some(status) if status.waiting_on_user_input() => false,
            Some(ThreadStatus::Active { active_flags }) if active_flags.is_empty() => true,
            Some(
                ThreadStatus::Active { .. } | ThreadStatus::NotLoaded | ThreadStatus::SystemError,
            ) => false,
        };
        if should_mark_idle {
            self.selected_thread_status = Some(ThreadStatus::Idle);
            MemoryMilestone::new("selected_thread_idle_settled")
                .thread_id(active_thread_id)
                .retained_state_if_enabled(|| self.retained_state_snapshot())
                .log();
        }
        should_mark_idle
    }

    fn set_notice(&mut self, notice: SurfaceNotice) {
        self.notices.push(notice);
    }

    fn completed_turn_title_candidate(
        &self,
        turn_index: usize,
    ) -> Option<CompletedTurnTitleCandidate> {
        let turn = self.execution_details.turns().get(turn_index)?;
        if turn.status != TurnExecutionStatus::Completed {
            return None;
        }
        let message = turn.terminal_assistant_message()?;
        if message.text.trim().is_empty() {
            return None;
        }

        Some(CompletedTurnTitleCandidate {
            user_input: turn.first_user_input_fragment_text()?.to_string(),
            assistant_text: message.text.clone(),
        })
    }

    fn clear_notice(&mut self) {
        self.notices.dismiss_active();
    }

    fn clear_notice_with_title(&mut self, title: &str) {
        self.notices.clear_with_title(title);
    }

    fn set_layout_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.layout_bounds = Some(bounds);
    }

    fn set_split_bounds(&mut self, bounds: Bounds<Pixels>) {
        self.split_bounds = Some(bounds);
    }

    fn begin_divider_drag(&mut self, divider_left: Pixels, pointer_x: Pixels) {
        self.divider_drag = Some(DividerDragState {
            pointer_offset: pointer_x - divider_left,
        });
    }

    fn update_divider_drag(&mut self, pointer_x: Pixels) -> bool {
        let (Some(bounds), Some(drag)) = (self.split_bounds, self.divider_drag) else {
            return false;
        };

        if bounds.size.width <= px(0.0) {
            return false;
        }

        let divider_left = pointer_x - drag.pointer_offset;
        let available_width = (bounds.size.width - px(layout::PANEL_DIVIDER_WIDTH)).max(px(0.0));
        let desired_secondary_width =
            (bounds.right() - divider_left - px(layout::PANEL_DIVIDER_WIDTH)).max(px(0.0));
        let desired_ratio = if available_width <= px(0.0) {
            DEFAULT_CHECKLIST_SIDEBAR_RATIO
        } else {
            layout::clamped_checklist_sidebar_ratio(
                available_width,
                desired_secondary_width / available_width,
            )
        };

        if (desired_ratio - self.checklist_sidebar_ratio).abs() < f32::EPSILON {
            return false;
        }

        self.checklist_sidebar_ratio = desired_ratio;
        true
    }

    fn end_divider_drag(&mut self) {
        self.divider_drag = None;
    }

    fn graph_overlay_height(&self, composer_height: Pixels) -> Pixels {
        let available_height = self.graph_overlay_available_height(composer_height);
        let desired_height = if self.graph_overlay_panel_height <= Pixels::ZERO {
            layout::default_graph_overlay_height(available_height)
        } else {
            self.graph_overlay_panel_height
        };

        layout::clamp_graph_overlay_height(available_height, desired_height)
    }

    fn begin_graph_overlay_drag(&mut self, handle_bottom: Pixels, pointer_y: Pixels) {
        self.graph_overlay_drag = Some(GraphOverlayDragState {
            pointer_offset: handle_bottom - pointer_y,
        });
    }

    fn update_graph_overlay_drag(&mut self, pointer_y: Pixels) -> bool {
        let (Some(bounds), Some(drag)) = (self.layout_bounds, self.graph_overlay_drag) else {
            return false;
        };

        let overlay_top = bounds.top() - px(layout::THREAD_STRIP_HEIGHT);
        let desired_bottom = pointer_y + drag.pointer_offset;
        let desired_height = desired_bottom - overlay_top;
        let clamped_height = layout::clamp_graph_overlay_height(
            self.graph_overlay_available_height(px(layout::COMPOSER_MIN_HEIGHT)),
            desired_height,
        );
        if clamped_height == self.graph_overlay_panel_height {
            return false;
        }

        self.graph_overlay_panel_height = clamped_height;
        true
    }

    fn end_graph_overlay_drag(&mut self) {
        self.graph_overlay_drag = None;
    }

    fn begin_tool_activity_panel_drag(
        &mut self,
        panel_top: Pixels,
        panel_bottom: Pixels,
        composer_height: Pixels,
        pointer_y: Pixels,
    ) {
        let main_region_height = self
            .layout_bounds
            .map(|bounds| bounds.size.height)
            .unwrap_or_else(|| px(layout::WINDOW_MIN_HEIGHT));
        let (min_height, max_height) =
            layout::tool_activity_panel_height_bounds(main_region_height, composer_height);
        self.tool_activity_panel_drag = Some(ToolActivityPanelDragState {
            panel_bottom,
            pointer_offset: pointer_y - panel_top,
            min_height,
            max_height,
        });
    }

    fn update_tool_activity_panel_drag(&mut self, pointer_y: Pixels) -> bool {
        let Some(drag) = self.tool_activity_panel_drag else {
            return false;
        };
        if drag.max_height <= Pixels::ZERO {
            return false;
        }

        let desired_top = pointer_y - drag.pointer_offset;
        let desired_height = drag.panel_bottom - desired_top;
        let clamped_height = desired_height.clamp(drag.min_height, drag.max_height);
        if clamped_height == self.tool_activity_panel_height {
            return false;
        }

        self.tool_activity_panel_height = clamped_height;
        true
    }

    fn end_tool_activity_panel_drag(&mut self) -> bool {
        self.tool_activity_panel_drag.take().is_some()
    }

    fn graph_overlay_available_height(&self, composer_height: Pixels) -> Pixels {
        let main_region_height = self
            .layout_bounds
            .map(|bounds| bounds.size.height)
            .unwrap_or_else(|| px(layout::MAIN_REGION_MIN_HEIGHT + layout::COMPOSER_MIN_HEIGHT));
        let transcript_height = (main_region_height - composer_height).max(Pixels::ZERO);
        px(layout::THREAD_STRIP_HEIGHT) + transcript_height
    }

    fn transcript_list_item_count(&self) -> usize {
        transcript_list_item_count(self.transcript_presentation.len())
    }

    fn sync_live_transcript_rows(&mut self, previous_turn_count: usize) {
        sync_live_transcript_rows(
            &self.transcript_list_state,
            LiveTranscriptRows {
                previous_turn_count,
                current_turn_count: self.transcript_presentation.len(),
                preserve_user_scroll: self.transcript_user_scrolled,
            },
        );
    }
}

impl ShellView {
    fn new(
        window: &mut Window,
        bootstrap: AppBootstrap,
        app_state: Result<ConfiguredAppState, String>,
        settings_window: SettingsWindowHandle,
        settings_state: settings::SettingsState,
        appearance_settings: SharedAppearanceSettings,
        gui_preferences: SharedGuiPreferences,
        diagnostic_target_receiver: Option<Receiver<DiagnosticTargetShellRequest>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let shell_entity = cx.entity();
        let mut milestone = MemoryMilestone::new("shell_view_new_start");
        if let Some(workspace) = bootstrap.initial_workspace() {
            milestone = milestone.runtime(workspace.runtime_mode().display_name());
        }
        milestone.log();

        let host_default = std::env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let host_path_input = cx.new(|cx| {
            SingleLineInput::new_with_options(
                host_default,
                "C:\\path\\to\\workspace",
                TextInputOptions::single_line()
                    .with_undo_byte_limit(SHORT_TEXT_INPUT_UNDO_BYTE_LIMIT),
                cx,
            )
        });
        let wsl_distro_input = cx.new(|cx| {
            SingleLineInput::new_with_options(
                "",
                "Debian",
                TextInputOptions::single_line()
                    .with_undo_byte_limit(SHORT_TEXT_INPUT_UNDO_BYTE_LIMIT),
                cx,
            )
        });
        let wsl_path_input = cx.new(|cx| {
            SingleLineInput::new_with_options(
                "/",
                "/path/in/wsl",
                TextInputOptions::single_line()
                    .with_undo_byte_limit(SHORT_TEXT_INPUT_UNDO_BYTE_LIMIT),
                cx,
            )
        });
        let workspace_picker_filter_input = cx.new(|cx| {
            SingleLineInput::new_with_options(
                "",
                "Filter workspace names or paths",
                TextInputOptions::single_line()
                    .with_undo_byte_limit(SHORT_TEXT_INPUT_UNDO_BYTE_LIMIT),
                cx,
            )
        });
        let workspace_rename_input = cx.new(|cx| {
            SingleLineInput::new_with_options(
                "",
                "Workspace name",
                TextInputOptions::single_line()
                    .with_undo_byte_limit(SHORT_TEXT_INPUT_UNDO_BYTE_LIMIT),
                cx,
            )
        });
        let conversation_input = cx.new(|cx| {
            let mut input = SingleLineInput::new_with_options(
                "",
                "Ask Codex about this workspace",
                TextInputOptions::multiline()
                    .with_undo_byte_limit(COMPOSER_TEXT_INPUT_UNDO_BYTE_LIMIT),
                cx,
            );
            input.set_enter_key(TextInputEnterKey::Propagate);
            input.set_rich_paste_policy(TextInputRichPastePolicy::Propagate);
            input.set_atom_clipboard_policy(TextInputAtomClipboardPolicy::Propagate);
            input
        });
        let surface_notice_text_input = cx.new(|cx| {
            SingleLineInput::new_with_options(
                "",
                "",
                TextInputOptions::multiline()
                    .with_read_only(true)
                    .with_undo_limit(0),
                cx,
            )
        });
        let transcript_panel =
            cx.new(|cx| render::transcript::TranscriptPanel::new(shell_entity.clone(), cx));
        let checklist_sidebar_panel = cx.new(|cx| {
            render::checklist_sidebar::ChecklistSidebarPanel::new(shell_entity.clone(), cx)
        });
        let workspace_persistence_queue = spawn_workspace_persistence_worker(
            app_state
                .as_ref()
                .map(|state| state.workspace_persistence.clone())
                .map_err(Clone::clone),
        );

        let mut view = Self {
            bootstrap,
            app_state,
            settings_window,
            settings_state,
            notification_sound_path_prompt: NotificationSoundPathPromptState::default(),
            notification_sound_player: NotificationSoundPlayer::spawn(),
            platform_attention_monitor: PlatformAttentionMonitor::spawn(),
            appearance_settings,
            gui_preferences,
            state: ShellState::Discovering(DiscoveringState {
                detail: "Preparing startup discovery".to_string(),
            }),
            backend_servers: HashMap::new(),
            workspace_open_cancellation: None,
            discovery_receiver: None,
            workspace_receiver: None,
            graph_receiver: None,
            graph_thread_start_receiver: None,
            transcript_branch_receiver: None,
            transcript_edit_commit_receiver: None,
            member_thread_inventory_receiver: None,
            thread_activation_receiver: None,
            thread_history_page_receiver: None,
            composer_image_label_scan_receiver: None,
            composer_image_asset_receiver: None,
            turn_receiver: None,
            shell_tool_receiver: None,
            diagnostic_target_receiver,
            diagnostic_child_supervisor: Arc::new(Mutex::new(DiagnosticChildSupervisor::default())),
            transcript_edit_replacement_turn: None,
            turn_steering_receivers: Vec::new(),
            composer_image_delivery_receiver: None,
            thread_title_receivers: Vec::new(),
            status_operation_receiver: None,
            pending_lifecycle_phase_continue: None,
            account_rate_limits_receiver: None,
            turn_stop_receiver: None,
            hard_stop_receiver: None,
            tool_activity_nickname_resolver: ToolActivityNicknameResolver::default(),
            workspace_picker_action_receiver: None,
            workspace_runtime_selector_distro_receiver: None,
            workspace_title_receiver: None,
            application_shutdown_receiver: None,
            workspace_persistence_queue,
            workspace_member_attach_pending_workspace_id: None,
            pending_workspace_title_candidate: None,
            workspace_persistence_pending_last_poll: false,
            status_model_cache: StatusModelListCache::default(),
            last_backend_liveness_poll_at: None,
            frame_poll_scheduled: false,
            ready_idle_poll_scheduled: false,
            host_path_input,
            wsl_distro_input,
            wsl_path_input,
            workspace_picker_filter_input,
            workspace_rename_input,
            conversation_input,
            surface_notice_text_input,
            surface_notice_text_input_notice_id: Cell::new(None),
            composer_draft: ComposerDraft::default(),
            composer_clipboard: ComposerClipboardStore::default(),
            pending_composer_image_asset_paste: None,
            composer_image_popup: None,
            transcript_panel,
            checklist_sidebar_panel,
            startup_scroll_handle: ScrollHandle::new(),
            scrollbar_activity: HashMap::new(),
            next_attempt: 1,
        };

        view.subscribe_settings_window(cx);
        view.subscribe_workspace_picker_filter_input(cx);
        view.subscribe_conversation_input(cx);

        if view.block_if_app_state_unavailable(window, cx) {
            view.schedule_poll_if_needed(window, cx);
            return view;
        }

        if let Some(workspace) = view.bootstrap.initial_workspace().cloned() {
            let app_state = view
                .app_state
                .as_ref()
                .expect("app state availability checked before initial workspace open");
            view.workspace_picker_action_receiver = Some(spawn_create_workspace_for_target_worker(
                app_state.startup_persistence.clone(),
                app_state.workspace_persistence.clone(),
                workspace,
                view.workspace_persistence_queue.flush(),
                view.bootstrap.probe_timeout(),
            ));
            view.schedule_poll_if_needed(window, cx);
            cx.notify();
        } else {
            view.begin_discovery(window, cx);
        }

        MemoryMilestone::new("shell_view_new_done").log();

        view
    }

    fn block_if_app_state_unavailable(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Err(error) = self.app_state.as_ref() else {
            return false;
        };

        let attempt = self.next_attempt;
        self.next_attempt = self.next_attempt.saturating_add(1);
        self.state = ShellState::Blocked(BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::Startup,
            intent: WorkspaceOpenIntent::None,
            workspace_label: "startup".to_string(),
            stage: None,
            title: "Beryl home directory is unavailable",
            summary:
                "Beryl could not resolve the configured Beryl home directory for GUI-owned state."
                    .to_string(),
            detail: error.clone(),
            next_steps: vec![
                "Start Beryl with --beryl-home-dir <path>.".to_string(),
                "Set USERPROFILE or HOME before starting Beryl without an explicit home directory."
                    .to_string(),
            ],
            disconnect: false,
            surface: None,
        });
        window.set_window_title("Beryl");
        cx.notify();
        true
    }

    pub(super) fn workspace_persistence_for_worker(
        &self,
    ) -> Option<crate::BerylWorkspacePersistence> {
        self.app_state
            .as_ref()
            .ok()
            .map(|state| state.workspace_persistence.clone())
    }

    fn app_state_for_worker(&self) -> Option<ConfiguredAppState> {
        self.app_state.as_ref().ok().cloned()
    }

    fn beryl_home_display(&self) -> String {
        self.app_state
            .as_ref()
            .map(ConfiguredAppState::home_display)
            .unwrap_or_else(|_| "the configured Beryl home directory".to_string())
    }

    fn appearance_settings(&self) -> crate::AppearanceSettings {
        self.appearance_settings
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_default()
    }

    fn sync_surface_notice_text_input(&self, notice_id: u64, text: &str, cx: &mut Context<Self>) {
        let already_synced = self.surface_notice_text_input_notice_id.get() == Some(notice_id)
            && self.surface_notice_text_input.read(cx).text() == text;
        if already_synced {
            return;
        }

        self.surface_notice_text_input.update(cx, |input, cx| {
            input.set_text(text.to_string(), cx);
            input.set_selection(0..0, false, cx);
        });
        self.surface_notice_text_input_notice_id
            .set(Some(notice_id));
    }

    fn general_ui_background(&self) -> gpui::Rgba {
        rgba_from_role_color(
            self.appearance_settings().general_ui.parsed_background(),
            rgb(0x020617),
        )
    }

    fn general_ui_foreground(&self) -> gpui::Rgba {
        rgba_from_role_color(
            self.appearance_settings().general_ui.parsed_foreground(),
            rgb(0xe2e8f0),
        )
    }

    fn toolbar_background(&self) -> gpui::Rgba {
        chrome_color(
            &self.appearance_settings().chrome.toolbar_background,
            rgb(0x020617),
        )
    }

    fn conversation_thread_strip_background(&self) -> gpui::Rgba {
        chrome_color(
            &self
                .appearance_settings()
                .chrome
                .conversation_thread_strip_background,
            rgb(0x091220),
        )
    }

    fn separator_color(&self) -> gpui::Rgba {
        chrome_color(&self.appearance_settings().chrome.separator, rgb(0x1e293b))
    }

    fn primary_button_theme(&self) -> ChromeButtonTheme {
        let settings = self.appearance_settings();
        chrome_button_theme(
            &settings.chrome.primary_button,
            ChromeButtonTheme::primary(),
        )
    }

    fn secondary_button_theme(&self) -> ChromeButtonTheme {
        let settings = self.appearance_settings();
        chrome_button_theme(
            &settings.chrome.secondary_button,
            ChromeButtonTheme::secondary(),
        )
    }

    fn input_panel_background(&self) -> gpui::Rgba {
        chrome_color(
            &self.appearance_settings().chrome.input.panel_background,
            rgb(0x020617),
        )
    }

    fn input_background(&self) -> gpui::Rgba {
        chrome_color(
            &self.appearance_settings().chrome.input.input_background,
            rgb(0x0f172a),
        )
    }

    fn input_border(&self) -> gpui::Rgba {
        chrome_color(
            &self.appearance_settings().chrome.input.input_border,
            rgb(0x334155),
        )
    }

    fn input_foreground(&self) -> gpui::Rgba {
        chrome_color(
            &self.appearance_settings().chrome.input.input_foreground,
            rgb(0xe2e8f0),
        )
    }

    fn composer_image_popup(&self) -> Option<&ComposerImagePopupState> {
        self.composer_image_popup.as_ref()
    }

    fn composer_image_preview_image(&self) -> Option<Arc<Image>> {
        self.composer_image_popup
            .as_ref()?
            .preview_image
            .as_ref()
            .cloned()
    }

    fn transcript_shell_background(&self) -> gpui::Rgba {
        chrome_color(
            &self
                .appearance_settings()
                .chrome
                .transcript_shell
                .background,
            rgb(0x091220),
        )
    }

    fn transcript_shell_foreground(&self) -> gpui::Rgba {
        chrome_color(
            &self
                .appearance_settings()
                .chrome
                .transcript_shell
                .foreground,
            rgb(0xe2e8f0),
        )
    }

    fn status_line_background(&self) -> gpui::Rgba {
        chrome_color(
            &self.appearance_settings().chrome.status_line.background,
            rgb(0x020617),
        )
    }

    fn status_line_title_foreground(&self) -> gpui::Rgba {
        chrome_color(
            &self
                .appearance_settings()
                .chrome
                .status_line
                .title_foreground,
            rgb(0x94a3b8),
        )
    }

    fn status_line_value_foreground(&self) -> gpui::Rgba {
        chrome_color(
            &self
                .appearance_settings()
                .chrome
                .status_line
                .value_foreground,
            rgb(0xe2e8f0),
        )
    }

    fn panel_surface_background(&self) -> gpui::Rgba {
        chrome_color(
            &self.appearance_settings().chrome.surfaces.panel_background,
            rgb(0x111827),
        )
    }

    fn row_surface_background(&self) -> gpui::Rgba {
        chrome_color(
            &self.appearance_settings().chrome.surfaces.row_background,
            rgb(0x1f2937),
        )
    }

    fn popup_surface_background(&self) -> gpui::Rgba {
        chrome_color(
            &self.appearance_settings().chrome.surfaces.popup_background,
            rgb(0x111827),
        )
    }

    fn surface_border(&self) -> gpui::Rgba {
        chrome_color(
            &self.appearance_settings().chrome.surfaces.border,
            rgb(0x374151),
        )
    }

    fn surface_muted_foreground(&self) -> gpui::Rgba {
        chrome_color(
            &self.appearance_settings().chrome.surfaces.muted_foreground,
            rgb(0x94a3b8),
        )
    }

    fn surface_foreground(&self) -> gpui::Rgba {
        rgb(0xe2e8f0)
    }

    fn cancel_thread_title_workers(&mut self) -> bool {
        let outcomes =
            thread_title::cancel_all_thread_title_tasks(&mut self.thread_title_receivers);
        self.finish_thread_title_task_outcomes(outcomes, false)
    }

    fn cancel_thread_title_workers_for_thread(&mut self, thread_id: &str) -> bool {
        let outcomes = thread_title::cancel_thread_title_tasks_for_thread(
            &mut self.thread_title_receivers,
            thread_id,
        );
        self.finish_thread_title_task_outcomes(outcomes, false)
    }

    fn cancel_workspace_open(&mut self) {
        if let Some(cancellation) = self.workspace_open_cancellation.take() {
            cancellation.cancel();
        }
    }

    fn discard_workspace_open_receiver(&mut self, reason: &'static str) {
        self.cancel_workspace_open();
        let Some(receiver) = self.workspace_receiver.take() else {
            return;
        };

        shutdown_queued_workspace_open_result(receiver, reason);
    }

    pub(super) fn shutdown_backend_server_for_target_in_background(
        &mut self,
        execution_target: &WorkspaceId,
        reason: &'static str,
    ) {
        self.tool_activity_nickname_resolver.reset();
        self.account_rate_limits_receiver = None;
        let Some(server) = self.backend_servers.remove(execution_target) else {
            return;
        };

        spawn_managed_backend_shutdown(server, reason);
    }

    pub(super) fn shutdown_active_backend_server_in_background(&mut self, reason: &'static str) {
        let target = match &self.state {
            ShellState::Ready(ready) => Some(ready.execution_target.clone()),
            ShellState::Blocked(blocked) => match &blocked.target {
                RetryTarget::Workspace(workspace) => Some(workspace.clone()),
                _ => None,
            },
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_) => None,
        };
        if let Some(target) = target {
            self.shutdown_backend_server_for_target_in_background(&target, reason);
        }
    }

    pub(super) fn shutdown_all_backend_servers_in_background(&mut self, reason: &'static str) {
        self.tool_activity_nickname_resolver.reset();
        self.account_rate_limits_receiver = None;
        for (_, server) in self.backend_servers.drain() {
            spawn_managed_backend_shutdown(server, reason);
        }
    }

    fn begin_application_shutdown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.application_shutdown_receiver.is_some() {
            self.schedule_poll_if_needed(window, cx);
            return;
        }

        self.cancel_thread_title_workers();
        self.cancel_workspace_open();
        let active_servers = self
            .backend_servers
            .drain()
            .map(|(_, server)| server)
            .collect();
        let workspace_receiver = self.workspace_receiver.take();
        self.discovery_receiver = None;
        self.graph_receiver = None;
        self.graph_thread_start_receiver = None;
        self.transcript_branch_receiver = None;
        self.transcript_edit_commit_receiver = None;
        self.member_thread_inventory_receiver = None;
        self.thread_activation_receiver = None;
        self.thread_history_page_receiver = None;
        self.composer_image_label_scan_receiver = None;
        self.composer_image_asset_receiver = None;
        self.pending_composer_image_asset_paste = None;
        self.turn_receiver = None;
        self.transcript_edit_replacement_turn = None;
        self.turn_steering_receivers.clear();
        self.status_operation_receiver = None;
        self.pending_lifecycle_phase_continue = None;
        self.account_rate_limits_receiver = None;
        self.turn_stop_receiver = None;
        self.hard_stop_receiver = None;
        self.tool_activity_nickname_resolver.reset();
        self.workspace_picker_action_receiver = None;
        self.workspace_runtime_selector_distro_receiver = None;
        self.workspace_title_receiver = None;
        self.pending_workspace_title_candidate = None;

        let timeout = self.bootstrap.probe_timeout() + APP_SHUTDOWN_OPEN_WORKER_GRACE_TIMEOUT;
        let workspace_persistence_flush = self.workspace_persistence_queue.flush();
        self.application_shutdown_receiver = Some(spawn_application_shutdown_worker(
            active_servers,
            workspace_receiver,
            workspace_persistence_flush,
            timeout,
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
    }

    fn clear_background_activity(&mut self) {
        self.cancel_thread_title_workers();
        self.shutdown_all_backend_servers_in_background("clearing shell background activity");
        self.discard_workspace_open_receiver("clearing pending workspace open");
        self.discovery_receiver = None;
        self.graph_receiver = None;
        self.graph_thread_start_receiver = None;
        self.transcript_branch_receiver = None;
        self.transcript_edit_commit_receiver = None;
        self.member_thread_inventory_receiver = None;
        self.thread_activation_receiver = None;
        self.thread_history_page_receiver = None;
        self.composer_image_label_scan_receiver = None;
        self.turn_receiver = None;
        self.transcript_edit_replacement_turn = None;
        self.turn_steering_receivers.clear();
        self.status_operation_receiver = None;
        self.pending_lifecycle_phase_continue = None;
        self.account_rate_limits_receiver = None;
        self.turn_stop_receiver = None;
        self.hard_stop_receiver = None;
        self.tool_activity_nickname_resolver.reset();
        self.workspace_picker_action_receiver = None;
        self.workspace_title_receiver = None;
        self.workspace_member_attach_pending_workspace_id = None;
        self.pending_workspace_title_candidate = None;
        self.status_model_cache = StatusModelListCache::default();
        self.last_backend_liveness_poll_at = None;
        self.ready_idle_poll_scheduled = false;
    }

    fn begin_discovery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(app_state) = self.app_state_for_worker() else {
            self.block_if_app_state_unavailable(window, cx);
            return;
        };

        self.clear_background_activity();
        self.state = ShellState::Discovering(DiscoveringState {
            detail: "Loading Beryl startup state".to_string(),
        });
        window.set_window_title("Beryl");

        let startup_persistence = app_state.startup_persistence;
        let workspace_persistence = app_state.workspace_persistence;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(DiscoveryUpdate::Progress(
                "Loading Beryl-owned semantic workspaces".to_string(),
            ));
            let startup = crate::resolve_startup_state(
                &startup_persistence,
                &workspace_persistence,
            )
            .and_then(|startup| {
                let mut workspace_state = workspace_persistence
                    .load_workspace_state(startup.active_workspace().id())
                    .map_err(crate::StartupStateError::WorkspacePersistence)?;
                if reconcile_workspace_member_availability(&mut workspace_state) {
                    workspace_persistence
                        .save_workspace_state(startup.active_workspace().id(), &workspace_state)
                        .map_err(crate::StartupStateError::WorkspacePersistence)?;
                }
                let workspace_ui_state = workspace_persistence
                    .load_workspace_ui_state(startup.active_workspace().id())
                    .map_err(crate::StartupStateError::WorkspacePersistence)?;
                let mut workspace_picker_member_paths =
                    workspace_picker::workspace_picker_member_paths_from_states(
                        startup.known_workspaces(),
                        |workspace_id| match workspace_persistence
                            .load_workspace_state(workspace_id)
                        {
                            Ok(state) => Some(state),
                            Err(error) => {
                                warn!(
                                    workspace_id = workspace_id.as_str(),
                                    error = %error,
                                    "could not load inactive workspace members for picker row"
                                );
                                None
                            }
                        },
                    );
                workspace_picker_member_paths.insert(
                    startup.active_workspace().id().clone(),
                    workspace_picker::explicit_member_path_strings(&workspace_state),
                );
                Ok(DiscoveryOutcome {
                    startup,
                    workspace_picker_member_paths,
                    workspace_state,
                    workspace_ui_state,
                })
            })
            .map_err(|error| {
                crate::backend_failure::source_chain_detail(error.to_string(), &error)
            });

            match startup {
                Ok(startup) => {
                    let _ = sender.send(DiscoveryUpdate::Progress(
                        "Opening startup workspace".to_string(),
                    ));
                    let _ = sender.send(DiscoveryUpdate::Finished(Ok(startup)));
                }
                Err(error) => {
                    let _ = sender.send(DiscoveryUpdate::Progress(
                        "Startup workspace resolution failed".to_string(),
                    ));
                    let _ = sender.send(DiscoveryUpdate::Finished(Err(error.to_string())));
                }
            }
        });

        self.discovery_receiver = Some(receiver);
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
    }

    fn begin_open_target(
        &mut self,
        target: RetryTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_open_target_with_intent(target, WorkspaceOpenIntent::None, window, cx);
    }

    fn begin_idle_primary_workspace_open_if_executable(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut loaded = match &self.state {
            ShellState::WorkspaceIdle(idle)
                if idle.loaded_workspace.selected_runtime().is_some() =>
            {
                idle.loaded_workspace.clone()
            }
            _ => return false,
        };

        loaded.workspace_picker.close();
        self.state = ShellState::WorkspaceLoaded(loaded);
        self.begin_open_target(RetryTarget::WorkspacePrimary, window, cx);
        true
    }

    fn begin_open_target_with_intent(
        &mut self,
        target: RetryTarget,
        intent: WorkspaceOpenIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let thread_selection = self.thread_selection_for_open_target(&target);
        self.begin_open_target_with_thread_selection_and_intent(
            target,
            thread_selection,
            intent,
            window,
            cx,
        );
    }

    fn begin_open_target_with_thread_selection(
        &mut self,
        target: RetryTarget,
        thread_selection: ThreadSelectionRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_open_target_with_thread_selection_and_intent(
            target,
            thread_selection,
            WorkspaceOpenIntent::None,
            window,
            cx,
        );
    }

    fn begin_open_target_with_thread_selection_and_intent(
        &mut self,
        target: RetryTarget,
        thread_selection: ThreadSelectionRequest,
        intent: WorkspaceOpenIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_failure = self.failure_summary();
        let attempt = self.next_attempt;
        self.next_attempt += 1;
        let preserved_surface = self.preserved_surface_for_open_target(&target);
        let loaded_workspace = self
            .workspace_shell_state()
            .cloned()
            .unwrap_or_else(|| self.bootstrap_workspace_state(&target.workspace()));
        let workspace_id = loaded_workspace.workspace.id().clone();
        MemoryMilestone::new("workspace_open_start")
            .workspace_id(workspace_id.as_str())
            .runtime(target.workspace().runtime_mode().display_name())
            .log();

        self.discovery_receiver = None;
        self.discard_workspace_open_receiver("starting a new workspace open");
        self.graph_receiver = None;
        self.graph_thread_start_receiver = None;
        self.transcript_branch_receiver = None;
        self.transcript_edit_commit_receiver = None;
        self.member_thread_inventory_receiver = None;
        self.thread_activation_receiver = None;
        self.thread_history_page_receiver = None;
        self.composer_image_label_scan_receiver = None;
        self.turn_receiver = None;
        self.transcript_edit_replacement_turn = None;
        self.turn_steering_receivers.clear();
        self.cancel_thread_title_workers();
        self.status_operation_receiver = None;
        self.pending_lifecycle_phase_continue = None;
        self.account_rate_limits_receiver = None;
        self.turn_stop_receiver = None;
        self.hard_stop_receiver = None;
        self.workspace_picker_action_receiver = None;
        self.workspace_title_receiver = None;
        self.pending_workspace_title_candidate = None;
        self.status_model_cache = StatusModelListCache::default();
        self.state = ShellState::Opening(OpeningState {
            attempt,
            workspace_label: target.workspace_label(),
            loaded_workspace,
            preserved_surface,
            target: target.clone(),
            intent,
            detail: "Preparing workspace selection".to_string(),
            progress: None,
            previous_failure,
        });
        window.set_window_title(&format!("Beryl - {}", target.workspace_label()));

        let timeout = self.bootstrap.probe_timeout();
        let cancellation = WorkspaceOpenCancellation::new();
        self.workspace_open_cancellation = Some(cancellation.clone());
        let workspace_persistence_flush = self.workspace_persistence_queue.flush();
        let Some(workspace_persistence) = self.workspace_persistence_for_worker() else {
            self.block_if_app_state_unavailable(window, cx);
            return;
        };
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            discovery::open_workspace_worker(
                workspace_persistence,
                workspace_id,
                target,
                thread_selection,
                intent,
                cancellation,
                workspace_persistence_flush,
                timeout,
                sender,
            )
        });

        self.workspace_receiver = Some(receiver);
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
    }

    fn schedule_poll_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_frame_poll_work() {
            if !self.frame_poll_scheduled {
                let window_handle = window.window_handle();
                self.frame_poll_scheduled = true;
                cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
                    let mut cx = cx.clone();
                    async move {
                        cx.background_executor().timer(FRAME_POLL_INTERVAL).await;
                        let _ = cx.update_window(window_handle, |_, window, cx| {
                            let _ = view.update(cx, |view, cx| {
                                view.frame_poll_scheduled = false;
                                view.poll(window, cx);
                            });
                        });
                    }
                })
                .detach();
            }
            return;
        }

        if self.has_ready_maintenance_poll_work() && !self.ready_idle_poll_scheduled {
            let window_handle = window.window_handle();
            self.ready_idle_poll_scheduled = true;
            cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
                let mut cx = cx.clone();
                async move {
                    cx.background_executor()
                        .timer(READY_IDLE_POLL_INTERVAL)
                        .await;
                    let _ = cx.update_window(window_handle, |_, window, cx| {
                        let _ = view.update(cx, |view, cx| {
                            view.ready_idle_poll_scheduled = false;
                            view.poll(window, cx);
                        });
                    });
                }
            })
            .detach();
        }
    }

    fn has_frame_poll_work(&self) -> bool {
        self.discovery_receiver.is_some()
            || self.workspace_receiver.is_some()
            || self.graph_receiver.is_some()
            || self.graph_thread_start_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
            || self.member_thread_inventory_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.composer_image_label_scan_receiver.is_some()
            || self.composer_image_asset_receiver.is_some()
            || self.turn_receiver.is_some()
            || self.shell_tool_receiver.is_some()
            || self.diagnostic_target_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
            || self.composer_image_delivery_receiver.is_some()
            || !self.thread_title_receivers.is_empty()
            || self.status_operation_receiver.is_some()
            || self.account_rate_limits_receiver.is_some()
            || self.turn_stop_receiver.is_some()
            || self.hard_stop_receiver.is_some()
            || self.workspace_picker_action_receiver.is_some()
            || self.workspace_runtime_selector_distro_receiver.is_some()
            || self.workspace_title_receiver.is_some()
            || self.application_shutdown_receiver.is_some()
            || self.pending_workspace_title_candidate.is_some()
            || self.workspace_persistence_pending_last_poll
            || self.workspace_persistence_queue.has_pending_work()
            || self
                .loaded_workspace()
                .is_some_and(|loaded| loaded.workspace_picker.delete_hold_active())
            || self
                .conversation_surface()
                .is_some_and(|surface| surface.status_line_operations().hard_stop_hold_active())
            || self
                .conversation_surface()
                .is_some_and(|surface| surface.graph_thread_link_menu().delete_hold_active())
    }

    fn has_ready_maintenance_poll_work(&self) -> bool {
        matches!(self.state, ShellState::Ready(_))
            && (self
                .conversation_surface()
                .is_some_and(|surface| surface.member_thread_inventory().needs_refresh())
                || self.tool_activity_nickname_resolver.has_retry_work()
                || !self.backend_servers.is_empty())
    }

    fn poll_workspace_persistence_pending_state(&mut self) -> bool {
        let pending = self.workspace_persistence_queue.has_pending_work();
        if self.workspace_persistence_pending_last_poll == pending {
            return false;
        }
        self.workspace_persistence_pending_last_poll = pending;
        true
    }

    pub(super) fn workspace_rename_disabled_reason(&self) -> Option<&'static str> {
        workspace_rename_disabled_reason(self.workspace_rename_blockers())
    }

    fn workspace_rename_blockers(&self) -> WorkspaceRenameBlockers {
        let mut blockers = WorkspaceRenameBlockers {
            workspace_lifecycle: self.workspace_receiver.is_some()
                || matches!(self.state, ShellState::Opening(_)),
            graph_work: self.graph_receiver.is_some() || self.graph_thread_start_receiver.is_some(),
            transcript_work: self.transcript_branch_receiver.is_some()
                || self.transcript_edit_commit_receiver.is_some()
                || self.thread_activation_receiver.is_some()
                || self.thread_history_page_receiver.is_some()
                || self.turn_receiver.is_some()
                || !self.turn_steering_receivers.is_empty()
                || !self.thread_title_receivers.is_empty()
                || self.pending_lifecycle_phase_continue.is_some(),
            inventory_work: self.member_thread_inventory_receiver.is_some(),
            image_work: self.composer_image_label_scan_receiver.is_some()
                || self.composer_image_asset_receiver.is_some()
                || self.composer_image_delivery_receiver.is_some()
                || self.pending_composer_image_asset_paste.is_some(),
            status_work: self.status_operation_receiver.is_some()
                || self.turn_stop_receiver.is_some()
                || self.hard_stop_receiver.is_some(),
            title_work: self.workspace_title_receiver.is_some()
                || self.pending_workspace_title_candidate.is_some(),
            member_work: self.workspace_member_attach_pending_workspace_id.is_some(),
            picker_work: self.workspace_picker_action_receiver.is_some(),
            persistence_work: self.workspace_persistence_queue.has_pending_work(),
        };

        if let Some(surface) = self.conversation_surface() {
            let surface_blockers = surface.workspace_rename_blockers();
            blockers.graph_work |= surface_blockers.graph_work;
            blockers.transcript_work |= surface_blockers.transcript_work;
            blockers.inventory_work |= surface_blockers.inventory_work;
            blockers.status_work |= surface_blockers.status_work;
        }

        blockers
    }

    fn poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.poll_application_shutdown_updates(cx) {
            return;
        }

        let mut updated = false;

        updated |= self.poll_discovery_updates(window, cx);
        updated |= self.poll_workspace_updates(window);
        updated |= self.poll_graph_updates();
        updated |= self.poll_graph_thread_start_updates();
        updated |= self.poll_transcript_branch_updates(window, cx);
        updated |= self.poll_transcript_edit_commit_updates(window, cx);
        updated |= self.poll_member_thread_inventory_updates();
        updated |= self.poll_thread_activation_updates();
        updated |= self.poll_thread_history_page_updates();
        updated |= self.poll_composer_image_label_scan_updates();
        updated |= self.poll_composer_image_asset_updates(cx);
        updated |= self.poll_diagnostic_target_requests(window, cx);
        updated |= self.poll_shell_dynamic_tool_requests(window, cx);
        updated |= self.poll_turn_updates(window, cx);
        updated |= self.poll_turn_steering_updates();
        updated |= self.poll_composer_image_delivery_updates(cx);
        updated |= self.poll_thread_title_updates();
        updated |= self.poll_status_operation_updates();
        updated |= self.poll_account_rate_limits_updates();
        updated |= self.poll_turn_stop_updates();
        updated |= self.poll_hard_stop_updates();
        updated |= self.poll_status_operation_hold(window, cx);
        updated |= self.poll_graph_node_action_menu_hold(window, cx);
        updated |= self.poll_workspace_picker_delete_hold(window, cx);
        updated |= self.poll_tool_activity_nickname_updates();
        updated |= self.poll_workspace_picker_action_updates(window, cx);
        updated |= self.poll_workspace_runtime_selector_distro_updates();
        updated |= self.poll_workspace_title_updates(window);
        updated |= self.poll_workspace_persistence_pending_state();
        updated |= self.begin_member_thread_inventory_refresh_if_needed();
        updated |= self.begin_tool_activity_nickname_resolution_if_needed(window, cx);
        updated |= self.begin_composer_image_label_scan_if_needed(window, cx);

        let should_poll_backend_liveness = matches!(self.state, ShellState::Ready(_))
            && self
                .last_backend_liveness_poll_at
                .map_or(true, |last_poll| {
                    last_poll.elapsed() >= BACKEND_LIVENESS_POLL_INTERVAL
                });
        if should_poll_backend_liveness {
            self.last_backend_liveness_poll_at = Some(Instant::now());
            let active_target = match &self.state {
                ShellState::Ready(ready) => Some(ready.execution_target.clone()),
                _ => None,
            };
            if let Some(active_target) = active_target
                && let Some(server) = self.backend_servers.get_mut(&active_target)
                && !server.is_process_alive()
            {
                self.handle_disconnect();
                updated = true;
            }
        }

        self.schedule_poll_if_needed(window, cx);
        if updated {
            self.notify_transcript_panel(cx);
            self.notify_checklist_sidebar_panel(cx);
            cx.notify();
        }
    }

    fn poll_application_shutdown_updates(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(receiver) = self.application_shutdown_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(ApplicationShutdownUpdate::Finished(Ok(()))) => {
                self.application_shutdown_receiver = None;
                cx.quit();
                true
            }
            Ok(ApplicationShutdownUpdate::Finished(Err(error))) => {
                self.application_shutdown_receiver = None;
                warn!(
                    error = %error,
                    "application shutdown completed with managed backend shutdown errors"
                );
                cx.quit();
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.application_shutdown_receiver = None;
                warn!("application shutdown worker stopped before returning a result");
                cx.quit();
                true
            }
        }
    }

    fn poll_discovery_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(receiver) = self.discovery_receiver.as_ref() else {
            return false;
        };

        let mut updated = false;
        let poll_started_at = Instant::now();
        let mut processed_updates = 0usize;
        loop {
            if processed_updates >= SHELL_WORKER_POLL_MAX_EVENTS_PER_FRAME
                || poll_started_at.elapsed() >= SHELL_WORKER_POLL_MAX_FRAME_TIME
            {
                return updated;
            }

            let update = match receiver.try_recv() {
                Ok(update) => {
                    processed_updates = processed_updates.saturating_add(1);
                    update
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.discovery_receiver = None;
                    warn!("startup workspace resolution worker stopped before returning a result");
                    self.state = ShellState::Blocked(BlockedState {
                        attempt: 0,
                        loaded_workspace: None,
                        target: RetryTarget::Startup,
                        intent: WorkspaceOpenIntent::None,
                        workspace_label: "startup".to_string(),
                        stage: None,
                        title: "Startup workspace resolution stopped unexpectedly",
                        summary: "Beryl lost the background task that was selecting the startup workspace.".to_string(),
                        detail: "Retry to reopen the previously active workspace or create a fresh untitled workspace.".to_string(),
                        next_steps: vec![
                            "Retry startup discovery.".to_string(),
                            "Close Beryl if you want to stop here.".to_string(),
                        ],
                        disconnect: false,
                        surface: None,
                    });
                    updated = true;
                    break;
                }
            };

            match update {
                DiscoveryUpdate::Progress(detail) => {
                    if let ShellState::Discovering(discovering) = &mut self.state {
                        discovering.detail = detail;
                        updated = true;
                    }
                }
                DiscoveryUpdate::Finished(Ok(outcome)) => {
                    self.discovery_receiver = None;
                    let startup_warning = outcome.startup.startup_warning().map(str::to_string);
                    let workspace = outcome.startup.active_workspace().clone();
                    let loaded = LoadedWorkspaceState::new(
                        workspace.clone(),
                        outcome.startup.known_workspaces().to_vec(),
                        outcome.workspace_picker_member_paths,
                        outcome.workspace_state,
                        outcome.workspace_ui_state,
                        startup_warning,
                    );
                    window.set_window_title(&format!("Beryl - {}", workspace.title()));
                    if loaded.selected_runtime().is_some() {
                        self.state = ShellState::WorkspaceLoaded(loaded);
                        self.begin_open_target(RetryTarget::WorkspacePrimary, window, cx);
                    } else {
                        self.state = ShellState::WorkspaceIdle(IdleWorkspaceState::new(loaded));
                    }
                    updated = true;
                    break;
                }
                DiscoveryUpdate::Finished(Err(error)) => {
                    self.discovery_receiver = None;
                    let detail = crate::backend_failure::non_empty_user_text(
                        &error,
                        "Startup workspace resolution failed without a detailed error message.",
                    );
                    warn!(
                        detail = %detail,
                        "startup workspace resolution failed"
                    );
                    self.state = ShellState::Blocked(BlockedState {
                        attempt: 0,
                        loaded_workspace: None,
                        target: RetryTarget::Startup,
                        intent: WorkspaceOpenIntent::None,
                        workspace_label: "startup".to_string(),
                        stage: None,
                        title: "Startup workspace resolution failed",
                        summary: format!(
                            "Beryl could not load the previously active semantic workspace or create a fresh untitled workspace from {}.",
                            self.beryl_home_display()
                        ),
                        detail,
                        next_steps: vec![
                            "Verify that the configured Beryl home directory is readable and writable."
                                .to_string(),
                            "Retry startup discovery.".to_string(),
                            "Close Beryl if you want to stop here.".to_string(),
                        ],
                        disconnect: false,
                        surface: None,
                    });
                    updated = true;
                    break;
                }
            }
        }

        updated
    }

    fn poll_workspace_updates(&mut self, window: &mut Window) -> bool {
        let Some(receiver) = self.workspace_receiver.as_ref() else {
            return false;
        };

        let mut updated = false;
        let poll_started_at = Instant::now();
        let mut processed_updates = 0usize;
        loop {
            if processed_updates >= SHELL_WORKER_POLL_MAX_EVENTS_PER_FRAME
                || poll_started_at.elapsed() >= SHELL_WORKER_POLL_MAX_FRAME_TIME
            {
                return updated;
            }

            let update = match receiver.try_recv() {
                Ok(update) => {
                    processed_updates = processed_updates.saturating_add(1);
                    update
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.workspace_receiver = None;
                    self.workspace_open_cancellation = None;
                    warn!("workspace startup worker stopped before returning a result");
                    self.finish_workspace_open(Err(OpenWorkspaceFailure {
                        stage: None,
                        title: "Workspace startup stopped unexpectedly",
                        summary:
                            "The background workspace startup task stopped before it reported a usable backend."
                                .to_string(),
                        detail:
                            "Beryl lost the worker thread that was opening the selected workspace."
                                .to_string(),
                        next_steps: vec![
                            "Retry the same workspace selection.".to_string(),
                            "Close Beryl if you want to stop here.".to_string(),
                        ],
                    }));
                    updated = true;
                    break;
                }
            };

            match update {
                WorkspaceUpdate::Detail(detail) => {
                    if let ShellState::Opening(opening) = &mut self.state {
                        opening.detail = detail;
                        updated = true;
                    }
                }
                WorkspaceUpdate::ResolvedExecutionTarget(workspace) => {
                    if let ShellState::Opening(opening) = &mut self.state {
                        opening.workspace_label = workspace.display_label();
                        opening.target = RetryTarget::Workspace(workspace.clone());
                        opening
                            .loaded_workspace
                            .set_resolved_implicit_home_path_from_target(&workspace);
                        window.set_window_title(&format!("Beryl - {}", workspace.display_label()));
                        updated = true;
                    }
                }
                WorkspaceUpdate::Progress(progress) => {
                    if let ShellState::Opening(opening) = &mut self.state {
                        opening.progress = Some(progress.clone());
                        opening.detail = match progress.detail() {
                            Some(detail) => {
                                format!("{} ({detail})", progress.stage().display_label())
                            }
                            None => progress.stage().display_label().to_string(),
                        };
                        updated = true;
                    }
                }
                WorkspaceUpdate::Finished(result) => {
                    self.workspace_receiver = None;
                    self.workspace_open_cancellation = None;
                    self.finish_workspace_open(result);
                    updated = true;
                    break;
                }
            }
        }

        updated
    }

    fn poll_graph_updates(&mut self) -> bool {
        let Some(receiver) = self.graph_receiver.as_ref() else {
            return false;
        };

        let mut updated = false;
        match receiver.try_recv() {
            Ok(GraphUpdate::MutationFinished(update)) => {
                self.graph_receiver = None;
                updated |= self.finish_graph_mutation_update(update);
            }
            Ok(GraphUpdate::ReloadFinished(result)) => {
                self.graph_receiver = None;
                updated |= self.finish_graph_reload_update(result);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                let update = receiver.disconnected_update(
                    "Beryl lost the background task that was mutating the semantic graph.",
                );
                self.graph_receiver = None;
                updated |= self.finish_graph_mutation_update(update);
            }
        }

        updated
    }

    fn poll_thread_activation_updates(&mut self) -> bool {
        let Some(receiver) = self.thread_activation_receiver.as_ref() else {
            return false;
        };

        let mut updated = false;
        match receiver.try_recv() {
            Ok(ThreadActivationUpdate::Finished(outcome)) => {
                self.thread_activation_receiver = None;
                self.finish_thread_activation_worker(outcome);
                updated = true;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.thread_activation_receiver = None;
                self.handle_thread_activation_worker_stopped();
                updated = true;
            }
        }

        updated
    }

    fn poll_thread_history_page_updates(&mut self) -> bool {
        let Some(receiver) = self.thread_history_page_receiver.as_ref() else {
            return false;
        };

        let mut updated = false;
        match receiver.try_recv() {
            Ok(ThreadHistoryPageUpdate::Finished(outcome)) => {
                self.thread_history_page_receiver = None;
                self.finish_thread_history_page_worker(outcome);
                updated = true;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.thread_history_page_receiver = None;
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.finish_loading_older_history_failure();
                    surface.set_notice(SurfaceNotice::new(
                        "Thread history load failed",
                        "Beryl lost the background task that was loading older thread history.",
                    ));
                }
                updated = true;
            }
        }

        updated
    }

    fn poll_composer_image_label_scan_updates(&mut self) -> bool {
        let Some(receiver) = self.composer_image_label_scan_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(ComposerImageLabelScanUpdate::Finished(outcome)) => {
                self.composer_image_label_scan_receiver = None;
                self.finish_composer_image_label_scan_worker(outcome);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.composer_image_label_scan_receiver = None;
                if let Some(surface) = self.conversation_surface_mut()
                    && let Some(thread_id) = surface.selected_thread_id().map(str::to_string)
                {
                    surface.fail_composer_image_label_scan(
                        &thread_id,
                        "Beryl lost the background task that was scanning image labels.",
                    );
                    surface.set_notice(SurfaceNotice::new(
                        "Image label scan failed",
                        "Beryl lost the background task that was scanning this thread's earlier image labels.",
                    ));
                }
                true
            }
        }
    }

    fn finish_composer_image_label_scan_worker(&mut self, outcome: ComposerImageLabelScanOutcome) {
        match outcome {
            ComposerImageLabelScanOutcome::Completed {
                thread_id,
                observations,
            } => {
                if let Some(surface) = self.conversation_surface_mut() {
                    let selected = surface.selected_thread_id() == Some(thread_id.as_str());
                    surface.finish_composer_image_label_scan(&thread_id, observations);
                    if selected {
                        surface.clear_notice_with_title("Image input unavailable");
                    }
                }
            }
            ComposerImageLabelScanOutcome::Failed { thread_id, message } => {
                if let Some(surface) = self.conversation_surface_mut() {
                    let selected = surface.selected_thread_id() == Some(thread_id.as_str());
                    surface.fail_composer_image_label_scan(&thread_id, message.clone());
                    if selected {
                        surface.set_notice(SurfaceNotice::new(
                            "Image label scan failed",
                            message.clone(),
                        ));
                    }
                }

                self.block_if_backend_process_dead(
                    "Managed backend disconnected during image label scanning",
                    "The backend process for the selected workspace exited before Beryl could scan earlier image labels.",
                    &message,
                );
            }
        }
    }

    fn poll_turn_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let mut updated = false;
        let poll_started_at = Instant::now();
        let mut processed_updates = 0usize;
        loop {
            if processed_updates >= TURN_UPDATE_POLL_MAX_EVENTS_PER_FRAME
                || poll_started_at.elapsed() >= TURN_UPDATE_POLL_MAX_FRAME_TIME
            {
                return updated;
            }

            let next_update = match self.turn_receiver.as_ref() {
                Some(receiver) => receiver.try_recv(),
                None => return updated,
            };

            let update = match next_update {
                Ok(update) => {
                    processed_updates = processed_updates.saturating_add(1);
                    update
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let edit_replacement_failed_before_start = self
                        .transcript_edit_replacement_turn
                        .as_ref()
                        .is_some_and(|replacement| !replacement.turn_started);
                    self.turn_receiver = None;
                    self.shell_tool_receiver = None;
                    self.pending_lifecycle_phase_continue = None;
                    let sound_candidate = self.handle_turn_worker_stopped();
                    self.finish_transcript_edit_replacement_turn(
                        edit_replacement_failed_before_start,
                        Some("Beryl lost the background task that was starting the replacement turn."),
                        window,
                        cx,
                    );
                    if let Some(candidate) = sound_candidate {
                        self.play_end_turn_sound_if_attention_triggered(candidate, window, cx);
                    }
                    updated = true;
                    break;
                }
            };

            match update {
                TurnWorkerUpdate::ThreadActivated {
                    execution_target,
                    mut thread,
                    session_metadata,
                } => {
                    if self.thread_ignores_backend_name_for_automatic_title(
                        &thread.id,
                        thread.name.as_deref(),
                    ) {
                        thread.name = None;
                    }
                    if let ShellState::Ready(ready) = &mut self.state {
                        ready.execution_target = execution_target.clone();
                    }
                    let mut activated_new_thread = false;
                    if let Some(surface) = self.conversation_surface_mut() {
                        activated_new_thread = surface.selected_thread_id().is_none();
                        surface.upsert_selected_thread(thread.clone());
                        if activated_new_thread {
                            surface.bind_pending_new_thread_image_labels_to_thread(&thread.id);
                            surface.bind_pending_new_thread_defaults_to_thread(&thread.id);
                        }
                        surface.set_thread_session_metadata(session_metadata);
                        updated = true;
                    }
                    self.remember_active_thread_summary(
                        &execution_target,
                        &thread,
                        activated_new_thread,
                    );
                    updated |= self.repair_selected_thread_title_if_needed(execution_target);
                }
                TurnWorkerUpdate::ThreadTitleEligible {
                    execution_target,
                    candidate,
                } => {
                    updated |= self.repair_thread_title_from_candidate(execution_target, candidate);
                }
                TurnWorkerUpdate::GraphMutationFinished(update) => {
                    updated |= self.finish_graph_mutation_update(update);
                }
                TurnWorkerUpdate::LifecycleYieldAccepted(yielded) => {
                    if let Some(surface) = self.conversation_surface_mut() {
                        updated |= surface.record_lifecycle_yield(yielded);
                    }
                }
                TurnWorkerUpdate::Event(event)
                    if self.conversation_surface().is_some_and(|surface| {
                        surface.stream_event_targets_invalidated_turn(&event)
                    }) =>
                {
                    updated = true;
                }
                TurnWorkerUpdate::Event(beryl_backend::TurnStreamEvent::TokenUsageUpdated {
                    thread_id,
                    turn_id,
                    token_usage,
                }) => {
                    updated |= self.apply_token_usage_update(thread_id, turn_id, token_usage);
                }
                TurnWorkerUpdate::Event(
                    beryl_backend::TurnStreamEvent::AccountRateLimitsUpdated { rate_limits },
                ) => {
                    updated |= self.apply_account_rate_limits_update(rate_limits);
                }
                TurnWorkerUpdate::Event(beryl_backend::TurnStreamEvent::ThreadNameUpdated {
                    thread_id,
                    thread_name,
                }) => {
                    updated |= self.apply_thread_name_update(thread_id, thread_name);
                }
                TurnWorkerUpdate::Event(event) => {
                    let execution_target = match &self.state {
                        ShellState::Ready(ready) => Some(ready.execution_target.clone()),
                        _ => None,
                    };
                    let started_turn = match &event {
                        beryl_backend::TurnStreamEvent::TurnStarted { thread_id, turn } => {
                            Some((thread_id.clone(), turn.id.clone()))
                        }
                        _ => None,
                    };
                    let mut applied_stream_event = AppliedStreamEvent::default();
                    let mut pending_steering = None;
                    if let Some((thread_id, _)) = started_turn.as_ref() {
                        self.note_transcript_edit_replacement_turn_started(thread_id, cx);
                    }
                    if let Some(surface) = self.conversation_surface_mut() {
                        if let Some((thread_id, _)) = started_turn.as_ref() {
                            surface.promote_pending_turn_defaults(thread_id);
                        }
                        applied_stream_event =
                            surface.apply_stream_event(event, execution_target.as_ref());
                        if let Some((thread_id, turn_id)) = started_turn.as_ref()
                            && let Some(fragments) = surface
                                .take_pending_active_turn_steering_for_started_turn(
                                    thread_id, turn_id,
                                )
                        {
                            pending_steering =
                                Some((thread_id.clone(), turn_id.clone(), fragments));
                        }
                        updated = true;
                    }
                    if let Some((thread_id, turn_id, fragments)) = pending_steering {
                        self.begin_turn_steering(thread_id, turn_id, fragments);
                    }
                    if let Some(candidate) = applied_stream_event.title_candidate {
                        self.queue_workspace_title_candidate(
                            candidate.user_input,
                            candidate.assistant_text,
                        );
                    }
                    if let Some(candidate) = applied_stream_event
                        .lifecycle_yield
                        .as_ref()
                        .and_then(TerminalLifecycleYield::lifecycle_notification_candidate)
                    {
                        self.play_lifecycle_notification_if_attention_triggered(
                            candidate, window, cx,
                        );
                    }
                    if let Some(request) = applied_stream_event
                        .lifecycle_yield
                        .as_ref()
                        .and_then(phase_continue_request)
                    {
                        self.pending_lifecycle_phase_continue = Some(request);
                    }
                    if let Some(candidate) = applied_stream_event.turn_completion_sound {
                        self.play_end_turn_sound_if_attention_triggered(candidate, window, cx);
                    }
                }
                TurnWorkerUpdate::Finished(outcome) => {
                    let pending_thread_id = match &outcome {
                        TurnWorkerOutcome::Finished {
                            active_thread_id, ..
                        } => Some(active_thread_id.clone()),
                        TurnWorkerOutcome::Failed { .. } => None,
                    };
                    let failure_message = match &outcome {
                        TurnWorkerOutcome::Failed { message } => Some(message.clone()),
                        TurnWorkerOutcome::Finished { .. } => None,
                    };
                    let edit_replacement_failed_before_start = failure_message.is_some()
                        && self
                            .transcript_edit_replacement_turn
                            .as_ref()
                            .is_some_and(|replacement| !replacement.turn_started);
                    self.turn_receiver = None;
                    self.shell_tool_receiver = None;
                    let sound_candidate = self.finish_turn_worker(outcome);
                    if failure_message.is_some() {
                        self.pending_lifecycle_phase_continue = None;
                    }
                    self.finish_transcript_edit_replacement_turn(
                        edit_replacement_failed_before_start,
                        failure_message.as_deref(),
                        window,
                        cx,
                    );
                    if let Some(thread_id) = pending_thread_id {
                        match self.pending_lifecycle_phase_continue.take() {
                            Some(request) if request.thread_id() == thread_id => {
                                self.begin_lifecycle_phase_continue(request, window, cx);
                            }
                            Some(_) | None => {
                                self.begin_pending_turn_input_queue_for_thread(&thread_id);
                            }
                        }
                    }
                    self.begin_workspace_title_generation_if_needed();
                    if let Some(candidate) = sound_candidate {
                        self.play_end_turn_sound_if_attention_triggered(candidate, window, cx);
                    }
                    updated = true;
                    break;
                }
            }
        }

        updated
    }

    fn poll_shell_dynamic_tool_requests(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut processed_updates = 0usize;
        let poll_started_at = Instant::now();
        loop {
            if processed_updates >= SHELL_WORKER_POLL_MAX_EVENTS_PER_FRAME
                || poll_started_at.elapsed() >= SHELL_WORKER_POLL_MAX_FRAME_TIME
            {
                return false;
            }

            let next_request = match self.shell_tool_receiver.as_ref() {
                Some(receiver) => receiver.try_recv(),
                None => return false,
            };

            match next_request {
                Ok(request) => {
                    processed_updates = processed_updates.saturating_add(1);
                    if !request.try_claim() {
                        continue;
                    }
                    if is_beryl_diagnostic_child_dynamic_tool(request.request()) {
                        self.spawn_diagnostic_child_dynamic_tool_worker(request);
                        continue;
                    }
                    let response = if is_beryl_gui_control_dynamic_tool(request.request()) {
                        self.handle_beryl_gui_control_dynamic_tool_request(
                            request.request(),
                            window,
                            cx,
                        )
                    } else {
                        let snapshot = self.diagnostic_tool_snapshot(window, cx);
                        dispatch_beryl_diagnostic_dynamic_tool_call(request.request(), snapshot)
                    };
                    request.respond(response);
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => {
                    self.shell_tool_receiver = None;
                    return false;
                }
            }
        }
    }

    fn spawn_diagnostic_child_dynamic_tool_worker(&self, request: ShellDynamicToolRequest) {
        let supervisor_home = match self.bootstrap.beryl_home_dir() {
            Ok(supervisor_home) => supervisor_home,
            Err(error) => {
                let response = diagnostic_child_failure_response(
                    request.request(),
                    "diagnostic_child_lifecycle_error",
                    error.to_string(),
                );
                request.respond(response);
                return;
            }
        };
        let supervisor = Arc::clone(&self.diagnostic_child_supervisor);
        thread::spawn(move || {
            let response = match supervisor.lock() {
                Ok(mut supervisor) => dispatch_beryl_diagnostic_child_dynamic_tool_call(
                    &mut supervisor,
                    &supervisor_home,
                    request.request(),
                ),
                Err(error) => diagnostic_child_failure_response(
                    request.request(),
                    "diagnostic_child_lifecycle_error",
                    format!("diagnostic child supervisor lock was poisoned: {error}"),
                ),
            };
            request.respond(response);
        });
    }

    fn poll_diagnostic_target_requests(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut processed_updates = 0usize;
        let poll_started_at = Instant::now();
        loop {
            if processed_updates >= SHELL_WORKER_POLL_MAX_EVENTS_PER_FRAME
                || poll_started_at.elapsed() >= SHELL_WORKER_POLL_MAX_FRAME_TIME
            {
                return false;
            }

            let next_request = match self.diagnostic_target_receiver.as_ref() {
                Some(receiver) => receiver.try_recv(),
                None => return false,
            };

            match next_request {
                Ok(DiagnosticTargetShellRequest::Execute(request)) => {
                    processed_updates = processed_updates.saturating_add(1);
                    if !request.try_claim() {
                        continue;
                    }
                    let response = self.handle_diagnostic_target_protocol_request(
                        request.request(),
                        window,
                        cx,
                    );
                    request.respond(response);
                }
                Ok(DiagnosticTargetShellRequest::Shutdown) => {
                    self.diagnostic_target_receiver = None;
                    cx.quit();
                    return true;
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => {
                    self.diagnostic_target_receiver = None;
                    return false;
                }
            }
        }
    }

    fn poll_tool_activity_nickname_updates(&mut self) -> bool {
        let outcomes = match self.tool_activity_nickname_resolver.poll() {
            ToolActivityNicknamePoll::Finished(outcomes) => outcomes,
            ToolActivityNicknamePoll::Idle | ToolActivityNicknamePoll::Pending => {
                return false;
            }
        };

        let resolved_metadata = outcomes
            .into_iter()
            .filter_map(|outcome| match outcome {
                ToolActivityNicknameOutcome::Resolved { metadata } => Some(metadata),
                ToolActivityNicknameOutcome::Unresolved {
                    thread_id: _thread_id,
                    message: _message,
                } => None,
            })
            .collect::<Vec<_>>();
        if resolved_metadata.is_empty() {
            return false;
        }

        self.conversation_surface_mut().is_some_and(|surface| {
            surface
                .tool_activity
                .apply_thread_read_metadata(resolved_metadata.iter())
        })
    }

    fn begin_tool_activity_nickname_resolution_if_needed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.tool_activity_nickname_resolver.has_active_worker() {
            return false;
        }

        let resolution_targets = self
            .conversation_surface()
            .map(ConversationSurfaceState::tool_activity_subagent_metadata_targets)
            .map(|targets| {
                targets
                    .into_iter()
                    .map(|target| ToolActivityNicknameResolutionTarget {
                        thread_id: target.thread_id,
                        requires_nickname: target.requires_nickname,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.tool_activity_nickname_resolver.retain_retry_threads(
            resolution_targets
                .iter()
                .map(|target| target.thread_id.as_str()),
        );
        if resolution_targets.is_empty() {
            return false;
        }

        let Some(connector) = self.backend_client_connector() else {
            return false;
        };
        let started = self.tool_activity_nickname_resolver.begin_if_needed(
            resolution_targets,
            connector,
            self.bootstrap.probe_timeout(),
        );
        if started {
            self.schedule_poll_if_needed(window, cx);
        }
        started
    }

    fn poll_turn_steering_updates(&mut self) -> bool {
        let mut updated = false;
        let mut index = 0;
        while index < self.turn_steering_receivers.len() {
            match self.turn_steering_receivers[index].receiver.try_recv() {
                Ok(TurnSteeringUpdate::Finished(outcome)) => {
                    self.turn_steering_receivers.remove(index);
                    self.finish_turn_steering(outcome);
                    updated = true;
                }
                Err(TryRecvError::Empty) => {
                    index += 1;
                }
                Err(TryRecvError::Disconnected) => {
                    let task = self.turn_steering_receivers.remove(index);
                    self.queue_steering_fragments_for_next_turn(
                        task.thread_id,
                        task.fragments,
                        "Beryl lost the background task that was steering the active turn."
                            .to_string(),
                    );
                    updated = true;
                }
            }
        }

        updated
    }

    fn finish_turn_steering(&mut self, outcome: TurnSteeringOutcome) {
        match outcome {
            TurnSteeringOutcome::Steered => {}
            TurnSteeringOutcome::QueueForNextTurn {
                thread_id,
                fragments,
                message,
            } => {
                self.queue_steering_fragments_for_next_turn(thread_id, fragments, message);
            }
        }
    }

    fn repair_selected_thread_title_if_needed(&mut self, execution_target: WorkspaceId) -> bool {
        let Some((thread_id, backend_name)) = self.selected_thread_title_repair_metadata() else {
            return false;
        };

        self.repair_missing_thread_title_for_thread(execution_target, thread_id, backend_name, None)
    }

    fn repair_thread_title_from_candidate(
        &mut self,
        execution_target: WorkspaceId,
        candidate: thread_title::ThreadTitleCandidate,
    ) -> bool {
        let thread_id = candidate.target_thread_id().to_string();
        self.repair_missing_thread_title_for_thread(
            execution_target,
            thread_id,
            None,
            Some(candidate.user_input().to_string()),
        )
    }

    fn repair_missing_thread_title_for_thread(
        &mut self,
        execution_target: WorkspaceId,
        thread_id: String,
        backend_name: Option<String>,
        fallback_user_input: Option<String>,
    ) -> bool {
        let mut updated = false;
        let title_task_active = self.thread_title_task_active_for_thread(&thread_id);
        let backend_name_for_guard = if self
            .thread_ignores_backend_name_for_automatic_title(&thread_id, backend_name.as_deref())
        {
            None
        } else {
            backend_name.clone()
        };
        if let Some(backend_name) = backend_name_for_guard.clone() {
            if !title_task_active {
                updated |= self.apply_thread_name_update(thread_id.clone(), Some(backend_name));
            }
        }

        let known_user_input = self.earliest_known_user_input_for_thread(&thread_id);
        let Some(candidate) = thread_title::thread_title_repair_candidate(
            &thread_id,
            self.thread_title_generation_can_start(&thread_id),
            backend_name_for_guard.as_deref(),
            title_task_active,
            known_user_input.as_deref(),
            fallback_user_input.as_deref(),
        ) else {
            return updated;
        };

        if self.begin_thread_title_generation(execution_target, candidate) {
            updated |= self.mark_thread_title_generation_started(&thread_id);
        }
        updated
    }

    fn poll_composer_image_delivery_updates(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(receiver) = self.composer_image_delivery_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(ComposerImageDeliveryUpdate::Finished(result)) => {
                self.composer_image_delivery_receiver = None;
                self.finish_composer_image_delivery(result, cx);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.composer_image_delivery_receiver = None;
                self.finish_composer_image_delivery(
                    Err(
                        "Beryl lost the background task that was preparing pasted images."
                            .to_string(),
                    ),
                    cx,
                );
                true
            }
        }
    }

    fn poll_composer_image_asset_updates(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(receiver) = self.composer_image_asset_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(ComposerImageAssetUpdate::Finished(result)) => {
                self.composer_image_asset_receiver = None;
                self.finish_composer_image_asset_paste(result, cx);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.composer_image_asset_receiver = None;
                self.finish_composer_image_asset_paste(
                    Err(
                        "Beryl lost the background task that was storing the pasted image."
                            .to_string(),
                    ),
                    cx,
                );
                true
            }
        }
    }

    fn selected_thread_title_repair_metadata(&self) -> Option<(String, Option<String>)> {
        let thread = self.conversation_surface()?.selected_thread()?;
        Some((
            thread.id.clone(),
            normalized_thread_name(thread.name.as_deref()),
        ))
    }

    fn earliest_known_user_input_for_thread(&self, thread_id: &str) -> Option<String> {
        let surface = self.conversation_surface()?;
        (surface.selected_thread_id() == Some(thread_id))
            .then(|| {
                surface
                    .earliest_known_user_input_fragment_text()
                    .map(str::to_string)
            })
            .flatten()
    }

    fn thread_title_task_active_for_thread(&self, thread_id: &str) -> bool {
        thread_title::thread_title_task_active_for_thread(&self.thread_title_receivers, thread_id)
    }

    fn thread_ignores_backend_name_for_automatic_title(
        &self,
        thread_id: &str,
        backend_name: Option<&str>,
    ) -> bool {
        let thread_id = ConversationThreadId::new(thread_id.to_string());
        self.workspace_shell_state()
            .and_then(|loaded| loaded.workspace_state.thread_registration(&thread_id))
            .is_some_and(|thread| thread.ignores_backend_name_for_automatic_title(backend_name))
    }

    fn begin_thread_title_generation(
        &mut self,
        execution_target: WorkspaceId,
        candidate: thread_title::ThreadTitleCandidate,
    ) -> bool {
        let thread_id = candidate.target_thread_id().to_string();
        if !self.thread_title_generation_can_start(&thread_id) {
            return false;
        }
        if self.thread_title_receivers.len() >= thread_title::MAX_THREAD_TITLE_WORKERS {
            warn!(
                thread_id = candidate.target_thread_id(),
                max_workers = thread_title::MAX_THREAD_TITLE_WORKERS,
                "skipping automatic thread-title generation because the worker limit is reached"
            );
            return false;
        }

        let Some(connector) = self.backend_client_connector() else {
            warn!(
                thread_id = candidate.target_thread_id(),
                "skipping automatic thread-title generation because no backend connector is available"
            );
            return false;
        };

        let cancellation = ThreadTitleCancellation::new();
        let receiver = spawn_thread_title_worker(
            connector,
            execution_target,
            candidate,
            cancellation.clone(),
            self.bootstrap.probe_timeout(),
        );
        self.thread_title_receivers
            .push(ThreadTitleTask::new(thread_id, cancellation, receiver));
        true
    }

    fn poll_thread_title_updates(&mut self) -> bool {
        let outcomes = thread_title::poll_thread_title_tasks(&mut self.thread_title_receivers);
        self.finish_thread_title_task_outcomes(outcomes, true)
    }

    fn thread_title_generation_can_start(&self, thread_id: &str) -> bool {
        let thread_id = ConversationThreadId::new(thread_id.to_string());
        self.workspace_shell_state().is_some_and(|loaded| {
            loaded
                .workspace_state
                .thread_automatic_title_generation_eligible(&thread_id)
        })
    }

    fn finish_thread_title_task_outcomes(
        &mut self,
        outcomes: Vec<ThreadTitleTaskOutcome>,
        persist_abandoned: bool,
    ) -> bool {
        let mut updated = false;
        for outcome in outcomes {
            match outcome {
                ThreadTitleTaskOutcome::Finished { thread_id, result } => {
                    updated |= self.finish_thread_title_update(thread_id, result);
                }
                ThreadTitleTaskOutcome::Abandoned { thread_id } => {
                    updated |=
                        self.mark_thread_title_generation_abandoned(&thread_id, persist_abandoned);
                }
                ThreadTitleTaskOutcome::Disconnected { thread_id } => {
                    warn!(
                        thread_id = thread_id.as_str(),
                        "thread-title worker stopped before returning a result"
                    );
                    updated = true;
                    updated |=
                        self.mark_thread_title_generation_abandoned(&thread_id, persist_abandoned);
                }
            }
        }

        updated
    }

    fn finish_thread_title_update(&mut self, thread_id: String, result: ThreadTitleResult) -> bool {
        match result {
            ThreadTitleResult::Applied { title } => {
                let _ = self.apply_authoritative_thread_name_update(thread_id, Some(title));
                self.mark_member_thread_inventory_refresh_needed();
                true
            }
            ThreadTitleResult::Cancelled => {
                self.mark_thread_title_generation_abandoned(&thread_id, true)
            }
            ThreadTitleResult::Failed { message } => {
                warn!(
                    thread_id = thread_id.as_str(),
                    error = %message,
                    "automatic thread-title generation failed"
                );
                self.block_if_backend_process_dead(
                    "Managed backend disconnected during title generation",
                    "The backend process exited while Beryl was generating an automatic thread title.",
                    &message,
                )
            }
        }
    }

    fn mark_thread_title_generation_started(&mut self, thread_id: &str) -> bool {
        let thread_id = ConversationThreadId::new(thread_id.to_string());
        let changed = {
            let Some(loaded) = self.workspace_shell_state_mut() else {
                return false;
            };

            match loaded
                .workspace_state
                .mark_thread_automatic_title_generation_started(&thread_id)
            {
                Ok(changed) => changed,
                Err(error) => {
                    warn!(
                        thread_id = thread_id.as_str(),
                        error = %error,
                        "could not record automatic thread-title generation start"
                    );
                    false
                }
            }
        };

        if changed {
            self.persist_current_workspace_state(false);
        }
        changed
    }

    fn mark_thread_title_generation_abandoned(&mut self, thread_id: &str, persist: bool) -> bool {
        let thread_id = ConversationThreadId::new(thread_id.to_string());
        let changed = {
            let Some(loaded) = self.workspace_shell_state_mut() else {
                return false;
            };

            match loaded
                .workspace_state
                .mark_thread_automatic_title_generation_abandoned(&thread_id)
            {
                Ok(changed) => changed,
                Err(error) => {
                    warn!(
                        thread_id = thread_id.as_str(),
                        error = %error,
                        "could not record automatic thread-title generation abandonment"
                    );
                    false
                }
            }
        };

        if changed && persist {
            self.persist_current_workspace_state(false);
        }
        changed
    }

    fn apply_token_usage_update(
        &mut self,
        thread_id: String,
        turn_id: String,
        token_usage: beryl_backend::ThreadTokenUsage,
    ) -> bool {
        let Some(surface) = self.conversation_surface_mut() else {
            return false;
        };

        let applied = surface.apply_token_usage_update(
            thread_id.clone(),
            turn_id.clone(),
            token_usage.clone(),
        );
        if applied {
            self.record_token_usage_update_snapshot(&thread_id, &turn_id, &token_usage);
        }

        applied
    }

    fn apply_account_rate_limits_update(
        &mut self,
        rate_limits: beryl_backend::RateLimitSnapshot,
    ) -> bool {
        let Some(surface) = self.conversation_surface_mut() else {
            return false;
        };

        surface.apply_account_rate_limits_update(rate_limits)
    }

    fn apply_account_rate_limits_read(
        &mut self,
        rate_limits: beryl_backend::AccountRateLimitsResponse,
    ) -> bool {
        let Some(surface) = self.conversation_surface_mut() else {
            return false;
        };

        surface.replace_account_rate_limits(rate_limits)
    }

    fn begin_account_rate_limits_read(&mut self) {
        if self.account_rate_limits_receiver.is_some() {
            return;
        }

        let Some(connector) = self.backend_client_connector() else {
            return;
        };

        self.account_rate_limits_receiver = Some(spawn_account_rate_limits_worker(
            connector,
            self.bootstrap.probe_timeout(),
        ));
    }

    fn poll_account_rate_limits_updates(&mut self) -> bool {
        let Some(receiver) = self.account_rate_limits_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(AccountRateLimitsUpdate::Finished(outcome)) => {
                self.account_rate_limits_receiver = None;
                match outcome {
                    AccountRateLimitsOutcome::Loaded(rate_limits) => {
                        self.apply_account_rate_limits_read(rate_limits)
                    }
                    AccountRateLimitsOutcome::Failed { message } => {
                        warn!(message = %message, "failed to seed account rate limits");
                        false
                    }
                }
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.account_rate_limits_receiver = None;
                warn!("account rate-limit startup worker stopped before returning a result");
                false
            }
        }
    }

    fn apply_thread_name_update(&mut self, thread_id: String, thread_name: Option<String>) -> bool {
        self.apply_thread_name_update_from_source(thread_id, thread_name, false)
    }

    fn apply_authoritative_thread_name_update(
        &mut self,
        thread_id: String,
        thread_name: Option<String>,
    ) -> bool {
        self.apply_thread_name_update_from_source(thread_id, thread_name, true)
    }

    fn apply_thread_name_update_from_source(
        &mut self,
        thread_id: String,
        thread_name: Option<String>,
        authoritative: bool,
    ) -> bool {
        let thread_id = ConversationThreadId::new(thread_id);
        let mut thread_name = normalized_thread_name(thread_name.as_deref());
        if !authoritative
            && self.thread_ignores_backend_name_for_automatic_title(
                thread_id.as_str(),
                thread_name.as_deref(),
            )
        {
            thread_name = None;
        }
        let title_task_changed = thread_name
            .is_some()
            .then(|| self.cancel_thread_title_workers_for_thread(thread_id.as_str()))
            .unwrap_or(false);
        let workspace_update = self.workspace_shell_state_mut().map(|loaded| {
            let result = if authoritative {
                loaded
                    .workspace_state
                    .set_authoritative_thread_backend_name(&thread_id, thread_name.clone())
            } else {
                loaded
                    .workspace_state
                    .set_thread_backend_name(&thread_id, thread_name.clone())
            };
            let changed = match result {
                Ok(changed) => changed,
                Err(error) => {
                    warn!(
                        thread_id = thread_id.as_str(),
                        error = %error,
                        "received backend thread-name update for an unregistered thread"
                    );
                    false
                }
            };
            (changed, loaded.workspace_state.clone())
        });

        let Some((workspace_changed, workspace_state)) = workspace_update else {
            return false;
        };
        if workspace_changed {
            self.persist_current_workspace_state(true);
        }

        let surface_changed = self.conversation_surface_mut().is_some_and(|surface| {
            surface.apply_thread_name_update(&workspace_state, &thread_id, thread_name.as_deref())
        });

        title_task_changed || workspace_changed || surface_changed
    }

    fn record_token_usage_update_snapshot(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        token_usage: &beryl_backend::ThreadTokenUsage,
    ) {
        let observed_at_millis = token_usage_snapshot::current_unix_millis();
        let snapshot = token_usage_snapshot::thread_token_usage_snapshot(
            turn_id,
            token_usage,
            observed_at_millis,
        );
        let record_result = {
            let Some(loaded) = self.workspace_shell_state_mut() else {
                return;
            };
            loaded.workspace_state.record_thread_token_usage_snapshot(
                &ConversationThreadId::new(thread_id.to_string()),
                snapshot.clone(),
            )
        };

        match record_result {
            Ok(true) => self.queue_token_usage_snapshot_persistence(
                thread_id.to_string(),
                turn_id.to_string(),
                snapshot,
            ),
            Ok(false) => {}
            Err(error) => warn!(
                thread_id,
                turn_id,
                error = %error,
                "failed to record last-known token usage snapshot"
            ),
        }
    }

    fn queue_token_usage_snapshot_persistence(
        &self,
        thread_id: String,
        turn_id: String,
        snapshot: ConversationThreadTokenUsageSnapshot,
    ) {
        let Some(loaded) = self.workspace_shell_state() else {
            return;
        };
        let workspace_id = loaded.workspace.id().clone();
        let model_thread_id = ConversationThreadId::new(thread_id.clone());

        self.workspace_persistence_queue
            .record_token_usage_snapshot(workspace_id, model_thread_id, turn_id, snapshot);
    }

    fn hydrate_selected_thread_token_usage_snapshot(&mut self) -> bool {
        let Some(workspace_state) = self
            .workspace_shell_state()
            .map(|loaded| loaded.workspace_state.clone())
        else {
            return false;
        };
        self.conversation_surface_mut().is_some_and(|surface| {
            surface.hydrate_selected_thread_token_usage_snapshot(&workspace_state)
        })
    }

    fn queue_workspace_title_candidate(&mut self, user_input: String, assistant_text: String) {
        if self.pending_workspace_title_candidate.is_some()
            || self.workspace_title_receiver.is_some()
        {
            return;
        }

        let Some(loaded) = self.loaded_workspace() else {
            return;
        };
        if !loaded.workspace.is_untitled() {
            return;
        }

        self.pending_workspace_title_candidate =
            WorkspaceTitleCandidate::new(loaded.workspace.id().clone(), user_input, assistant_text);
    }

    fn begin_workspace_title_generation_if_needed(&mut self) -> bool {
        if self.workspace_title_receiver.is_some() {
            return false;
        }

        let Some(candidate) = self.pending_workspace_title_candidate.take() else {
            return false;
        };
        let still_active_untitled = self.loaded_workspace().is_some_and(|loaded| {
            loaded.workspace.id() == candidate.workspace_id() && loaded.workspace.is_untitled()
        });
        if !still_active_untitled {
            return false;
        }

        let mut blockers = self.workspace_rename_blockers();
        blockers.persistence_work = false;
        if blockers.any() {
            warn!(
                workspace_id = candidate.workspace_id().as_str(),
                "skipping automatic workspace title because workspace work is active or queued"
            );
            return false;
        }

        let Some(persistence) = self.workspace_persistence_for_worker() else {
            self.pending_workspace_title_candidate = Some(candidate);
            return false;
        };

        self.workspace_title_receiver = Some(spawn_workspace_title_worker(
            persistence,
            candidate,
            self.workspace_persistence_queue.flush(),
            self.bootstrap.probe_timeout(),
        ));
        true
    }

    fn poll_workspace_title_updates(&mut self, window: &mut Window) -> bool {
        let Some(receiver) = self.workspace_title_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(WorkspaceTitleUpdate::Generated {
                workspace_id,
                result,
            }) => self.finish_workspace_title_update(workspace_id, result, false, window),
            Ok(WorkspaceTitleUpdate::Manual {
                workspace_id,
                result,
            }) => self.finish_workspace_title_update(workspace_id, result, true, window),
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.workspace_title_receiver = None;
                warn!("workspace title worker stopped before returning a result");
                false
            }
        }
    }

    fn finish_workspace_title_update(
        &mut self,
        workspace_id: BerylWorkspaceId,
        result: WorkspaceTitleResult,
        manual: bool,
        window: &mut Window,
    ) -> bool {
        self.workspace_title_receiver = None;
        match result {
            WorkspaceTitleResult::Updated(change) => {
                let old_workspace_id = change.old_workspace_id;
                let new_workspace_id = change.new_workspace_id;
                let manifest = change.manifest;
                let mut active_workspace_updated = false;
                if let Some(loaded) = self.loaded_workspace_mut() {
                    let close_rename_editor = loaded
                        .workspace_picker
                        .rename_editor_target()
                        .is_some_and(|target| target == &old_workspace_id);
                    if loaded.workspace.id() == &old_workspace_id {
                        loaded.replace_manifest_for_rename(&old_workspace_id, manifest.clone());
                        window.set_window_title(&format!("Beryl - {}", loaded.workspace.title()));
                        active_workspace_updated = true;
                    } else if let Some(existing) = loaded
                        .known_workspaces
                        .iter_mut()
                        .find(|workspace| workspace.id() == &old_workspace_id)
                    {
                        *existing = manifest;
                        if old_workspace_id != new_workspace_id {
                            let member_paths = loaded
                                .workspace_picker_member_paths
                                .remove(&old_workspace_id)
                                .unwrap_or_default();
                            loaded
                                .workspace_picker_member_paths
                                .insert(new_workspace_id.clone(), member_paths);
                        }
                    }
                    if close_rename_editor {
                        loaded.workspace_picker.close_rename_editor();
                        loaded.clear_workspace_picker_notice();
                    }
                }
                if active_workspace_updated {
                    self.rekey_workspace_session_after_title_change(new_workspace_id);
                }
                true
            }
            WorkspaceTitleResult::Unchanged => {
                if manual && let Some(loaded) = self.loaded_workspace_mut() {
                    let close_rename_editor = loaded
                        .workspace_picker
                        .rename_editor_target()
                        .map_or(true, |target| target == &workspace_id);
                    if close_rename_editor {
                        loaded.workspace_picker.close_rename_editor();
                        loaded.clear_workspace_picker_notice();
                        return true;
                    }
                }
                false
            }
            WorkspaceTitleResult::Failed(message) => {
                if manual && let Some(loaded) = self.loaded_workspace_mut() {
                    loaded.set_workspace_picker_notice(format!(
                        "Beryl could not rename the workspace: {message}"
                    ));
                    return true;
                }
                warn!(
                    workspace_id = workspace_id.as_str(),
                    error = %message,
                    "failed to auto-title workspace"
                );
                false
            }
        }
    }

    fn rekey_workspace_session_after_title_change(&mut self, workspace_id: BerylWorkspaceId) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface
                .member_thread_inventory_mut()
                .rekey_workspace_id(workspace_id);
        }
    }

    fn toggle_workspace_picker(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut focus_filter = false;
        if let Some(loaded) = self.loaded_workspace_mut() {
            let opening = !loaded.workspace_picker.is_open();
            loaded.workspace_picker.toggle();
            if opening {
                loaded.clear_workspace_picker_notice();
                loaded.reset_workspace_picker_scroll();
                loaded.reset_workspace_members_scroll();
                focus_filter = true;
            }
            cx.notify();
        }
        if focus_filter {
            self.workspace_picker_filter_input.update(cx, |input, cx| {
                input.set_text(String::new(), cx);
                input.set_selection(0..0, false, cx);
            });
            let focus_handle = self
                .workspace_picker_filter_input
                .read(cx)
                .tab_focus_handle();
            window.focus(&focus_handle);
            self.begin_runtime_selector_distro_refresh_if_needed(window, cx);
            self.begin_implicit_home_path_resolution_if_needed(cx);
        }
    }

    fn begin_runtime_selector_distro_refresh_if_needed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_runtime_selector_distro_receiver.is_some() {
            return;
        }

        let should_start = self
            .workspace_shell_state_mut()
            .is_some_and(LoadedWorkspaceState::begin_runtime_selector_distro_refresh);
        if !should_start {
            return;
        }

        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = list_wsl_distros().map_err(|error| error.to_string());
            let _ = sender.send(WorkspaceRuntimeSelectorDistroUpdate::Finished(result));
        });
        self.workspace_runtime_selector_distro_receiver = Some(receiver);
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
    }

    fn begin_implicit_home_path_resolution_if_needed(&mut self, cx: &mut Context<Self>) {
        let (workspace_id, runtime) = {
            let Some(loaded) = self.workspace_shell_state_mut() else {
                return;
            };
            let Some(runtime) = loaded.implicit_home_path_resolution_needed() else {
                return;
            };
            loaded.begin_implicit_home_path_resolution(runtime.clone());
            (loaded.workspace.id().clone(), runtime)
        };

        let worker_runtime = runtime.clone();
        let completion_runtime = runtime.clone();
        cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        workspace_members::resolve_runtime_home_directory(&worker_runtime)
                            .map_err(|error| error.to_string())
                    })
                    .await;
                let _ = view.update(&mut cx, |view, cx| {
                    view.finish_implicit_home_path_resolution(
                        workspace_id,
                        completion_runtime,
                        result,
                        cx,
                    );
                });
            }
        })
        .detach();

        cx.notify();
    }

    fn finish_implicit_home_path_resolution(
        &mut self,
        workspace_id: BerylWorkspaceId,
        runtime: RuntimeMode,
        result: Result<PathBuf, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(loaded) = self.workspace_shell_state_mut() else {
            return;
        };
        if loaded.workspace.id() != &workspace_id {
            return;
        }

        let restored = loaded.finish_implicit_home_path_resolution(&runtime, result);
        if restored {
            self.persist_current_workspace_state(true);
        }
        cx.notify();
    }

    fn note_startup_scrollbar_motion(
        &mut self,
        _: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::Startup, cx);
    }

    fn note_startup_scrollbar_scroll(
        &mut self,
        _: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::Startup, cx);
    }

    fn release_transcript_submit_anchor(&mut self, cx: &mut Context<Self>) {
        let released = self
            .conversation_surface_mut()
            .is_some_and(ConversationSurfaceState::release_transcript_submit_anchor);
        if released {
            self.notify_transcript_panel(cx);
            cx.notify();
        }
    }

    fn note_transcript_scroll(&mut self, is_scrolled: bool, cx: &mut Context<Self>) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_transcript_user_scrolled(is_scrolled);
        }
        self.note_scrollbar_activity(ScrollbarRegion::Transcript, cx);
    }

    fn note_transcript_scroll_event(
        &mut self,
        event: &ListScrollEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_transcript_scroll(event.is_scrolled, cx);
        self.begin_older_thread_history_page_if_needed(event, window, cx);
    }

    fn begin_older_thread_history_page_if_needed(
        &mut self,
        event: &ListScrollEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.thread_history_page_receiver.is_some()
            || self.workspace_receiver.is_some()
            || self.graph_thread_start_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.status_operation_receiver.is_some()
            || self.turn_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
        {
            return;
        }

        let released = self
            .conversation_surface_mut()
            .is_some_and(|surface| surface.release_cold_history_pages(&event.visible_range));
        if released {
            self.notify_transcript_panel(cx);
        }

        let Some(connector) = self.backend_client_connector() else {
            return;
        };
        let Some((workspace_id, runtime_mode)) = (match &self.state {
            ShellState::Ready(ready) => Some((
                ready.loaded_workspace.workspace.id().clone(),
                ready.execution_target.runtime_mode().clone(),
            )),
            ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::Blocked(_)
            | ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_) => None,
        }) else {
            return;
        };
        let Some((thread_id, request)) = self.conversation_surface_mut().and_then(|surface| {
            let request = surface.begin_loading_thread_history_page(&event.visible_range)?;
            surface.cancel_transcript_edit_mode();
            Some(request)
        }) else {
            return;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return;
        };

        self.thread_history_page_receiver = Some(spawn_older_thread_history_page_worker(
            persistence,
            connector,
            workspace_id,
            runtime_mode,
            thread_id,
            request,
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        self.notify_transcript_panel(cx);
    }

    fn begin_composer_image_label_scan_if_needed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.composer_image_label_scan_receiver.is_some()
            || self.workspace_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
        {
            return false;
        }

        let Some(thread_id) = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_needing_composer_image_label_scan)
        else {
            return false;
        };

        let Some(connector) = self.backend_client_connector() else {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.fail_composer_image_label_scan(
                    &thread_id,
                    "Beryl does not have an active managed backend for image-label scanning.",
                );
            }
            return true;
        };

        self.composer_image_label_scan_receiver = Some(spawn_composer_image_label_scan_worker(
            connector,
            thread_id,
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        true
    }

    fn install_loaded_history_transcript_anchor(&mut self, cx: &mut Context<Self>) {
        let installed = self
            .conversation_surface_mut()
            .is_some_and(ConversationSurfaceState::install_loaded_history_transcript_anchor);
        if installed {
            self.notify_transcript_panel(cx);
            cx.notify();
        }
    }

    fn note_workspace_picker_scrollbar_motion(
        &mut self,
        _: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::WorkspacePicker, cx);
    }

    fn note_workspace_picker_scrollbar_scroll(
        &mut self,
        _: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::WorkspacePicker, cx);
    }

    fn note_workspace_members_scrollbar_motion(
        &mut self,
        _: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::WorkspaceMembers, cx);
    }

    fn note_workspace_members_scrollbar_scroll(
        &mut self,
        _: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::WorkspaceMembers, cx);
    }

    fn note_tool_activity_scrollbar_motion(
        &mut self,
        _: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::ToolActivity, cx);
    }

    fn note_tool_activity_scrollbar_scroll(
        &mut self,
        _: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::ToolActivity, cx);
        cx.stop_propagation();
    }

    fn note_composer_scrollbar_motion(
        &mut self,
        _: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::Composer, cx);
    }

    fn note_composer_scrollbar_scroll(
        &mut self,
        _: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::Composer, cx);
        cx.stop_propagation();
    }

    fn note_column_selector_horizontal_scrollbar_motion(
        &mut self,
        surface: ColumnSelectorSurface,
        _: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(column_selector_scrollbar_region(surface), cx);
    }

    fn note_column_selector_horizontal_scrollbar_scroll(
        &mut self,
        surface: ColumnSelectorSurface,
        _: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(column_selector_scrollbar_region(surface), cx);
    }

    fn note_graph_column_scrollbar_motion(
        &mut self,
        column_key: GraphColumnKey,
        _: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::GraphColumn(column_key), cx);
    }

    fn note_graph_column_scrollbar_scroll(
        &mut self,
        column_key: GraphColumnKey,
        _: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::GraphColumn(column_key), cx);
    }

    fn note_thread_selector_column_scrollbar_motion(
        &mut self,
        column_key: ThreadSelectorColumnKey,
        _: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::ThreadSelectorColumn(column_key), cx);
    }

    fn note_thread_selector_column_scrollbar_scroll(
        &mut self,
        column_key: ThreadSelectorColumnKey,
        _: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(ScrollbarRegion::ThreadSelectorColumn(column_key), cx);
    }

    fn activate_workspace_picker_item(
        &mut self,
        item_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let filter_text = self
            .workspace_picker_filter_input
            .read(cx)
            .text()
            .to_string();
        let Some(selected_item_index) = self.loaded_workspace().map(|loaded| {
            let visible_workspace_count = workspace_picker::filtered_workspace_indices(
                &loaded.known_workspaces,
                &loaded.workspace_picker_member_paths,
                &filter_text,
            )
            .len();
            let item_count = workspace_picker::workspace_picker_item_count(visible_workspace_count);
            item_index.min(item_count.saturating_sub(1))
        }) else {
            return false;
        };

        if selected_item_index == workspace_picker::CREATE_NEW_ITEM_INDEX {
            return self.begin_workspace_picker_create_new(window, cx);
        }

        let Some((current_workspace_id, selected_workspace_id)) =
            self.loaded_workspace().and_then(|loaded| {
                let visible_workspace_indices = workspace_picker::filtered_workspace_indices(
                    &loaded.known_workspaces,
                    &loaded.workspace_picker_member_paths,
                    &filter_text,
                );
                let workspace_index = workspace_picker::workspace_index_for_filtered_item_index(
                    selected_item_index,
                    &visible_workspace_indices,
                )?;
                loaded
                    .known_workspaces
                    .get(workspace_index)
                    .map(|selected| (loaded.workspace.id().clone(), selected.id().clone()))
            })
        else {
            return false;
        };

        if current_workspace_id == selected_workspace_id {
            if let Some(loaded) = self.loaded_workspace_mut() {
                loaded.workspace_picker.close();
            }
            cx.notify();
            return true;
        }

        if self.block_workspace_picker_transition_if_needed(
            workspace_picker::WorkspacePickerTransitionPath::SwitchWorkspace,
            cx,
        ) {
            return true;
        }

        if self.workspace_picker_action_receiver.is_some() {
            return true;
        }

        self.cancel_thread_title_workers();
        let Some(app_state) = self.app_state_for_worker() else {
            self.block_if_app_state_unavailable(window, cx);
            return true;
        };
        self.workspace_picker_action_receiver = Some(spawn_switch_workspace_worker(
            app_state.startup_persistence,
            app_state.workspace_persistence,
            selected_workspace_id,
            self.workspace_persistence_queue.flush(),
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
        true
    }

    fn begin_workspace_picker_create_new(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.block_workspace_picker_transition_if_needed(
            workspace_picker::WorkspacePickerTransitionPath::CreateWorkspace,
            cx,
        ) {
            return true;
        }

        if self.workspace_picker_action_receiver.is_some() {
            return true;
        }

        self.cancel_thread_title_workers();
        let Some(app_state) = self.app_state_for_worker() else {
            self.block_if_app_state_unavailable(window, cx);
            return true;
        };
        self.workspace_picker_action_receiver = Some(spawn_create_workspace_worker(
            app_state.startup_persistence,
            app_state.workspace_persistence,
            self.workspace_persistence_queue.flush(),
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
        true
    }

    fn begin_delete_workspace(
        &mut self,
        workspace_id: BerylWorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active_workspace_id) = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return;
        };

        if self.block_workspace_picker_transition_if_needed(
            workspace_picker::WorkspacePickerTransitionPath::DeleteWorkspace,
            cx,
        ) {
            return;
        }

        if self.workspace_picker_action_receiver.is_some() {
            return;
        }

        if active_workspace_id == workspace_id {
            self.cancel_thread_title_workers();
        }

        let Some(app_state) = self.app_state_for_worker() else {
            self.block_if_app_state_unavailable(window, cx);
            return;
        };
        self.workspace_picker_action_receiver = Some(spawn_delete_workspace_worker(
            app_state.startup_persistence,
            app_state.workspace_persistence,
            workspace_id,
            active_workspace_id,
            self.workspace_persistence_queue.flush(),
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
    }

    pub(crate) fn open_workspace_row_action_menu(
        &mut self,
        workspace_id: BerylWorkspaceId,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded
                .workspace_picker
                .open_row_action_menu(workspace_id, event.position);
            loaded.clear_workspace_picker_notice();
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(crate) fn toggle_workspace_runtime_selector_dropdown(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.loaded_workspace_mut() {
            let item_count = workspace_picker::runtime_selector_item_count(
                loaded.runtime_selector_distro_list().distro_names(),
            );
            loaded
                .workspace_picker
                .toggle_runtime_selector_dropdown(item_count);
            loaded
                .workspace_picker
                .set_focused_column(workspace_picker::WorkspacePickerFocusedColumn::Members);
            cx.notify();
        }
    }

    pub(crate) fn open_workspace_member_action_menu(
        &mut self,
        member_id: WorkspaceMemberId,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded
                .workspace_picker
                .open_member_action_menu(member_id, event.position);
            loaded.clear_workspace_members_notice();
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(crate) fn record_workspace_row_action_menu_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded.workspace_picker.set_row_action_menu_bounds(bounds);
        }
    }

    pub(crate) fn begin_workspace_delete_hold_from_action_menu(
        &mut self,
        _: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_workspace_delete_hold_from_action_menu_source(
            workspace_picker::WorkspaceDeleteHoldSource::Pointer,
            window,
            cx,
        );
    }

    pub(crate) fn cancel_workspace_delete_hold_from_action_menu(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.loaded_workspace_mut()
            && loaded
                .workspace_picker
                .cancel_delete_hold_source(workspace_picker::WorkspaceDeleteHoldSource::Pointer)
        {
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub(crate) fn cancel_workspace_delete_hold_on_hover_change(
        &mut self,
        hovered: &bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if *hovered {
            return;
        }

        if let Some(loaded) = self.loaded_workspace_mut()
            && loaded.workspace_picker.cancel_delete_hold()
        {
            cx.notify();
        }
    }

    pub(crate) fn begin_workspace_delete_keyboard_hold_from_action_menu(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.is_held
            || !workspace_picker_action_keyboard_activation_key(event.keystroke.key.as_str())
        {
            return;
        }

        self.begin_workspace_delete_hold_from_action_menu_source(
            workspace_picker::WorkspaceDeleteHoldSource::Keyboard,
            window,
            cx,
        );
    }

    pub(crate) fn cancel_workspace_delete_keyboard_hold_from_action_menu(
        &mut self,
        event: &KeyUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !workspace_picker_action_keyboard_activation_key(event.keystroke.key.as_str()) {
            return;
        }

        if let Some(loaded) = self.loaded_workspace_mut()
            && loaded
                .workspace_picker
                .cancel_delete_hold_source(workspace_picker::WorkspaceDeleteHoldSource::Keyboard)
        {
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn begin_workspace_delete_hold_from_action_menu_source(
        &mut self,
        source: workspace_picker::WorkspaceDeleteHoldSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_picker_action_receiver.is_some() {
            return;
        }

        let Some(workspace_id) = self.loaded_workspace().and_then(|loaded| {
            let workspace_id = loaded
                .workspace_picker
                .row_action_menu_active()?
                .workspace_id()
                .clone();
            loaded
                .known_workspaces
                .iter()
                .any(|workspace| workspace.id() == &workspace_id)
                .then_some(workspace_id)
        }) else {
            return;
        };

        let started = self.loaded_workspace_mut().is_some_and(|loaded| {
            loaded
                .workspace_picker
                .begin_delete_hold(workspace_id, source, Instant::now())
        });
        if !started {
            return;
        }

        self.schedule_poll_if_needed(window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    fn block_workspace_picker_transition_if_needed(
        &mut self,
        path: workspace_picker::WorkspacePickerTransitionPath,
        cx: &mut Context<Self>,
    ) -> bool {
        let reason = workspace_picker::workspace_picker_transition_path_disabled_reason(
            path,
            workspace_picker::WorkspacePickerTransitionBlockers {
                edit_rollback_work: self.transcript_edit_commit_receiver.is_some(),
                edit_replacement_work: self.transcript_edit_replacement_turn.is_some(),
            },
        );
        let Some(reason) = reason else {
            return false;
        };

        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded.set_workspace_picker_notice(reason);
        }
        cx.notify();
        true
    }

    fn poll_workspace_picker_delete_hold(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let target_exists = self
            .loaded_workspace()
            .and_then(|loaded| {
                let workspace_id = loaded
                    .workspace_picker
                    .row_action_menu_active()?
                    .workspace_id();
                Some(
                    loaded
                        .known_workspaces
                        .iter()
                        .any(|workspace| workspace.id() == workspace_id),
                )
            })
            .unwrap_or(false);
        let workspace_action_in_flight = self.workspace_picker_action_receiver.is_some();
        let now = Instant::now();
        let mut completed_workspace_id = None;
        let mut updated = false;

        if let Some(loaded) = self.loaded_workspace_mut() {
            let picker = &mut loaded.workspace_picker;
            if !window.is_window_active() || workspace_action_in_flight {
                updated |= picker.cancel_delete_hold();
            } else {
                updated |= picker.cancel_delete_hold_for_stale_target(target_exists);
                if picker.delete_hold_active() {
                    completed_workspace_id = picker.complete_delete_hold_if_ready(now);
                    updated = true;
                }
            }
        }

        if let Some(workspace_id) = completed_workspace_id {
            updated |=
                self.complete_workspace_delete_hold_from_action_menu(workspace_id, window, cx);
        }

        updated
    }

    fn complete_workspace_delete_hold_from_action_menu(
        &mut self,
        workspace_id: BerylWorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded.workspace_picker.close_row_action_menu();
        }
        self.begin_delete_workspace(workspace_id, window, cx);
        true
    }

    fn poll_workspace_picker_action_updates(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(receiver) = self.workspace_picker_action_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(update) => {
                self.workspace_picker_action_receiver = None;
                self.finish_workspace_picker_action(update, window, cx);
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.workspace_picker_action_receiver = None;
                warn!("workspace picker action worker stopped before returning a result");
                true
            }
        }
    }

    fn poll_workspace_runtime_selector_distro_updates(&mut self) -> bool {
        let Some(receiver) = self.workspace_runtime_selector_distro_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(WorkspaceRuntimeSelectorDistroUpdate::Finished(result)) => {
                self.workspace_runtime_selector_distro_receiver = None;
                if let Some(loaded) = self.workspace_shell_state_mut() {
                    loaded.finish_runtime_selector_distro_refresh(result);
                }
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.workspace_runtime_selector_distro_receiver = None;
                if let Some(loaded) = self.workspace_shell_state_mut() {
                    loaded.finish_runtime_selector_distro_refresh(Err(
                        "WSL distro discovery stopped before returning a result.".to_string(),
                    ));
                }
                warn!("runtime selector WSL distro worker stopped before returning a result");
                true
            }
        }
    }

    fn finish_workspace_picker_action(
        &mut self,
        update: WorkspacePickerActionUpdate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match update {
            WorkspacePickerActionUpdate::Created(Ok(opened)) => {
                self.finish_workspace_picker_opened_workspace(opened, window, cx);
            }
            WorkspacePickerActionUpdate::Created(Err(message)) => {
                warn!(
                    error = %message,
                    "failed to create a fresh semantic workspace from the picker"
                );
            }
            WorkspacePickerActionUpdate::Switched(Ok(opened)) => {
                self.finish_workspace_picker_opened_workspace(opened, window, cx);
            }
            WorkspacePickerActionUpdate::Switched(Err(message)) => {
                warn!(
                    error = %message,
                    "failed to switch semantic workspaces from the picker"
                );
            }
            WorkspacePickerActionUpdate::Deleted {
                workspace_id,
                result: Ok(outcome),
            } => {
                self.finish_workspace_picker_deleted_workspace(&workspace_id, outcome, window, cx);
            }
            WorkspacePickerActionUpdate::Deleted {
                workspace_id,
                result: Err(message),
            } => {
                warn!(
                    workspace_id = workspace_id.as_str(),
                    error = %message,
                    "failed to delete Beryl workspace from the picker"
                );
            }
        }
    }

    fn finish_workspace_picker_opened_workspace(
        &mut self,
        opened: WorkspacePickerOpenedWorkspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_background_activity();
        window.set_window_title(&format!("Beryl - {}", opened.workspace.title()));
        let loaded = LoadedWorkspaceState::new(
            opened.workspace,
            opened.known_workspaces,
            opened.workspace_picker_member_paths,
            opened.workspace_state,
            opened.workspace_ui_state,
            None,
        );
        if loaded.selected_runtime().is_some() {
            self.state = ShellState::WorkspaceLoaded(loaded);
            self.begin_open_target(RetryTarget::WorkspacePrimary, window, cx);
            return;
        }

        self.state = ShellState::WorkspaceIdle(IdleWorkspaceState::new(loaded));
        cx.notify();
    }

    fn finish_workspace_picker_deleted_workspace(
        &mut self,
        workspace_id: &BerylWorkspaceId,
        outcome: WorkspacePickerDeletionOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !outcome.deleted {
            warn!(
                workspace_id = workspace_id.as_str(),
                "workspace picker delete target was already absent"
            );
        }

        if let Some(replacement) = outcome.replacement_workspace {
            let workspace_state = outcome.replacement_workspace_state.unwrap_or_default();
            self.clear_background_activity();
            window.set_window_title(&format!("Beryl - {}", replacement.title()));
            let loaded = LoadedWorkspaceState::new(
                replacement,
                outcome.known_workspaces,
                outcome.workspace_picker_member_paths,
                workspace_state,
                outcome.replacement_workspace_ui_state.unwrap_or_default(),
                None,
            );
            if loaded.selected_runtime().is_some() {
                self.state = ShellState::WorkspaceLoaded(loaded);
                self.begin_open_target(RetryTarget::WorkspacePrimary, window, cx);
            } else {
                self.state = ShellState::WorkspaceIdle(IdleWorkspaceState::new(loaded));
                cx.notify();
            }
            return;
        }

        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded.known_workspaces = outcome.known_workspaces;
            loaded.workspace_picker_member_paths = outcome.workspace_picker_member_paths;
            loaded.refresh_active_workspace_picker_member_paths();
            loaded.workspace_picker.close();
        }
        cx.notify();
    }

    fn handle_workspace_picker_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(loaded) = self.loaded_workspace() else {
            return false;
        };
        if !loaded.workspace_picker.is_open() {
            return false;
        }
        if loaded.workspace_picker.rename_editor_open() {
            return match event.keystroke.key.as_str() {
                "escape" => {
                    if let Some(loaded) = self.loaded_workspace_mut() {
                        loaded.workspace_picker.close_rename_editor();
                        loaded.clear_workspace_picker_notice();
                    }
                    cx.notify();
                    true
                }
                "enter" => {
                    self.begin_submit_workspace_rename(window, cx);
                    true
                }
                _ => false,
            };
        }

        if loaded.workspace_picker.row_action_menu_is_open() {
            return if event.keystroke.key.as_str() == "escape" {
                if let Some(loaded) = self.loaded_workspace_mut() {
                    loaded.workspace_picker.close_row_action_menu();
                    cx.notify();
                }
                true
            } else {
                false
            };
        }

        if loaded.workspace_picker.runtime_selector_dropdown_is_open() {
            let item_count = workspace_picker::runtime_selector_item_count(
                loaded.runtime_selector_distro_list().distro_names(),
            );
            return match event.keystroke.key.as_str() {
                "escape" => {
                    if let Some(loaded) = self.loaded_workspace_mut() {
                        loaded.workspace_picker.close_runtime_selector_dropdown();
                        cx.notify();
                    }
                    true
                }
                "up" => {
                    if let Some(loaded) = self.loaded_workspace_mut()
                        && loaded
                            .workspace_picker
                            .move_runtime_selector_highlight(-1, item_count)
                    {
                        cx.notify();
                    }
                    true
                }
                "down" => {
                    if let Some(loaded) = self.loaded_workspace_mut()
                        && loaded
                            .workspace_picker
                            .move_runtime_selector_highlight(1, item_count)
                    {
                        cx.notify();
                    }
                    true
                }
                key if workspace_picker_action_keyboard_activation_key(key) => {
                    let runtime = self.loaded_workspace().and_then(|loaded| {
                        let highlighted_index = loaded
                            .workspace_picker
                            .runtime_selector_dropdown()
                            .highlighted_index();
                        workspace_picker::runtime_selector_row_for_index(
                            loaded.runtime_selector_distro_list().distro_names(),
                            highlighted_index,
                        )
                        .map(|row| workspace_picker::runtime_selector_row_runtime(&row))
                    });
                    if let Some(runtime) = runtime {
                        if let Some(loaded) = self.loaded_workspace_mut() {
                            loaded.workspace_picker.close_runtime_selector_dropdown();
                        }
                        self.select_workspace_runtime(runtime, window, cx);
                    }
                    true
                }
                _ => false,
            };
        }

        if loaded.workspace_picker.member_action_menu_is_open() {
            return if event.keystroke.key.as_str() == "escape" {
                if let Some(loaded) = self.loaded_workspace_mut() {
                    loaded.workspace_picker.close_member_action_menu();
                    cx.notify();
                }
                true
            } else {
                false
            };
        }

        match event.keystroke.key.as_str() {
            "escape" => {
                if let Some(loaded) = self.loaded_workspace_mut() {
                    loaded.workspace_picker.close();
                    cx.notify();
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn handle_workspace_picker_key_up(
        &mut self,
        event: &KeyUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(loaded) = self.loaded_workspace() else {
            return false;
        };
        if !loaded.workspace_picker.is_open()
            || !workspace_picker_action_keyboard_activation_key(event.keystroke.key.as_str())
        {
            return false;
        }

        let cancelled = self.loaded_workspace_mut().is_some_and(|loaded| {
            loaded
                .workspace_picker
                .cancel_delete_hold_source(workspace_picker::WorkspaceDeleteHoldSource::Keyboard)
        });
        if cancelled {
            cx.notify();
        }
        cancelled
    }

    fn handle_workspace_picker_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let should_dismiss = self.loaded_workspace().is_some_and(|loaded| {
            loaded
                .workspace_picker
                .should_dismiss_for_mouse_down(event.position)
        });
        if should_dismiss && let Some(loaded) = self.loaded_workspace_mut() {
            loaded.workspace_picker.close();
            cx.notify();
            return;
        }

        let should_dismiss_row_action_menu = self.loaded_workspace().is_some_and(|loaded| {
            loaded
                .workspace_picker
                .should_dismiss_row_action_menu_for_mouse_down(event.position)
        });
        if should_dismiss_row_action_menu && let Some(loaded) = self.loaded_workspace_mut() {
            loaded.workspace_picker.close_row_action_menu();
            cx.notify();
        }

        let should_dismiss_runtime_dropdown = self.loaded_workspace().is_some_and(|loaded| {
            loaded
                .workspace_picker
                .should_dismiss_runtime_selector_dropdown_for_mouse_down(event.position)
        });
        if should_dismiss_runtime_dropdown && let Some(loaded) = self.loaded_workspace_mut() {
            loaded.workspace_picker.close_runtime_selector_dropdown();
            cx.notify();
        }

        let should_dismiss_member_action_menu = self.loaded_workspace().is_some_and(|loaded| {
            loaded
                .workspace_picker
                .should_dismiss_member_action_menu_for_mouse_down(event.position)
        });
        if should_dismiss_member_action_menu && let Some(loaded) = self.loaded_workspace_mut() {
            loaded.workspace_picker.close_member_action_menu();
            cx.notify();
        }
    }

    fn handle_thread_selector_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(surface) = self.conversation_surface() else {
            return false;
        };
        if !surface.thread_selector().is_open() {
            return false;
        }

        let keystroke = event.keystroke.unparse();
        match keystroke.as_str() {
            "escape" => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.close_thread_selector();
                    cx.notify();
                }
                true
            }
            _ => match column_selector::keyboard_intent_for_keystroke(&keystroke) {
                Some(ColumnSelectorKeyboardIntent::Activate) => {
                    self.activate_thread_selector_selection(window, cx)
                }
                Some(_) | None => false,
            },
        }
    }

    fn handle_thread_selector_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let should_dismiss = self.conversation_surface().is_some_and(|surface| {
            surface
                .thread_selector()
                .should_dismiss_for_mouse_down(event.position)
        });
        if !should_dismiss {
            return;
        }

        if let Some(surface) = self.conversation_surface_mut() {
            surface.close_thread_selector();
            cx.notify();
        }
    }

    fn handle_composer_image_popup_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.composer_image_popup.is_none() || event.keystroke.key.as_str() != "escape" {
            return false;
        }

        self.close_composer_image_popup(cx);
        true
    }

    fn handle_composer_image_popup_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let should_dismiss = self.composer_image_popup.as_ref().is_some_and(|popup| {
            image_preview_popup::should_dismiss_for_mouse_down(popup.bounds, event.position)
        });
        if should_dismiss {
            self.close_composer_image_popup(cx);
        }
    }

    fn subscribe_settings_window(&mut self, cx: &mut Context<Self>) {
        match self.settings_window.entity(cx) {
            Ok(settings_window) => {
                cx.subscribe(&settings_window, |shell, _, event, cx| {
                    shell.handle_settings_window_event(event, cx);
                })
                .detach();
            }
            Err(error) => {
                warn!(error = %error, "failed to subscribe to Beryl settings window events");
            }
        }
    }

    fn subscribe_conversation_input(&mut self, cx: &mut Context<Self>) {
        let conversation_input = self.conversation_input.clone();
        cx.subscribe(&conversation_input, |shell, _, event, cx| {
            shell.handle_conversation_input_event(event, cx);
        })
        .detach();
    }

    fn subscribe_workspace_picker_filter_input(&mut self, cx: &mut Context<Self>) {
        let filter_input = self.workspace_picker_filter_input.clone();
        cx.subscribe(&filter_input, |shell, _, _event, cx| {
            shell.handle_workspace_picker_filter_input_event(cx);
        })
        .detach();
    }

    fn handle_workspace_picker_filter_input_event(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }

    fn handle_conversation_input_event(&mut self, event: &TextInputEvent, cx: &mut Context<Self>) {
        match event {
            TextInputEvent::InlineAtomClicked { atom_id, position } => {
                if let Some(label) = composer_image_label_from_atom_id(atom_id) {
                    self.open_composer_image_marker_menu(
                        atom_id.clone(),
                        label.to_string(),
                        *position,
                        cx,
                    );
                }
            }
            _ => {}
        }
    }

    fn handle_settings_window_event(
        &mut self,
        event: &SettingsWindowEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SettingsWindowEvent::SectionSelected { section_id } => {
                self.settings_state.select_section(section_id.clone());
                self.sync_settings_window_model(cx);
            }
            SettingsWindowEvent::FieldChanged { field_id, value } => {
                self.settings_state.set_field_value(field_id, value.clone());
                self.sync_settings_window_model(cx);
            }
            SettingsWindowEvent::ColorPickerRequested { .. } => {}
            SettingsWindowEvent::RowActionRequested {
                field_id,
                action_id,
            } => match self.settings_state.handle_row_action(field_id, action_id) {
                Some(settings::SettingsRowActionOutcome::PromptForEndTurnSoundPath) => {
                    self.prompt_notification_end_turn_sound_path(cx);
                }
                Some(settings::SettingsRowActionOutcome::Updated) => {
                    self.sync_settings_window_model(cx);
                }
                None => {}
            },
            SettingsWindowEvent::ApplyRequested => {
                self.apply_settings_window_changes(false, cx);
            }
            SettingsWindowEvent::AcceptRequested => {
                self.apply_settings_window_changes(true, cx);
            }
            SettingsWindowEvent::CancelRequested | SettingsWindowEvent::CloseRequested => {
                self.discard_settings_window_changes(cx);
            }
            _ => {}
        }
    }

    fn open_settings_window(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notification_sound_path_prompt.cancel_active();
        self.settings_state.reset_draft_from_active();
        self.sync_settings_window_options(cx);
        if let Err(error) = self
            .settings_window
            .show(cx, self.settings_state.model(), true)
        {
            warn!(error = %error, "failed to open Beryl settings window");
        }
    }

    fn apply_settings_window_changes(&mut self, hide_after_apply: bool, cx: &mut Context<Self>) {
        if !self.settings_state.apply() {
            self.sync_settings_window_model(cx);
            return;
        }

        cx.refresh_windows();
        self.sync_settings_window_model(cx);
        self.schedule_settings_save_poll(cx);

        if hide_after_apply {
            self.notification_sound_path_prompt.cancel_active();
            self.hide_settings_window(cx);
        }
    }

    fn discard_settings_window_changes(&mut self, cx: &mut Context<Self>) {
        self.notification_sound_path_prompt.cancel_active();
        self.settings_state.reset_draft_from_active();
        self.sync_settings_window_model(cx);
        self.hide_settings_window(cx);
    }

    fn sync_settings_window_model(&self, cx: &mut Context<Self>) {
        if let Err(error) = self
            .settings_window
            .update_model(cx, self.settings_state.model())
        {
            warn!(error = %error, "failed to synchronize Beryl settings window");
        }
        self.sync_settings_window_options(cx);
    }

    fn sync_settings_window_options(&self, cx: &mut Context<Self>) {
        if let Err(error) = self
            .settings_window
            .update_options(cx, self.settings_state.window_options())
        {
            warn!(error = %error, "failed to synchronize Beryl settings window options");
        }
    }

    fn hide_settings_window(&self, cx: &mut Context<Self>) {
        if let Err(error) = self.settings_window.hide(cx) {
            warn!(error = %error, "failed to hide Beryl settings window");
        }
    }

    fn play_end_turn_sound_if_attention_triggered(
        &self,
        _candidate: TurnCompletionSoundCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.enqueue_notification_policy_decision(self.terminal_parent_turn_notification_decision(
            NotificationCandidateKind::OrdinaryEndTurn,
            self.selected_end_turn_sound_path(),
            window,
            cx,
        ));
    }

    fn play_lifecycle_notification_if_attention_triggered(
        &self,
        candidate: LifecycleNotificationCandidate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _thread_id = candidate.thread_id.as_deref();
        let _turn_id = candidate.turn_id.as_deref();
        self.enqueue_notification_policy_decision(self.terminal_parent_turn_notification_decision(
            NotificationCandidateKind::Lifecycle(candidate.kind),
            self.selected_lifecycle_notification_sound_path(candidate.kind),
            window,
            cx,
        ));
    }

    fn terminal_parent_turn_notification_decision(
        &self,
        candidate_kind: NotificationCandidateKind,
        configured_sound_path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> NotificationPolicyDecision {
        terminal_parent_turn_notification_decision(
            candidate_kind,
            configured_sound_path,
            self.beryl_window_focus_state(window, cx),
            self.platform_attention_monitor.snapshot(),
        )
    }

    fn enqueue_notification_policy_decision(&self, decision: NotificationPolicyDecision) {
        if let NotificationPolicyDecision::Play(request) = decision {
            match request {
                NotificationPlaybackRequest::EndTurn { path } => {
                    self.notification_sound_player.enqueue_end_turn_sound(path);
                }
                NotificationPlaybackRequest::Lifecycle { kind, path } => {
                    self.notification_sound_player
                        .enqueue_lifecycle_notification_sound(kind, path);
                }
            }
        }
    }

    fn selected_end_turn_sound_path(&self) -> Option<PathBuf> {
        match self.gui_preferences.lock() {
            Ok(preferences) => preferences.notifications.end_turn_sound_path.clone(),
            Err(error) => {
                warn!(
                    error = %error,
                    "failed to read notification sound preferences"
                );
                None
            }
        }
    }

    fn current_developer_instructions_preference(&self) -> Option<String> {
        match self.gui_preferences.lock() {
            Ok(preferences) => preferences.agent.developer_instructions.clone(),
            Err(error) => {
                warn!(
                    error = %error,
                    "failed to read developer-instructions preferences"
                );
                None
            }
        }
    }

    fn turn_options_with_current_developer_instructions(
        &self,
        selected_thread_id: Option<&str>,
        options: TurnStartOptions,
    ) -> TurnStartOptions {
        let Some(defaults) = self
            .conversation_surface()
            .map(|surface| surface.effective_turn_context_defaults(selected_thread_id))
        else {
            return options;
        };
        self.turn_options_with_current_developer_instructions_defaults(
            selected_thread_id,
            options,
            defaults,
        )
    }

    fn turn_options_with_current_developer_instructions_defaults(
        &self,
        selected_thread_id: Option<&str>,
        options: TurnStartOptions,
        defaults: ThreadTurnDefaults,
    ) -> TurnStartOptions {
        let Some(_model) = defaults.model() else {
            warn!(
                thread_id = selected_thread_id.unwrap_or("<new-thread>"),
                "developer-instructions settings could not be applied or reset because no effective model is known for turn-start collaboration settings"
            );
            return options.without_developer_instructions_context();
        };

        status_line::turn_start_options_with_developer_instructions_context(
            options,
            self.current_developer_instructions_preference(),
            defaults,
        )
    }

    fn selected_lifecycle_notification_sound_path(
        &self,
        _kind: LifecycleNotificationKind,
    ) -> Option<PathBuf> {
        self.selected_end_turn_sound_path()
    }

    fn beryl_window_focus_state(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> BerylWindowFocusState {
        match os_window_focus_state(window) {
            Ok(BerylWindowFocusState::Focused) => return BerylWindowFocusState::Focused,
            Ok(BerylWindowFocusState::Unfocused) => {}
            Ok(BerylWindowFocusState::Unknown) => return BerylWindowFocusState::Unknown,
            Err(error) => {
                warn!(
                    error = %error,
                    "failed to determine Beryl main window focus for notification focus gate"
                );
                return BerylWindowFocusState::Unknown;
            }
        }

        let settings_visible = match self.settings_window.is_visible(cx) {
            Ok(visible) => visible,
            Err(error) => {
                warn!(
                    error = %error,
                    "failed to determine Beryl settings window visibility for notification focus gate"
                );
                return BerylWindowFocusState::Unknown;
            }
        };
        if !settings_visible {
            return BerylWindowFocusState::Unfocused;
        }

        match self
            .settings_window
            .window_handle()
            .update(cx, |_, window, _| os_window_focus_state(window))
        {
            Ok(Ok(BerylWindowFocusState::Focused)) => BerylWindowFocusState::Focused,
            Ok(Ok(BerylWindowFocusState::Unfocused)) => BerylWindowFocusState::Unfocused,
            Ok(Ok(BerylWindowFocusState::Unknown)) => BerylWindowFocusState::Unknown,
            Ok(Err(error)) => {
                warn!(
                    error = %error,
                    "failed to determine Beryl settings window focus for notification focus gate"
                );
                BerylWindowFocusState::Unknown
            }
            Err(error) => {
                warn!(
                    error = %error,
                    "failed to determine Beryl settings window focus for notification focus gate"
                );
                BerylWindowFocusState::Unknown
            }
        }
    }

    fn prompt_notification_end_turn_sound_path(&mut self, cx: &mut Context<Self>) {
        let Some(prompt_token) = self.notification_sound_path_prompt.begin() else {
            return;
        };

        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose end-turn sound WAV".into()),
        });
        cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let outcome = match paths.await {
                    Ok(Ok(Some(paths))) => paths
                        .into_iter()
                        .next()
                        .map(NotificationSoundPathPromptResult::Selected)
                        .unwrap_or(NotificationSoundPathPromptResult::Cancelled),
                    Ok(Ok(None)) => NotificationSoundPathPromptResult::Cancelled,
                    Ok(Err(error)) => NotificationSoundPathPromptResult::Failed(error.to_string()),
                    Err(error) => NotificationSoundPathPromptResult::Failed(error.to_string()),
                };
                let _ = view.update(&mut cx, |view, cx| {
                    view.finish_notification_end_turn_sound_path_prompt(prompt_token, outcome, cx);
                });
            }
        })
        .detach();
    }

    fn finish_notification_end_turn_sound_path_prompt(
        &mut self,
        prompt_token: u64,
        outcome: NotificationSoundPathPromptResult,
        cx: &mut Context<Self>,
    ) {
        if !self.notification_sound_path_prompt.finish(prompt_token) {
            return;
        }

        match outcome {
            NotificationSoundPathPromptResult::Selected(path) => {
                self.settings_state
                    .stage_notification_end_turn_sound_path_from_picker(path);
                self.sync_settings_window_model(cx);
            }
            NotificationSoundPathPromptResult::Cancelled => {}
            NotificationSoundPathPromptResult::Failed(error) => {
                warn!(error = %error, "failed to open Beryl notification sound picker");
                self.settings_state
                    .set_notification_end_turn_sound_picker_error(format!(
                        "Beryl could not open the sound picker: {error}"
                    ));
                self.sync_settings_window_model(cx);
            }
        }

        cx.notify();
    }

    fn schedule_settings_save_poll(&mut self, cx: &mut Context<Self>) {
        if !self.settings_state.has_pending_save() {
            return;
        }

        cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let _ = view.update(&mut cx, |view, cx| view.poll_settings_save(cx));
            }
        })
        .detach();
    }

    fn poll_settings_save(&mut self, cx: &mut Context<Self>) {
        match self.settings_state.poll_save() {
            settings::SettingsSavePoll::Idle | settings::SettingsSavePoll::Saved => {}
            settings::SettingsSavePoll::Pending => self.schedule_settings_save_poll(cx),
            settings::SettingsSavePoll::Failed(error) => {
                warn!(error = %error, "failed to save Beryl settings");
                if self.settings_state.has_pending_save() {
                    self.schedule_settings_save_poll(cx);
                }
            }
        }
    }

    fn begin_workspace_rename(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if let Some(reason) = self.workspace_rename_disabled_reason() {
            if let Some(loaded) = self.loaded_workspace_mut() {
                loaded.set_workspace_picker_notice(reason);
                cx.notify();
            }
            return;
        }

        let Some((workspace_id, title)) = self.loaded_workspace().and_then(|loaded| {
            let workspace_id = loaded
                .workspace_picker
                .row_action_menu_active()?
                .workspace_id()
                .clone();
            loaded
                .known_workspaces
                .iter()
                .find(|workspace| workspace.id() == &workspace_id)
                .map(|workspace| (workspace_id, workspace.title().to_string()))
        }) else {
            return;
        };

        self.workspace_rename_input.update(cx, |input, cx| {
            input.set_text_and_select(title, cx);
        });
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded.clear_workspace_picker_notice();
            loaded.workspace_picker.open_rename_editor_for(workspace_id);
        }
        let focus_handle = self.workspace_rename_input.read(cx).tab_focus_handle();
        window.focus(&focus_handle);
        cx.notify();
    }

    fn cancel_workspace_rename(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded.workspace_picker.close_rename_editor();
            loaded.clear_workspace_picker_notice();
            cx.notify();
        }
    }

    fn submit_workspace_rename(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.begin_submit_workspace_rename(window, cx);
    }

    fn begin_submit_workspace_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(reason) = self.workspace_rename_disabled_reason() {
            if let Some(loaded) = self.loaded_workspace_mut() {
                loaded.set_workspace_picker_notice(reason);
                cx.notify();
            }
            return;
        }

        let title = self.workspace_rename_input.read(cx).text().to_string();
        let Some(workspace_id) = self
            .loaded_workspace()
            .and_then(|loaded| loaded.workspace_picker.rename_editor_target().cloned())
        else {
            return;
        };
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded.clear_workspace_picker_notice();
        }
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            self.block_if_app_state_unavailable(window, cx);
            return;
        };
        self.workspace_title_receiver = Some(spawn_workspace_manual_title_worker(
            persistence,
            workspace_id,
            title,
            self.workspace_persistence_queue.flush(),
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
    }

    fn record_workspace_picker_anchor_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded.workspace_picker.set_anchor_bounds(bounds);
        }
    }

    fn record_workspace_picker_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded.workspace_picker.set_popup_bounds(bounds);
        }
    }

    fn record_workspace_runtime_selector_trigger_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded
                .workspace_picker
                .set_runtime_selector_trigger_bounds(bounds);
        }
    }

    fn record_workspace_runtime_selector_dropdown_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded
                .workspace_picker
                .set_runtime_selector_dropdown_bounds(bounds);
        }
    }

    fn record_workspace_member_action_menu_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded
                .workspace_picker
                .set_member_action_menu_bounds(bounds);
        }
    }

    fn record_thread_selector_anchor_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.thread_selector_mut().set_anchor_bounds(bounds);
        }
    }

    fn record_thread_selector_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.thread_selector_mut().set_popup_bounds(bounds);
        }
    }

    fn record_composer_image_popup_bounds(
        &mut self,
        bounds: Option<Bounds<Pixels>>,
        _: &mut Context<Self>,
    ) {
        if let Some(popup) = self.composer_image_popup.as_mut() {
            popup.bounds = bounds;
        }
    }

    fn handle_graph_overlay_key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let keystroke = event.keystroke.unparse();
        if keystroke != GRAPH_OVERLAY_TOGGLE_KEYSTROKE {
            if column_selector::keyboard_intent_for_keystroke(&keystroke).is_some() {
                return false;
            }
            return false;
        }

        if let Some(surface) = self.conversation_surface_mut()
            && surface.toggle_graph_overlay()
        {
            cx.notify();
            return true;
        }

        false
    }

    fn select_graph_node(
        &mut self,
        column_index: usize,
        node_id: SemanticNodeId,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = self
            .conversation_surface_mut()
            .is_some_and(|surface| surface.select_graph_node(column_index, &node_id));
        if changed {
            self.prune_graph_scrollbar_activity();
            self.notify_checklist_sidebar_panel(cx);
            cx.notify();
        }
    }

    fn select_graph_soft_link(
        &mut self,
        column_index: usize,
        link_id: SoftLinkId,
        target_node_id: SemanticNodeId,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = self.conversation_surface_mut().is_some_and(|surface| {
            surface.select_graph_soft_link(column_index, &link_id, &target_node_id)
        });
        if changed {
            self.prune_graph_scrollbar_activity();
            self.notify_checklist_sidebar_panel(cx);
            cx.notify();
        }
    }

    fn select_thread_selector_member(
        &mut self,
        column_index: usize,
        member_key: MemberThreadInventoryMemberKey,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = self
            .conversation_surface_mut()
            .is_some_and(|surface| surface.select_thread_selector_member(column_index, member_key));
        if changed {
            cx.notify();
        }
    }

    fn select_thread_selector_thread(
        &mut self,
        column_index: usize,
        thread_id: ConversationThreadId,
        event: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let should_activate = event.click_count() >= 2;
        let changed = self
            .conversation_surface_mut()
            .is_some_and(|surface| surface.select_thread_selector_thread(column_index, thread_id));
        if should_activate && self.activate_thread_selector_selection(window, cx) {
            return;
        }
        if changed {
            cx.notify();
        }
    }

    fn activate_thread_selector_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::thread_selector_activation_target)
        else {
            return false;
        };

        self.activate_thread_selector_target(target, window, cx);
        true
    }

    fn activate_thread_selector_target(
        &mut self,
        target: ThreadSelectorActivationTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ThreadActivationStart {
        if self.workspace_receiver.is_some()
            || self.graph_thread_start_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.status_operation_receiver.is_some()
            || self.turn_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
        {
            return ThreadActivationStart::Rejected {
                kind: "busy",
                message: "Beryl is already running workspace, transcript, status, or turn work that blocks thread activation.".to_string(),
            };
        }

        let (beryl_workspace_id, current_execution_target) = match &self.state {
            ShellState::Ready(ready) => (
                ready.loaded_workspace.workspace.id().clone(),
                ready.execution_target.clone(),
            ),
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::Blocked(_) => {
                return ThreadActivationStart::Rejected {
                    kind: "not_ready",
                    message: "Beryl is not on a ready workspace surface.".to_string(),
                };
            }
        };

        let thread_selection = exact_thread_selection_request(&target.thread_id, &target.label);
        let thread_id = target.thread_id.as_str().to_string();
        let label = target.label;
        let execution_target = target.execution_target;

        let connector = self.backend_client_connector_for_execution_target(&execution_target);
        if current_execution_target != execution_target && connector.is_none() {
            self.begin_open_target_with_thread_selection_and_intent(
                RetryTarget::Workspace(execution_target),
                thread_selection,
                WorkspaceOpenIntent::ThreadSelectorActivation,
                window,
                cx,
            );
            return ThreadActivationStart::Started;
        }

        if self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_id)
            == Some(thread_id.as_str())
        {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.clear_notice();
                surface.close_thread_selector();
                cx.notify();
            }
            return ThreadActivationStart::AlreadySelected;
        }

        let Some(connector) = connector else {
            let message =
                "Beryl does not have an active managed backend for this execution target.";
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new("Thread activation failed", message));
                cx.notify();
            }
            return ThreadActivationStart::Rejected {
                kind: "backend_unavailable",
                message: message.to_string(),
            };
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return ThreadActivationStart::Rejected {
                kind: "not_ready",
                message: "Beryl has no workspace persistence handle for thread activation."
                    .to_string(),
            };
        };

        let activation_ui_started = Instant::now();
        if let Some(surface) = self.conversation_surface_mut() {
            surface.begin_thread_activation(label.clone());
            surface.close_thread_selector();
        }
        MemoryMilestone::new("thread_activation_start")
            .workspace_id(beryl_workspace_id.as_str())
            .runtime(execution_target.runtime_mode().display_name())
            .thread_id(thread_id.as_str())
            .log();
        debug!(
            thread_id = thread_id.as_str(),
            pending_visible_ms = elapsed_ms(activation_ui_started.elapsed()),
            "thread activation pending state set from selector"
        );
        self.composer_image_label_scan_receiver = None;
        self.notify_transcript_panel(cx);
        let worker_spawn_started = Instant::now();
        let thread_id_for_log = thread_id.clone();
        self.thread_activation_receiver = Some(spawn_thread_activation_worker(
            persistence,
            connector,
            beryl_workspace_id,
            execution_target,
            thread_id,
            label,
            self.bootstrap.probe_timeout(),
        ));
        debug!(
            thread_id = thread_id_for_log.as_str(),
            worker_spawn_ms = elapsed_ms(worker_spawn_started.elapsed()),
            activation_ui_enqueue_total_ms = elapsed_ms(activation_ui_started.elapsed()),
            "thread activation worker spawned from selector"
        );
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
        ThreadActivationStart::Started
    }

    fn select_graph_thread_ref(
        &mut self,
        thread_ref_id: ThreadRefId,
        thread_id: String,
        execution_target: WorkspaceId,
        label: String,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_receiver.is_some()
            || self.graph_thread_start_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.status_operation_receiver.is_some()
            || self.turn_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
        {
            return;
        }

        let (current_execution_target, availability) = match &self.state {
            ShellState::Ready(ready) => {
                let Some(thread_ref) = ready
                    .surface
                    .graph_overlay()
                    .graph()
                    .thread_ref(&thread_ref_id)
                else {
                    return;
                };
                (
                    ready.execution_target.clone(),
                    graph_thread_ref_availability(
                        &ready.loaded_workspace.workspace_state,
                        thread_ref,
                        ready
                            .loaded_workspace
                            .resolved_implicit_home_execution_target()
                            .as_ref(),
                    ),
                )
            }
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::Blocked(_) => return,
        };

        if let Some(notice_title) = availability.notice_title() {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    notice_title,
                    availability
                        .detail()
                        .unwrap_or("That thread link is unavailable.")
                        .to_string(),
                ));
                cx.notify();
            }
            return;
        }

        let connector = self.backend_client_connector_for_execution_target(&execution_target);
        if current_execution_target != execution_target && connector.is_none() {
            self.begin_open_target_with_thread_selection(
                RetryTarget::Workspace(execution_target),
                ThreadSelectionRequest::exact(thread_id, label),
                window,
                cx,
            );
            return;
        }

        if self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_id)
            == Some(thread_id.as_str())
        {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.clear_notice();
                cx.notify();
            }
            return;
        }

        let Some(connector) = connector else {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Thread activation failed",
                    "Beryl does not have an active managed backend for this execution target.",
                ));
                cx.notify();
            }
            return;
        };

        let activation_ui_started = Instant::now();
        if let Some(surface) = self.conversation_surface_mut() {
            surface.begin_thread_activation(label.clone());
        }
        MemoryMilestone::new("thread_activation_start")
            .runtime(execution_target.runtime_mode().display_name())
            .thread_id(thread_id.as_str())
            .log();
        debug!(
            thread_id = thread_id.as_str(),
            pending_visible_ms = elapsed_ms(activation_ui_started.elapsed()),
            "thread activation pending state set from graph"
        );
        self.composer_image_label_scan_receiver = None;
        self.notify_transcript_panel(cx);
        let Some(beryl_workspace_id) = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return;
        };
        MemoryMilestone::new("thread_activation_workspace_resolved")
            .workspace_id(beryl_workspace_id.as_str())
            .thread_id(thread_id.as_str())
            .log();
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return;
        };
        let worker_spawn_started = Instant::now();
        let thread_id_for_log = thread_id.clone();
        self.thread_activation_receiver = Some(spawn_thread_activation_worker(
            persistence,
            connector,
            beryl_workspace_id,
            execution_target,
            thread_id,
            label,
            self.bootstrap.probe_timeout(),
        ));
        debug!(
            thread_id = thread_id_for_log.as_str(),
            worker_spawn_ms = elapsed_ms(worker_spawn_started.elapsed()),
            activation_ui_enqueue_total_ms = elapsed_ms(activation_ui_started.elapsed()),
            "thread activation worker spawned from graph"
        );
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
    }

    fn toggle_graph_node_expansion(
        &mut self,
        column_index: usize,
        node_id: SemanticNodeId,
        depth: usize,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut()
            && surface.toggle_graph_node_expansion(column_index, &node_id, depth)
        {
            cx.notify();
        }
    }

    fn open_workspace_choice(
        &mut self,
        workspace: WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_open_target_with_intent(
            RetryTarget::Workspace(workspace),
            WorkspaceOpenIntent::UseAsPrimaryMember,
            window,
            cx,
        );
    }

    fn open_host_path(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = self.host_path_input.read(cx).text().trim().to_string();
        if path.is_empty() {
            match &mut self.state {
                ShellState::Picker(picker) => {
                    picker.notice =
                        Some("Enter a host-Windows workspace path before opening it.".to_string());
                }
                ShellState::WorkspaceLoaded(loaded) => {
                    loaded.set_workspace_members_notice(
                        "Enter a host-Windows member path before attaching it.",
                    );
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        if matches!(&self.state, ShellState::WorkspaceLoaded(_)) {
            self.begin_workspace_member_attach_resolution(
                WorkspaceMemberAttachRequest::HostPath {
                    path: PathBuf::from(path),
                },
                window,
                cx,
            );
            return;
        }

        self.begin_open_target_with_intent(
            RetryTarget::HostPath(path),
            WorkspaceOpenIntent::UseAsPrimaryMember,
            window,
            cx,
        );
    }

    fn select_wsl_distro(
        &mut self,
        distro_name: &str,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let ShellState::Picker(picker) = &mut self.state {
            picker.model.selected_wsl_distro = Some(distro_name.to_string());
            picker.notice = None;
            cx.notify();
        }
    }

    fn select_workspace_runtime(
        &mut self,
        runtime: RuntimeMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = {
            let Some(loaded) = self.workspace_shell_state_mut() else {
                return;
            };
            loaded.clear_workspace_members_notice();
            loaded.workspace_state.select_runtime(runtime)
        };

        match result {
            Ok(true) => {
                self.persist_current_workspace_state(true);
                self.reset_member_thread_inventory_for_workspace_state();
                if !self.begin_idle_primary_workspace_open_if_executable(window, cx) {
                    self.begin_implicit_home_path_resolution_if_needed(cx);
                }
            }
            Ok(false) => {}
            Err(error) => {
                self.set_workspace_members_notice(format!(
                    "Beryl could not select that runtime environment: {error}"
                ));
            }
        }

        cx.notify();
    }

    fn open_wsl_path(&mut self, _: &gpui::ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let distro_name = self
            .selected_wsl_distro()
            .filter(|distro| !distro.trim().is_empty())
            .or_else(|| {
                let value = self.wsl_distro_input.read(cx).text().trim().to_string();
                (!value.is_empty()).then_some(value)
            });
        let Some(distro_name) = distro_name else {
            match &mut self.state {
                ShellState::Picker(picker) => {
                    picker.notice = Some(
                        "Select or enter a WSL distro before opening a WSL workspace.".to_string(),
                    );
                }
                ShellState::WorkspaceLoaded(loaded) => {
                    loaded.set_workspace_members_notice(
                        "Select or enter a WSL distro before attaching a WSL member.",
                    );
                }
                _ => {}
            }
            cx.notify();
            return;
        };

        let path = self.wsl_path_input.read(cx).text().trim().to_string();
        if path.is_empty() {
            match &mut self.state {
                ShellState::Picker(picker) => {
                    picker.notice =
                        Some("Enter a WSL workspace path before opening it.".to_string());
                }
                ShellState::WorkspaceLoaded(loaded) => {
                    loaded.set_workspace_members_notice(
                        "Enter a WSL member path before attaching it.",
                    );
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        if matches!(&self.state, ShellState::WorkspaceLoaded(_)) {
            self.begin_workspace_member_attach_resolution(
                WorkspaceMemberAttachRequest::WslPath {
                    distro_name,
                    path: PathBuf::from(path),
                },
                window,
                cx,
            );
            return;
        }

        self.begin_open_target_with_intent(
            RetryTarget::WslPath { distro_name, path },
            WorkspaceOpenIntent::UseAsPrimaryMember,
            window,
            cx,
        );
    }

    fn retry_workspace(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.state {
            ShellState::Blocked(blocked) if matches!(blocked.target, RetryTarget::Startup) => {
                self.begin_discovery(window, cx);
            }
            ShellState::Blocked(blocked) => {
                self.begin_open_target_with_intent(
                    blocked.target.clone(),
                    blocked.intent,
                    window,
                    cx,
                );
            }
            ShellState::Ready(ready) => {
                self.begin_open_target(
                    RetryTarget::Workspace(ready.execution_target.clone()),
                    window,
                    cx,
                );
            }
            _ => {}
        }
    }

    fn quit(&mut self, _: &gpui::ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_application_shutdown(window, cx);
    }

    fn attach_workspace_member(
        &mut self,
        execution_target: WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = {
            let Some(loaded) = self.workspace_shell_state_mut() else {
                return;
            };
            loaded.clear_workspace_members_notice();
            apply_workspace_member_attachment(&mut loaded.workspace_state, &execution_target)
        };

        match result {
            Ok(true) => {
                if let Some(loaded) = self.workspace_shell_state_mut() {
                    loaded.clear_implicit_home_path_resolution();
                }
                self.persist_current_workspace_state(true);
                self.reset_member_thread_inventory_for_workspace_state();
                let _ = self.begin_idle_primary_workspace_open_if_executable(window, cx);
            }
            Ok(false) => {
                self.set_workspace_members_notice("That workspace member is already attached.");
            }
            Err(error) => {
                self.set_workspace_members_notice(format!(
                    "Beryl could not attach that workspace member: {error}"
                ));
            }
        }

        cx.notify();
    }

    fn begin_workspace_member_attach_resolution(
        &mut self,
        request: WorkspaceMemberAttachRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace_id) = self
            .workspace_shell_state()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return;
        };

        if self.workspace_member_attach_pending_workspace_id.is_some() {
            self.set_workspace_members_notice(
                "Beryl is still resolving the previous workspace member path.",
            );
            cx.notify();
            return;
        }

        self.workspace_member_attach_pending_workspace_id = Some(workspace_id.clone());
        self.set_workspace_members_notice("Resolving workspace member path...");

        let window_handle = window.window_handle();
        cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        resolve_workspace_member_attach_request(request)
                            .map_err(|error| error.to_string())
                    })
                    .await;
                let _ = cx.update_window(window_handle, |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.finish_workspace_member_attach_resolution(
                            workspace_id,
                            result,
                            window,
                            cx,
                        );
                    });
                });
            }
        })
        .detach();

        cx.notify();
    }

    fn finish_workspace_member_attach_resolution(
        &mut self,
        workspace_id: BerylWorkspaceId,
        result: Result<WorkspaceId, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .workspace_member_attach_pending_workspace_id
            .as_ref()
            .is_some_and(|pending| pending == &workspace_id)
        {
            self.workspace_member_attach_pending_workspace_id = None;
        }

        let current_workspace_matches = self
            .workspace_shell_state()
            .is_some_and(|loaded| loaded.workspace.id() == &workspace_id);
        if !current_workspace_matches {
            return;
        }

        match result {
            Ok(execution_target) => self.attach_workspace_member(execution_target, window, cx),
            Err(error) => {
                self.set_workspace_members_notice(format!(
                    "Beryl could not attach that workspace member: {error}"
                ));
                cx.notify();
            }
        }
    }

    fn prompt_attach_workspace_member(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(runtime) = self
            .workspace_shell_state()
            .and_then(|loaded| loaded.selected_runtime().cloned())
        else {
            self.set_workspace_members_notice(
                "Select a runtime environment before attaching a workspace member.",
            );
            cx.notify();
            return;
        };

        if self
            .workspace_shell_state()
            .is_some_and(|loaded| loaded.workspace_members.path_prompt_active())
        {
            return;
        }

        if let Some(loaded) = self.workspace_shell_state_mut() {
            loaded.clear_workspace_members_notice();
            loaded.workspace_members.set_path_prompt_active(true);
        }

        let prompt = match &runtime {
            RuntimeMode::HostWindows => "Attach host-Windows member directory",
            RuntimeMode::WslLinux { .. } => "Attach WSL member directory",
        };
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(prompt.into()),
        });
        let window_handle = window.window_handle();
        cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                let outcome = match paths.await {
                    Ok(Ok(Some(paths))) => paths
                        .into_iter()
                        .next()
                        .map(WorkspaceMemberPathPromptResult::Selected)
                        .unwrap_or(WorkspaceMemberPathPromptResult::Cancelled),
                    Ok(Ok(None)) => WorkspaceMemberPathPromptResult::Cancelled,
                    Ok(Err(error)) => WorkspaceMemberPathPromptResult::Failed(error.to_string()),
                    Err(error) => WorkspaceMemberPathPromptResult::Failed(error.to_string()),
                };
                let _ = cx.update_window(window_handle, |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.finish_workspace_member_path_prompt(runtime, outcome, window, cx);
                    });
                });
            }
        })
        .detach();
        cx.notify();
    }

    fn finish_workspace_member_path_prompt(
        &mut self,
        runtime: RuntimeMode,
        outcome: WorkspaceMemberPathPromptResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.workspace_shell_state_mut() {
            loaded.workspace_members.set_path_prompt_active(false);
        }

        let picked_path = match outcome {
            WorkspaceMemberPathPromptResult::Selected(path) => path,
            WorkspaceMemberPathPromptResult::Cancelled => {
                cx.notify();
                return;
            }
            WorkspaceMemberPathPromptResult::Failed(error) => {
                self.set_workspace_members_notice(format!(
                    "Beryl could not open the workspace-member picker: {error}"
                ));
                cx.notify();
                return;
            }
        };

        self.begin_workspace_member_attach_resolution(
            WorkspaceMemberAttachRequest::PickerPath {
                runtime,
                picked_path,
            },
            window,
            cx,
        );
    }

    fn make_workspace_member_primary(
        &mut self,
        member_id: WorkspaceMemberId,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = {
            let Some(loaded) = self.workspace_shell_state_mut() else {
                return;
            };
            loaded.clear_workspace_members_notice();
            loaded.workspace_picker.close_member_action_menu();
            apply_workspace_member_primary_selection(&mut loaded.workspace_state, &member_id)
        };

        match result {
            Ok(true) => {
                self.persist_current_workspace_state(true);
            }
            Ok(false) => {}
            Err(error) => {
                self.set_workspace_members_notice(format!(
                    "Beryl could not make that member primary: {error}"
                ));
            }
        }

        cx.notify();
    }

    fn prompt_detach_workspace_member(
        &mut self,
        member_id: WorkspaceMemberId,
        member_path: String,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(loaded) = self.workspace_shell_state_mut() {
            loaded.workspace_picker.close_member_action_menu();
            cx.notify();
        }

        let answer = window.prompt(
            PromptLevel::Warning,
            "Detach workspace member?",
            Some(&format!(
                "Detach {member_path} from this Beryl workspace. Codex threads that used this member remain backend-owned, but Beryl will require explicit rebinding before continuing them in another member."
            )),
            &[
                PromptButton::cancel("Cancel"),
                PromptButton::ok("Detach"),
            ],
            cx,
        );
        cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                if answer.await.unwrap_or(0) != 1 {
                    return;
                }
                let _ = view.update(&mut cx, |view, cx| {
                    view.detach_workspace_member(member_id, cx);
                });
            }
        })
        .detach();
    }

    fn detach_workspace_member(&mut self, member_id: WorkspaceMemberId, cx: &mut Context<Self>) {
        let (result, restored) = {
            let Some(loaded) = self.workspace_shell_state_mut() else {
                return;
            };
            loaded.clear_workspace_members_notice();
            loaded.workspace_picker.close_member_action_menu();
            let result = apply_workspace_member_detach(&mut loaded.workspace_state, &member_id);
            let restored = if matches!(result, Ok(true)) {
                loaded.restore_resolved_implicit_home_threads()
            } else {
                false
            };
            (result, restored)
        };

        match result {
            Ok(true) => {
                self.persist_current_workspace_state(true);
                self.reset_member_thread_inventory_for_workspace_state();
                self.begin_implicit_home_path_resolution_if_needed(cx);
            }
            Ok(false) => {
                if restored {
                    self.persist_current_workspace_state(true);
                }
            }
            Err(error) => {
                self.set_workspace_members_notice(format!(
                    "Beryl could not detach that workspace member: {error}"
                ));
            }
        }

        cx.notify();
    }

    fn toggle_graph_overlay(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut()
            && surface.toggle_graph_overlay()
        {
            cx.notify();
        }
    }

    fn cycle_tool_activity_panel_mode(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.cycle_tool_activity_panel_mode();
            self.persist_current_workspace_ui_state();
            cx.notify();
        }
    }

    fn toggle_thread_selector(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = self.conversation_surface_mut().is_some_and(|surface| {
            surface.toggle_thread_selector();
            true
        });
        if changed {
            self.schedule_poll_if_needed(window, cx);
            cx.notify();
        }
    }

    fn dismiss_surface_notice(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dismissed = self.conversation_surface_mut().is_some_and(|surface| {
            let had_notice = surface.notice().is_some();
            surface.clear_notice();
            had_notice
        });
        if dismissed {
            self.surface_notice_text_input_notice_id.set(None);
            cx.notify();
        }
    }

    fn toggle_checklist_sidebar(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.toggle_checklist_sidebar();
            self.notify_checklist_sidebar_panel(cx);
            cx.notify();
        }
    }

    fn start_new_thread(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.graph_thread_start_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
            || self.turn_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
            || self.status_operation_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.composer_image_asset_receiver.is_some()
        {
            return;
        }

        let mut updated = false;
        let cleared_active_thread = self
            .workspace_shell_state_mut()
            .is_some_and(|loaded| loaded.workspace_state.clear_active_thread());
        if let Some(surface) = self.conversation_surface_mut() {
            surface.start_new_thread();
            updated = true;
        }
        self.composer_image_label_scan_receiver = None;
        if cleared_active_thread {
            self.persist_current_workspace_state(false);
        }

        if updated {
            self.notify_transcript_panel(cx);
            cx.notify();
        }
    }

    fn queue_turn_from_composer_action(
        &mut self,
        _: &SubmitComposer,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.conversation_input.read(cx).has_marked_text() {
            return;
        }

        self.queue_turn_from_composer(cx);
    }

    fn queue_turn_from_composer_text_enter_action(
        &mut self,
        _: &SharedTextInputEnter,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.conversation_input.read(cx).has_marked_text() {
            return;
        }

        self.queue_turn_from_composer(cx);
    }

    fn copy_composer_selection_action(
        &mut self,
        _: &SharedTextInputCopy,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.conversation_input.read(cx).selection_export() else {
            cx.propagate();
            return;
        };
        if !selection.has_atoms() {
            cx.propagate();
            return;
        }

        self.sync_composer_draft_from_input(cx);
        self.write_composer_selection_to_clipboard(&selection, cx);
    }

    fn cut_composer_selection_action(
        &mut self,
        _: &SharedTextInputCut,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.conversation_input.read(cx).selection_export() else {
            cx.propagate();
            return;
        };
        if !selection.has_atoms() {
            cx.propagate();
            return;
        }

        self.sync_composer_draft_from_input(cx);
        self.write_composer_selection_to_clipboard(&selection, cx);
        self.conversation_input.update(cx, |input, cx| {
            input.cut_selection_export(cx);
        });
        self.sync_composer_draft_from_input(cx);
        cx.notify();
    }

    fn write_composer_selection_to_clipboard(
        &mut self,
        selection: &TextInputSelectionExport,
        cx: &mut Context<Self>,
    ) {
        match self.composer_clipboard_payload_from_selection(selection) {
            Ok(payload) => {
                let item = self.composer_clipboard.store_payload(payload);
                cx.write_to_clipboard(item);
            }
            Err(error) => {
                warn!(
                    ?error,
                    "falling back to plain text for composer image-marker clipboard selection"
                );
                cx.write_to_clipboard(ClipboardItem::new_string(selection.copy_text().to_string()));
            }
        }
    }

    fn composer_clipboard_payload_from_selection(
        &self,
        selection: &TextInputSelectionExport,
    ) -> Result<ComposerClipboardPayload, ComposerClipboardPayloadError> {
        let scope = self
            .conversation_surface()
            .map(ConversationSurfaceState::composer_clipboard_label_scope)
            .unwrap_or(ComposerClipboardLabelScope::PendingNewThread(0));
        let mut atoms = Vec::new();
        let mut images = Vec::new();

        for atom in selection.atoms() {
            let label = composer_image_label_from_atom_id(atom.id())
                .ok_or(ComposerClipboardPayloadError::InvalidAtomId)?;
            atoms.push(ComposerClipboardAtom::new(
                label.to_string(),
                atom.range(),
                atom.display_text().to_string(),
                atom.copy_text().to_string(),
            ));

            if images
                .iter()
                .any(|image: &ComposerClipboardImage| image.label() == label)
            {
                continue;
            }

            let data = self
                .composer_draft
                .image_data_for_label(label)
                .ok_or(ComposerClipboardPayloadError::MissingImageData)?
                .clone();
            images.push(ComposerClipboardImage::new(label.to_string(), data));
        }

        ComposerClipboardPayload::new(
            selection.display_text().to_string(),
            selection.copy_text().to_string(),
            scope,
            atoms,
            images,
        )
    }

    fn paste_composer_clipboard_image_action(
        &mut self,
        _: &SharedTextInputPaste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.conversation_input.read(cx).has_marked_text() {
            return;
        }

        let Some(item) = cx.read_from_clipboard() else {
            cx.propagate();
            return;
        };

        if let Some(payload) = self.composer_clipboard.resolve_payload(&item) {
            self.paste_resolved_composer_clipboard_payload(payload, window, cx);
            return;
        }

        let Some(image) = first_clipboard_image(&item) else {
            self.paste_plain_composer_clipboard_text(item, cx);
            return;
        };

        if !self.ensure_composer_image_paste_readiness(window, cx) {
            return;
        }

        self.begin_composer_image_asset_paste(
            ComposerDraftImageData::from_gpui_image(&image),
            window,
            cx,
        );
    }

    fn paste_plain_composer_clipboard_text(&mut self, item: ClipboardItem, cx: &mut Context<Self>) {
        let Some(text) = item.text() else {
            cx.propagate();
            return;
        };

        let changed = self
            .conversation_input
            .update(cx, |input, cx| input.replace_selected_text(&text, cx));
        self.sync_composer_draft_from_input(cx);
        if changed {
            cx.notify();
        }
    }

    fn begin_composer_image_asset_paste(
        &mut self,
        data: ComposerDraftImageData,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_composer_draft_from_input(cx);
        if let Err(error) = self.composer_draft.validate_new_image_admission(&data) {
            self.report_composer_image_admission_error(error, cx);
            return;
        }

        if self.composer_image_asset_receiver.is_some() {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Image input busy",
                    "Beryl is still storing the previous pasted image. Try again when that finishes.",
                ));
            }
            cx.notify();
            return;
        }

        let Some(workspace_id) = self
            .workspace_shell_state()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return;
        };
        let (display_text_snapshot, replacement_range) = {
            let input = self.conversation_input.read(cx);
            (input.text().to_string(), input.selection())
        };

        self.pending_composer_image_asset_paste = Some(PendingComposerImageAssetPaste {
            workspace_id: workspace_id.clone(),
            display_text_snapshot,
            replacement_range,
        });
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            self.pending_composer_image_asset_paste = None;
            return;
        };
        self.composer_image_asset_receiver = Some(spawn_composer_image_asset_worker(
            persistence,
            workspace_id,
            data,
        ));
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Storing image input",
                "Beryl is storing the pasted image in this workspace before inserting its marker.",
            ));
        }
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
    }

    fn finish_composer_image_asset_paste(
        &mut self,
        result: Result<ComposerDraftImageData, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.pending_composer_image_asset_paste.take() else {
            return;
        };

        let data = match result {
            Ok(data) => data,
            Err(message) => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new("Image input rejected", message));
                }
                return;
            }
        };
        let asset_id = data.asset_id().map(str::to_string);

        if self
            .workspace_shell_state()
            .is_none_or(|loaded| loaded.workspace.id() != &pending.workspace_id)
        {
            self.mark_image_asset_unreferenced(&pending.workspace_id, asset_id.as_deref());
            return;
        }

        if self.conversation_input.read(cx).text() != pending.display_text_snapshot {
            self.mark_image_asset_unreferenced(&pending.workspace_id, asset_id.as_deref());
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Image input not inserted",
                    "The composer changed while Beryl stored the pasted image. Paste the image again to insert it into the current draft.",
                ));
            }
            return;
        }

        self.sync_composer_draft_from_input(cx);
        let Some(label) = self
            .conversation_surface_mut()
            .map(ConversationSurfaceState::allocate_composer_image_label)
        else {
            self.mark_image_asset_unreferenced(&pending.workspace_id, asset_id.as_deref());
            return;
        };
        let insertion = match self.composer_draft.stage_image(label, data) {
            Ok(insertion) => insertion,
            Err(error) => {
                self.mark_image_asset_unreferenced(&pending.workspace_id, asset_id.as_deref());
                self.report_composer_image_admission_error(error, cx);
                return;
            }
        };
        let changed = match self.conversation_input.update(cx, |input, cx| {
            input.replace_text_range_with_atom(
                pending.replacement_range.clone(),
                insertion.marker(),
                insertion.atom_id(),
                insertion.copy_text(),
                cx,
            )
        }) {
            Ok(changed) => changed,
            Err(error) => {
                self.composer_draft.remove_image_by_label(insertion.label());
                self.mark_image_asset_unreferenced(&pending.workspace_id, asset_id.as_deref());
                warn!(
                    ?error,
                    label = insertion.label(),
                    "failed to insert durable composer image atom"
                );
                return;
            }
        };
        self.sync_composer_draft_from_input(cx);
        if changed {
            cx.notify();
        }
    }

    fn paste_resolved_composer_clipboard_payload(
        &mut self,
        payload: ComposerClipboardPayload,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same_scope = self.conversation_surface().is_some_and(|surface| {
            surface.is_composer_clipboard_label_scope_current(payload.source_label_scope())
        });
        if !same_scope && !self.ensure_composer_image_paste_readiness(window, cx) {
            return;
        }

        self.sync_composer_draft_from_input(cx);
        let Some(label_mapping) = self.composer_clipboard_paste_label_mapping(&payload, same_scope)
        else {
            return;
        };

        let plan = {
            let draft = &mut self.composer_draft;
            ComposerClipboardPastePlan::new(&payload, &label_mapping, |label| {
                draft.allocate_image_reference(label).atom_id().to_string()
            })
        };
        let plan = match plan {
            Ok(plan) => plan,
            Err(error) => {
                warn!(
                    ?error,
                    "failed to build composer clipboard image-marker paste plan"
                );
                return;
            }
        };

        let display_text = plan.display_text().to_string();
        let atoms = plan.atoms().to_vec();
        let images = plan.images().to_vec();
        let mut admission_probe = self.composer_draft.clone();
        for image in &images {
            if let Err(error) = admission_probe
                .ensure_image_payload(image.label().to_string(), image.data().clone())
            {
                self.report_composer_image_admission_error(error, cx);
                return;
            }
        }
        let changed = match self.conversation_input.update(cx, |input, cx| {
            input.replace_selected_text_with_atoms(&display_text, atoms, cx)
        }) {
            Ok(changed) => changed,
            Err(error) => {
                warn!(?error, "failed to insert composer clipboard image atoms");
                return;
            }
        };

        for image in images {
            let _ = self
                .composer_draft
                .ensure_image_payload(image.label().to_string(), image.data().clone());
            self.mark_current_workspace_image_asset_referenced(image.data().asset_id());
        }
        self.sync_composer_draft_from_input(cx);
        if changed {
            cx.notify();
        }
    }

    fn composer_clipboard_paste_label_mapping(
        &mut self,
        payload: &ComposerClipboardPayload,
        same_scope: bool,
    ) -> Option<HashMap<String, String>> {
        let mut mapping = HashMap::new();
        if same_scope {
            for image in payload.images() {
                mapping.insert(image.label().to_string(), image.label().to_string());
            }
            return Some(mapping);
        }

        let surface = self.conversation_surface_mut()?;
        for image in payload.images() {
            mapping.insert(
                image.label().to_string(),
                surface.allocate_composer_image_label(),
            );
        }
        Some(mapping)
    }

    fn ensure_composer_image_paste_readiness(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let paste_readiness = self
            .conversation_surface()
            .map(ConversationSurfaceState::composer_image_paste_readiness)
            .unwrap_or(ComposerImagePasteReadiness::Ready);
        match paste_readiness {
            ComposerImagePasteReadiness::Ready => true,
            ComposerImagePasteReadiness::Scanning => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new(
                        "Image input unavailable",
                        "Beryl is still scanning this thread's earlier image labels. Try again when scanning finishes.",
                    ));
                }
                self.begin_composer_image_label_scan_if_needed(window, cx);
                cx.notify();
                false
            }
            ComposerImagePasteReadiness::Failed { message } => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new(
                        "Image input unavailable",
                        format!(
                            "Beryl could not scan this thread's earlier image labels: {message}"
                        ),
                    ));
                }
                cx.notify();
                false
            }
        }
    }

    fn report_composer_image_admission_error(
        &mut self,
        error: ComposerDraftImageAdmissionError,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Image input rejected",
                error.user_message(),
            ));
        }
        cx.notify();
    }

    fn sync_composer_draft_from_input(&mut self, cx: &mut Context<Self>) {
        let previous_asset_ids = self.composer_draft.active_image_asset_ids();
        let (display_text, image_atoms) = {
            let input = self.conversation_input.read(cx);
            (
                input.text().to_string(),
                input
                    .atoms()
                    .iter()
                    .filter_map(|atom| {
                        let label = composer_image_label_from_atom_id(atom.id())?;
                        Some(ComposerDraftImageAtom::new_with_atom_id(
                            atom.id().to_string(),
                            label.to_string(),
                            atom.range(),
                        ))
                    })
                    .collect::<Vec<_>>(),
            )
        };
        self.composer_draft
            .sync_from_input(display_text, image_atoms);
        let current_asset_ids = self.composer_draft.active_image_asset_ids();
        self.mark_removed_draft_image_assets_unreferenced(previous_asset_ids, current_asset_ids);
    }

    fn clear_composer_draft(&mut self, cx: &mut Context<Self>) {
        self.composer_draft.clear();
        self.take_composer_image_popup(cx);
        self.conversation_input.update(cx, |input, cx| {
            let changed = input.set_text("", cx);
            input.clear_edit_history();
            changed
        });
    }

    fn mark_current_workspace_image_asset_referenced(&self, asset_id: Option<&str>) {
        let Some(workspace_id) = self
            .workspace_shell_state()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return;
        };
        self.mark_image_asset_referenced(&workspace_id, asset_id);
    }

    fn mark_image_asset_referenced(&self, workspace_id: &BerylWorkspaceId, asset_id: Option<&str>) {
        let Some(asset_id) = asset_id else {
            return;
        };
        self.workspace_persistence_queue
            .mark_image_assets_referenced(workspace_id.clone(), vec![asset_id.to_string()]);
    }

    fn mark_image_asset_unreferenced(
        &self,
        workspace_id: &BerylWorkspaceId,
        asset_id: Option<&str>,
    ) {
        let Some(asset_id) = asset_id else {
            return;
        };
        self.workspace_persistence_queue
            .mark_image_assets_unreferenced(workspace_id.clone(), vec![asset_id.to_string()]);
    }

    fn mark_removed_draft_image_assets_unreferenced(
        &self,
        previous_asset_ids: Vec<String>,
        current_asset_ids: Vec<String>,
    ) {
        let Some(workspace_id) = self
            .workspace_shell_state()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return;
        };
        let current_asset_ids = current_asset_ids.into_iter().collect::<HashSet<_>>();
        let removed = previous_asset_ids
            .into_iter()
            .filter(|asset_id| !current_asset_ids.contains(asset_id))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        self.workspace_persistence_queue
            .mark_image_assets_unreferenced(workspace_id, removed);
    }

    fn mark_accepted_composer_image_assets_retained(&self, draft: &AcceptedComposerDraft) {
        let Some(workspace_id) = self
            .workspace_shell_state()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return;
        };
        self.workspace_persistence_queue
            .mark_image_assets_retained(workspace_id, draft.image_asset_ids());
    }

    fn open_composer_image_marker_menu(
        &mut self,
        atom_id: String,
        label: String,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.sync_composer_draft_from_input(cx);
        if !self.composer_draft.has_active_image_atom(&atom_id) {
            self.close_composer_image_popup(cx);
            return;
        }

        self.take_composer_image_popup(cx);
        self.composer_image_popup = Some(ComposerImagePopupState {
            atom_id,
            label,
            position,
            bounds: None,
            mode: ComposerImagePopupMode::Menu,
            preview_image: None,
            preview_image_bytes: 0,
        });
        cx.notify();
    }

    fn close_composer_image_popup(&mut self, cx: &mut Context<Self>) {
        if self.take_composer_image_popup(cx).is_some() {
            cx.notify();
        }
    }

    fn take_composer_image_popup(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<ComposerImagePopupState> {
        let mut popup = self.composer_image_popup.take();
        if let Some(image) = popup.as_mut().and_then(|popup| popup.preview_image.take()) {
            image.remove_asset(cx);
        }
        popup
    }

    fn view_composer_image_from_popup(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preview_image = self.composer_image_popup.as_ref().and_then(|popup| {
            if popup.preview_image.is_some() {
                return None;
            }
            self.composer_draft
                .image_data_for_label(&popup.label)
                .filter(|data| !data.bytes().is_empty())
                .map(|data| {
                    let bytes = data.bytes().to_vec();
                    let byte_count = bytes.len();
                    (
                        Arc::new(Image::from_bytes(data.format(), bytes)),
                        byte_count,
                    )
                })
        });
        if let Some(popup) = self.composer_image_popup.as_mut() {
            if popup.preview_image.is_none() {
                if let Some((image, byte_count)) = preview_image {
                    popup.preview_image = Some(image);
                    popup.preview_image_bytes = byte_count;
                }
            }
            popup.mode = ComposerImagePopupMode::Preview;
            popup.bounds = None;
            cx.notify();
        }
    }

    fn remove_composer_image_from_popup(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(label) = self
            .composer_image_popup
            .as_ref()
            .map(|popup| popup.label.clone())
        else {
            return;
        };
        let atom_id = self
            .composer_image_popup
            .as_ref()
            .map(|popup| popup.atom_id.clone())
            .expect("popup label and atom id should be present together");

        self.sync_composer_draft_from_input(cx);
        self.conversation_input.update(cx, |input, cx| {
            input.remove_atom_by_id(&atom_id, cx);
        });
        self.sync_composer_draft_from_input(cx);
        if !self.composer_draft.has_active_image_marker(&label) {
            self.composer_draft.remove_image_by_label(&label);
        }
        self.take_composer_image_popup(cx);
        cx.notify();
    }

    fn begin_composer_image_delivery(&mut self, draft: AcceptedComposerDraft) -> bool {
        let runtime_mode = match self.composer_image_delivery_runtime_mode() {
            Ok(Some(runtime_mode)) => runtime_mode,
            Ok(None) => return false,
            Err(message) => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new("Image input rejected", message));
                    return true;
                }
                return false;
            }
        };

        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Preparing image input",
                "Beryl is preparing pasted image data for the selected backend runtime.",
            ));
        }
        let Some(workspace_id) = self
            .workspace_shell_state()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return false;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Image input rejected",
                    "Beryl could not open the configured workspace image storage.",
                ));
            }
            return true;
        };
        self.composer_image_delivery_receiver = Some(spawn_composer_image_delivery_worker(
            persistence,
            workspace_id,
            draft,
            runtime_mode,
        ));
        true
    }

    fn composer_image_delivery_runtime_mode(&self) -> Result<Option<RuntimeMode>, String> {
        let ShellState::Ready(ready) = &self.state else {
            return Ok(None);
        };
        if ready.surface.selected_thread_id().is_some() {
            return Ok(Some(ready.execution_target.runtime_mode().clone()));
        }

        let execution_target = resolve_new_thread_execution_target(
            &ready.loaded_workspace.workspace_state,
            &ready.execution_target,
        )
        .map_err(|error| error.to_string())?;
        Ok(Some(execution_target.runtime_mode().clone()))
    }

    fn finish_composer_image_delivery(
        &mut self,
        result: Result<PreparedComposerDraft, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(staged) => {
                self.sync_composer_draft_from_input(cx);
                if self.composer_draft.accepted().as_ref() != Some(staged.draft()) {
                    if let Some(surface) = self.conversation_surface_mut() {
                        surface.set_notice(SurfaceNotice::new(
                            "Image input not sent",
                            "The composer changed while Beryl was preparing pasted image data. Submit again to send the current draft.",
                        ));
                    }
                    return;
                }

                let fragment = match prepared_composer_draft_fragment(&staged) {
                    Ok(fragment) => fragment,
                    Err(message) => {
                        if let Some(surface) = self.conversation_surface_mut() {
                            surface.set_notice(SurfaceNotice::new("Image input rejected", message));
                        }
                        return;
                    }
                };
                self.mark_accepted_composer_image_assets_retained(staged.draft());
                let accepted_draft = staged.draft().with_durable_image_references();

                if self.queue_accepted_composer_fragment(fragment, cx) {
                    self.record_accepted_composer_history(&accepted_draft);
                    cx.notify();
                }
            }
            Err(message) => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new("Image input rejected", message));
                }
            }
        }
    }

    fn insert_transcript_quote_into_draft(
        &mut self,
        selected_text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let (draft, cursor_offset) = {
            let input = self.conversation_input.read(cx);
            (input.text().to_string(), input.cursor_offset())
        };
        let Some(insertion) =
            transcript_quote::quote_insertion_for_draft(&draft, cursor_offset, selected_text)
        else {
            return false;
        };

        let inserted = self.conversation_input.update(cx, |input, cx| {
            input.insert_text_at_offset(insertion.insertion_offset, &insertion.inserted_text, cx)
        });
        if inserted {
            cx.notify();
        }
        inserted
    }

    fn browse_composer_history_previous_action(
        &mut self,
        _: &BrowseComposerHistoryPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.browse_composer_history(ComposerHistoryDirection::Previous, window, cx);
    }

    fn browse_composer_history_next_action(
        &mut self,
        _: &BrowseComposerHistoryNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.browse_composer_history(ComposerHistoryDirection::Next, window, cx);
    }

    fn browse_composer_history(
        &mut self,
        direction: ComposerHistoryDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.conversation_input.read(cx).has_marked_text()
            || self
                .conversation_surface()
                .is_some_and(|surface| surface.transcript_edit_mode().is_some())
        {
            return;
        }

        self.sync_composer_draft_from_input(cx);
        let current_draft = self.composer_draft.clone();
        let result = match (direction, self.conversation_surface_mut()) {
            (ComposerHistoryDirection::Previous, Some(surface)) => {
                surface.browse_composer_history_previous(current_draft)
            }
            (ComposerHistoryDirection::Next, Some(surface)) => {
                surface.browse_composer_history_next()
            }
            (_, None) => None,
        };

        if let Some(result) = result
            && self.replace_composer_with_history_result(result, window, cx)
        {
            cx.notify();
        }
    }

    fn replace_composer_with_history_result(
        &mut self,
        result: ComposerHistoryBrowseResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match result {
            ComposerHistoryBrowseResult::Accepted(draft) => {
                let mut next_draft = ComposerDraft::default();
                let atoms = next_draft.replace_with_accepted(&draft);
                self.replace_composer_with_draft(next_draft, atoms, window, cx)
            }
            ComposerHistoryBrowseResult::Draft(draft) => {
                let atoms = draft.image_atoms().to_vec();
                self.replace_composer_with_draft(draft, atoms, window, cx)
            }
        }
    }

    fn replace_composer_with_draft(
        &mut self,
        next_draft: ComposerDraft,
        atoms: Vec<ComposerDraftImageAtom>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let previous_asset_ids = self.composer_draft.active_image_asset_ids();
        let display_text = next_draft.display_text().to_string();
        let caret = display_text.len();
        if atoms.is_empty() {
            self.conversation_input.update(cx, |input, cx| {
                input.set_text(&display_text, cx);
                input.set_selection(caret..caret, false, cx);
                input.focus(window, cx);
            });
        } else {
            let input_atoms = atoms
                .iter()
                .map(composer_history_text_input_atom)
                .collect::<Vec<_>>();
            match self.conversation_input.update(cx, |input, cx| {
                input.set_text("", cx);
                match input.replace_selected_text_with_atoms(&display_text, input_atoms, cx) {
                    Ok(inserted) => {
                        input.set_selection(caret..caret, false, cx);
                        input.focus(window, cx);
                        Ok(inserted)
                    }
                    Err(error) => Err(error),
                }
            }) {
                Ok(true) => {}
                Ok(false) => return false,
                Err(error) => {
                    warn!(?error, "failed to restore composer history image atoms");
                    return false;
                }
            }
        }
        self.take_composer_image_popup(cx);
        let current_asset_ids = next_draft.active_image_asset_ids();
        self.composer_draft = next_draft;
        self.mark_removed_draft_image_assets_unreferenced(previous_asset_ids, current_asset_ids);
        true
    }

    fn jump_transcript_turn_up_action(
        &mut self,
        _: &JumpTranscriptTurnUp,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_transcript_turn(TranscriptTurnJumpDirection::Up, cx);
    }

    fn jump_transcript_turn_down_action(
        &mut self,
        _: &JumpTranscriptTurnDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_transcript_turn(TranscriptTurnJumpDirection::Down, cx);
    }

    fn jump_transcript_turn(
        &mut self,
        direction: TranscriptTurnJumpDirection,
        cx: &mut Context<Self>,
    ) {
        let did_scroll = self.conversation_surface_mut().is_some_and(|surface| {
            let list_state = surface.transcript_list_state();
            let turn_count = surface.transcript_presentation().len();
            let Some(target) = transcript_turn_jump_target(&list_state, turn_count, direction)
            else {
                return false;
            };

            list_state.scroll_to_position(target);
            surface.release_transcript_submit_anchor();
            surface.set_transcript_user_scrolled(true);
            true
        });

        if did_scroll {
            self.note_scrollbar_activity(ScrollbarRegion::Transcript, cx);
        }
    }

    fn queue_turn_from_composer(&mut self, cx: &mut Context<Self>) -> bool {
        if self.graph_thread_start_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.composer_image_asset_receiver.is_some()
            || self.composer_image_delivery_receiver.is_some()
        {
            return false;
        }

        self.sync_composer_draft_from_input(cx);
        let Some(accepted_draft) = self.composer_draft.accepted() else {
            return false;
        };
        if self.queue_transcript_edit_commit_from_composer(&accepted_draft, cx) {
            return true;
        }
        if accepted_draft.contains_images() {
            if self.status_operation_receiver.is_some()
                && !self.conversation_surface().is_some_and(|surface| {
                    surface.selected_thread_context_compaction_id().is_some()
                })
            {
                return false;
            }
            if self.thread_history_page_receiver.is_some() {
                return false;
            }
            if self.begin_composer_image_delivery(accepted_draft) {
                cx.notify();
                return true;
            }
            return false;
        }

        let Some(draft) = accepted_draft
            .text_only()
            .or_else(|| accepted_composer_draft(self.conversation_input.read(cx).text()))
        else {
            return false;
        };
        let fragment = UserInputFragment::text(draft);

        if self.queue_accepted_composer_fragment(fragment, cx) {
            self.record_accepted_composer_history(&accepted_draft);
            cx.notify();
            return true;
        }
        false
    }

    fn record_accepted_composer_history(&mut self, draft: &AcceptedComposerDraft) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.record_accepted_composer_history(draft);
        }
    }

    fn queue_accepted_composer_fragment(
        &mut self,
        fragment: UserInputFragment,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.status_operation_receiver.is_some()
            && self.queue_context_compaction_turn_from_composer(fragment.clone(), cx)
        {
            self.notify_transcript_panel(cx);
            return true;
        }

        if self.turn_receiver.is_some() {
            if self.queue_active_turn_steering_from_composer(fragment, cx) {
                self.notify_transcript_panel(cx);
                return true;
            }
            return false;
        }

        if self.status_operation_receiver.is_some() {
            return false;
        }

        if self.thread_history_page_receiver.is_some() {
            return false;
        }

        let (
            beryl_workspace_id,
            workspace,
            selected_thread_id,
            automatic_title_generation_allowed,
            turn_options,
        ) = match &self.state {
            ShellState::Ready(ready) => {
                let selected_thread_id = ready.surface.selected_thread_id().map(str::to_string);
                let automatic_title_generation_allowed = selected_thread_id
                    .as_deref()
                    .map(|thread_id| {
                        ready
                            .loaded_workspace
                            .workspace_state
                            .thread_automatic_title_generation_eligible(&ConversationThreadId::new(
                                thread_id.to_string(),
                            ))
                    })
                    .unwrap_or(true);
                let turn_options = ready
                    .surface
                    .pending_turn_start_options(selected_thread_id.as_deref());
                let workspace = if selected_thread_id.is_some() {
                    ready.execution_target.clone()
                } else {
                    match resolve_new_thread_execution_target(
                        &ready.loaded_workspace.workspace_state,
                        &ready.execution_target,
                    ) {
                        Ok(execution_target) => execution_target,
                        Err(error) => {
                            if let Some(surface) = self.conversation_surface_mut() {
                                surface.set_notice(SurfaceNotice::new(
                                    "New thread unavailable",
                                    error.to_string(),
                                ));
                                cx.notify();
                            }
                            return false;
                        }
                    }
                };
                (
                    ready.loaded_workspace.workspace.id().clone(),
                    workspace,
                    selected_thread_id,
                    automatic_title_generation_allowed,
                    turn_options,
                )
            }
            ShellState::WorkspaceIdle(_) => return false,
            ShellState::WorkspaceLoaded(_) => return false,
            ShellState::Blocked(_) => return false,
            ShellState::Discovering(_) | ShellState::Picker(_) | ShellState::Opening(_) => {
                return false;
            }
        };

        let turn_options = self.turn_options_with_current_developer_instructions(
            selected_thread_id.as_deref(),
            turn_options,
        );

        if let ShellState::Ready(ready) = &mut self.state {
            ready.surface.begin_turn(fragment.clone());
        }
        self.clear_composer_draft(cx);

        self.notify_transcript_panel(cx);

        let Some(connector) = self.backend_client_connector_for_execution_target(&workspace) else {
            if let Some(surface) = self.conversation_surface_mut() {
                let _ = surface.finish_turn_failure(
                    "Beryl does not have an active managed backend for the resolved workspace member.",
                );
            }
            self.notify_transcript_panel(cx);
            return true;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            if let Some(surface) = self.conversation_surface_mut() {
                let _ = surface.finish_turn_failure(
                    "Beryl could not open the configured workspace persistence root.",
                );
            }
            self.notify_transcript_panel(cx);
            return true;
        };

        let (shell_tool_sender, shell_tool_receiver) = shell_dynamic_tool_request_channel();
        self.shell_tool_receiver = Some(shell_tool_receiver);
        self.turn_receiver = Some(spawn_turn_worker(
            persistence,
            connector,
            beryl_workspace_id,
            workspace,
            selected_thread_id,
            automatic_title_generation_allowed,
            vec![fragment],
            turn_options,
            Some(shell_tool_sender),
            self.bootstrap.probe_timeout(),
        ));
        true
    }

    fn queue_active_turn_steering_from_composer(
        &mut self,
        fragment: UserInputFragment,
        cx: &mut Context<Self>,
    ) -> bool {
        let steering_request = match &mut self.state {
            ShellState::Ready(ready) => {
                let Some(target) = ready.surface.selected_active_turn_steering_target() else {
                    return false;
                };
                let pending_steering_fragment =
                    SteeringInputFragment::from_user_input_fragment(target.turn_index, &fragment);
                if target.turn_id.is_none() {
                    match ready.surface.pending_active_turn_steering_admission(
                        &target.thread_id,
                        target.turn_index,
                        &pending_steering_fragment,
                    ) {
                        Ok(true) => {}
                        Ok(false) => return false,
                        Err(error) => {
                            ready.surface.report_pending_input_admission_error(error);
                            return false;
                        }
                    }
                }
                let Some(steering_fragment) = ready
                    .surface
                    .append_active_turn_steering_fragment(&target, fragment)
                else {
                    return false;
                };
                match target.turn_id {
                    Some(turn_id) => Some((target.thread_id, turn_id, vec![steering_fragment])),
                    None => {
                        if !ready.surface.queue_pending_active_turn_steering_fragment(
                            target.thread_id,
                            target.turn_index,
                            steering_fragment,
                        ) {
                            return false;
                        }
                        None
                    }
                }
            }
            ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::Blocked(_)
            | ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_) => return false,
        };

        self.clear_composer_draft(cx);

        if let Some((thread_id, expected_turn_id, fragments)) = steering_request {
            self.begin_turn_steering(thread_id, expected_turn_id, fragments);
        }

        true
    }

    fn queue_context_compaction_turn_from_composer(
        &mut self,
        fragment: UserInputFragment,
        cx: &mut Context<Self>,
    ) -> bool {
        let (thread_id, execution_target, automatic_title_generation_allowed, turn_options) =
            match &self.state {
                ShellState::Ready(ready) => {
                    let Some(thread_id) = ready
                        .surface
                        .selected_thread_context_compaction_id()
                        .map(str::to_string)
                    else {
                        return false;
                    };
                    let automatic_title_generation_allowed = ready
                        .loaded_workspace
                        .workspace_state
                        .thread_automatic_title_generation_eligible(&ConversationThreadId::new(
                            thread_id.clone(),
                        ));
                    let turn_options = ready
                        .surface
                        .pending_turn_start_options(Some(thread_id.as_str()));
                    (
                        thread_id,
                        ready.execution_target.clone(),
                        automatic_title_generation_allowed,
                        turn_options,
                    )
                }
                ShellState::WorkspaceIdle(_)
                | ShellState::WorkspaceLoaded(_)
                | ShellState::Blocked(_)
                | ShellState::Discovering(_)
                | ShellState::Picker(_)
                | ShellState::Opening(_) => return false,
            };

        let queued = match &mut self.state {
            ShellState::Ready(ready) => ready.surface.queue_pending_turn_fragment(
                thread_id,
                execution_target,
                automatic_title_generation_allowed,
                turn_options,
                fragment,
            ),
            ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::Blocked(_)
            | ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_) => false,
        };

        if queued {
            self.clear_composer_draft(cx);
        }
        queued
    }

    fn begin_lifecycle_phase_continue(
        &mut self,
        request: PhaseContinueRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.status_operation_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
        {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Lifecycle continuation unavailable",
                    "Beryl could not compact and continue because another background thread operation is already running.",
                ));
            }
            return false;
        }

        let Some(connector) = self.backend_client_connector() else {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Lifecycle continuation unavailable",
                    "Beryl does not have an active managed backend for this workspace.",
                ));
            }
            return false;
        };

        let thread_id = request.thread_id().to_string();
        if !self
            .conversation_surface()
            .is_some_and(|surface| surface.selected_thread_id() == Some(thread_id.as_str()))
        {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Lifecycle continuation skipped",
                    "Beryl can only automatically compact and continue the currently selected thread.",
                ));
            }
            return false;
        }

        let (execution_target, automatic_title_generation_allowed, turn_options) = match &self.state
        {
            ShellState::Ready(ready) => {
                let automatic_title_generation_allowed = ready
                    .loaded_workspace
                    .workspace_state
                    .thread_automatic_title_generation_eligible(&ConversationThreadId::new(
                        thread_id.clone(),
                    ));
                let turn_options = ready
                    .surface
                    .pending_turn_start_options(Some(thread_id.as_str()));
                (
                    ready.execution_target.clone(),
                    automatic_title_generation_allowed,
                    turn_options,
                )
            }
            ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::Blocked(_)
            | ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_) => return false,
        };

        let queued = match &mut self.state {
            ShellState::Ready(ready) => ready.surface.queue_pending_turn_fragment(
                thread_id.clone(),
                execution_target,
                automatic_title_generation_allowed,
                turn_options,
                request.resume_fragment(),
            ),
            ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::Blocked(_)
            | ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_) => false,
        };
        if !queued {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Lifecycle continuation skipped",
                    "Beryl could not queue the generated continuation turn for this thread.",
                ));
            }
            return false;
        }

        if let Some(surface) = self.conversation_surface_mut() {
            surface.begin_context_compaction(&thread_id);
        }
        self.status_operation_receiver = Some(spawn_context_compaction_worker(
            connector,
            thread_id,
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
        true
    }

    fn begin_turn_steering(
        &mut self,
        thread_id: String,
        expected_turn_id: String,
        fragments: Vec<SteeringInputFragment>,
    ) -> bool {
        if self.turn_steering_receivers.len() >= MAX_CONCURRENT_TURN_STEERING_TASKS {
            return self.queue_steering_fragments_for_next_turn(
                thread_id,
                fragments,
                "Beryl queued this input for the next turn because too many active-turn steering requests are already in flight.".to_string(),
            );
        }

        let Some(connector) = self.backend_client_connector() else {
            return self.queue_steering_fragments_for_next_turn(
                thread_id,
                fragments,
                "Beryl does not have an active managed backend for this workspace.".to_string(),
            );
        };

        let receiver = spawn_turn_steering_worker(
            connector,
            thread_id.clone(),
            expected_turn_id,
            fragments.clone(),
            self.bootstrap.probe_timeout(),
        );
        self.turn_steering_receivers.push(TurnSteeringTask {
            thread_id,
            fragments,
            receiver,
        });
        true
    }

    fn queue_steering_fragments_for_next_turn(
        &mut self,
        thread_id: String,
        fragments: Vec<SteeringInputFragment>,
        message: String,
    ) -> bool {
        let mut queued = false;
        if let ShellState::Ready(ready) = &mut self.state {
            let automatic_title_generation_allowed = ready
                .loaded_workspace
                .workspace_state
                .thread_automatic_title_generation_eligible(&ConversationThreadId::new(
                    thread_id.clone(),
                ));
            let turn_options = ready
                .surface
                .pending_turn_start_options(Some(thread_id.as_str()));
            queued = ready.surface.move_steering_fragments_to_pending_turn(
                thread_id.clone(),
                ready.execution_target.clone(),
                automatic_title_generation_allowed,
                turn_options,
                fragments,
            );
            if queued {
                ready.surface.set_notice(SurfaceNotice::new(
                    "Input queued for next turn",
                    message.clone(),
                ));
            }
        }

        if queued && self.turn_receiver.is_none() {
            self.begin_pending_turn_input_queue_for_thread(&thread_id);
        }
        queued
    }

    fn begin_pending_turn_input_queue_for_thread(&mut self, thread_id: &str) -> bool {
        if self.conversation_surface().is_some_and(|surface| {
            pending_turn_queue_should_wait_for_compaction(
                surface.context_compaction_thread_id(),
                thread_id,
            )
        }) {
            return false;
        }

        let Some(queue) = self
            .conversation_surface_mut()
            .and_then(|surface| surface.take_pending_turn_input_queue_for_thread(thread_id))
        else {
            return false;
        };

        let Some(connector) = self.backend_client_connector() else {
            if let Some(surface) = self.conversation_surface_mut() {
                let _ = surface.finish_turn_failure(
                    "Beryl does not have an active managed backend for this workspace.",
                );
            }
            return true;
        };

        let workspace = queue.execution_target().clone();
        let Some(beryl_workspace_id) = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return false;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            return false;
        };
        let selected_thread_id = Some(queue.thread_id().to_string());
        let automatic_title_generation_allowed = queue.automatic_title_generation_allowed();
        let turn_options = self.turn_options_with_current_developer_instructions(
            selected_thread_id.as_deref(),
            queue.turn_options().clone(),
        );
        let user_input_fragments = queue.into_fragments();
        let (shell_tool_sender, shell_tool_receiver) = shell_dynamic_tool_request_channel();
        self.shell_tool_receiver = Some(shell_tool_receiver);
        self.turn_receiver = Some(spawn_turn_worker(
            persistence,
            connector,
            beryl_workspace_id,
            workspace,
            selected_thread_id,
            automatic_title_generation_allowed,
            user_input_fragments,
            turn_options,
            Some(shell_tool_sender),
            self.bootstrap.probe_timeout(),
        ));
        true
    }

    fn record_surface_layout_bounds(&mut self, bounds: Bounds<Pixels>, _: &mut Context<Self>) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_layout_bounds(bounds);
        }
    }

    fn record_surface_split_bounds(&mut self, bounds: Bounds<Pixels>, _: &mut Context<Self>) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_split_bounds(bounds);
        }
    }

    fn begin_surface_divider_drag(
        &mut self,
        divider_left: Pixels,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.begin_divider_drag(divider_left, event.position.x);
            cx.notify();
        }
    }

    fn begin_surface_graph_overlay_drag(
        &mut self,
        handle_bottom: Pixels,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.begin_graph_overlay_drag(handle_bottom, event.position.y);
            cx.notify();
        }
    }

    fn begin_surface_tool_activity_panel_drag(
        &mut self,
        panel_top: Pixels,
        panel_bottom: Pixels,
        composer_height: Pixels,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.begin_tool_activity_panel_drag(
                panel_top,
                panel_bottom,
                composer_height,
                event.position.y,
            );
            cx.notify();
        }
    }

    fn update_surface_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let mut updated = false;
        if let Some(surface) = self.conversation_surface_mut() {
            updated |= surface.update_divider_drag(event.position.x);
            updated |= surface.update_graph_overlay_drag(event.position.y);
            updated |= surface.update_tool_activity_panel_drag(event.position.y);
        }

        if updated {
            cx.notify();
        }
    }

    fn end_surface_drag(&mut self, _: &MouseUpEvent, cx: &mut Context<Self>) {
        let mut persist_ui_state = false;
        if let Some(surface) = self.conversation_surface_mut() {
            surface.end_divider_drag();
            surface.end_graph_overlay_drag();
            persist_ui_state = surface.end_tool_activity_panel_drag();
            cx.notify();
        }
        if persist_ui_state {
            self.persist_current_workspace_ui_state();
        }
    }

    fn selected_wsl_distro(&self) -> Option<String> {
        match &self.state {
            ShellState::Picker(picker) => picker.model.selected_wsl_distro.clone(),
            ShellState::Discovering(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::Ready(_)
            | ShellState::Blocked(_) => None,
        }
    }

    fn workspace_shell_state_mut(&mut self) -> Option<&mut LoadedWorkspaceState> {
        match &mut self.state {
            ShellState::WorkspaceIdle(idle) => Some(&mut idle.loaded_workspace),
            ShellState::WorkspaceLoaded(loaded) => Some(loaded),
            ShellState::Ready(ready) => Some(&mut ready.loaded_workspace),
            ShellState::Blocked(blocked) => blocked.loaded_workspace.as_mut(),
            ShellState::Discovering(_) | ShellState::Picker(_) | ShellState::Opening(_) => None,
        }
    }

    fn workspace_shell_state(&self) -> Option<&LoadedWorkspaceState> {
        match &self.state {
            ShellState::WorkspaceIdle(idle) => Some(&idle.loaded_workspace),
            ShellState::WorkspaceLoaded(loaded) => Some(loaded),
            ShellState::Ready(ready) => Some(&ready.loaded_workspace),
            ShellState::Blocked(blocked) => blocked.loaded_workspace.as_ref(),
            ShellState::Discovering(_) | ShellState::Picker(_) | ShellState::Opening(_) => None,
        }
    }

    fn set_workspace_members_notice(&mut self, notice: impl Into<String>) {
        if let Some(loaded) = self.workspace_shell_state_mut() {
            loaded.set_workspace_members_notice(notice);
        }
    }

    fn bootstrap_workspace_state(&self, execution_target: &WorkspaceId) -> LoadedWorkspaceState {
        let mut workspace_state = WorkspaceConversationState::default();
        let _ = apply_primary_execution_target_selection(&mut workspace_state, execution_target);

        let workspace =
            BerylWorkspaceManifest::untitled(1, token_usage_snapshot::current_unix_millis());
        let mut workspace_picker_member_paths = workspace_picker::WorkspacePickerMemberPaths::new();
        workspace_picker_member_paths.insert(
            workspace.id().clone(),
            workspace_picker::explicit_member_path_strings(&workspace_state),
        );
        LoadedWorkspaceState::new(
            workspace.clone(),
            vec![workspace],
            workspace_picker_member_paths,
            workspace_state,
            WorkspaceUiState::default(),
            None,
        )
    }

    fn preferred_thread_id_for_target(&self, execution_target: &WorkspaceId) -> Option<String> {
        match &self.state {
            ShellState::Ready(ready) if &ready.execution_target == execution_target => {
                ready.surface.selected_thread_id().map(str::to_string)
            }
            ShellState::Blocked(blocked) if matches!(&blocked.target, RetryTarget::Workspace(target) if target == execution_target) => {
                blocked
                    .surface
                    .as_ref()
                    .and_then(|surface| surface.selected_thread_id().map(str::to_string))
            }
            _ => self
                .workspace_shell_state()
                .and_then(|loaded| loaded.workspace_state.active_thread_registration())
                .filter(|thread| {
                    thread.execution_target() == execution_target && !thread.requires_rebind()
                })
                .map(|thread| thread.thread_id().as_str().to_string()),
        }
    }

    fn thread_selection_for_open_target(&self, target: &RetryTarget) -> ThreadSelectionRequest {
        if let Some((thread_id, label)) = self.recovery_thread_for_target(target) {
            return ThreadSelectionRequest::exact(thread_id, label);
        }

        let preferred_thread_id = match target {
            RetryTarget::Workspace(execution_target) => {
                self.preferred_thread_id_for_target(execution_target)
            }
            RetryTarget::WorkspacePrimary => self
                .workspace_shell_state()
                .and_then(|loaded| loaded.workspace_state.active_thread())
                .map(|thread_id| thread_id.as_str().to_string()),
            RetryTarget::Startup | RetryTarget::HostPath(_) | RetryTarget::WslPath { .. } => None,
        };
        ThreadSelectionRequest::RestorePreferred(preferred_thread_id)
    }

    fn recovery_thread_for_target(&self, target: &RetryTarget) -> Option<(String, String)> {
        let RetryTarget::Workspace(execution_target) = target else {
            return None;
        };
        let ShellState::Blocked(blocked) = &self.state else {
            return None;
        };
        if !blocked.disconnect
            || !matches!(&blocked.target, RetryTarget::Workspace(target) if target == execution_target)
        {
            return None;
        }

        let workspace_state = blocked
            .loaded_workspace
            .as_ref()
            .map(|loaded| &loaded.workspace_state);
        blocked.surface.as_ref().and_then(|surface| {
            let thread = surface.selected_thread()?;
            let label = workspace_state
                .and_then(|workspace_state| {
                    surface.selected_thread_display_label(workspace_state, execution_target)
                })
                .unwrap_or_else(|| {
                    normalized_thread_name(thread.name.as_deref())
                        .unwrap_or_else(|| "Untitled thread".to_string())
                });
            Some((thread.id.clone(), label))
        })
    }

    fn preserved_surface_for_open_target(
        &self,
        target: &RetryTarget,
    ) -> Option<ConversationSurfaceState> {
        let RetryTarget::Workspace(execution_target) = target else {
            return None;
        };
        let ShellState::Blocked(blocked) = &self.state else {
            return None;
        };
        if !blocked.disconnect
            || !matches!(&blocked.target, RetryTarget::Workspace(target) if target == execution_target)
        {
            return None;
        }

        blocked
            .surface
            .as_ref()
            .map(ConversationSurfaceState::snapshot_for_backend_reopen)
    }

    fn remember_known_threads_for_target(
        &mut self,
        execution_target: &WorkspaceId,
        known_threads: &[ThreadSummary],
    ) -> bool {
        let Some(loaded) = self.workspace_shell_state_mut() else {
            return false;
        };

        let mut touched_manifest = false;
        for summary in known_threads {
            touched_manifest |= loaded
                .workspace_state
                .remember_thread(registered_thread_from_summary(execution_target, summary));
        }

        touched_manifest
    }

    fn update_workspace_state_for_opened_target(
        &mut self,
        execution_target: &WorkspaceId,
        known_threads: &[ThreadSummary],
        active_thread_id: Option<&str>,
        workspace_backend_state_changed: bool,
    ) {
        let mut should_persist = false;
        let thread_registry_changed =
            self.remember_known_threads_for_target(execution_target, known_threads);

        if let Some(loaded) = self.workspace_shell_state_mut() {
            if let Some(active_thread_id) = active_thread_id {
                should_persist |= loaded
                    .workspace_state
                    .activate_thread(&ConversationThreadId::new(active_thread_id.to_string()))
                    .is_some();
            }
        }

        should_persist |= thread_registry_changed || workspace_backend_state_changed;
        if should_persist {
            self.persist_current_workspace_state(
                thread_registry_changed || workspace_backend_state_changed,
            );
        }
    }

    fn remember_active_thread_summary(
        &mut self,
        execution_target: &WorkspaceId,
        summary: &ThreadSummary,
        beryl_created: bool,
    ) {
        let ignored_backend_name = self
            .thread_ignores_backend_name_for_automatic_title(&summary.id, summary.name.as_deref());
        let Some(loaded) = self.workspace_shell_state_mut() else {
            return;
        };

        let mut registered_thread = registered_thread_from_summary(execution_target, summary);
        if ignored_backend_name {
            registered_thread.set_backend_name(None);
        }
        if beryl_created {
            registered_thread = registered_thread.with_beryl_created();
        }
        let touched_manifest = loaded.workspace_state.remember_thread(registered_thread);
        let should_persist = loaded
            .workspace_state
            .activate_thread(&ConversationThreadId::new(summary.id.clone()))
            .is_some()
            || touched_manifest;

        if should_persist {
            self.persist_current_workspace_state(touched_manifest);
        }
    }

    fn persist_current_workspace_state(&mut self, touch_manifest: bool) {
        if touch_manifest {
            let Some(workspace_id) = self
                .workspace_shell_state()
                .map(|loaded| loaded.workspace.id().clone())
            else {
                return;
            };
            self.touch_loaded_workspace_manifest_in_memory(&workspace_id);
        }

        let Some((workspace_id, workspace_state)) =
            self.workspace_shell_state_mut().map(|loaded| {
                loaded.refresh_active_workspace_picker_member_paths();
                (
                    loaded.workspace.id().clone(),
                    loaded.workspace_state.clone(),
                )
            })
        else {
            return;
        };

        self.workspace_persistence_queue.save_workspace_state(
            workspace_id,
            workspace_state,
            touch_manifest,
        );
    }

    fn touch_loaded_workspace_manifest_in_memory(&mut self, workspace_id: &BerylWorkspaceId) {
        let touched_at_millis = token_usage_snapshot::current_unix_millis();
        let Some(loaded) = self.workspace_shell_state_mut() else {
            return;
        };
        if loaded.workspace.id() != workspace_id {
            return;
        }

        loaded
            .workspace
            .set_last_updated_at_millis(touched_at_millis);
        if let Some(known_workspace) = loaded
            .known_workspaces
            .iter_mut()
            .find(|known_workspace| known_workspace.id() == workspace_id)
        {
            *known_workspace = loaded.workspace.clone();
        }
    }

    fn persist_current_workspace_ui_state(&mut self) {
        let Some((workspace_id, workspace_ui_state)) =
            self.current_workspace_ui_state_for_persistence()
        else {
            return;
        };

        if let Some(loaded) = self.workspace_shell_state_mut()
            && loaded.workspace.id() == &workspace_id
        {
            loaded.workspace_ui_state = workspace_ui_state.clone();
        }

        self.workspace_persistence_queue
            .save_workspace_ui_state(workspace_id, workspace_ui_state);
    }

    fn current_workspace_ui_state_for_persistence(
        &self,
    ) -> Option<(BerylWorkspaceId, WorkspaceUiState)> {
        let loaded = self.workspace_shell_state()?;
        let mut workspace_ui_state = loaded.workspace_ui_state.clone();
        if let Some(surface) = self.conversation_surface() {
            workspace_ui_state = surface.workspace_ui_state();
        }

        Some((loaded.workspace.id().clone(), workspace_ui_state))
    }

    fn finish_graph_mutation_update(&mut self, update: GraphMutationUpdate) -> bool {
        let workspace_id = update.workspace_id().clone();
        if !self
            .loaded_workspace()
            .is_some_and(|loaded| loaded.workspace.id() == &workspace_id)
        {
            return false;
        }

        match update {
            GraphMutationUpdate::Commit(commit_update) => {
                let application = if let Some(surface) = self.conversation_surface_mut() {
                    match surface.finish_graph_mutation_commit_update(commit_update) {
                        Ok(application) => Some(application),
                        Err(error) => {
                            surface.report_graph_mutation_failure(error);
                            None
                        }
                    }
                } else {
                    None
                };

                let mut updated = application.is_some();
                match application {
                    Some(GraphCommitApplication::Applied {
                        latest_manifest,
                        graph_changed,
                        ..
                    }) => {
                        if let Some(loaded) = self.loaded_workspace_mut() {
                            loaded.replace_manifest(latest_manifest);
                            updated = true;
                        }
                        if graph_changed {
                            self.prune_graph_scrollbar_activity();
                        }
                    }
                    Some(GraphCommitApplication::RecoveryRequired { reason }) => {
                        updated |= self.start_graph_reload_recovery(workspace_id, reason);
                    }
                    _ => {}
                }
                updated
            }
            GraphMutationUpdate::Failure(failure) => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.report_optimistic_graph_mutation_failure(
                        failure.optimistic_mutation_id,
                        failure.message,
                    );
                    true
                } else {
                    false
                }
            }
        }
    }

    fn finish_graph_reload_update(&mut self, result: Result<GraphReloadUpdate, String>) -> bool {
        let update = match result {
            Ok(update) => update,
            Err(error) => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.report_graph_mutation_failure(error);
                    return true;
                }
                return false;
            }
        };

        if !self
            .loaded_workspace()
            .is_some_and(|loaded| loaded.workspace.id() == &update.workspace_id)
        {
            return false;
        }

        if let Some(loaded) = self.loaded_workspace_mut() {
            loaded.replace_manifest(update.manifest);
        }
        if let Some(surface) = self.conversation_surface_mut() {
            surface.finish_graph_mutation(update.graph, update.revision, update.warning);
        }
        self.prune_graph_scrollbar_activity();
        true
    }

    fn start_graph_reload_recovery(
        &mut self,
        workspace_id: BerylWorkspaceId,
        reason: String,
    ) -> bool {
        if self.graph_receiver.is_some() {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.report_graph_mutation_failure(format!(
                    "{reason}; semantic graph reload recovery is waiting for another graph worker to finish."
                ));
                return true;
            }
            return false;
        }

        let Some(persistence) = self.workspace_persistence_for_worker() else {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.report_graph_mutation_failure(format!(
                    "{reason}; semantic graph reload recovery is unavailable because workspace persistence is not configured."
                ));
                return true;
            }
            return false;
        };

        self.graph_receiver = Some(spawn_graph_reload_worker(
            persistence,
            workspace_id,
            Some(format!(
                "Recovered semantic graph projection after {reason}."
            )),
        ));
        true
    }

    fn conversation_surface_mut(&mut self) -> Option<&mut ConversationSurfaceState> {
        match &mut self.state {
            ShellState::Ready(ready) => Some(&mut ready.surface),
            ShellState::Blocked(blocked) => blocked.surface.as_mut(),
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_) => None,
        }
    }

    fn loaded_workspace_mut(&mut self) -> Option<&mut LoadedWorkspaceState> {
        self.workspace_shell_state_mut()
    }

    fn loaded_workspace(&self) -> Option<&LoadedWorkspaceState> {
        self.workspace_shell_state()
    }

    pub(super) fn backend_client_connector(&self) -> Option<ManagedBackendClientConnector> {
        let execution_target = match &self.state {
            ShellState::Ready(ready) => Some(&ready.execution_target),
            ShellState::Blocked(blocked) => match &blocked.target {
                RetryTarget::Workspace(workspace) => Some(workspace),
                _ => None,
            },
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_) => None,
        }?;
        self.backend_client_connector_for_execution_target(execution_target)
    }

    pub(super) fn backend_client_connector_for_execution_target(
        &self,
        execution_target: &WorkspaceId,
    ) -> Option<ManagedBackendClientConnector> {
        self.backend_servers
            .get(execution_target)
            .map(ManagedBackendServer::client_connector)
    }

    pub(super) fn backend_client_connectors(
        &self,
    ) -> Vec<(WorkspaceId, ManagedBackendClientConnector)> {
        self.backend_servers
            .iter()
            .map(|(execution_target, server)| (execution_target.clone(), server.client_connector()))
            .collect()
    }

    fn retained_state_snapshot(&self) -> RetainedStateSnapshot {
        let mut snapshot = self
            .conversation_surface()
            .map(ConversationSurfaceState::retained_state_snapshot)
            .unwrap_or_default();
        let backend_work_receivers = self.backend_work_receiver_count();
        let backend_client_connection_estimate = self.backend_client_connection_estimate();
        let composer_draft = self.composer_draft.retained_counts();
        let composer_clipboard = self.composer_clipboard.retained_counts();
        snapshot.backend_work_receivers = Some(backend_work_receivers);
        snapshot.backend_event_queue_estimate = Some(backend_client_connection_estimate);
        snapshot.backend_client_connection_estimate = Some(backend_client_connection_estimate);
        snapshot.turn_steering_receivers = Some(self.turn_steering_receivers.len());
        snapshot.composer_draft_text_bytes = Some(composer_draft.display_text_bytes);
        snapshot.composer_draft_images = Some(composer_draft.image_count);
        snapshot.composer_draft_image_bytes = Some(composer_draft.image_bytes);
        snapshot.composer_draft_atoms = Some(composer_draft.atom_count);
        snapshot.composer_draft_atom_bytes = Some(composer_draft.atom_bytes);
        snapshot.composer_clipboard_payloads = Some(composer_clipboard.payloads);
        snapshot.composer_clipboard_text_bytes = Some(
            composer_clipboard
                .selected_text_bytes
                .saturating_add(composer_clipboard.fallback_text_bytes),
        );
        snapshot.composer_clipboard_images = Some(composer_clipboard.image_count);
        snapshot.composer_clipboard_image_bytes = Some(composer_clipboard.image_bytes);
        snapshot.composer_clipboard_atoms = Some(composer_clipboard.atom_count);
        snapshot.composer_clipboard_atom_bytes = Some(composer_clipboard.atom_bytes);
        snapshot.pending_composer_image_asset_paste_bytes = Some(
            self.pending_composer_image_asset_paste
                .as_ref()
                .map_or(0, |pending| {
                    pending
                        .workspace_id
                        .as_str()
                        .len()
                        .saturating_add(pending.display_text_snapshot.len())
                }),
        );
        snapshot.composer_image_popup_bytes =
            Some(self.composer_image_popup.as_ref().map_or(0, |popup| {
                popup
                    .atom_id
                    .len()
                    .saturating_add(popup.label.len())
                    .saturating_add(popup.preview_image_bytes)
            }));
        snapshot.workspace_persistence_pending_work =
            Some(self.workspace_persistence_queue.pending_work_count());
        snapshot.thread_title_workers = Some(self.thread_title_receivers.len());
        snapshot.inventory_worker_active =
            Some(usize::from(self.member_thread_inventory_receiver.is_some()));
        if let Some(total) = snapshot.retained_payload_bytes_lower_bound.as_mut() {
            *total = total
                .saturating_add(composer_draft.display_text_bytes)
                .saturating_add(composer_draft.image_bytes)
                .saturating_add(composer_draft.atom_bytes)
                .saturating_add(composer_clipboard.selected_text_bytes)
                .saturating_add(composer_clipboard.fallback_text_bytes)
                .saturating_add(composer_clipboard.image_bytes)
                .saturating_add(composer_clipboard.atom_bytes)
                .saturating_add(
                    snapshot
                        .pending_composer_image_asset_paste_bytes
                        .unwrap_or_default(),
                )
                .saturating_add(snapshot.composer_image_popup_bytes.unwrap_or_default());
        }
        snapshot
    }

    fn handle_diagnostic_target_protocol_request(
        &mut self,
        request: &DiagnosticProtocolRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DiagnosticProtocolResponse {
        match self.diagnostic_target_protocol_result(request, window, cx) {
            Ok(result) => DiagnosticProtocolResponse::success(request.id(), result),
            Err((kind, message)) => {
                DiagnosticProtocolResponse::error(Some(request.id().to_string()), kind, message)
            }
        }
    }

    fn diagnostic_target_protocol_result(
        &mut self,
        request: &DiagnosticProtocolRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Value, (&'static str, String)> {
        match request.command() {
            DiagnosticChildCommand::Handshake => Ok(json!({
                "protocol": crate::diagnostic_child_protocol::DIAGNOSTIC_CHILD_PROTOCOL_NAME,
                "protocolVersion": crate::diagnostic_child_protocol::DIAGNOSTIC_CHILD_PROTOCOL_VERSION,
            })),
            DiagnosticChildCommand::ReadProcess => {
                parse_diagnostic_target_arguments::<EmptyDiagnosticTargetArguments>(
                    request.params(),
                )?;
                Ok(json!(self.process_diagnostic_snapshot()))
            }
            DiagnosticChildCommand::ReadMemory => {
                parse_diagnostic_target_arguments::<EmptyDiagnosticTargetArguments>(
                    request.params(),
                )?;
                let process = self.process_diagnostic_snapshot();
                Ok(json!(self.memory_diagnostic_snapshot(&process)))
            }
            DiagnosticChildCommand::ReadRenderer => {
                parse_diagnostic_target_arguments::<EmptyDiagnosticTargetArguments>(
                    request.params(),
                )?;
                let snapshot = self.diagnostic_tool_snapshot(window, cx);
                Ok(json!(snapshot.renderer))
            }
            DiagnosticChildCommand::PrepareRendererWindow => {
                parse_diagnostic_target_arguments::<EmptyDiagnosticTargetArguments>(
                    request.params(),
                )?;
                Ok(json!(
                    self.handle_prepare_renderer_window_tool_result(window, cx)
                ))
            }
            DiagnosticChildCommand::ReadRetainedState => {
                parse_diagnostic_target_arguments::<EmptyDiagnosticTargetArguments>(
                    request.params(),
                )?;
                let snapshot = self.diagnostic_tool_snapshot(window, cx);
                Ok(json!({ "retainedState": snapshot.retained_state }))
            }
            DiagnosticChildCommand::ReadVisibleMedia => {
                let arguments = parse_diagnostic_target_arguments::<DiagnosticTargetLimitArguments>(
                    request.params(),
                )?;
                let snapshot = self.diagnostic_tool_snapshot(window, cx);
                Ok(json!(visible_media_result(
                    snapshot.visible_media,
                    arguments
                        .limit_or_default(DEFAULT_VISIBLE_MEDIA_LIMIT, MAX_VISIBLE_MEDIA_LIMIT),
                )))
            }
            DiagnosticChildCommand::ReadMediaEvents => {
                let arguments = parse_diagnostic_target_arguments::<
                    DiagnosticTargetMediaEventsArguments,
                >(request.params())?;
                let snapshot = self.diagnostic_tool_snapshot(window, cx);
                Ok(json!(media_events_result(
                    snapshot.media_events,
                    arguments.after_sequence,
                    arguments.limit_or_default(DEFAULT_MEDIA_EVENT_LIMIT, MAX_MEDIA_EVENT_LIMIT),
                )))
            }
            DiagnosticChildCommand::ReadUiState => {
                let parsed =
                    parse_gui_control_tool_request(READ_UI_STATE_COMMAND, request.params())
                        .map_err(|error| (error.kind(), error.to_string()))?;
                match parsed {
                    GuiControlToolRequest::ReadUiState { visible_row_limit } => {
                        Ok(json!(self.ui_state_snapshot(cx, visible_row_limit)))
                    }
                    _ => Err((
                        "internal",
                        "diagnostic target read_ui_state parsed to the wrong command".to_string(),
                    )),
                }
            }
            DiagnosticChildCommand::ListWorkspaceThreads => {
                let arguments = parse_diagnostic_target_arguments::<DiagnosticThreadListArguments>(
                    request.params(),
                )?;
                Ok(self.handle_list_workspace_threads_tool_result(arguments, window, cx))
            }
            DiagnosticChildCommand::CreateNewThread => {
                parse_diagnostic_target_arguments::<EmptyDiagnosticTargetArguments>(
                    request.params(),
                )?;
                Ok(self.handle_create_new_thread_tool_result(cx))
            }
            DiagnosticChildCommand::StartTurn => {
                let arguments = parse_diagnostic_target_arguments::<DiagnosticStartTurnArguments>(
                    request.params(),
                )?;
                self.handle_start_turn_tool_result(arguments, cx)
            }
            DiagnosticChildCommand::SoftStopTurn => {
                let arguments = parse_diagnostic_target_arguments::<DiagnosticStopTurnArguments>(
                    request.params(),
                )?;
                arguments
                    .validate()
                    .map_err(|message| ("invalid_arguments", message))?;
                Ok(self.handle_soft_stop_turn_tool_result(arguments, window, cx))
            }
            DiagnosticChildCommand::HardStopTurn => {
                let arguments = parse_diagnostic_target_arguments::<DiagnosticStopTurnArguments>(
                    request.params(),
                )?;
                arguments
                    .validate()
                    .map_err(|message| ("invalid_arguments", message))?;
                Ok(self.handle_hard_stop_turn_tool_result(arguments, window, cx))
            }
            DiagnosticChildCommand::SwitchWorkspace => {
                let parsed =
                    parse_gui_control_tool_request(SWITCH_WORKSPACE_COMMAND, request.params())
                        .map_err(|error| (error.kind(), error.to_string()))?;
                match parsed {
                    GuiControlToolRequest::SwitchWorkspace(arguments) => self
                        .handle_switch_workspace_tool_result(arguments, window, cx)
                        .map(|result| json!(result)),
                    _ => Err((
                        "internal",
                        "diagnostic target switch_workspace parsed to the wrong command"
                            .to_string(),
                    )),
                }
            }
            DiagnosticChildCommand::SwitchThread => {
                let parsed =
                    parse_gui_control_tool_request(SWITCH_THREAD_COMMAND, request.params())
                        .map_err(|error| (error.kind(), error.to_string()))?;
                match parsed {
                    GuiControlToolRequest::SwitchThread(arguments) => self
                        .handle_switch_thread_tool_result(arguments, window, cx)
                        .map(|result| json!(result)),
                    _ => Err((
                        "internal",
                        "diagnostic target switch_thread parsed to the wrong command".to_string(),
                    )),
                }
            }
            DiagnosticChildCommand::ScrollTranscript => {
                let parsed =
                    parse_gui_control_tool_request(SCROLL_TRANSCRIPT_COMMAND, request.params())
                        .map_err(|error| (error.kind(), error.to_string()))?;
                match parsed {
                    GuiControlToolRequest::ScrollTranscript(arguments) => Ok(json!(
                        self.handle_scroll_transcript_tool_result(arguments, cx)
                    )),
                    _ => Err((
                        "internal",
                        "diagnostic target scroll_transcript parsed to the wrong command"
                            .to_string(),
                    )),
                }
            }
            DiagnosticChildCommand::ClosePopups => {
                let parsed = parse_gui_control_tool_request(CLOSE_POPUPS_COMMAND, request.params())
                    .map_err(|error| (error.kind(), error.to_string()))?;
                match parsed {
                    GuiControlToolRequest::ClosePopups => {
                        Ok(json!(self.handle_close_popups_tool_result(cx)))
                    }
                    _ => Err((
                        "internal",
                        "diagnostic target close_popups parsed to the wrong command".to_string(),
                    )),
                }
            }
        }
    }

    fn handle_beryl_gui_control_dynamic_tool_request(
        &mut self,
        request: &DynamicToolCallRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> beryl_backend::DynamicToolCallResponse {
        let parsed = match parse_beryl_gui_control_dynamic_tool_request(request) {
            Ok(parsed) => parsed,
            Err(error) => {
                return gui_control_failure_response(request, error.kind(), error.to_string());
            }
        };

        match parsed {
            GuiControlToolRequest::ReadUiState { visible_row_limit } => {
                ui_state_tool_response(request, self.ui_state_snapshot(cx, visible_row_limit))
            }
            GuiControlToolRequest::SwitchWorkspace(_) => gui_control_failure_response(
                request,
                "unsupported_tool",
                "switch_workspace is only available through beryl_diagnostic against a diagnostic child.",
            ),
            GuiControlToolRequest::SwitchThread(arguments) => {
                match self.handle_switch_thread_tool_result(arguments, window, cx) {
                    Ok(result) => switch_thread_tool_response(request, result),
                    Err((kind, message)) => gui_control_failure_response(request, kind, message),
                }
            }
            GuiControlToolRequest::ScrollTranscript(arguments) => scroll_transcript_tool_response(
                request,
                self.handle_scroll_transcript_tool_result(arguments, cx),
            ),
            GuiControlToolRequest::ClosePopups => {
                close_popups_tool_response(request, self.handle_close_popups_tool_result(cx))
            }
        }
    }

    fn ui_state_snapshot(
        &self,
        cx: &mut Context<Self>,
        visible_row_limit: usize,
    ) -> UiStateSnapshot {
        let panel_snapshot = self.transcript_panel.read(cx).diagnostic_snapshot();
        let selected_workspace_id = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().as_str().to_string())
            .map(bounded_control_string);
        let selected_thread_id = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_id)
            .map(str::to_string)
            .map(bounded_control_string);
        let selected_runtime_target = match &self.state {
            ShellState::Ready(ready) => Some(runtime_target_diagnostic(&ready.execution_target)),
            ShellState::Blocked(blocked) => {
                Some(runtime_target_diagnostic(&blocked.target.workspace()))
            }
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_) => None,
        };
        let mut visible_media = panel_snapshot.visible_media;
        visible_media.preview.composer_image_preview = self.composer_image_preview_diagnostic();

        UiStateSnapshot {
            shell_state: self.shell_state_diagnostic_label().to_string(),
            selected_surface: self.selected_surface_diagnostic_label(cx).to_string(),
            selected_workspace_id,
            selected_thread_id,
            selected_runtime_target,
            turn_state: self.turn_ui_state(),
            transcript: self.transcript_ui_state(visible_row_limit),
            visible_media,
            activity_panel: self.activity_panel_ui_state(),
            popups: self.popup_ui_state(cx),
            background_work: BackgroundWorkUiState {
                backend_work_receivers: self.backend_work_receiver_count(),
                thread_activation_pending: self.thread_activation_receiver.is_some(),
                turn_stream_pending: self.turn_receiver.is_some(),
                workspace_transition_pending: self.workspace_picker_action_receiver.is_some()
                    || self.workspace_receiver.is_some()
                    || matches!(self.state, ShellState::Opening(_)),
            },
        }
    }

    fn turn_ui_state(&self) -> TurnUiState {
        let Some(surface) = self.conversation_surface() else {
            return TurnUiState {
                selected_thread_state: "none".to_string(),
                last_turn_state: "Unknown".to_string(),
                ..TurnUiState::default()
            };
        };
        let projection = surface.status_line_projection();
        let selected_thread_status = surface
            .selected_thread_status
            .as_ref()
            .map(thread_status_diagnostic_label)
            .map(str::to_string);
        let selected_thread_state = selected_thread_state_diagnostic_label(surface).to_string();
        let cancellable_active_turn = projection
            .cancellable_active_turn
            .as_ref()
            .map(cancellable_turn_ui_state);
        let (hard_stop_target_count, hard_stop_limitation_count) = projection
            .hard_stop_targets
            .as_ref()
            .map(|targets| (targets.targets.len(), targets.limitations.len()))
            .unwrap_or_default();

        TurnUiState {
            selected_thread_state,
            selected_thread_status,
            last_turn_state: projection.last_turn_state,
            cancellable_active_turn,
            hard_stop_target_count,
            hard_stop_limitation_count,
            turn_stop_request_in_flight: surface
                .status_line_operations()
                .turn_stop_request_in_flight(),
            hard_stop_request_in_flight: surface
                .status_line_operations()
                .hard_stop_request_in_flight(),
            hard_stop_hold_active: surface.status_line_operations().hard_stop_hold_active(),
        }
    }

    fn shell_state_diagnostic_label(&self) -> &'static str {
        match self.state {
            ShellState::Discovering(_) => "discovering",
            ShellState::Picker(_) => "picker",
            ShellState::Opening(_) => "opening",
            ShellState::WorkspaceIdle(_) => "workspace_idle",
            ShellState::WorkspaceLoaded(_) => "workspace_loaded",
            ShellState::Ready(_) => "ready",
            ShellState::Blocked(_) => "blocked",
        }
    }

    fn selected_surface_diagnostic_label(&self, cx: &mut Context<Self>) -> &'static str {
        if self
            .settings_window
            .is_visible(cx)
            .ok()
            .is_some_and(|visible| visible)
        {
            return "settings";
        }
        if self
            .loaded_workspace()
            .is_some_and(|loaded| loaded.workspace_picker.is_open())
        {
            return "workspace_picker";
        }
        let Some(surface) = self.conversation_surface() else {
            return self.shell_state_diagnostic_label();
        };
        if surface.thread_selector().is_open() {
            return "thread_selector";
        }
        if surface.graph_overlay().visible() {
            return "graph";
        }
        "conversation"
    }

    fn transcript_ui_state(&self, visible_row_limit: usize) -> TranscriptUiState {
        let Some(surface) = self.conversation_surface() else {
            return TranscriptUiState::default();
        };
        let list_state = surface.transcript_list_state();
        let item_count = surface.transcript_presentation().len();
        let visible_range = clamp_ui_range(list_state.visible_range(), item_count);
        let presentation_range = clamp_ui_range(list_state.presentation_range(), item_count);
        let (visible_rows, visible_rows_truncated) =
            visible_transcript_rows(surface, visible_range.clone(), visible_row_limit);

        TranscriptUiState {
            item_count,
            visible_range: Some(ui_range_diagnostic(&visible_range)),
            presentation_range: Some(ui_range_diagnostic(&presentation_range)),
            scroll_position: transcript_scroll_position_diagnostic(list_state.scroll_position()),
            user_scrolled: surface.transcript_user_scrolled,
            pending_thread_activation_label: surface
                .pending_thread_activation_label()
                .map(str::to_string)
                .map(bounded_control_string),
            older_history_loading: surface.older_history_loading(),
            visible_row_count: visible_range.len(),
            visible_rows,
            visible_rows_truncated,
        }
    }

    fn activity_panel_ui_state(&self) -> ActivityPanelUiState {
        self.conversation_surface()
            .map(|surface| ActivityPanelUiState {
                mode: format!("{:?}", surface.tool_activity_panel_mode()).to_ascii_lowercase(),
                visible: surface.tool_activity_panel_visible(),
                row_count: surface.tool_activity_row_count(),
                height_px: f64::from(f32::from(surface.tool_activity_panel_height())),
            })
            .unwrap_or_default()
    }

    fn popup_ui_state(&self, cx: &mut Context<Self>) -> PopupUiState {
        let workspace_picker = self
            .loaded_workspace()
            .map(|loaded| &loaded.workspace_picker);
        let surface = self.conversation_surface();
        let transcript_preview_open = self
            .transcript_panel
            .read(cx)
            .diagnostic_snapshot()
            .visible_media
            .preview
            .transcript_image_preview
            .is_some();

        PopupUiState {
            workspace_picker_open: workspace_picker.is_some_and(|picker| picker.is_open()),
            workspace_picker_row_action_menu_open: workspace_picker
                .is_some_and(|picker| picker.row_action_menu_is_open()),
            workspace_picker_member_action_menu_open: workspace_picker
                .is_some_and(|picker| picker.member_action_menu_is_open()),
            workspace_picker_runtime_selector_open: workspace_picker
                .is_some_and(|picker| picker.runtime_selector_dropdown_is_open()),
            workspace_picker_rename_editor_open: workspace_picker
                .is_some_and(|picker| picker.rename_editor_open()),
            thread_selector_open: surface
                .is_some_and(|surface| surface.thread_selector().is_open()),
            graph_thread_link_menu_open: surface
                .is_some_and(|surface| surface.graph_thread_link_menu().is_open()),
            transcript_branch_menu_open: surface
                .is_some_and(|surface| surface.transcript_branch_menu().is_open()),
            checklist_thread_start_menu_open: surface
                .is_some_and(|surface| surface.checklist_thread_start_menu().is_open()),
            status_line_operations_open: surface
                .is_some_and(|surface| surface.status_line_operations().is_open()),
            composer_image_popup_open: self.composer_image_popup.is_some(),
            transcript_image_preview_open: transcript_preview_open,
            settings_window_visible: self.settings_window.is_visible(cx).ok(),
            settings_window_transient_popup_open: self
                .settings_window
                .has_transient_popups(cx)
                .ok(),
        }
    }

    fn handle_list_workspace_threads_tool_result(
        &mut self,
        arguments: DiagnosticThreadListArguments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Value {
        let refresh_started = self.begin_member_thread_inventory_refresh_if_needed();
        if refresh_started {
            self.schedule_poll_if_needed(window, cx);
        }

        let limit = arguments.normalized_limit();
        let selected_workspace_id = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().as_str().to_string())
            .map(bounded_control_string);
        let selected_thread_id = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_id)
            .map(str::to_string)
            .map(bounded_control_string);
        let Some(surface) = self.conversation_surface() else {
            return json!({
                "status": "not_ready",
                "message": "Beryl has no active conversation surface.",
                "selectedWorkspaceId": selected_workspace_id,
                "selectedThreadId": selected_thread_id,
                "pendingNewThread": false,
                "refreshStarted": refresh_started,
                "refreshing": false,
                "refreshNeeded": false,
                "lastError": null,
                "refreshedAtMillis": 0,
                "groupCount": 0,
                "threadCount": 0,
                "threadsTruncated": false,
                "groupsTruncated": false,
                "groups": [],
                "uiState": self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
            });
        };

        let inventory = surface.member_thread_inventory();
        let snapshot = inventory.snapshot();
        let pending_new_thread = surface.selected_thread_id().is_none();
        let mut remaining = limit;
        let group_count = snapshot.groups().len();
        let thread_count = snapshot
            .groups()
            .iter()
            .map(|group| group.threads().len())
            .sum::<usize>();
        let mut returned_thread_count = 0usize;
        let mut threads_truncated = false;
        let mut groups_truncated = false;
        let groups = snapshot
            .groups()
            .iter()
            .enumerate()
            .filter_map(|(group_index, group)| {
                if group_index >= limit {
                    groups_truncated = true;
                    return None;
                }
                Some(group)
            })
            .map(|group| {
                let threads = group
                    .threads()
                    .iter()
                    .filter_map(|thread| {
                        if remaining == 0 {
                            threads_truncated = true;
                            return None;
                        }
                        remaining = remaining.saturating_sub(1);
                        returned_thread_count = returned_thread_count.saturating_add(1);
                        Some(json!({
                            "threadId": bounded_control_string(thread.thread_id().as_str().to_string()),
                            "forkedFromId": thread
                                .forked_from_id()
                                .map(|id| bounded_control_string(id.as_str().to_string())),
                            "title": bounded_control_string(thread.title().to_string()),
                            "executionTarget": runtime_target_diagnostic(thread.execution_target()),
                            "createdAtMillis": thread.created_at_millis(),
                            "updatedAtMillis": thread.updated_at_millis(),
                            "selected": surface.selected_thread_id()
                                == Some(thread.thread_id().as_str()),
                        }))
                    })
                    .collect::<Vec<_>>();
                let (member_key, member_id) = match group.key() {
                    MemberThreadInventoryMemberKey::ImplicitHome => ("implicit_home", None),
                    MemberThreadInventoryMemberKey::Explicit(id) => {
                        ("explicit", Some(bounded_control_string(id.as_str().to_string())))
                    }
                };
                let member_kind = match group.kind() {
                    MemberThreadInventoryMemberKind::ImplicitHome => "implicit_home",
                    MemberThreadInventoryMemberKind::Explicit => "explicit",
                };
                json!({
                    "memberKey": member_key,
                    "memberId": member_id,
                    "kind": member_kind,
                    "label": bounded_control_string(group.label().to_string()),
                    "runtime": bounded_control_string(group.runtime().display_name()),
                    "canonicalPath": group
                        .canonical_path()
                        .map(|path| bounded_control_string(path.display().to_string())),
                    "threads": threads,
                })
            })
            .collect::<Vec<_>>();
        if returned_thread_count < thread_count {
            threads_truncated = true;
        }
        let inventory_workspace_id =
            bounded_control_string(snapshot.workspace_id().as_str().to_string());
        let refreshed_at_millis = snapshot.refreshed_at_millis();
        let refreshing = inventory.refreshing();
        let refresh_needed = inventory.needs_refresh();
        let last_error = inventory
            .last_error()
            .map(|error| bounded_control_string(error.to_string()));
        let ui_state = self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT);

        json!({
            "status": "ok",
            "selectedWorkspaceId": selected_workspace_id,
            "selectedThreadId": selected_thread_id,
            "pendingNewThread": pending_new_thread,
            "inventoryWorkspaceId": inventory_workspace_id,
            "refreshStarted": refresh_started,
            "refreshing": refreshing,
            "refreshNeeded": refresh_needed,
            "lastError": last_error,
            "refreshedAtMillis": refreshed_at_millis,
            "groupCount": group_count,
            "threadCount": thread_count,
            "threadsTruncated": threads_truncated,
            "groupsTruncated": groups_truncated,
            "groups": groups,
            "uiState": ui_state,
        })
    }

    fn handle_create_new_thread_tool_result(&mut self, cx: &mut Context<Self>) -> Value {
        let result = self.select_pending_new_thread_from_control(cx);
        let (status, message) = match result {
            Ok(status) => (status, None),
            Err((status, message)) => (status, Some(message)),
        };
        json!({
            "status": status,
            "message": message.map(bounded_control_string),
            "uiState": self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
        })
    }

    fn handle_start_turn_tool_result(
        &mut self,
        arguments: DiagnosticStartTurnArguments,
        cx: &mut Context<Self>,
    ) -> Result<Value, (&'static str, String)> {
        let text = arguments
            .validated_text()
            .map_err(|message| ("invalid_arguments", message))?;
        let composer_busy = {
            let input = self.conversation_input.read(cx);
            input.has_marked_text() || !input.text().trim().is_empty() || !input.atoms().is_empty()
        } || !self.composer_draft.is_empty();
        if composer_busy {
            return Ok(json!({
                "status": "unavailable",
                "message": "The child composer already contains draft input.",
                "uiState": self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
            }));
        }

        self.conversation_input.update(cx, |input, cx| {
            input.set_text(text.as_str(), cx);
            input.clear_edit_history();
        });
        let accepted = self.queue_turn_from_composer(cx);
        let (status, message) = if accepted {
            ("accepted", None)
        } else {
            (
                "unavailable",
                Some("The child composer submission was not available in the current state."),
            )
        };
        Ok(json!({
            "status": status,
            "message": message,
            "uiState": self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
        }))
    }

    fn handle_soft_stop_turn_tool_result(
        &mut self,
        arguments: DiagnosticStopTurnArguments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Value {
        let current_target = self
            .conversation_surface()
            .and_then(|surface| surface.status_line_projection().cancellable_active_turn);
        if let Some(message) =
            diagnostic_expected_turn_mismatch(&arguments, current_target.as_ref())
        {
            return json!({
                "status": "stale_turn_target",
                "message": bounded_control_string(message),
                "uiState": self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
            });
        }

        match self.begin_soft_stop_selected_turn_from_control(window, cx) {
            Ok(target) => json!({
                "status": "accepted",
                "target": cancellable_turn_ui_state(&target),
                "uiState": self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
            }),
            Err((kind, message)) => json!({
                "status": kind,
                "message": bounded_control_string(message),
                "uiState": self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
            }),
        }
    }

    fn handle_hard_stop_turn_tool_result(
        &mut self,
        arguments: DiagnosticStopTurnArguments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Value {
        let current_turn = self.conversation_surface().and_then(|surface| {
            let projection = surface.status_line_projection();
            projection
                .hard_stop_targets
                .map(|targets| targets.selected_turn)
                .or(projection.cancellable_active_turn)
        });
        if let Some(message) = diagnostic_expected_turn_mismatch(&arguments, current_turn.as_ref())
        {
            return json!({
                "status": "stale_turn_target",
                "message": bounded_control_string(message),
                "uiState": self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
            });
        }

        match self.begin_hard_stop_selected_turn_from_control(window, cx) {
            Ok(targets) => json!({
                "status": "accepted",
                "target": cancellable_turn_ui_state(&targets.selected_turn),
                "targetCount": targets.targets.len(),
                "limitationCount": targets.limitations.len(),
                "uiState": self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
            }),
            Err((kind, message)) => json!({
                "status": kind,
                "message": bounded_control_string(message),
                "uiState": self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
            }),
        }
    }

    fn select_pending_new_thread_from_control(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<&'static str, (&'static str, String)> {
        if self.conversation_surface().is_none() {
            return Err((
                "not_ready",
                "Beryl has no active conversation surface.".to_string(),
            ));
        }
        if self
            .conversation_surface()
            .is_some_and(|surface| surface.selected_thread_id().is_none())
        {
            return Ok("already_selected");
        }
        if self.graph_thread_start_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
            || self.turn_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
            || self.status_operation_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.composer_image_asset_receiver.is_some()
        {
            return Err((
                "unsafe_state",
                "Beryl has workspace, thread, turn, or composer work in progress.".to_string(),
            ));
        }

        let cleared_active_thread = self
            .workspace_shell_state_mut()
            .is_some_and(|loaded| loaded.workspace_state.clear_active_thread());
        if let Some(surface) = self.conversation_surface_mut() {
            surface.start_new_thread();
        }
        self.composer_image_label_scan_receiver = None;
        if cleared_active_thread {
            self.persist_current_workspace_state(false);
        }
        self.notify_transcript_panel(cx);
        cx.notify();
        Ok("selected")
    }

    fn handle_switch_workspace_tool_result(
        &mut self,
        arguments: SwitchWorkspaceArguments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<SwitchWorkspaceResult, (&'static str, String)> {
        let workspace_id = arguments.workspace_id;
        let requested_workspace_id =
            BerylWorkspaceId::new(workspace_id.clone()).map_err(|error| {
                (
                    "invalid_arguments",
                    format!("invalid workspaceId {workspace_id:?}: {error}"),
                )
            })?;
        let Some(current_workspace_id) = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return Err((
                "not_ready",
                "Beryl has no loaded workspace to switch from.".to_string(),
            ));
        };

        if current_workspace_id == requested_workspace_id {
            return Ok(SwitchWorkspaceResult {
                status: "already_selected".to_string(),
                workspace_id: bounded_control_string(workspace_id),
                message: None,
                ui_state: self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
            });
        }

        if self.workspace_picker_action_receiver.is_some()
            || self.workspace_receiver.is_some()
            || matches!(self.state, ShellState::Opening(_))
        {
            return Err((
                "workspace_transition_pending",
                "Beryl already has workspace transition work in progress.".to_string(),
            ));
        }

        if let Some(reason) = workspace_picker::workspace_picker_transition_path_disabled_reason(
            workspace_picker::WorkspacePickerTransitionPath::SwitchWorkspace,
            workspace_picker::WorkspacePickerTransitionBlockers {
                edit_rollback_work: self.transcript_edit_commit_receiver.is_some(),
                edit_replacement_work: self.transcript_edit_replacement_turn.is_some(),
            },
        ) {
            return Err(("unsafe_state", reason.to_string()));
        }

        let target_known = self.loaded_workspace().is_some_and(|loaded| {
            loaded
                .known_workspaces
                .iter()
                .any(|workspace| workspace.id() == &requested_workspace_id)
        });
        if !target_known {
            return Err((
                "unknown_workspace",
                format!(
                    "Workspace id {:?} is not present in the current bounded workspace list.",
                    requested_workspace_id.as_str()
                ),
            ));
        }

        self.cancel_thread_title_workers();
        let Some(app_state) = self.app_state_for_worker() else {
            return Err((
                "not_ready",
                "Beryl app state is unavailable for workspace switching.".to_string(),
            ));
        };
        self.workspace_picker_action_receiver = Some(spawn_switch_workspace_worker(
            app_state.startup_persistence,
            app_state.workspace_persistence,
            requested_workspace_id.clone(),
            self.workspace_persistence_queue.flush(),
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        cx.notify();

        Ok(SwitchWorkspaceResult {
            status: "pending".to_string(),
            workspace_id: bounded_control_string(workspace_id),
            message: Some(
                "Workspace activation started through the ordinary Beryl activation path."
                    .to_string(),
            ),
            ui_state: self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
        })
    }

    fn handle_switch_thread_tool_result(
        &mut self,
        arguments: SwitchThreadArguments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<SwitchThreadResult, (&'static str, String)> {
        let thread_id = arguments.thread_id;
        if self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_id)
            == Some(thread_id.as_str())
        {
            return Ok(SwitchThreadResult {
                status: "already_selected".to_string(),
                thread_id: bounded_control_string(thread_id),
                message: None,
                ui_state: self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
            });
        }

        let target = match self.gui_control_thread_activation_target(&thread_id) {
            Ok(target) => target,
            Err((kind, message)) => {
                return Err((kind, message));
            }
        };
        let started = self.activate_thread_selector_target(target, window, cx);
        let (status, message) = match started {
            ThreadActivationStart::Started => (
                "pending".to_string(),
                Some(
                    "Thread activation started through the ordinary Beryl activation path."
                        .to_string(),
                ),
            ),
            ThreadActivationStart::AlreadySelected => ("already_selected".to_string(), None),
            ThreadActivationStart::Rejected { kind, message } => {
                return Err((kind, message));
            }
        };

        Ok(SwitchThreadResult {
            status,
            thread_id: bounded_control_string(thread_id),
            message,
            ui_state: self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
        })
    }

    fn gui_control_thread_activation_target(
        &self,
        thread_id: &str,
    ) -> Result<ThreadSelectorActivationTarget, (&'static str, String)> {
        let Some(surface) = self.conversation_surface() else {
            return Err((
                "not_ready",
                "Beryl has no active conversation surface to switch threads.".to_string(),
            ));
        };
        let requested = ConversationThreadId::new(thread_id.to_string());
        let mut matches = Vec::new();
        for group in surface.member_thread_inventory().snapshot().groups() {
            for thread in group.threads() {
                if thread.thread_id() == &requested {
                    matches.push(ThreadSelectorActivationTarget {
                        thread_id: requested.clone(),
                        label: thread.title().to_string(),
                        execution_target: thread.execution_target().clone(),
                    });
                }
            }
        }

        match matches.len() {
            1 => Ok(matches.remove(0)),
            0 => Err((
                "unknown_thread",
                format!(
                    "Thread id {thread_id:?} is not present in the current bounded member-thread inventory."
                ),
            )),
            _ => Err((
                "ambiguous_thread",
                format!(
                    "Thread id {thread_id:?} appears more than once in the current member-thread inventory."
                ),
            )),
        }
    }

    fn handle_scroll_transcript_tool_result(
        &mut self,
        arguments: ScrollTranscriptArguments,
        cx: &mut Context<Self>,
    ) -> ScrollTranscriptResult {
        let command = arguments.command;
        let repeat = arguments.repeat;
        let result = self.apply_transcript_scroll_command(command, repeat, cx);
        let (status, message) = match result {
            Ok(()) => ("applied".to_string(), None),
            Err(message) => ("unavailable".to_string(), Some(message)),
        };
        ScrollTranscriptResult {
            status,
            command,
            repeat,
            message,
            ui_state: self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
        }
    }

    fn apply_transcript_scroll_command(
        &mut self,
        command: ScrollTranscriptCommand,
        repeat: usize,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(surface) = self.conversation_surface_mut() else {
            return Err("Beryl has no active conversation surface.".to_string());
        };
        let list_state = surface.transcript_list_state();
        let item_count = surface.transcript_presentation().len();
        if item_count == 0 {
            return Err("The selected transcript has no rows to scroll.".to_string());
        }

        match command {
            ScrollTranscriptCommand::Top => {
                list_state.scroll_to_position(ListScrollPosition::Content(ListOffset {
                    item_ix: 0,
                    offset_in_item: px(0.0),
                }));
                surface.release_transcript_submit_anchor();
                surface.set_transcript_user_scrolled(true);
            }
            ScrollTranscriptCommand::Bottom => {
                list_state.scroll_to_position(ListScrollPosition::Bottom);
                surface.release_transcript_submit_anchor();
                surface.set_transcript_user_scrolled(false);
            }
            ScrollTranscriptCommand::PageUp | ScrollTranscriptCommand::PageDown => {
                let viewport_height = list_state.viewport_bounds().size.height;
                if viewport_height <= px(0.0) {
                    return Err(
                        "The transcript viewport has not been measured yet, so page scrolling is unavailable."
                            .to_string(),
                    );
                }
                let direction = match command {
                    ScrollTranscriptCommand::PageUp => -1.0_f32,
                    ScrollTranscriptCommand::PageDown => 1.0_f32,
                    ScrollTranscriptCommand::Top | ScrollTranscriptCommand::Bottom => 0.0,
                };
                for _ in 0..repeat {
                    list_state.scroll_by(viewport_height * direction);
                }
                surface.release_transcript_submit_anchor();
                let at_bottom = matches!(list_state.scroll_position(), ListScrollPosition::Bottom);
                surface.set_transcript_user_scrolled(!at_bottom);
            }
        }
        self.note_scrollbar_activity(ScrollbarRegion::Transcript, cx);
        self.notify_transcript_panel(cx);
        Ok(())
    }

    fn handle_close_popups_tool_result(&mut self, cx: &mut Context<Self>) -> ClosePopupsResult {
        let mut closed = Vec::new();

        if self.composer_image_popup.is_some() {
            self.close_composer_image_popup(cx);
            closed.push("composer_image_popup".to_string());
        }
        self.transcript_panel.update(cx, |panel, cx| {
            if panel.close_transient_popups_for_dynamic_tool(cx) {
                closed.push("transcript_panel_popup".to_string());
            }
        });
        match self.settings_window.close_transient_popups(cx) {
            Ok(true) => closed.push(SETTINGS_WINDOW_POPUP_CLOSE_REASON.to_string()),
            Ok(false) => {}
            Err(error) => {
                warn!(error = %error, "failed to close Beryl settings window transient popups");
            }
        }

        if let Some(loaded) = self.loaded_workspace_mut() {
            if loaded.workspace_picker.row_action_menu_is_open() {
                loaded.workspace_picker.close_row_action_menu();
                closed.push("workspace_picker_row_action_menu".to_string());
            }
            if loaded.workspace_picker.member_action_menu_is_open() {
                loaded.workspace_picker.close_member_action_menu();
                closed.push("workspace_picker_member_action_menu".to_string());
            }
            if loaded.workspace_picker.runtime_selector_dropdown_is_open() {
                loaded.workspace_picker.close_runtime_selector_dropdown();
                closed.push("workspace_picker_runtime_selector".to_string());
            }
            if loaded.workspace_picker.is_open() {
                loaded.workspace_picker.close();
                closed.push("workspace_picker".to_string());
            }
        }

        if let Some(surface) = self.conversation_surface_mut() {
            if surface.close_thread_selector() {
                closed.push("thread_selector".to_string());
            }
            if surface.graph_thread_link_menu().is_open() {
                surface.graph_thread_link_menu_mut().close();
                closed.push("graph_thread_link_menu".to_string());
            }
            if surface.transcript_branch_menu().is_open() {
                surface.transcript_branch_menu_mut().close();
                closed.push("transcript_branch_menu".to_string());
            }
            if surface.checklist_thread_start_menu().is_open() {
                surface.checklist_thread_start_menu_mut().close();
                closed.push("checklist_thread_start_menu".to_string());
            }
            if surface.status_line_operations().is_open() {
                surface.status_line_operations_mut().close();
                closed.push("status_line_operations".to_string());
            }
        }

        if !closed.is_empty() {
            cx.notify();
        }

        ClosePopupsResult {
            closed_count: closed.len(),
            closed,
            ui_state: self.ui_state_snapshot(cx, DEFAULT_UI_VISIBLE_ROW_LIMIT),
        }
    }

    fn handle_prepare_renderer_window_tool_result(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> RendererDiagnosticSnapshot {
        cx.activate(true);
        window.resize(size(px(1040.0), px(760.0)));
        window.activate_window();
        window.refresh();
        self.diagnostic_tool_snapshot(window, cx).renderer
    }

    fn diagnostic_tool_snapshot(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DiagnosticToolSnapshot {
        let panel_snapshot = self.transcript_panel.read(cx).diagnostic_snapshot();
        let mut retained_state = self.retained_state_snapshot();
        self.add_text_input_retained_counts(&mut retained_state, cx);
        panel_snapshot.add_retained_counts(&mut retained_state);
        let mut visible_media = panel_snapshot.visible_media;
        visible_media.preview.composer_image_preview = self.composer_image_preview_diagnostic();
        let process = self.process_diagnostic_snapshot();
        let memory = self.memory_diagnostic_snapshot(&process);
        let renderer = renderer_snapshot_with_shell_window(
            process.clone(),
            cx.renderer_diagnostic_snapshot(),
            window.renderer_diagnostic_snapshot(),
        );
        DiagnosticToolSnapshot {
            process,
            memory,
            renderer,
            retained_state,
            visible_media,
            media_events: panel_snapshot.media_events,
        }
    }

    fn process_diagnostic_snapshot(&self) -> ProcessDiagnosticSnapshot {
        let selected_target = match &self.state {
            ShellState::Ready(ready) => Some(&ready.execution_target),
            _ => None,
        };
        let selected_workspace_id = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().as_str().to_string());
        let selected_thread_id = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_id)
            .map(str::to_string);
        let managed_backend_child_pids = self
            .backend_servers
            .iter()
            .filter_map(|(target, server)| {
                server
                    .process_id()
                    .map(|pid| ManagedBackendProcessDiagnostic {
                        pid,
                        runtime_target: runtime_target_diagnostic(target),
                        selected: selected_target.is_some_and(|selected| selected == target),
                    })
            })
            .take(32)
            .collect();

        ProcessDiagnosticSnapshot {
            pid: std::process::id(),
            executable_path: std::env::current_exe()
                .ok()
                .map(|path| bounded_diagnostic_string(path.display().to_string())),
            beryl_home: self
                .app_state
                .as_ref()
                .ok()
                .map(ConfiguredAppState::home_display)
                .map(bounded_diagnostic_string),
            selected_workspace_id: selected_workspace_id.map(bounded_diagnostic_string),
            selected_thread_id: selected_thread_id.map(bounded_diagnostic_string),
            selected_runtime_target: selected_target.map(runtime_target_diagnostic),
            managed_backend_child_pids,
        }
    }

    fn memory_diagnostic_snapshot(
        &self,
        process: &ProcessDiagnosticSnapshot,
    ) -> MemoryDiagnosticSnapshot {
        let ui = MemoryDiagnosticUiCorrelation::from_process(process);
        match memory_diagnostics::current_process_memory_snapshot() {
            Ok(counters) => MemoryDiagnosticSnapshot {
                counters: Some(counters),
                unavailable_reason: None,
                ui,
            },
            Err(error) => MemoryDiagnosticSnapshot {
                counters: None,
                unavailable_reason: Some(error.to_string()),
                ui,
            },
        }
    }

    fn composer_image_preview_diagnostic(&self) -> Option<PreviewStateDiagnostic> {
        let popup = self.composer_image_popup.as_ref()?;
        let state = match &popup.mode {
            ComposerImagePopupMode::Menu => "menu",
            ComposerImagePopupMode::Preview => {
                if popup.preview_image.is_some() {
                    "loaded"
                } else {
                    "pending"
                }
            }
        };
        Some(PreviewStateDiagnostic {
            state: state.to_string(),
            compressed_bytes: (popup.preview_image_bytes > 0).then_some(popup.preview_image_bytes),
        })
    }

    fn add_text_input_retained_counts(&self, snapshot: &mut RetainedStateSnapshot, cx: &App) {
        let mut counts = TextInputRetainedAggregate::default();
        for input in [
            &self.host_path_input,
            &self.wsl_distro_input,
            &self.wsl_path_input,
            &self.workspace_picker_filter_input,
            &self.workspace_rename_input,
            &self.conversation_input,
            &self.surface_notice_text_input,
        ] {
            counts.add(input.read(cx).retained_counts());
        }

        snapshot.text_input_count = Some(counts.count);
        snapshot.text_input_current_text_bytes = Some(counts.current_text_bytes);
        snapshot.text_input_current_atoms = Some(counts.current_atom_count);
        snapshot.text_input_current_atom_bytes = Some(counts.current_atom_bytes);
        snapshot.text_input_undo_snapshots = Some(counts.undo_snapshot_count);
        snapshot.text_input_redo_snapshots = Some(counts.redo_snapshot_count);
        snapshot.text_input_undo_bytes = Some(counts.undo_bytes);
        snapshot.text_input_redo_bytes = Some(counts.redo_bytes);
        snapshot.text_input_widget_layout_lines = Some(counts.widget_layout_lines);
        snapshot.text_input_widget_visual_lines = Some(counts.widget_visual_lines);
        snapshot.text_input_widget_visible_text_bytes = Some(counts.widget_visible_text_bytes);
        if let Some(total) = snapshot.retained_payload_bytes_lower_bound.as_mut() {
            *total = total.saturating_add(counts.payload_bytes_lower_bound());
        }
    }

    fn backend_work_receiver_count(&self) -> usize {
        [
            self.discovery_receiver.is_some(),
            self.workspace_receiver.is_some(),
            self.graph_receiver.is_some(),
            self.graph_thread_start_receiver.is_some(),
            self.transcript_branch_receiver.is_some(),
            self.transcript_edit_commit_receiver.is_some(),
            self.member_thread_inventory_receiver.is_some(),
            self.thread_activation_receiver.is_some(),
            self.thread_history_page_receiver.is_some(),
            self.composer_image_label_scan_receiver.is_some(),
            self.composer_image_asset_receiver.is_some(),
            self.turn_receiver.is_some(),
            self.composer_image_delivery_receiver.is_some(),
            self.status_operation_receiver.is_some(),
            self.account_rate_limits_receiver.is_some(),
            self.turn_stop_receiver.is_some(),
            self.hard_stop_receiver.is_some(),
            self.workspace_picker_action_receiver.is_some(),
            self.workspace_title_receiver.is_some(),
            self.application_shutdown_receiver.is_some(),
            self.tool_activity_nickname_resolver.has_active_worker(),
        ]
        .into_iter()
        .filter(|active| *active)
        .count()
        .saturating_add(self.turn_steering_receivers.len())
        .saturating_add(self.thread_title_receivers.len())
    }

    fn backend_client_connection_estimate(&self) -> usize {
        [
            self.workspace_receiver.is_some(),
            self.member_thread_inventory_receiver.is_some(),
            self.thread_activation_receiver.is_some(),
            self.thread_history_page_receiver.is_some(),
            self.turn_receiver.is_some(),
            self.status_operation_receiver.is_some(),
            self.account_rate_limits_receiver.is_some(),
            self.turn_stop_receiver.is_some(),
            self.hard_stop_receiver.is_some(),
            self.tool_activity_nickname_resolver.has_active_worker(),
        ]
        .into_iter()
        .filter(|active| *active)
        .count()
        .saturating_add(self.turn_steering_receivers.len())
        .saturating_add(self.thread_title_receivers.len())
    }

    pub(super) fn block_if_backend_process_dead(
        &mut self,
        title: &'static str,
        summary: &str,
        detail: &str,
    ) -> bool {
        let active_target = match &self.state {
            ShellState::Ready(ready) => Some(ready.execution_target.clone()),
            _ => None,
        };
        if let Some(active_target) = active_target.as_ref()
            && self
                .backend_servers
                .get_mut(active_target)
                .is_some_and(ManagedBackendServer::is_process_alive)
        {
            return false;
        }

        self.cancel_thread_title_workers();
        self.shutdown_active_backend_server_in_background("backend process marked dead");
        self.block_ready_surface(title, summary, detail);
        true
    }

    fn scrollbar_opacity(&self, region: &ScrollbarRegion) -> f32 {
        self.scrollbar_activity
            .get(region)
            .map_or(0.0, |activity| activity.opacity(Instant::now()))
    }

    fn scrollbar_animating(&self, region: &ScrollbarRegion) -> bool {
        self.scrollbar_activity
            .get(region)
            .is_some_and(|activity| activity.is_animating(Instant::now()))
    }

    fn shell_scrollbars_animating(&self) -> bool {
        let now = Instant::now();
        self.scrollbar_activity.iter().any(|(region, activity)| {
            !matches!(region, ScrollbarRegion::Transcript) && activity.is_animating(now)
        })
    }

    fn note_scrollbar_activity(&mut self, region: ScrollbarRegion, cx: &mut Context<Self>) {
        let generation = self
            .scrollbar_activity
            .entry(region.clone())
            .or_default()
            .record_activity();
        self.schedule_scrollbar_animation(region.clone(), generation, cx);
        self.notify_scrollbar_region(&region, cx);
    }

    fn schedule_scrollbar_animation(
        &mut self,
        region: ScrollbarRegion,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let Some(next_delay) = self.next_scrollbar_animation_delay(&region, generation) else {
            return;
        };

        let animation_region = region.clone();
        let animation_task = cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                cx.background_executor().timer(next_delay).await;
                let _ = view.update(&mut cx, |view: &mut Self, cx: &mut Context<Self>| {
                    view.advance_scrollbar_animation(&animation_region, generation, cx);
                });
            }
        });

        if let Some(activity) = self.scrollbar_activity.get_mut(&region) {
            activity.animation_task = Some(animation_task);
        }
    }

    fn next_scrollbar_animation_delay(
        &self,
        region: &ScrollbarRegion,
        generation: u64,
    ) -> Option<Duration> {
        let now = Instant::now();
        let activity = self.scrollbar_activity.get(region)?;
        if activity.generation != generation {
            return None;
        }

        if let Some(transition) = &activity.transition {
            return transition.remaining_duration(now);
        }

        let last_activity_at = activity.last_activity_at?;
        let fade_deadline = last_activity_at + SCROLLBAR_FADE_DELAY;
        (now < fade_deadline).then_some(fade_deadline.saturating_duration_since(now))
    }

    fn advance_scrollbar_animation(
        &mut self,
        region: &ScrollbarRegion,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        let Some(activity) = self.scrollbar_activity.get_mut(region) else {
            return;
        };
        if activity.generation != generation {
            return;
        }
        activity.animation_task = None;

        let mut should_notify = false;
        if let Some(transition) = &activity.transition {
            if !transition.is_active(now) {
                let target_opacity = transition.to_opacity;
                activity.transition = None;
                should_notify = true;
                if target_opacity <= 0.0 {
                    activity.last_activity_at = None;
                }
            }
        }

        if activity.transition.is_none() {
            let Some(last_activity_at) = activity.last_activity_at else {
                if should_notify {
                    self.notify_scrollbar_region(region, cx);
                }
                return;
            };
            let fade_deadline = last_activity_at + SCROLLBAR_FADE_DELAY;
            if now >= fade_deadline {
                let current_opacity = activity.opacity(now);
                if current_opacity <= 0.0 {
                    activity.last_activity_at = None;
                } else {
                    activity.transition = Some(ScrollbarTransition {
                        started_at: now,
                        from_opacity: current_opacity,
                        to_opacity: 0.0,
                    });
                    should_notify = true;
                }
            }
        }

        if should_notify {
            self.notify_scrollbar_region(region, cx);
        }

        if self
            .scrollbar_activity
            .get(region)
            .is_some_and(|activity| activity.generation == generation)
        {
            self.schedule_scrollbar_animation(region.clone(), generation, cx);
        }
    }

    fn notify_scrollbar_region(&self, region: &ScrollbarRegion, cx: &mut Context<Self>) {
        cx.notify();
        if matches!(region, ScrollbarRegion::Transcript) {
            self.notify_transcript_panel(cx);
        }
    }

    fn prune_graph_scrollbar_activity(&mut self) {
        let active_graph_columns: Vec<_> = self
            .conversation_surface()
            .map(|surface| {
                surface
                    .graph_column_selector_scroll
                    .column_keys()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        self.scrollbar_activity.retain(|region, _| match region {
            ScrollbarRegion::GraphColumn(column_key) => active_graph_columns.contains(column_key),
            _ => true,
        });
    }

    fn notify_transcript_panel(&self, cx: &mut Context<Self>) {
        self.transcript_panel.update(cx, |_, cx| {
            cx.notify();
        });
    }

    fn notify_checklist_sidebar_panel(&self, cx: &mut Context<Self>) {
        self.checklist_sidebar_panel.update(cx, |_, cx| {
            cx.notify();
        });
    }

    fn transcript_panel_snapshot(&self) -> Option<render::transcript::TranscriptPanelSnapshot> {
        match &self.state {
            ShellState::Ready(ready) => Some(render::transcript::TranscriptPanelSnapshot {
                workspace_id: Some(ready.loaded_workspace.workspace.id().clone()),
                workspace: ready.execution_target.clone(),
                appearance: self.appearance_settings(),
                selected_thread_present: ready.surface.selected_thread().is_some(),
                selected_thread_id: ready.surface.selected_thread_id().map(str::to_string),
                pending_thread_activation_label: ready
                    .surface
                    .pending_thread_activation_label()
                    .map(str::to_string),
                transcript_width: ready.surface.transcript_width(),
                transcript_list_state: ready.surface.transcript_list_state(),
                submit_anchor: ready.surface.transcript_submit_anchor_snapshot(),
                loaded_history_anchor_pending: ready.surface.loaded_history_anchor_pending(),
                older_history_loading: ready.surface.older_history_loading(),
                metrics: tracing::enabled!(tracing::Level::DEBUG)
                    .then(|| ready.surface.transcript_presentation().render_metrics()),
                activity_caret: ready.surface.transcript_activity_caret(),
                transcript_edit_mode: ready.surface.transcript_edit_mode_snapshot(),
                transcript_reset_generation: ready.surface.transcript_reset_generation(),
                content_release_generation: ready.surface.transcript_content_release_generation(),
                content_release_row_identities: ready
                    .surface
                    .transcript_content_release_row_identities()
                    .to_vec(),
            }),
            ShellState::Blocked(blocked) => blocked.surface.as_ref().map(|surface| {
                render::transcript::TranscriptPanelSnapshot {
                    workspace_id: blocked
                        .loaded_workspace
                        .as_ref()
                        .map(|loaded| loaded.workspace.id().clone()),
                    workspace: blocked.target.workspace(),
                    appearance: self.appearance_settings(),
                    selected_thread_present: surface.selected_thread().is_some(),
                    selected_thread_id: surface.selected_thread_id().map(str::to_string),
                    pending_thread_activation_label: surface
                        .pending_thread_activation_label()
                        .map(str::to_string),
                    transcript_width: surface.transcript_width(),
                    transcript_list_state: surface.transcript_list_state(),
                    submit_anchor: surface.transcript_submit_anchor_snapshot(),
                    loaded_history_anchor_pending: surface.loaded_history_anchor_pending(),
                    older_history_loading: surface.older_history_loading(),
                    metrics: tracing::enabled!(tracing::Level::DEBUG)
                        .then(|| surface.transcript_presentation().render_metrics()),
                    activity_caret: surface.transcript_activity_caret(),
                    transcript_edit_mode: surface.transcript_edit_mode_snapshot(),
                    transcript_reset_generation: surface.transcript_reset_generation(),
                    content_release_generation: surface.transcript_content_release_generation(),
                    content_release_row_identities: surface
                        .transcript_content_release_row_identities()
                        .to_vec(),
                }
            }),
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_) => None,
        }
    }

    fn conversation_surface(&self) -> Option<&ConversationSurfaceState> {
        match &self.state {
            ShellState::Ready(ready) => Some(&ready.surface),
            ShellState::Blocked(blocked) => blocked.surface.as_ref(),
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_) => None,
        }
    }
}

fn registered_thread_from_summary(
    execution_target: &WorkspaceId,
    summary: &ThreadSummary,
) -> RegisteredConversationThread {
    RegisteredConversationThread::new(
        ConversationThreadId::new(summary.id.clone()),
        execution_target.clone(),
        summary.preview.clone(),
        summary.name.clone(),
        summary.created_at,
        summary.updated_at,
    )
}

fn normalized_thread_name(name: Option<&str>) -> Option<String> {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}
