mod parser;
mod response;
mod schema;

use std::path::PathBuf;

use beryl_backend::{DynamicToolCallRequest, DynamicToolSpec};
use serde_json::Value;

use crate::dynamic_tool_namespace::BERYL_DYNAMIC_TOOL_NAMESPACE;
pub use parser::parse_beryl_settings_dynamic_tool_request;
pub use response::{settings_tool_failure_response, settings_tool_success_response};

pub const READ_GUI_SETTINGS_TOOL: &str = "read_gui_settings";
pub const VALIDATE_GUI_SETTINGS_UPDATE_TOOL: &str = "validate_gui_settings_update";
pub const UPDATE_GUI_SETTINGS_TOOL: &str = "update_gui_settings";

pub(super) const MAX_SETTINGS_TOOL_ERROR_BYTES: usize = 512;
pub(super) const MAX_SETTINGS_TOOL_STRING_BYTES: usize = 64 * 1024;
pub(super) const MAX_SETTINGS_TOOL_TIMEOUT_STRING_BYTES: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsDynamicToolRequest {
    Read,
    Validate { update: GuiSettingsUpdate },
    Update { update: GuiSettingsUpdate },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuiSettingsUpdate {
    operations: Option<OperationSettingsUpdate>,
    notifications: Option<NotificationSettingsUpdate>,
    agent: Option<AgentSettingsUpdate>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct OperationSettingsUpdate {
    context_compaction_timeout_seconds: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct NotificationSettingsUpdate {
    end_turn_sound_path: Option<Option<PathBuf>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct AgentSettingsUpdate {
    developer_instructions: Option<Option<String>>,
}

#[derive(Debug)]
pub struct SettingsDynamicToolError {
    kind: &'static str,
    message: String,
}

pub fn beryl_settings_dynamic_tool_specs() -> Vec<DynamicToolSpec> {
    vec![
        settings_tool_spec(
            READ_GUI_SETTINGS_TOOL,
            "Read a bounded redacted snapshot of Beryl-owned GUI settings.",
            schema::empty_object_schema(),
        ),
        settings_tool_spec(
            VALIDATE_GUI_SETTINGS_UPDATE_TOOL,
            "Validate a typed Beryl-owned GUI settings update without mutating active settings or settings-window drafts.",
            schema::settings_update_schema(),
        ),
        settings_tool_spec(
            UPDATE_GUI_SETTINGS_TOOL,
            "Commit a typed Beryl-owned GUI settings update through Beryl's settings validation, active update, and persistence path.",
            schema::settings_update_schema(),
        ),
    ]
}

pub fn is_beryl_settings_dynamic_tool(request: &DynamicToolCallRequest) -> bool {
    request
        .namespace()
        .is_none_or(|namespace| namespace == BERYL_DYNAMIC_TOOL_NAMESPACE)
        && matches!(
            request.tool(),
            READ_GUI_SETTINGS_TOOL | VALIDATE_GUI_SETTINGS_UPDATE_TOOL | UPDATE_GUI_SETTINGS_TOOL
        )
}

impl SettingsDynamicToolError {
    pub fn kind(&self) -> &'static str {
        self.kind
    }

    pub(super) fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: response::bounded_tool_string(message),
        }
    }

    pub(super) fn invalid_arguments(message: impl Into<String>) -> Self {
        Self::new("invalid_arguments", message)
    }

    pub(super) fn invalid_field(field: &'static str, message: impl Into<String>) -> Self {
        Self::new("invalid_field", format!("{field}: {}", message.into()))
    }
}

impl std::fmt::Display for SettingsDynamicToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SettingsDynamicToolError {}

fn settings_tool_spec(name: &str, description: &str, input_schema: Value) -> DynamicToolSpec {
    DynamicToolSpec::new(name, description, input_schema)
        .with_namespace(BERYL_DYNAMIC_TOOL_NAMESPACE)
        .with_defer_loading(false)
}
