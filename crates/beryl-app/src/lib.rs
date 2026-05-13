//! High-level application-shell types for Beryl.
//!
//! ```no_run
//! use beryl_app::{
//!     AppBootstrap, NotificationPreferences, WorkspaceGraphSummaryRequest,
//!     WorkspaceGraphToolService, WorkspaceUiState, beryl_user_thread_start_options, run_app,
//! };
//! use beryl_model::workspace::{BerylWorkspaceId, WorkspaceId};
//!
//! let workspace = WorkspaceId::host_windows(r"C:\work\beryl");
//! let bootstrap = AppBootstrap::new(Some(workspace));
//! let beryl_home_dir = bootstrap.beryl_home_dir().unwrap();
//! let workspace_store = beryl_home_dir.workspace_persistence();
//! let preferences_store = beryl_home_dir.gui_preferences_store();
//! let _default_ui_state = WorkspaceUiState::default();
//! let _notifications = NotificationPreferences::default();
//! let _thread_options = beryl_user_thread_start_options();
//! let graph_tools = WorkspaceGraphToolService::new(workspace_store.clone());
//! let _summary_request = WorkspaceGraphSummaryRequest {
//!     workspace_id: BerylWorkspaceId::untitled(1),
//! };
//! let _ = (graph_tools, preferences_store);
//! run_app(bootstrap);
//! ```

mod appearance;
mod backend_failure;
mod beryl_home_dir;
mod diagnostic_child_dynamic_tools;
mod diagnostic_child_protocol;
mod diagnostic_child_supervisor;
mod diagnostic_child_target;
mod diagnostic_dynamic_tools;
mod dynamic_tools;
mod graph_dynamic_tools;
mod graph_tools;
mod gui_control_dynamic_tools;
mod lifecycle_dynamic_tools;
mod member_thread_inventory;
mod memory_diagnostics;
mod persistence;
mod preferences;
mod shell;
mod startup_state;
mod text_input;
mod thread_start_options;
mod title_generation;
mod workspace_graph_commit;
mod workspace_image_assets;
mod workspace_persistence;

use std::{error::Error, fmt, path::PathBuf, time::Duration};

use beryl_model::workspace::WorkspaceId;

pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub struct AppBootstrap {
    initial_workspace: Option<WorkspaceId>,
    beryl_home_dir: Option<BerylHomeDir>,
    probe_timeout: Duration,
    memory_milestones_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppBootstrapError {
    ZeroProbeTimeout,
    BerylHomeDir(BerylHomeDirError),
}

impl fmt::Display for AppBootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroProbeTimeout => write!(f, "app bootstrap probe timeout must be non-zero"),
            Self::BerylHomeDir(error) => write!(f, "{error}"),
        }
    }
}

impl Error for AppBootstrapError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ZeroProbeTimeout => None,
            Self::BerylHomeDir(error) => Some(error),
        }
    }
}

impl From<BerylHomeDirError> for AppBootstrapError {
    fn from(error: BerylHomeDirError) -> Self {
        Self::BerylHomeDir(error)
    }
}

impl AppBootstrap {
    pub fn new(initial_workspace: Option<WorkspaceId>) -> Self {
        Self {
            initial_workspace,
            beryl_home_dir: None,
            probe_timeout: DEFAULT_PROBE_TIMEOUT,
            memory_milestones_enabled: false,
        }
    }

    pub fn initial_workspace(&self) -> Option<&WorkspaceId> {
        self.initial_workspace.as_ref()
    }

    pub fn probe_timeout(&self) -> Duration {
        self.probe_timeout
    }

    pub fn memory_milestones_enabled(&self) -> bool {
        self.memory_milestones_enabled
    }

    pub fn beryl_home_dir(&self) -> Result<BerylHomeDir, BerylHomeDirError> {
        self.beryl_home_dir
            .clone()
            .map(Ok)
            .unwrap_or_else(BerylHomeDir::from_environment)
    }

    pub fn with_beryl_home_dir(
        mut self,
        root_dir: impl Into<PathBuf>,
    ) -> Result<Self, AppBootstrapError> {
        self.beryl_home_dir = Some(BerylHomeDir::from_explicit_path(root_dir)?);
        Ok(self)
    }

    pub fn with_probe_timeout(
        mut self,
        probe_timeout: Duration,
    ) -> Result<Self, AppBootstrapError> {
        if probe_timeout.is_zero() {
            return Err(AppBootstrapError::ZeroProbeTimeout);
        }

        self.probe_timeout = probe_timeout;
        Ok(self)
    }

    pub fn with_memory_milestones(mut self, enabled: bool) -> Self {
        self.memory_milestones_enabled = enabled;
        self
    }

    pub fn window_title(&self) -> String {
        self.initial_workspace
            .as_ref()
            .map(|workspace| format!("Beryl - {}", workspace.display_label()))
            .unwrap_or_else(|| "Beryl".to_string())
    }
}

pub fn run_app(bootstrap: AppBootstrap) {
    shell::run_app(bootstrap);
}

pub fn run_diagnostic_target_stdio(bootstrap: AppBootstrap) {
    shell::run_diagnostic_target_stdio(bootstrap);
}

pub use appearance::{
    AppearanceButtonSettings, AppearanceButtonStateSettings, AppearanceChromeSettings,
    AppearanceForegroundSettings, AppearanceInputSettings, AppearanceRoleSettings,
    AppearanceSettings, AppearanceSettingsError, AppearanceSettingsStore,
    AppearanceStatusLineSettings, AppearanceSurfaceSettings, AppearanceTranscriptShellSettings,
    ParsedHexColor,
};
pub use beryl_home_dir::{BerylHomeDir, BerylHomeDirError};
pub use diagnostic_child_dynamic_tools::{
    BERYL_DIAGNOSTIC_DYNAMIC_TOOL_NAMESPACE, DIAGNOSTIC_CHILD_CLOSE_POPUPS_TOOL,
    DIAGNOSTIC_CHILD_READ_MEDIA_EVENTS_TOOL, DIAGNOSTIC_CHILD_READ_MEMORY_TOOL,
    DIAGNOSTIC_CHILD_READ_PROCESS_TOOL, DIAGNOSTIC_CHILD_READ_RETAINED_STATE_TOOL,
    DIAGNOSTIC_CHILD_READ_UI_STATE_TOOL, DIAGNOSTIC_CHILD_READ_VISIBLE_MEDIA_TOOL,
    DIAGNOSTIC_CHILD_SCROLL_TRANSCRIPT_TOOL, DIAGNOSTIC_CHILD_START_TOOL,
    DIAGNOSTIC_CHILD_STATUS_TOOL, DIAGNOSTIC_CHILD_STOP_TOOL, DIAGNOSTIC_CHILD_SWITCH_THREAD_TOOL,
    DIAGNOSTIC_CHILD_SWITCH_WORKSPACE_TOOL,
    beryl_diagnostic_child_dynamic_tool_shell_response_timeout,
    beryl_diagnostic_child_dynamic_tool_specs, is_beryl_diagnostic_child_dynamic_tool,
};
pub use diagnostic_dynamic_tools::{
    READ_MEDIA_EVENTS_TOOL, READ_MEMORY_DIAGNOSTICS_TOOL, READ_PROCESS_DIAGNOSTICS_TOOL,
    READ_RETAINED_STATE_SUMMARY_TOOL, READ_VISIBLE_MEDIA_TOOL, beryl_diagnostic_dynamic_tool_specs,
    diagnostic_bridge_unavailable_response, is_beryl_diagnostic_dynamic_tool,
};
pub use dynamic_tools::{
    BERYL_DYNAMIC_TOOL_NAMESPACE, BerylDynamicToolDispatch, DynamicToolRegistryError,
    beryl_dynamic_tool_specs, dispatch_beryl_dynamic_tool_call_with_metadata,
    validate_unique_dynamic_tool_names,
};
pub use graph_dynamic_tools::{
    BERYL_GRAPH_DYNAMIC_TOOL_NAMESPACE, BerylGraphDynamicToolDispatch, BerylGraphDynamicWrite,
    MAX_DYNAMIC_NODE_SUMMARY_CHARS, MAX_DYNAMIC_NODE_TITLE_CHARS, beryl_graph_dynamic_tool_specs,
    dispatch_beryl_graph_dynamic_tool_call, dispatch_beryl_graph_dynamic_tool_call_with_metadata,
};
pub use graph_tools::{
    ChecklistItemSnapshot, ChecklistReadRequest, ChecklistReadResponse, GraphNeighborhoodNode,
    GraphNeighborhoodRequest, GraphNeighborhoodResponse, GraphNodeSnapshot, GraphPatchWriteRequest,
    GraphPatchWriteResponse, GraphSoftLinkSnapshot, GraphThreadRefSnapshot,
    MAX_CHECKLIST_ITEM_COUNT, MAX_GRAPH_NEIGHBORHOOD_CHILD_DEPTH,
    MAX_GRAPH_NEIGHBORHOOD_NODE_COUNT, MAX_GRAPH_NEIGHBORHOOD_PARENT_DEPTH,
    MAX_GRAPH_SUMMARY_ROOT_COUNT, NodeLeafDeleteRequest, NodeLeafDeleteResponse,
    NodeSubtreeDeleteRequest, NodeSubtreeDeleteResponse, READ_CHECKLIST_TOOL,
    READ_GRAPH_NEIGHBORHOOD_TOOL, READ_WORKSPACE_GRAPH_SUMMARY_TOOL, READ_WORKSPACE_STATE_TOOL,
    SET_CHECKLIST_ITEM_STATUS_TOOL, SET_GRAPH_NODE_PARENT_TOOL, ThreadRefUpsertRequest,
    ThreadRefUpsertResponse, UPSERT_GRAPH_NODE_TOOL, UPSERT_GRAPH_SOFT_LINK_TOOL,
    UPSERT_THREAD_REF_TOOL, WorkspaceGraphSummary, WorkspaceGraphSummaryRequest,
    WorkspaceGraphToolError, WorkspaceGraphToolService, WorkspaceMemberSnapshot,
    WorkspaceMemberSnapshotKind, WorkspacePrimaryMemberSnapshot, WorkspaceStateReadRequest,
    WorkspaceStateSnapshot, WorkspaceThreadMetadataSnapshot, node_leaf_delete_patch,
    node_subtree_delete_patch, thread_ref_upsert_patch,
};
pub use lifecycle_dynamic_tools::{
    BerylLifecycleDynamicToolDispatch, LifecycleYieldOutcome, YIELD_TOOL,
    beryl_lifecycle_dynamic_tool_specs, dispatch_beryl_lifecycle_dynamic_tool_call,
    dispatch_beryl_lifecycle_dynamic_tool_call_with_metadata,
};
pub use persistence::{StartupMetadata, StartupPersistence, StartupPersistenceError};
pub use preferences::{
    AgentPreferences, GuiPreferences, GuiPreferencesError, GuiPreferencesStore,
    NotificationPreferences, NotificationSoundPathError, normalize_developer_instructions_text,
    parse_notification_sound_path_text, validate_notification_sound_path,
};
pub use startup_state::{
    ResolvedStartupState, StartupStateError, WorkspaceDeletionResolution,
    create_fresh_untitled_workspace, delete_workspace_and_resolve_active_replacement,
    resolve_startup_state,
};
pub use thread_start_options::{beryl_thread_start_options, beryl_user_thread_start_options};
pub use workspace_graph_commit::{
    WorkspaceGraphMutationCommit, WorkspaceGraphRevision, WorkspaceGraphStateSnapshot,
};
pub use workspace_image_assets::{
    WorkspaceImageAsset, WorkspaceImageAssetFormat, WorkspaceImageAssetMetadata,
    WorkspaceImageAssetStatus,
};
pub use workspace_persistence::{
    BerylWorkspacePersistence, WorkspaceActivityPanelMode, WorkspacePersistenceError,
    WorkspaceUiState,
};
