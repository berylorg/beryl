use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use beryl_model::conversation::{
    ConversationThreadId, ConversationThreadTokenUsageSnapshot, WorkspaceConversationState,
    WorkspaceConversationStateError,
};
use beryl_model::semantic_graph::{SemanticGraph, SemanticGraphError, SemanticGraphPatch};
use beryl_model::workspace::{
    BerylWorkspaceId, BerylWorkspaceManifest, BerylWorkspaceTitleError, RuntimeMode,
    derive_workspace_slug,
};
use gpui::ImageFormat;
use once_cell::sync::Lazy;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use tracing::warn;

use crate::workspace_image_assets::{
    WorkspaceImageAsset, WorkspaceImageAssetFileWriteError, WorkspaceImageAssetMetadataRecord,
    WorkspaceImageAssetStatus, begin_workspace_image_asset_write,
    current_unix_millis as image_asset_current_unix_millis, materialize_workspace_image_asset,
    workspace_image_assets_dir, write_pending_workspace_image_asset_file,
};
use crate::{
    StartupPersistence, StartupPersistenceError, WorkspaceGraphMutationCommit,
    WorkspaceGraphRevision, WorkspaceGraphStateSnapshot,
};

const APP_ROOT_DIR_NAME: &str = ".beryl";
const WORKSPACES_DIR_NAME: &str = "workspaces";
const WORKSPACE_DATABASE_FILE_NAME: &str = "workspace.redb";
const WORKSPACE_METADATA_TABLE: TableDefinition<&str, &[u8]> =
    TableDefinition::new("workspace_metadata");
const WORKSPACE_MANIFEST_KEY: &str = "manifest";
const WORKSPACE_CONVERSATION_STATE_KEY: &str = "conversation_state";
const WORKSPACE_UI_STATE_KEY: &str = "ui_state";
const WORKSPACE_GRAPH_STATE_KEY: &str = "semantic_graph_state";
const WORKSPACE_GRAPH_REVISION_KEY: &str = "semantic_graph_revision";
const WORKSPACE_IMAGE_ASSETS_KEY: &str = "image_assets";
const WORKSPACE_RENAME_TRANSACTION_FILE_NAME: &str = "workspace-rename-transaction.json";
const DEFAULT_TOOL_ACTIVITY_PANEL_HEIGHT_PX: f32 = 112.0;
static WORKSPACE_DATABASE_LOCKS: Lazy<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BerylWorkspacePersistence {
    root_dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct WorkspaceRenameTransactionRecord {
    old_workspace_id: BerylWorkspaceId,
    new_workspace_id: BerylWorkspaceId,
    old_manifest: BerylWorkspaceManifest,
    new_manifest: BerylWorkspaceManifest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkspaceTitleUpdateMode {
    GeneratedIfUntitled,
    Manual,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceActivityPanelMode {
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WorkspaceUiState {
    tool_activity_panel_mode: WorkspaceActivityPanelMode,
    #[serde(default = "default_tool_activity_panel_height_px")]
    tool_activity_panel_height_px: f32,
}

#[derive(Deserialize)]
struct WorkspaceUiStateRecord {
    #[serde(default)]
    tool_activity_panel_mode: Option<WorkspaceActivityPanelMode>,
    #[serde(default)]
    tool_activity_panel_enabled: Option<bool>,
    #[serde(default = "default_tool_activity_panel_height_px")]
    tool_activity_panel_height_px: f32,
}

#[derive(Debug, Error)]
pub enum WorkspacePersistenceError {
    #[error("could not determine the current user's home directory")]
    MissingHomeDirectory,
    #[error("failed to create directory {path}")]
    CreateDirectory {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to inspect workspace path {path}")]
    InspectWorkspacePath {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to canonicalize workspace path {path}")]
    CanonicalizeWorkspacePath {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("workspace path {workspace_path} is outside workspace root {root_path}")]
    WorkspacePathOutsideRoot {
        workspace_path: String,
        root_path: String,
    },
    #[error("refusing to delete symlinked workspace path {path}")]
    SymlinkedWorkspacePath { path: String },
    #[error("failed to delete workspace state directory {path}")]
    DeleteWorkspace {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("workspace image asset bytes are empty")]
    EmptyWorkspaceImageAsset,
    #[error("failed to create workspace image asset directory {path}")]
    CreateWorkspaceImageAssetDirectory {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to write workspace image asset file {path}")]
    WriteWorkspaceImageAssetFile {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to read workspace image asset file {path}")]
    ReadWorkspaceImageAssetFile {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to move workspace image asset file from {from} to {to}")]
    RenameWorkspaceImageAssetFile {
        from: String,
        to: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to move workspace state directory from {from} to {to}")]
    RenameWorkspaceDirectory {
        from: String,
        to: String,
        #[source]
        source: io::Error,
    },
    #[error("workspace image asset {asset_id} is missing")]
    MissingWorkspaceImageAsset { asset_id: String },
    #[error("workspace image asset {asset_id} file is missing at {path}")]
    MissingWorkspaceImageAssetFile { asset_id: String, path: String },
    #[error("workspace image asset {asset_id} file at {path} is corrupt")]
    CorruptWorkspaceImageAssetFile { asset_id: String, path: String },
    #[error("failed to enumerate workspace storage under {path}")]
    ReadDirectory {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to read workspace rename transaction from {path}")]
    ReadWorkspaceRenameTransaction {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse workspace rename transaction from {path}")]
    ParseWorkspaceRenameTransaction {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to write workspace rename transaction to {path}")]
    WriteWorkspaceRenameTransaction {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to delete workspace rename transaction {path}")]
    DeleteWorkspaceRenameTransaction {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("workspace rename recovery is ambiguous because both {old_path} and {new_path} exist")]
    AmbiguousWorkspaceRenameRecovery { old_path: String, new_path: String },
    #[error("workspace rename recovery cannot find either {old_path} or {new_path}")]
    MissingWorkspaceRenameRecovery { old_path: String, new_path: String },
    #[error("failed to serialize {record_label}")]
    SerializeWorkspaceRecord {
        record_label: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to parse {record_label} from {path}")]
    ParseWorkspaceRecord {
        record_label: &'static str,
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to open workspace database at {path}: {detail}")]
    OpenWorkspaceDatabase { path: String, detail: String },
    #[error("failed to read {record_label} from {path}: {detail}")]
    ReadWorkspaceRecord {
        record_label: &'static str,
        path: String,
        detail: String,
    },
    #[error("failed to write {record_label} to {path}: {detail}")]
    WriteWorkspaceRecord {
        record_label: &'static str,
        path: String,
        detail: String,
    },
    #[error("workspace manifest for {workspace_id} is missing from {path}")]
    MissingWorkspaceManifest { workspace_id: String, path: String },
    #[error("semantic graph revision for workspace {workspace_id} is missing from {path}")]
    MissingWorkspaceGraphRevision { workspace_id: String, path: String },
    #[error("invalid workspace title")]
    WorkspaceTitle {
        #[source]
        source: BerylWorkspaceTitleError,
    },
    #[error("failed to update startup metadata after workspace rename")]
    StartupPersistence {
        #[source]
        source: StartupPersistenceError,
    },
    #[error("failed to apply semantic graph patch for workspace {workspace_id}")]
    ApplyWorkspaceGraphPatch {
        workspace_id: String,
        #[source]
        source: SemanticGraphError,
    },
    #[error(
        "semantic graph revision conflict for workspace {workspace_id}: expected {expected_revision}, found {actual_revision}"
    )]
    WorkspaceGraphRevisionConflict {
        workspace_id: String,
        expected_revision: WorkspaceGraphRevision,
        actual_revision: WorkspaceGraphRevision,
    },
    #[error("failed to record token usage snapshot for workspace {workspace_id}")]
    RecordThreadTokenUsageSnapshot {
        workspace_id: String,
        #[source]
        source: WorkspaceConversationStateError,
    },
}

impl WorkspaceActivityPanelMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Activity Auto",
            Self::On => "Activity On",
            Self::Off => "Activity Off",
        }
    }

    pub fn value_label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::On => "On",
            Self::Off => "Off",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::On,
            Self::On => Self::Off,
            Self::Off => Self::Auto,
        }
    }

    pub fn panel_visible(self, parent_turn_active: bool, context_compaction_active: bool) -> bool {
        match self {
            Self::Auto => parent_turn_active || context_compaction_active,
            Self::On => true,
            Self::Off => false,
        }
    }
}

impl WorkspaceUiState {
    pub fn new(
        tool_activity_panel_mode: WorkspaceActivityPanelMode,
        tool_activity_panel_height_px: f32,
    ) -> Self {
        Self {
            tool_activity_panel_mode,
            tool_activity_panel_height_px: normalize_tool_activity_panel_height_px(
                tool_activity_panel_height_px,
            ),
        }
    }

    pub fn tool_activity_panel_mode(&self) -> WorkspaceActivityPanelMode {
        self.tool_activity_panel_mode
    }

    pub fn tool_activity_panel_height_px(&self) -> f32 {
        normalize_tool_activity_panel_height_px(self.tool_activity_panel_height_px)
    }

    pub fn set_tool_activity_panel_mode(&mut self, mode: WorkspaceActivityPanelMode) -> bool {
        if self.tool_activity_panel_mode == mode {
            return false;
        }

        self.tool_activity_panel_mode = mode;
        true
    }

    pub fn set_tool_activity_panel_height_px(&mut self, height_px: f32) -> bool {
        let height_px = normalize_tool_activity_panel_height_px(height_px);
        if (self.tool_activity_panel_height_px() - height_px).abs() <= f32::EPSILON {
            return false;
        }

        self.tool_activity_panel_height_px = height_px;
        true
    }

    fn normalized(mut self) -> Self {
        self.tool_activity_panel_height_px = self.tool_activity_panel_height_px();
        self
    }
}

impl Default for WorkspaceUiState {
    fn default() -> Self {
        Self::new(
            WorkspaceActivityPanelMode::Auto,
            default_tool_activity_panel_height_px(),
        )
    }
}

impl<'de> Deserialize<'de> for WorkspaceUiState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let record = WorkspaceUiStateRecord::deserialize(deserializer)?;
        let mode = record
            .tool_activity_panel_mode
            .or_else(|| {
                record.tool_activity_panel_enabled.map(|enabled| {
                    if enabled {
                        WorkspaceActivityPanelMode::On
                    } else {
                        WorkspaceActivityPanelMode::Off
                    }
                })
            })
            .unwrap_or_default();
        Ok(Self::new(mode, record.tool_activity_panel_height_px))
    }
}

impl BerylWorkspacePersistence {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn from_environment() -> Result<Self, WorkspacePersistenceError> {
        let home = home_directory().ok_or(WorkspacePersistenceError::MissingHomeDirectory)?;
        Ok(Self::new(home.join(APP_ROOT_DIR_NAME)))
    }

    pub fn workspaces_root(&self) -> PathBuf {
        self.root_dir.join(WORKSPACES_DIR_NAME)
    }

    pub fn workspace_dir(&self, workspace_id: &BerylWorkspaceId) -> PathBuf {
        self.workspaces_root().join(workspace_id.as_str())
    }

    pub fn workspace_database_path(&self, workspace_id: &BerylWorkspaceId) -> PathBuf {
        self.workspace_dir(workspace_id)
            .join(WORKSPACE_DATABASE_FILE_NAME)
    }

    pub fn workspace_image_assets_dir(&self, workspace_id: &BerylWorkspaceId) -> PathBuf {
        workspace_image_assets_dir(&self.workspace_dir(workspace_id))
    }

    pub fn recover_interrupted_workspace_rename(
        &self,
        startup_persistence: &StartupPersistence,
    ) -> Result<bool, WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let Some(transaction) = self.load_workspace_rename_transaction()? else {
            return Ok(false);
        };

        self.recover_workspace_rename_transaction(&transaction, Some(startup_persistence))?;
        if let Err(error) = self.delete_workspace_rename_transaction() {
            warn!(
                error = %error,
                old_workspace_id = transaction.old_workspace_id.as_str(),
                new_workspace_id = transaction.new_workspace_id.as_str(),
                "workspace rename transaction was recovered but its marker could not be deleted"
            );
        }
        Ok(true)
    }

    pub fn create_workspace_image_asset(
        &self,
        workspace_id: &BerylWorkspaceId,
        format: ImageFormat,
        bytes: &[u8],
    ) -> Result<WorkspaceImageAsset, WorkspacePersistenceError> {
        self.create_workspace_image_asset_with_source(workspace_id, format, bytes, None)
    }

    pub fn import_workspace_image_asset(
        &self,
        workspace_id: &BerylWorkspaceId,
        format: ImageFormat,
        bytes: &[u8],
        source_backend_path: &str,
    ) -> Result<WorkspaceImageAsset, WorkspacePersistenceError> {
        self.create_workspace_image_asset_with_source(
            workspace_id,
            format,
            bytes,
            Some(source_backend_path.to_string()),
        )
    }

    fn create_workspace_image_asset_with_source(
        &self,
        workspace_id: &BerylWorkspaceId,
        format: ImageFormat,
        bytes: &[u8],
        source_backend_path: Option<String>,
    ) -> Result<WorkspaceImageAsset, WorkspacePersistenceError> {
        if bytes.is_empty() {
            return Err(WorkspacePersistenceError::EmptyWorkspaceImageAsset);
        }

        self.ensure_workspaces_root()?;
        let workspace_dir = self.workspace_dir(workspace_id);
        fs::create_dir_all(&workspace_dir).map_err(|source| {
            WorkspacePersistenceError::CreateDirectory {
                path: workspace_dir.display().to_string(),
                source,
            }
        })?;

        let pending = begin_workspace_image_asset_write(
            &workspace_dir,
            format,
            bytes.len() as u64,
            image_asset_current_unix_millis(),
            source_backend_path,
        );
        write_pending_workspace_image_asset_file(&pending, bytes)
            .map_err(workspace_image_asset_write_error)?;

        let database_path = self.workspace_database_path(workspace_id);
        let metadata = pending.metadata.clone();
        with_workspace_database_lock(&database_path, || {
            let mut record = if database_path.exists() {
                load_workspace_record_from_database_unlocked(
                    &database_path,
                    WORKSPACE_IMAGE_ASSETS_KEY,
                    "workspace image assets",
                )?
                .unwrap_or_default()
            } else {
                WorkspaceImageAssetMetadataRecord::default()
            };
            if record.push(metadata.clone()) {
                save_workspace_record_to_database_unlocked(
                    &database_path,
                    WORKSPACE_IMAGE_ASSETS_KEY,
                    "workspace image assets",
                    &record,
                )?;
            }
            Ok(())
        })?;

        Ok(materialize_workspace_image_asset(&workspace_dir, metadata))
    }

    pub fn load_workspace_image_assets(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<Vec<WorkspaceImageAsset>, WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let workspace_dir = self.workspace_dir(workspace_id);
        let database_path = self.workspace_database_path(workspace_id);
        if !database_path.exists() {
            return Ok(Vec::new());
        }

        let record = load_workspace_record_from_database::<WorkspaceImageAssetMetadataRecord>(
            &database_path,
            WORKSPACE_IMAGE_ASSETS_KEY,
            "workspace image assets",
        )?
        .unwrap_or_default();
        Ok(record
            .assets()
            .iter()
            .cloned()
            .map(|metadata| materialize_workspace_image_asset(&workspace_dir, metadata))
            .collect())
    }

    pub fn read_workspace_image_asset_bytes(
        &self,
        workspace_id: &BerylWorkspaceId,
        asset_id: &str,
    ) -> Result<Vec<u8>, WorkspacePersistenceError> {
        let asset = self
            .load_workspace_image_assets(workspace_id)?
            .into_iter()
            .find(|asset| asset.id() == asset_id)
            .ok_or_else(|| WorkspacePersistenceError::MissingWorkspaceImageAsset {
                asset_id: asset_id.to_string(),
            })?;
        match asset.status() {
            WorkspaceImageAssetStatus::Available => {}
            WorkspaceImageAssetStatus::MissingFile => {
                return Err(WorkspacePersistenceError::MissingWorkspaceImageAssetFile {
                    asset_id: asset_id.to_string(),
                    path: asset.file_path().display().to_string(),
                });
            }
            WorkspaceImageAssetStatus::CorruptFile => {
                return Err(WorkspacePersistenceError::CorruptWorkspaceImageAssetFile {
                    asset_id: asset_id.to_string(),
                    path: asset.file_path().display().to_string(),
                });
            }
        }

        fs::read(asset.file_path()).map_err(|source| {
            WorkspacePersistenceError::ReadWorkspaceImageAssetFile {
                path: asset.file_path().display().to_string(),
                source,
            }
        })
    }

    pub fn mark_workspace_image_asset_referenced(
        &self,
        workspace_id: &BerylWorkspaceId,
        asset_id: &str,
    ) -> Result<bool, WorkspacePersistenceError> {
        self.update_workspace_image_asset_metadata(
            workspace_id,
            "workspace image assets",
            |record| record.mark_referenced(asset_id),
        )
    }

    pub fn mark_workspace_image_asset_retained(
        &self,
        workspace_id: &BerylWorkspaceId,
        asset_id: &str,
    ) -> Result<bool, WorkspacePersistenceError> {
        let millis = image_asset_current_unix_millis();
        self.update_workspace_image_asset_metadata(
            workspace_id,
            "workspace image assets",
            |record| record.mark_retained(asset_id, millis),
        )
    }

    pub fn mark_workspace_image_asset_unreferenced(
        &self,
        workspace_id: &BerylWorkspaceId,
        asset_id: &str,
    ) -> Result<bool, WorkspacePersistenceError> {
        let millis = image_asset_current_unix_millis();
        self.update_workspace_image_asset_metadata(
            workspace_id,
            "workspace image assets",
            |record| record.mark_unreferenced(asset_id, millis),
        )
    }

    fn update_workspace_image_asset_metadata(
        &self,
        workspace_id: &BerylWorkspaceId,
        record_label: &'static str,
        update: impl FnOnce(&mut WorkspaceImageAssetMetadataRecord) -> bool,
    ) -> Result<bool, WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let database_path = self.workspace_database_path(workspace_id);
        if !database_path.exists() {
            return Ok(false);
        }

        with_workspace_database_lock(&database_path, || {
            let mut record = load_workspace_record_from_database_unlocked(
                &database_path,
                WORKSPACE_IMAGE_ASSETS_KEY,
                record_label,
            )?
            .unwrap_or_default();
            let changed = update(&mut record);
            if changed {
                save_workspace_record_to_database_unlocked(
                    &database_path,
                    WORKSPACE_IMAGE_ASSETS_KEY,
                    record_label,
                    &record,
                )?;
            }
            Ok(changed)
        })
    }

    pub fn create_untitled_workspace(
        &self,
        sequence: u64,
    ) -> Result<Option<BerylWorkspaceManifest>, WorkspacePersistenceError> {
        let manifest = BerylWorkspaceManifest::untitled(sequence, current_unix_millis());
        let workspace_dir = self.workspace_dir(manifest.id());
        if workspace_dir.exists() {
            return Ok(None);
        }

        let mut state = WorkspaceConversationState::default();
        state
            .select_runtime(RuntimeMode::HostWindows)
            .expect("host-Windows runtime selection is valid for a fresh workspace");
        self.save_initial_workspace_manifest_and_state(&manifest, &state)?;
        Ok(Some(manifest))
    }

    pub fn load_workspace_manifest(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<Option<BerylWorkspaceManifest>, WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let database_path = self.workspace_database_path(workspace_id);
        if !database_path.exists() {
            return Ok(None);
        }

        load_workspace_record_from_database(
            &database_path,
            WORKSPACE_MANIFEST_KEY,
            "workspace manifest",
        )
    }

    pub fn save_workspace_manifest(
        &self,
        manifest: &BerylWorkspaceManifest,
    ) -> Result<(), WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let workspace_dir = self.workspace_dir(manifest.id());
        fs::create_dir_all(&workspace_dir).map_err(|source| {
            WorkspacePersistenceError::CreateDirectory {
                path: workspace_dir.display().to_string(),
                source,
            }
        })?;
        save_workspace_record_to_database(
            &self.workspace_database_path(manifest.id()),
            WORKSPACE_MANIFEST_KEY,
            "workspace manifest",
            manifest,
        )?;
        if load_workspace_record_from_database::<WorkspaceConversationState>(
            &self.workspace_database_path(manifest.id()),
            WORKSPACE_CONVERSATION_STATE_KEY,
            "workspace conversation state",
        )?
        .is_none()
        {
            self.save_workspace_state(manifest.id(), &WorkspaceConversationState::default())?;
        }
        Ok(())
    }

    pub fn load_workspace_state(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<WorkspaceConversationState, WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let database_path = self.workspace_database_path(workspace_id);
        if !database_path.exists() {
            return Ok(WorkspaceConversationState::default());
        }

        Ok(load_workspace_record_from_database(
            &database_path,
            WORKSPACE_CONVERSATION_STATE_KEY,
            "workspace conversation state",
        )?
        .unwrap_or_default())
    }

    pub fn save_workspace_state(
        &self,
        workspace_id: &BerylWorkspaceId,
        state: &WorkspaceConversationState,
    ) -> Result<(), WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let workspace_dir = self.workspace_dir(workspace_id);
        fs::create_dir_all(&workspace_dir).map_err(|source| {
            WorkspacePersistenceError::CreateDirectory {
                path: workspace_dir.display().to_string(),
                source,
            }
        })?;
        save_workspace_record_to_database(
            &self.workspace_database_path(workspace_id),
            WORKSPACE_CONVERSATION_STATE_KEY,
            "workspace conversation state",
            state,
        )
    }

    fn save_initial_workspace_manifest_and_state(
        &self,
        manifest: &BerylWorkspaceManifest,
        state: &WorkspaceConversationState,
    ) -> Result<(), WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let workspace_dir = self.workspace_dir(manifest.id());
        fs::create_dir_all(&workspace_dir).map_err(|source| {
            WorkspacePersistenceError::CreateDirectory {
                path: workspace_dir.display().to_string(),
                source,
            }
        })?;
        save_workspace_manifest_and_state_to_database(
            &self.workspace_database_path(manifest.id()),
            manifest,
            state,
        )
    }

    pub fn load_workspace_ui_state(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<WorkspaceUiState, WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let database_path = self.workspace_database_path(workspace_id);
        if !database_path.exists() {
            return Ok(WorkspaceUiState::default());
        }

        Ok(load_workspace_record_from_database::<WorkspaceUiState>(
            &database_path,
            WORKSPACE_UI_STATE_KEY,
            "workspace UI state",
        )?
        .unwrap_or_default()
        .normalized())
    }

    pub fn save_workspace_ui_state(
        &self,
        workspace_id: &BerylWorkspaceId,
        state: &WorkspaceUiState,
    ) -> Result<(), WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let workspace_dir = self.workspace_dir(workspace_id);
        fs::create_dir_all(&workspace_dir).map_err(|source| {
            WorkspacePersistenceError::CreateDirectory {
                path: workspace_dir.display().to_string(),
                source,
            }
        })?;
        save_workspace_record_to_database(
            &self.workspace_database_path(workspace_id),
            WORKSPACE_UI_STATE_KEY,
            "workspace UI state",
            &state.clone().normalized(),
        )
    }

    pub fn record_thread_token_usage_snapshot(
        &self,
        workspace_id: &BerylWorkspaceId,
        thread_id: &ConversationThreadId,
        snapshot: ConversationThreadTokenUsageSnapshot,
    ) -> Result<bool, WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let workspace_dir = self.workspace_dir(workspace_id);
        fs::create_dir_all(&workspace_dir).map_err(|source| {
            WorkspacePersistenceError::CreateDirectory {
                path: workspace_dir.display().to_string(),
                source,
            }
        })?;
        let database_path = self.workspace_database_path(workspace_id);
        with_workspace_database_lock(&database_path, || {
            let database = open_or_create_workspace_database(&database_path)?;
            let write_txn = database.begin_write().map_err(|error| {
                WorkspacePersistenceError::WriteWorkspaceRecord {
                    record_label: "workspace conversation state",
                    path: database_path.display().to_string(),
                    detail: error.to_string(),
                }
            })?;
            let changed = {
                let mut table =
                    write_txn
                        .open_table(WORKSPACE_METADATA_TABLE)
                        .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                            record_label: "workspace conversation state",
                            path: database_path.display().to_string(),
                            detail: error.to_string(),
                        })?;
                let mut state = {
                    let record_bytes =
                        table
                            .get(WORKSPACE_CONVERSATION_STATE_KEY)
                            .map_err(|error| WorkspacePersistenceError::ReadWorkspaceRecord {
                                record_label: "workspace conversation state",
                                path: database_path.display().to_string(),
                                detail: error.to_string(),
                            })?;
                    match record_bytes {
                        Some(record_bytes) => serde_json::from_slice(record_bytes.value())
                            .map_err(|source| WorkspacePersistenceError::ParseWorkspaceRecord {
                                record_label: "workspace conversation state",
                                path: database_path.display().to_string(),
                                source,
                            })?,
                        None => WorkspaceConversationState::default(),
                    }
                };
                let changed = state
                    .record_thread_token_usage_snapshot(thread_id, snapshot)
                    .map_err(|source| {
                        WorkspacePersistenceError::RecordThreadTokenUsageSnapshot {
                            workspace_id: workspace_id.as_str().to_string(),
                            source,
                        }
                    })?;
                if changed {
                    let record_bytes = serde_json::to_vec(&state).map_err(|source| {
                        WorkspacePersistenceError::SerializeWorkspaceRecord {
                            record_label: "workspace conversation state",
                            source,
                        }
                    })?;
                    table
                        .insert(WORKSPACE_CONVERSATION_STATE_KEY, record_bytes.as_slice())
                        .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                            record_label: "workspace conversation state",
                            path: database_path.display().to_string(),
                            detail: error.to_string(),
                        })?;
                }
                changed
            };
            write_txn.commit().map_err(|error| {
                WorkspacePersistenceError::WriteWorkspaceRecord {
                    record_label: "workspace conversation state",
                    path: database_path.display().to_string(),
                    detail: error.to_string(),
                }
            })?;

            Ok(changed)
        })
    }

    pub fn load_workspace_graph_state(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<SemanticGraph, WorkspacePersistenceError> {
        self.load_workspace_graph_state_snapshot(workspace_id)
            .map(|snapshot| snapshot.graph)
    }

    pub fn load_workspace_graph_revision(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<WorkspaceGraphRevision, WorkspacePersistenceError> {
        self.load_workspace_graph_state_snapshot(workspace_id)
            .map(|snapshot| snapshot.revision)
    }

    pub fn load_workspace_graph_state_snapshot(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<WorkspaceGraphStateSnapshot, WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let database_path = self.workspace_database_path(workspace_id);
        if !database_path.exists() {
            return Ok(WorkspaceGraphStateSnapshot::new(
                SemanticGraph::default(),
                WorkspaceGraphRevision::default(),
            ));
        }

        load_workspace_graph_state_snapshot_from_database(&database_path, workspace_id)
    }

    pub fn save_workspace_graph_state(
        &self,
        workspace_id: &BerylWorkspaceId,
        graph: &SemanticGraph,
    ) -> Result<(), WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let workspace_dir = self.workspace_dir(workspace_id);
        fs::create_dir_all(&workspace_dir).map_err(|source| {
            WorkspacePersistenceError::CreateDirectory {
                path: workspace_dir.display().to_string(),
                source,
            }
        })?;
        let database_path = self.workspace_database_path(workspace_id);
        let revision = if database_path.exists() {
            self.load_workspace_graph_revision(workspace_id)?
        } else {
            WorkspaceGraphRevision::default()
        };
        save_graph_and_revision_to_database(&database_path, graph, revision)
    }

    pub fn apply_workspace_graph_patch(
        &self,
        workspace_id: &BerylWorkspaceId,
        patch: &SemanticGraphPatch,
        expected_base_revision: Option<WorkspaceGraphRevision>,
    ) -> Result<WorkspaceGraphMutationCommit, WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let database_path = self.workspace_database_path(workspace_id);
        with_workspace_database_lock(&database_path, || {
            let (mut manifest, mut graph, base_revision) =
                load_workspace_graph_mutation_state_unlocked(&database_path, workspace_id)?;
            if let Some(expected_base_revision) = expected_base_revision
                && expected_base_revision != base_revision
            {
                return Err(WorkspacePersistenceError::WorkspaceGraphRevisionConflict {
                    workspace_id: workspace_id.as_str().to_string(),
                    expected_revision: expected_base_revision,
                    actual_revision: base_revision,
                });
            }

            let changed = graph.apply_patch(patch).map_err(|source| {
                WorkspacePersistenceError::ApplyWorkspaceGraphPatch {
                    workspace_id: workspace_id.as_str().to_string(),
                    source,
                }
            })?;
            if changed {
                manifest.set_last_updated_at_millis(current_unix_millis());
            }

            let committed_revision = base_revision.next();
            save_manifest_graph_revision_to_database_unlocked(
                &database_path,
                &manifest,
                &graph,
                committed_revision,
            )?;

            Ok(WorkspaceGraphMutationCommit::new(
                workspace_id.clone(),
                base_revision,
                committed_revision,
                changed,
                patch.clone(),
                manifest,
            ))
        })
    }

    pub fn touch_workspace_manifest(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<BerylWorkspaceManifest, WorkspacePersistenceError> {
        let database_path = self.workspace_database_path(workspace_id);
        let mut manifest = self.load_workspace_manifest(workspace_id)?.ok_or_else(|| {
            WorkspacePersistenceError::MissingWorkspaceManifest {
                workspace_id: workspace_id.as_str().to_string(),
                path: database_path.display().to_string(),
            }
        })?;
        manifest.set_last_updated_at_millis(current_unix_millis());
        self.save_workspace_manifest(&manifest)?;
        Ok(manifest)
    }

    pub fn set_workspace_generated_title_if_untitled(
        &self,
        workspace_id: &BerylWorkspaceId,
        title: impl Into<String>,
    ) -> Result<Option<BerylWorkspaceManifest>, WorkspacePersistenceError> {
        self.update_workspace_title(
            workspace_id,
            title.into(),
            WorkspaceTitleUpdateMode::GeneratedIfUntitled,
        )
    }

    pub fn set_workspace_manual_title(
        &self,
        workspace_id: &BerylWorkspaceId,
        title: impl Into<String>,
    ) -> Result<Option<BerylWorkspaceManifest>, WorkspacePersistenceError> {
        self.update_workspace_title(workspace_id, title.into(), WorkspaceTitleUpdateMode::Manual)
    }

    fn update_workspace_title(
        &self,
        workspace_id: &BerylWorkspaceId,
        title: String,
        mode: WorkspaceTitleUpdateMode,
    ) -> Result<Option<BerylWorkspaceManifest>, WorkspacePersistenceError> {
        let startup_persistence = StartupPersistence::new(self.root_dir.clone());
        self.recover_interrupted_workspace_rename(&startup_persistence)?;

        let database_path = self.workspace_database_path(workspace_id);
        let mut manifest = self.load_workspace_manifest(workspace_id)?.ok_or_else(|| {
            WorkspacePersistenceError::MissingWorkspaceManifest {
                workspace_id: workspace_id.as_str().to_string(),
                path: database_path.display().to_string(),
            }
        })?;
        let old_manifest = manifest.clone();
        let changed = match mode {
            WorkspaceTitleUpdateMode::GeneratedIfUntitled => manifest
                .set_generated_title_if_untitled(title)
                .map_err(|source| WorkspacePersistenceError::WorkspaceTitle { source })?,
            WorkspaceTitleUpdateMode::Manual => manifest
                .set_manual_title(title)
                .map_err(|source| WorkspacePersistenceError::WorkspaceTitle { source })?,
        };
        if !changed {
            return Ok(None);
        }

        manifest.set_last_updated_at_millis(current_unix_millis());
        self.commit_workspace_manifest_id_change(
            workspace_id,
            old_manifest,
            manifest.clone(),
            &startup_persistence,
        )?;
        Ok(Some(manifest))
    }

    fn commit_workspace_manifest_id_change(
        &self,
        old_workspace_id: &BerylWorkspaceId,
        old_manifest: BerylWorkspaceManifest,
        new_manifest: BerylWorkspaceManifest,
        startup_persistence: &StartupPersistence,
    ) -> Result<(), WorkspacePersistenceError> {
        let new_workspace_id = new_manifest.id().clone();
        if old_workspace_id == &new_workspace_id {
            save_workspace_record_to_database(
                &self.workspace_database_path(old_workspace_id),
                WORKSPACE_MANIFEST_KEY,
                "workspace manifest",
                &new_manifest,
            )?;
            self.rewrite_startup_workspace_id(
                startup_persistence,
                old_workspace_id,
                &new_workspace_id,
            )?;
            return Ok(());
        }

        self.ensure_workspace_slug_available(old_workspace_id, &new_workspace_id)?;
        let transaction = WorkspaceRenameTransactionRecord {
            old_workspace_id: old_workspace_id.clone(),
            new_workspace_id: new_workspace_id.clone(),
            old_manifest,
            new_manifest,
        };
        self.write_workspace_rename_transaction(&transaction)?;

        match self.apply_workspace_rename_transaction(&transaction, startup_persistence) {
            Ok(()) => {
                if let Err(error) = self.delete_workspace_rename_transaction() {
                    warn!(
                        error = %error,
                        old_workspace_id = transaction.old_workspace_id.as_str(),
                        new_workspace_id = transaction.new_workspace_id.as_str(),
                        "workspace rename transaction committed but its marker could not be deleted"
                    );
                }
                Ok(())
            }
            Err(error) => {
                if self
                    .rollback_workspace_rename_transaction(&transaction)
                    .is_ok()
                {
                    let _ = self.delete_workspace_rename_transaction();
                }
                Err(error)
            }
        }
    }

    pub fn delete_workspace(
        &self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<bool, WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let workspace_dir = self.workspace_dir(workspace_id);
        if !workspace_dir.exists() {
            return Ok(false);
        }

        let metadata = fs::symlink_metadata(&workspace_dir).map_err(|source| {
            WorkspacePersistenceError::InspectWorkspacePath {
                path: workspace_dir.display().to_string(),
                source,
            }
        })?;
        if metadata.file_type().is_symlink() {
            return Err(WorkspacePersistenceError::SymlinkedWorkspacePath {
                path: workspace_dir.display().to_string(),
            });
        }

        let root = self.workspaces_root();
        let canonical_root = fs::canonicalize(&root).map_err(|source| {
            WorkspacePersistenceError::CanonicalizeWorkspacePath {
                path: root.display().to_string(),
                source,
            }
        })?;
        let canonical_workspace = fs::canonicalize(&workspace_dir).map_err(|source| {
            WorkspacePersistenceError::CanonicalizeWorkspacePath {
                path: workspace_dir.display().to_string(),
                source,
            }
        })?;
        if !canonical_workspace.starts_with(&canonical_root) {
            return Err(WorkspacePersistenceError::WorkspacePathOutsideRoot {
                workspace_path: canonical_workspace.display().to_string(),
                root_path: canonical_root.display().to_string(),
            });
        }

        fs::remove_dir_all(&workspace_dir).map_err(|source| {
            WorkspacePersistenceError::DeleteWorkspace {
                path: workspace_dir.display().to_string(),
                source,
            }
        })?;
        Ok(true)
    }

    pub fn list_workspace_manifests(
        &self,
    ) -> Result<Vec<BerylWorkspaceManifest>, WorkspacePersistenceError> {
        self.ensure_workspaces_root()?;
        let mut manifests: Vec<BerylWorkspaceManifest> = Vec::new();

        let root = self.workspaces_root();
        let entries =
            fs::read_dir(&root).map_err(|source| WorkspacePersistenceError::ReadDirectory {
                path: root.display().to_string(),
                source,
            })?;

        for entry in entries {
            let entry = entry.map_err(|source| WorkspacePersistenceError::ReadDirectory {
                path: root.display().to_string(),
                source,
            })?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let database_path = path.join(WORKSPACE_DATABASE_FILE_NAME);
            if let Some(manifest) = load_workspace_record_from_database(
                &database_path,
                WORKSPACE_MANIFEST_KEY,
                "workspace manifest",
            )? {
                manifests.push(manifest);
            }
        }

        manifests.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
        Ok(manifests)
    }

    fn ensure_workspace_slug_available(
        &self,
        old_workspace_id: &BerylWorkspaceId,
        new_workspace_id: &BerylWorkspaceId,
    ) -> Result<(), WorkspacePersistenceError> {
        let root = self.workspaces_root();
        let entries =
            fs::read_dir(&root).map_err(|source| WorkspacePersistenceError::ReadDirectory {
                path: root.display().to_string(),
                source,
            })?;

        for entry in entries {
            let entry = entry.map_err(|source| WorkspacePersistenceError::ReadDirectory {
                path: root.display().to_string(),
                source,
            })?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let Some(directory_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if directory_name.eq_ignore_ascii_case(old_workspace_id.as_str()) {
                continue;
            }
            if directory_name.eq_ignore_ascii_case(new_workspace_id.as_str()) {
                return Err(workspace_slug_collision_error(new_workspace_id));
            }

            let database_path = path.join(WORKSPACE_DATABASE_FILE_NAME);
            if let Some(manifest) = load_workspace_record_from_database::<BerylWorkspaceManifest>(
                &database_path,
                WORKSPACE_MANIFEST_KEY,
                "workspace manifest",
            )? && workspace_manifest_conflicts_with_slug(&manifest, new_workspace_id)
            {
                return Err(workspace_slug_collision_error(new_workspace_id));
            }
        }

        Ok(())
    }

    fn apply_workspace_rename_transaction(
        &self,
        transaction: &WorkspaceRenameTransactionRecord,
        startup_persistence: &StartupPersistence,
    ) -> Result<(), WorkspacePersistenceError> {
        let old_dir = self.workspace_dir(&transaction.old_workspace_id);
        let new_dir = self.workspace_dir(&transaction.new_workspace_id);
        if new_dir.exists() {
            return Err(workspace_slug_collision_error(
                &transaction.new_workspace_id,
            ));
        }
        fs::rename(&old_dir, &new_dir).map_err(|source| {
            WorkspacePersistenceError::RenameWorkspaceDirectory {
                from: old_dir.display().to_string(),
                to: new_dir.display().to_string(),
                source,
            }
        })?;

        save_workspace_record_to_database(
            &self.workspace_database_path(&transaction.new_workspace_id),
            WORKSPACE_MANIFEST_KEY,
            "workspace manifest",
            &transaction.new_manifest,
        )?;
        self.rewrite_startup_workspace_id(
            startup_persistence,
            &transaction.old_workspace_id,
            &transaction.new_workspace_id,
        )
    }

    fn rollback_workspace_rename_transaction(
        &self,
        transaction: &WorkspaceRenameTransactionRecord,
    ) -> Result<(), WorkspacePersistenceError> {
        let old_dir = self.workspace_dir(&transaction.old_workspace_id);
        let new_dir = self.workspace_dir(&transaction.new_workspace_id);
        if new_dir.exists() && !old_dir.exists() {
            save_workspace_record_to_database(
                &self.workspace_database_path(&transaction.new_workspace_id),
                WORKSPACE_MANIFEST_KEY,
                "workspace manifest",
                &transaction.old_manifest,
            )?;
            fs::rename(&new_dir, &old_dir).map_err(|source| {
                WorkspacePersistenceError::RenameWorkspaceDirectory {
                    from: new_dir.display().to_string(),
                    to: old_dir.display().to_string(),
                    source,
                }
            })?;
        }
        Ok(())
    }

    fn recover_workspace_rename_transaction(
        &self,
        transaction: &WorkspaceRenameTransactionRecord,
        startup_persistence: Option<&StartupPersistence>,
    ) -> Result<(), WorkspacePersistenceError> {
        let old_dir = self.workspace_dir(&transaction.old_workspace_id);
        let new_dir = self.workspace_dir(&transaction.new_workspace_id);
        match (old_dir.exists(), new_dir.exists()) {
            (true, false) => Ok(()),
            (false, true) => {
                let existing_manifest =
                    load_workspace_record_from_database::<BerylWorkspaceManifest>(
                        &self.workspace_database_path(&transaction.new_workspace_id),
                        WORKSPACE_MANIFEST_KEY,
                        "workspace manifest",
                    )?;
                if !matches!(
                    existing_manifest.as_ref(),
                    Some(manifest) if manifest.id() == &transaction.new_workspace_id
                ) {
                    save_workspace_record_to_database(
                        &self.workspace_database_path(&transaction.new_workspace_id),
                        WORKSPACE_MANIFEST_KEY,
                        "workspace manifest",
                        &transaction.new_manifest,
                    )?;
                }
                if let Some(startup_persistence) = startup_persistence {
                    self.rewrite_startup_workspace_id(
                        startup_persistence,
                        &transaction.old_workspace_id,
                        &transaction.new_workspace_id,
                    )?;
                }
                Ok(())
            }
            (false, false) => Err(WorkspacePersistenceError::MissingWorkspaceRenameRecovery {
                old_path: old_dir.display().to_string(),
                new_path: new_dir.display().to_string(),
            }),
            (true, true) => Err(
                WorkspacePersistenceError::AmbiguousWorkspaceRenameRecovery {
                    old_path: old_dir.display().to_string(),
                    new_path: new_dir.display().to_string(),
                },
            ),
        }
    }

    fn rewrite_startup_workspace_id(
        &self,
        startup_persistence: &StartupPersistence,
        old_workspace_id: &BerylWorkspaceId,
        new_workspace_id: &BerylWorkspaceId,
    ) -> Result<(), WorkspacePersistenceError> {
        let mut metadata = startup_persistence
            .load()
            .map_err(|source| WorkspacePersistenceError::StartupPersistence { source })?;
        metadata.replace_workspace(old_workspace_id, new_workspace_id.clone());
        startup_persistence
            .save(&metadata)
            .map_err(|source| WorkspacePersistenceError::StartupPersistence { source })
    }

    fn workspace_rename_transaction_path(&self) -> PathBuf {
        self.root_dir.join(WORKSPACE_RENAME_TRANSACTION_FILE_NAME)
    }

    fn load_workspace_rename_transaction(
        &self,
    ) -> Result<Option<WorkspaceRenameTransactionRecord>, WorkspacePersistenceError> {
        let path = self.workspace_rename_transaction_path();
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&path).map_err(|source| {
            WorkspacePersistenceError::ReadWorkspaceRenameTransaction {
                path: path.display().to_string(),
                source,
            }
        })?;
        serde_json::from_str(&contents).map(Some).map_err(|source| {
            WorkspacePersistenceError::ParseWorkspaceRenameTransaction {
                path: path.display().to_string(),
                source,
            }
        })
    }

    fn write_workspace_rename_transaction(
        &self,
        transaction: &WorkspaceRenameTransactionRecord,
    ) -> Result<(), WorkspacePersistenceError> {
        fs::create_dir_all(&self.root_dir).map_err(|source| {
            WorkspacePersistenceError::CreateDirectory {
                path: self.root_dir.display().to_string(),
                source,
            }
        })?;
        let path = self.workspace_rename_transaction_path();
        let temp_path = path.with_extension("json.tmp");
        let contents = serde_json::to_vec_pretty(transaction).map_err(|source| {
            WorkspacePersistenceError::SerializeWorkspaceRecord {
                record_label: "workspace rename transaction",
                source,
            }
        })?;
        fs::write(&temp_path, contents).map_err(|source| {
            WorkspacePersistenceError::WriteWorkspaceRenameTransaction {
                path: temp_path.display().to_string(),
                source,
            }
        })?;
        if path.exists() {
            fs::remove_file(&path).map_err(|source| {
                WorkspacePersistenceError::WriteWorkspaceRenameTransaction {
                    path: path.display().to_string(),
                    source,
                }
            })?;
        }
        fs::rename(&temp_path, &path).map_err(|source| {
            WorkspacePersistenceError::WriteWorkspaceRenameTransaction {
                path: path.display().to_string(),
                source,
            }
        })
    }

    fn delete_workspace_rename_transaction(&self) -> Result<(), WorkspacePersistenceError> {
        let path = self.workspace_rename_transaction_path();
        if !path.exists() {
            return Ok(());
        }

        fs::remove_file(&path).map_err(|source| {
            WorkspacePersistenceError::DeleteWorkspaceRenameTransaction {
                path: path.display().to_string(),
                source,
            }
        })
    }

    fn ensure_workspaces_root(&self) -> Result<(), WorkspacePersistenceError> {
        fs::create_dir_all(self.workspaces_root()).map_err(|source| {
            WorkspacePersistenceError::CreateDirectory {
                path: self.workspaces_root().display().to_string(),
                source,
            }
        })
    }
}

fn home_directory() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn default_tool_activity_panel_height_px() -> f32 {
    DEFAULT_TOOL_ACTIVITY_PANEL_HEIGHT_PX
}

fn workspace_image_asset_write_error(
    error: WorkspaceImageAssetFileWriteError,
) -> WorkspacePersistenceError {
    match error {
        WorkspaceImageAssetFileWriteError::CreateDirectory { path, source } => {
            WorkspacePersistenceError::CreateWorkspaceImageAssetDirectory { path, source }
        }
        WorkspaceImageAssetFileWriteError::Write { path, source } => {
            WorkspacePersistenceError::WriteWorkspaceImageAssetFile { path, source }
        }
        WorkspaceImageAssetFileWriteError::Rename { from, to, source } => {
            WorkspacePersistenceError::RenameWorkspaceImageAssetFile { from, to, source }
        }
    }
}

fn normalize_tool_activity_panel_height_px(height_px: f32) -> f32 {
    if height_px.is_finite() && height_px > 0.0 {
        height_px
    } else {
        default_tool_activity_panel_height_px()
    }
}

fn workspace_slug_collision_error(slug: &BerylWorkspaceId) -> WorkspacePersistenceError {
    WorkspacePersistenceError::WorkspaceTitle {
        source: BerylWorkspaceTitleError::SlugEquivalentCollision { slug: slug.clone() },
    }
}

fn workspace_manifest_conflicts_with_slug(
    manifest: &BerylWorkspaceManifest,
    slug: &BerylWorkspaceId,
) -> bool {
    manifest.id() == slug
        || derive_workspace_slug(manifest.title())
            .map(|title_slug| &title_slug == slug)
            .unwrap_or(false)
}

fn load_workspace_record_from_database<T: DeserializeOwned>(
    database_path: &Path,
    key: &str,
    record_label: &'static str,
) -> Result<Option<T>, WorkspacePersistenceError> {
    if !database_path.exists() {
        return Ok(None);
    }

    with_workspace_database_lock(database_path, || {
        load_workspace_record_from_database_unlocked(database_path, key, record_label)
    })
}

fn load_workspace_record_from_database_unlocked<T: DeserializeOwned>(
    database_path: &Path,
    key: &str,
    record_label: &'static str,
) -> Result<Option<T>, WorkspacePersistenceError> {
    let database = Database::open(database_path).map_err(|error| {
        WorkspacePersistenceError::OpenWorkspaceDatabase {
            path: database_path.display().to_string(),
            detail: error.to_string(),
        }
    })?;
    let read_txn =
        database
            .begin_read()
            .map_err(|error| WorkspacePersistenceError::ReadWorkspaceRecord {
                record_label,
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;
    let table = read_txn
        .open_table(WORKSPACE_METADATA_TABLE)
        .map_err(|error| WorkspacePersistenceError::ReadWorkspaceRecord {
            record_label,
            path: database_path.display().to_string(),
            detail: error.to_string(),
        })?;
    let record_bytes =
        table
            .get(key)
            .map_err(|error| WorkspacePersistenceError::ReadWorkspaceRecord {
                record_label,
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;
    let Some(record_bytes) = record_bytes else {
        return Ok(None);
    };

    serde_json::from_slice(record_bytes.value())
        .map(Some)
        .map_err(|source| WorkspacePersistenceError::ParseWorkspaceRecord {
            record_label,
            path: database_path.display().to_string(),
            source,
        })
}

fn load_workspace_graph_state_snapshot_from_database(
    database_path: &Path,
    workspace_id: &BerylWorkspaceId,
) -> Result<WorkspaceGraphStateSnapshot, WorkspacePersistenceError> {
    with_workspace_database_lock(database_path, || {
        load_workspace_graph_state_snapshot_from_database_unlocked(database_path, workspace_id)
    })
}

fn load_workspace_graph_state_snapshot_from_database_unlocked(
    database_path: &Path,
    workspace_id: &BerylWorkspaceId,
) -> Result<WorkspaceGraphStateSnapshot, WorkspacePersistenceError> {
    let database = Database::open(database_path).map_err(|error| {
        WorkspacePersistenceError::OpenWorkspaceDatabase {
            path: database_path.display().to_string(),
            detail: error.to_string(),
        }
    })?;
    let read_txn =
        database
            .begin_read()
            .map_err(|error| WorkspacePersistenceError::ReadWorkspaceRecord {
                record_label: "workspace semantic graph",
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;
    let table = read_txn
        .open_table(WORKSPACE_METADATA_TABLE)
        .map_err(|error| WorkspacePersistenceError::ReadWorkspaceRecord {
            record_label: "workspace semantic graph",
            path: database_path.display().to_string(),
            detail: error.to_string(),
        })?;
    let graph = read_optional_table_record(
        &table,
        database_path,
        WORKSPACE_GRAPH_STATE_KEY,
        "workspace semantic graph",
    )?;
    let revision = read_optional_table_record(
        &table,
        database_path,
        WORKSPACE_GRAPH_REVISION_KEY,
        "workspace semantic graph revision",
    )?;

    workspace_graph_snapshot_from_optional_records(database_path, workspace_id, graph, revision)
}

fn save_workspace_record_to_database<T: Serialize>(
    database_path: &Path,
    key: &str,
    record_label: &'static str,
    value: &T,
) -> Result<(), WorkspacePersistenceError> {
    with_workspace_database_lock(database_path, || {
        save_workspace_record_to_database_unlocked(database_path, key, record_label, value)
    })
}

fn save_workspace_record_to_database_unlocked<T: Serialize>(
    database_path: &Path,
    key: &str,
    record_label: &'static str,
    value: &T,
) -> Result<(), WorkspacePersistenceError> {
    let database = open_or_create_workspace_database(database_path)?;
    let record_bytes = serde_json::to_vec(value).map_err(|source| {
        WorkspacePersistenceError::SerializeWorkspaceRecord {
            record_label,
            source,
        }
    })?;
    let write_txn = database.begin_write().map_err(|error| {
        WorkspacePersistenceError::WriteWorkspaceRecord {
            record_label,
            path: database_path.display().to_string(),
            detail: error.to_string(),
        }
    })?;
    {
        let mut table = write_txn
            .open_table(WORKSPACE_METADATA_TABLE)
            .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                record_label,
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;
        table
            .insert(key, record_bytes.as_slice())
            .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                record_label,
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;
    }
    write_txn
        .commit()
        .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
            record_label,
            path: database_path.display().to_string(),
            detail: error.to_string(),
        })?;

    Ok(())
}

fn save_workspace_manifest_and_state_to_database(
    database_path: &Path,
    manifest: &BerylWorkspaceManifest,
    state: &WorkspaceConversationState,
) -> Result<(), WorkspacePersistenceError> {
    with_workspace_database_lock(database_path, || {
        let database = open_or_create_workspace_database(database_path)?;
        let manifest_bytes = serde_json::to_vec(manifest).map_err(|source| {
            WorkspacePersistenceError::SerializeWorkspaceRecord {
                record_label: "workspace manifest",
                source,
            }
        })?;
        let state_bytes = serde_json::to_vec(state).map_err(|source| {
            WorkspacePersistenceError::SerializeWorkspaceRecord {
                record_label: "workspace conversation state",
                source,
            }
        })?;
        let write_txn = database.begin_write().map_err(|error| {
            WorkspacePersistenceError::WriteWorkspaceRecord {
                record_label: "workspace manifest",
                path: database_path.display().to_string(),
                detail: error.to_string(),
            }
        })?;
        {
            let mut table = write_txn
                .open_table(WORKSPACE_METADATA_TABLE)
                .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                    record_label: "workspace manifest",
                    path: database_path.display().to_string(),
                    detail: error.to_string(),
                })?;
            table
                .insert(WORKSPACE_MANIFEST_KEY, manifest_bytes.as_slice())
                .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                    record_label: "workspace manifest",
                    path: database_path.display().to_string(),
                    detail: error.to_string(),
                })?;
            table
                .insert(WORKSPACE_CONVERSATION_STATE_KEY, state_bytes.as_slice())
                .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                    record_label: "workspace conversation state",
                    path: database_path.display().to_string(),
                    detail: error.to_string(),
                })?;
        }
        write_txn
            .commit()
            .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                record_label: "workspace manifest",
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;

        Ok(())
    })
}

fn read_optional_table_record<T: DeserializeOwned>(
    table: &redb::ReadOnlyTable<&str, &[u8]>,
    database_path: &Path,
    key: &str,
    record_label: &'static str,
) -> Result<Option<T>, WorkspacePersistenceError> {
    let record_bytes =
        table
            .get(key)
            .map_err(|error| WorkspacePersistenceError::ReadWorkspaceRecord {
                record_label,
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;
    let Some(record_bytes) = record_bytes else {
        return Ok(None);
    };

    serde_json::from_slice(record_bytes.value())
        .map(Some)
        .map_err(|source| WorkspacePersistenceError::ParseWorkspaceRecord {
            record_label,
            path: database_path.display().to_string(),
            source,
        })
}

fn load_workspace_graph_mutation_state_unlocked(
    database_path: &Path,
    workspace_id: &BerylWorkspaceId,
) -> Result<
    (
        BerylWorkspaceManifest,
        SemanticGraph,
        WorkspaceGraphRevision,
    ),
    WorkspacePersistenceError,
> {
    if !database_path.exists() {
        return Err(WorkspacePersistenceError::MissingWorkspaceManifest {
            workspace_id: workspace_id.as_str().to_string(),
            path: database_path.display().to_string(),
        });
    }

    let database = Database::open(database_path).map_err(|error| {
        WorkspacePersistenceError::OpenWorkspaceDatabase {
            path: database_path.display().to_string(),
            detail: error.to_string(),
        }
    })?;
    let read_txn =
        database
            .begin_read()
            .map_err(|error| WorkspacePersistenceError::ReadWorkspaceRecord {
                record_label: "workspace semantic graph",
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;
    let table = read_txn
        .open_table(WORKSPACE_METADATA_TABLE)
        .map_err(|error| WorkspacePersistenceError::ReadWorkspaceRecord {
            record_label: "workspace semantic graph",
            path: database_path.display().to_string(),
            detail: error.to_string(),
        })?;
    let manifest = read_optional_table_record(
        &table,
        database_path,
        WORKSPACE_MANIFEST_KEY,
        "workspace manifest",
    )?
    .ok_or_else(|| WorkspacePersistenceError::MissingWorkspaceManifest {
        workspace_id: workspace_id.as_str().to_string(),
        path: database_path.display().to_string(),
    })?;
    let graph = read_optional_table_record(
        &table,
        database_path,
        WORKSPACE_GRAPH_STATE_KEY,
        "workspace semantic graph",
    )?;
    let revision = read_optional_table_record(
        &table,
        database_path,
        WORKSPACE_GRAPH_REVISION_KEY,
        "workspace semantic graph revision",
    )?;
    let snapshot = workspace_graph_snapshot_from_optional_records(
        database_path,
        workspace_id,
        graph,
        revision,
    )?;

    Ok((manifest, snapshot.graph, snapshot.revision))
}

fn workspace_graph_snapshot_from_optional_records(
    database_path: &Path,
    workspace_id: &BerylWorkspaceId,
    graph: Option<SemanticGraph>,
    revision: Option<WorkspaceGraphRevision>,
) -> Result<WorkspaceGraphStateSnapshot, WorkspacePersistenceError> {
    match (graph, revision) {
        (Some(graph), Some(revision)) => Ok(WorkspaceGraphStateSnapshot::new(graph, revision)),
        (Some(_), None) => Err(WorkspacePersistenceError::MissingWorkspaceGraphRevision {
            workspace_id: workspace_id.as_str().to_string(),
            path: database_path.display().to_string(),
        }),
        (None, Some(revision)) => Ok(WorkspaceGraphStateSnapshot::new(
            SemanticGraph::default(),
            revision,
        )),
        (None, None) => Ok(WorkspaceGraphStateSnapshot::new(
            SemanticGraph::default(),
            WorkspaceGraphRevision::default(),
        )),
    }
}

fn save_graph_and_revision_to_database(
    database_path: &Path,
    graph: &SemanticGraph,
    revision: WorkspaceGraphRevision,
) -> Result<(), WorkspacePersistenceError> {
    with_workspace_database_lock(database_path, || {
        let database = open_or_create_workspace_database(database_path)?;
        let graph_bytes = serde_json::to_vec(graph).map_err(|source| {
            WorkspacePersistenceError::SerializeWorkspaceRecord {
                record_label: "workspace semantic graph",
                source,
            }
        })?;
        let revision_bytes = serde_json::to_vec(&revision).map_err(|source| {
            WorkspacePersistenceError::SerializeWorkspaceRecord {
                record_label: "workspace semantic graph revision",
                source,
            }
        })?;
        let write_txn = database.begin_write().map_err(|error| {
            WorkspacePersistenceError::WriteWorkspaceRecord {
                record_label: "workspace semantic graph",
                path: database_path.display().to_string(),
                detail: error.to_string(),
            }
        })?;
        {
            let mut table = write_txn
                .open_table(WORKSPACE_METADATA_TABLE)
                .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                    record_label: "workspace semantic graph",
                    path: database_path.display().to_string(),
                    detail: error.to_string(),
                })?;
            table
                .insert(WORKSPACE_GRAPH_STATE_KEY, graph_bytes.as_slice())
                .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                    record_label: "workspace semantic graph",
                    path: database_path.display().to_string(),
                    detail: error.to_string(),
                })?;
            table
                .insert(WORKSPACE_GRAPH_REVISION_KEY, revision_bytes.as_slice())
                .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                    record_label: "workspace semantic graph revision",
                    path: database_path.display().to_string(),
                    detail: error.to_string(),
                })?;
        }
        write_txn
            .commit()
            .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                record_label: "workspace semantic graph",
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;

        Ok(())
    })
}

fn save_manifest_graph_revision_to_database_unlocked(
    database_path: &Path,
    manifest: &BerylWorkspaceManifest,
    graph: &SemanticGraph,
    revision: WorkspaceGraphRevision,
) -> Result<(), WorkspacePersistenceError> {
    let database = open_or_create_workspace_database(database_path)?;
    let manifest_bytes = serde_json::to_vec(manifest).map_err(|source| {
        WorkspacePersistenceError::SerializeWorkspaceRecord {
            record_label: "workspace manifest",
            source,
        }
    })?;
    let graph_bytes = serde_json::to_vec(graph).map_err(|source| {
        WorkspacePersistenceError::SerializeWorkspaceRecord {
            record_label: "workspace semantic graph",
            source,
        }
    })?;
    let revision_bytes = serde_json::to_vec(&revision).map_err(|source| {
        WorkspacePersistenceError::SerializeWorkspaceRecord {
            record_label: "workspace semantic graph revision",
            source,
        }
    })?;
    let write_txn = database.begin_write().map_err(|error| {
        WorkspacePersistenceError::WriteWorkspaceRecord {
            record_label: "workspace semantic graph",
            path: database_path.display().to_string(),
            detail: error.to_string(),
        }
    })?;
    {
        let mut table = write_txn
            .open_table(WORKSPACE_METADATA_TABLE)
            .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                record_label: "workspace semantic graph",
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;
        table
            .insert(WORKSPACE_MANIFEST_KEY, manifest_bytes.as_slice())
            .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                record_label: "workspace manifest",
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;
        table
            .insert(WORKSPACE_GRAPH_STATE_KEY, graph_bytes.as_slice())
            .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                record_label: "workspace semantic graph",
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;
        table
            .insert(WORKSPACE_GRAPH_REVISION_KEY, revision_bytes.as_slice())
            .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
                record_label: "workspace semantic graph revision",
                path: database_path.display().to_string(),
                detail: error.to_string(),
            })?;
    }
    write_txn
        .commit()
        .map_err(|error| WorkspacePersistenceError::WriteWorkspaceRecord {
            record_label: "workspace semantic graph",
            path: database_path.display().to_string(),
            detail: error.to_string(),
        })?;

    Ok(())
}

fn open_or_create_workspace_database(
    database_path: &Path,
) -> Result<Database, WorkspacePersistenceError> {
    if database_path.exists() {
        Database::open(database_path)
    } else {
        Database::create(database_path)
    }
    .map_err(|error| WorkspacePersistenceError::OpenWorkspaceDatabase {
        path: database_path.display().to_string(),
        detail: error.to_string(),
    })
}

fn with_workspace_database_lock<T>(
    database_path: &Path,
    operation: impl FnOnce() -> Result<T, WorkspacePersistenceError>,
) -> Result<T, WorkspacePersistenceError> {
    let lock = workspace_database_lock(database_path);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    operation()
}

fn workspace_database_lock(database_path: &Path) -> Arc<Mutex<()>> {
    let mut locks = WORKSPACE_DATABASE_LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks
        .entry(database_path.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}
