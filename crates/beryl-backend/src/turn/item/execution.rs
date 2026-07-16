use std::path::PathBuf;

use beryl_model::CasItemId;
use serde::Deserialize;
use serde_json::Value;

use crate::DynamicToolCallOutputContentItem;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandExecutionSource {
    #[default]
    Agent,
    UserShell,
    UnifiedExecStartup,
    UnifiedExecInteraction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandExecutionStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
}

impl CommandExecutionStatus {
    pub(crate) const fn as_wire_str(self) -> &'static str {
        match self {
            Self::InProgress => "inProgress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Declined => "declined",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CommandAction {
    Read {
        command: String,
        name: String,
        path: PathBuf,
    },
    ListFiles {
        command: String,
        path: Option<String>,
    },
    Search {
        command: String,
        query: Option<String>,
        path: Option<String>,
    },
    Unknown {
        command: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandExecutionItem {
    pub id: CasItemId,
    pub command: String,
    pub cwd: String,
    pub process_id: Option<String>,
    #[serde(default)]
    pub source: CommandExecutionSource,
    pub status: CommandExecutionStatus,
    pub command_actions: Vec<CommandAction>,
    pub aggregated_output: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PatchApplyStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
}

impl PatchApplyStatus {
    pub(crate) const fn as_wire_str(self) -> &'static str {
        match self {
            Self::InProgress => "inProgress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Declined => "declined",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PatchChangeKind {
    Add,
    Delete,
    Update {
        #[serde(default)]
        move_path: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileUpdateChange {
    pub path: String,
    pub diff: String,
    pub kind: PatchChangeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangeItem {
    pub id: CasItemId,
    pub status: PatchApplyStatus,
    pub changes: Vec<FileUpdateChange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

impl McpToolCallStatus {
    pub(crate) const fn as_wire_str(self) -> &'static str {
        match self {
            Self::InProgress => "inProgress",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallAppContext {
    pub connector_id: String,
    pub link_id: Option<String>,
    pub resource_uri: Option<String>,
    pub app_name: Option<String>,
    pub template_id: Option<String>,
    pub action_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallResult {
    pub content: Vec<Value>,
    pub structured_content: Option<Value>,
    #[serde(rename = "_meta")]
    pub meta: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallError {
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallItem {
    pub id: CasItemId,
    pub server: String,
    pub tool: String,
    pub status: McpToolCallStatus,
    pub arguments: Value,
    pub app_context: Option<McpToolCallAppContext>,
    #[serde(default)]
    pub mcp_app_resource_uri: Option<String>,
    pub plugin_id: Option<String>,
    pub result: Option<Box<McpToolCallResult>>,
    pub error: Option<McpToolCallError>,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DynamicToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

impl DynamicToolCallStatus {
    pub(crate) const fn as_wire_str(self) -> &'static str {
        match self {
            Self::InProgress => "inProgress",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolCallItem {
    pub id: CasItemId,
    pub namespace: Option<String>,
    pub tool: String,
    pub arguments: Value,
    pub status: DynamicToolCallStatus,
    pub content_items: Option<Vec<DynamicToolCallOutputContentItem>>,
    pub success: Option<bool>,
    pub duration_ms: Option<i64>,
}
