use super::super::{
    ProviderImageLocatorV1, ProviderInlineImageAssetV1, ProviderMcpContentV1,
    ProviderStructuredValueV1, ProviderTextV1,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCommandSourceV1 {
    Agent,
    UserShell,
    UnifiedExecStartup,
    UnifiedExecInteraction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCommandStatusV1 {
    InProgress,
    Completed,
    Failed,
    Declined,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderCommandActionV1 {
    Read {
        command: ProviderTextV1,
        name: ProviderTextV1,
        path: ProviderTextV1,
    },
    ListFiles {
        command: ProviderTextV1,
        path: Option<ProviderTextV1>,
    },
    Search {
        command: ProviderTextV1,
        query: Option<ProviderTextV1>,
        path: Option<ProviderTextV1>,
    },
    Unknown {
        command: ProviderTextV1,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCommandExecutionV1 {
    pub command: ProviderTextV1,
    pub cwd: ProviderTextV1,
    pub process_id: Option<ProviderTextV1>,
    pub source: ProviderCommandSourceV1,
    pub status: ProviderCommandStatusV1,
    pub command_actions: Vec<ProviderCommandActionV1>,
    pub aggregated_output: Option<ProviderTextV1>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderPatchStatusV1 {
    InProgress,
    Completed,
    Failed,
    Declined,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderPatchChangeKindV1 {
    Add,
    Delete,
    Update { move_path: Option<ProviderTextV1> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderFileUpdateChangeV1 {
    pub path: ProviderTextV1,
    pub diff: ProviderTextV1,
    pub kind: ProviderPatchChangeKindV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderFileChangeV1 {
    pub status: ProviderPatchStatusV1,
    pub changes: Vec<ProviderFileUpdateChangeV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderToolCallStatusV1 {
    InProgress,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderMcpAppContextV1 {
    pub connector_id: ProviderTextV1,
    pub link_id: Option<ProviderTextV1>,
    pub resource_uri: Option<ProviderTextV1>,
    pub app_name: Option<ProviderTextV1>,
    pub template_id: Option<ProviderTextV1>,
    pub action_name: Option<ProviderTextV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderMcpResultV1 {
    pub content: Vec<ProviderMcpContentV1>,
    pub structured_content: Option<ProviderStructuredValueV1>,
    pub meta: Option<ProviderStructuredValueV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderMcpErrorV1 {
    pub message: ProviderTextV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderMcpToolCallV1 {
    pub server: ProviderTextV1,
    pub tool: ProviderTextV1,
    pub status: ProviderToolCallStatusV1,
    pub arguments: ProviderStructuredValueV1,
    pub app_context: Option<ProviderMcpAppContextV1>,
    pub mcp_app_resource_uri: Option<ProviderTextV1>,
    pub plugin_id: Option<ProviderTextV1>,
    pub result: Option<ProviderMcpResultV1>,
    pub error: Option<ProviderMcpErrorV1>,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderDynamicToolOutputV1 {
    InputText {
        text: ProviderTextV1,
    },
    /// Exact non-`data:` `image_url` retained as a locator.
    InputImageLocator {
        locator: ProviderImageLocatorV1,
    },
    /// Typed inline/data image after admission to Beryl-owned asset authority.
    InputImageAsset {
        asset: ProviderInlineImageAssetV1,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderDynamicToolCallV1 {
    pub namespace: Option<ProviderTextV1>,
    pub tool: ProviderTextV1,
    pub arguments: ProviderStructuredValueV1,
    pub status: ProviderToolCallStatusV1,
    pub content_items: Option<Vec<ProviderDynamicToolOutputV1>>,
    pub success: Option<bool>,
    pub duration_ms: Option<i64>,
}
