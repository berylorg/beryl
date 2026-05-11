use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::fmt;

use crate::{
    DynamicToolCallRequest, DynamicToolSpec, JsonRpcError, ProtocolPhase, ThreadSummary,
    activity::{
        ToolActivityAgentLabel, ToolActivityCollabAgentSpawnMetadata, ToolActivityEvent,
        ToolActivityFileChangeSummary, ToolActivityLifecycle, ToolActivitySource,
    },
};

const THREAD_STATUS_CHANGED_METHOD: &str = "thread/status/changed";
const THREAD_STARTED_METHOD: &str = "thread/started";
const THREAD_CLOSED_METHOD: &str = "thread/closed";
const TURN_STARTED_METHOD: &str = "turn/started";
const TURN_COMPLETED_METHOD: &str = "turn/completed";
const ITEM_STARTED_METHOD: &str = "item/started";
const ITEM_COMPLETED_METHOD: &str = "item/completed";
const AGENT_MESSAGE_DELTA_METHOD: &str = "item/agentMessage/delta";
const REASONING_SUMMARY_PART_ADDED_METHOD: &str = "item/reasoning/summaryPartAdded";
const REASONING_SUMMARY_TEXT_DELTA_METHOD: &str = "item/reasoning/summaryTextDelta";
const REASONING_TEXT_DELTA_METHOD: &str = "item/reasoning/textDelta";
const COMMAND_EXECUTION_OUTPUT_DELTA_METHOD: &str = "item/commandExecution/outputDelta";
const FILE_CHANGE_OUTPUT_DELTA_METHOD: &str = "item/fileChange/outputDelta";
const THREAD_NAME_UPDATED_METHOD: &str = "thread/name/updated";
const THREAD_TOKEN_USAGE_UPDATED_METHOD: &str = "thread/tokenUsage/updated";
const ACCOUNT_RATE_LIMITS_UPDATED_METHOD: &str = "account/rateLimits/updated";
const CODEX_EVENT_COLLAB_AGENT_SPAWN_END_METHOD: &str = "codex/event/collab_agent_spawn_end";
const COMMAND_EXECUTION_REQUEST_APPROVAL_METHOD: &str = "item/commandExecution/requestApproval";
const FILE_CHANGE_REQUEST_APPROVAL_METHOD: &str = "item/fileChange/requestApproval";
const PERMISSIONS_REQUEST_APPROVAL_METHOD: &str = "item/permissions/requestApproval";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadInfo {
    #[serde(flatten)]
    summary: ThreadSummary,
    pub status: ThreadStatus,
    #[serde(default)]
    pub turns: Vec<TurnInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSessionResponse {
    pub thread: ThreadInfo,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub model_provider: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadSessionMetadata {
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartResponse {
    pub turn: TurnInfo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnSteerResponse {
    pub turn_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnInfo {
    pub id: String,
    pub status: TurnStatus,
    #[serde(default)]
    pub items: Vec<ThreadItem>,
    #[serde(default)]
    pub error: Option<TurnError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnError {
    pub message: String,
    #[serde(default)]
    pub additional_details: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveTurnNotSteerable {
    pub turn_kind: NonSteerableTurnKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NonSteerableTurnKind {
    Review,
    Compact,
    Other(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnStatus {
    Completed,
    Interrupted,
    Failed,
    InProgress,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ThreadStatus {
    NotLoaded,
    Idle,
    SystemError,
    Active {
        #[serde(default, rename = "activeFlags")]
        active_flags: Vec<ThreadActiveFlag>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThreadActiveFlag {
    WaitingOnApproval,
    WaitingOnUserInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandExecutionStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PatchApplyStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PatchChangeKind {
    Add,
    Delete,
    Update {
        #[serde(default)]
        move_path: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileUpdateChange {
    pub path: PathBuf,
    pub diff: String,
    pub kind: PatchChangeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageItem {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub phase: Option<ProtocolPhase>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningItem {
    pub id: String,
    #[serde(default)]
    pub content: Vec<String>,
    #[serde(default)]
    pub summary: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandExecutionItem {
    pub id: String,
    pub command: String,
    pub cwd: String,
    pub status: CommandExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<String>,
    #[serde(default)]
    pub aggregated_output: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessageItem {
    pub id: String,
    pub content: Vec<UserInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UserInput {
    Text { text: String },
    Image { url: String },
    LocalImage { path: String },
    Skill { name: String, path: String },
    Mention { name: String, path: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangeItem {
    pub id: String,
    pub status: PatchApplyStatus,
    pub changes: Vec<FileUpdateChange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageGenerationItem {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenericThreadItem {
    pub id: String,
    #[serde(rename = "type")]
    pub item_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(
        default,
        rename = "mcpAppResourceUri",
        skip_serializing_if = "Option::is_none"
    )]
    pub mcp_app_resource_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(
        default,
        rename = "reasoningEffort",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_effort: Option<String>,
    #[serde(
        default,
        rename = "receiverThreadIds",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub receiver_thread_ids: Vec<String>,
    #[serde(
        default,
        rename = "agentsStates",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub agents_states: BTreeMap<String, CollabAgentState>,
    #[serde(
        default,
        rename = "agentNickname",
        alias = "agent_nickname",
        alias = "nickname",
        alias = "name",
        alias = "displayName",
        alias = "label",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent_nickname: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollabAgentState {
    #[serde(
        default,
        alias = "agent_nickname",
        alias = "nickname",
        alias = "name",
        alias = "displayName",
        alias = "label",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent_nickname: Option<String>,
    #[serde(
        default,
        alias = "thread_id",
        alias = "threadID",
        skip_serializing_if = "Option::is_none"
    )]
    pub thread_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ThreadItem {
    UserMessage(UserMessageItem),
    AgentMessage(AgentMessageItem),
    Reasoning(ReasoningItem),
    CommandExecution(CommandExecutionItem),
    FileChange(FileChangeItem),
    ImageGeneration(ImageGenerationItem),
    Generic(GenericThreadItem),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnStreamEvent {
    ThreadStarted {
        thread: ThreadSummary,
    },
    AgentLabelUpdated {
        thread_id: String,
        label: String,
    },
    ThreadStatusChanged {
        thread_id: String,
        status: ThreadStatus,
    },
    ThreadClosed {
        thread_id: String,
    },
    TurnStarted {
        thread_id: String,
        turn: TurnInfo,
    },
    TurnCompleted {
        thread_id: String,
        turn: TurnInfo,
    },
    ItemStarted {
        thread_id: String,
        turn_id: String,
        item: ThreadItem,
    },
    ItemCompleted {
        thread_id: String,
        turn_id: String,
        item: ThreadItem,
    },
    AgentMessageDelta {
        thread_id: String,
        turn_id: String,
        item_id: String,
        delta: String,
    },
    ReasoningSummaryPartAdded {
        thread_id: String,
        turn_id: String,
        item_id: String,
        summary_index: usize,
    },
    ReasoningSummaryTextDelta {
        thread_id: String,
        turn_id: String,
        item_id: String,
        summary_index: usize,
        delta: String,
    },
    ReasoningTextDelta {
        thread_id: String,
        turn_id: String,
        item_id: String,
        content_index: usize,
        delta: String,
    },
    CommandExecutionOutputDelta {
        thread_id: String,
        turn_id: String,
        item_id: String,
        delta: String,
    },
    FileChangeOutputDelta {
        thread_id: String,
        turn_id: String,
        item_id: String,
        delta: String,
    },
    TokenUsageUpdated {
        thread_id: String,
        turn_id: String,
        token_usage: ThreadTokenUsage,
    },
    AccountRateLimitsUpdated {
        rate_limits: RateLimitSnapshot,
    },
    ThreadNameUpdated {
        thread_id: String,
        thread_name: Option<String>,
    },
    ApprovalRequested(ApprovalRequest),
    DynamicToolCallRequested(DynamicToolCallRequest),
    ProtocolError {
        error: JsonRpcError,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalRequestKind {
    CommandExecution,
    FileChange,
    Permissions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalRequest {
    request_id: Value,
    method: String,
    kind: ApprovalRequestKind,
    params: Value,
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTokenUsage {
    pub last: TokenUsageBreakdown,
    pub total: TokenUsageBreakdown,
    #[serde(default)]
    pub model_context_window: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitSnapshot {
    #[serde(default)]
    pub limit_id: Option<String>,
    #[serde(default)]
    pub limit_name: Option<String>,
    #[serde(default)]
    pub primary: Option<RateLimitWindow>,
    #[serde(default)]
    pub secondary: Option<RateLimitWindow>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRateLimitsResponse {
    pub rate_limits: RateLimitSnapshot,
    #[serde(default)]
    pub rate_limits_by_limit_id: Option<BTreeMap<String, RateLimitSnapshot>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitWindow {
    pub used_percent: i32,
    #[serde(default)]
    pub window_duration_mins: Option<i64>,
    #[serde(default)]
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadStartOptions {
    ephemeral: bool,
    developer_instructions: Option<String>,
    dynamic_tools: Vec<DynamicToolSpec>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnStartOptions {
    model: Option<String>,
    reasoning_effort: Option<String>,
    developer_instructions_context: Option<TurnDeveloperInstructionsContext>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnDeveloperInstructionsContext {
    developer_instructions: Option<String>,
    model: String,
    reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadUnsubscribeResponse {
    pub status: ThreadUnsubscribeStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ThreadUnsubscribeStatus {
    NotLoaded,
    NotSubscribed,
    Unsubscribed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageBreakdown {
    #[serde(default)]
    pub cached_input_tokens: i64,
    #[serde(default)]
    pub input_tokens: i64,
    #[serde(default)]
    pub output_tokens: i64,
    #[serde(default)]
    pub reasoning_output_tokens: i64,
    #[serde(default)]
    pub total_tokens: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadStartParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub ephemeral: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub developer_instructions: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dynamic_tools: Vec<DynamicToolSpec>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnStartParams {
    pub thread_id: String,
    pub input: Vec<UserInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collaboration_mode: Option<TurnStartCollaborationMode>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnStartCollaborationMode {
    mode: TurnStartCollaborationModeKind,
    settings: TurnStartCollaborationModeSettings,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum TurnStartCollaborationModeKind {
    Default,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct TurnStartCollaborationModeSettings {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    developer_instructions: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TurnSteerParams {
    pub thread_id: String,
    pub expected_turn_id: String,
    pub input: Vec<UserInput>,
}

impl ThreadInfo {
    pub fn summary(&self) -> ThreadSummary {
        self.summary.clone()
    }
}

impl ThreadSessionResponse {
    pub fn metadata(&self) -> ThreadSessionMetadata {
        ThreadSessionMetadata {
            model: non_empty_string(self.model.clone()),
            model_provider: non_empty_string(self.model_provider.clone()),
            reasoning_effort: non_empty_string(self.reasoning_effort.clone()),
        }
    }
}

impl TurnInfo {
    pub fn is_terminal(&self) -> bool {
        !matches!(self.status, TurnStatus::InProgress)
    }
}

impl ThreadStatus {
    pub fn waiting_on_user_input(&self) -> bool {
        matches!(
            self,
            Self::Active { active_flags }
                if active_flags.contains(&ThreadActiveFlag::WaitingOnUserInput)
        )
    }
}

impl CommandExecutionStatus {
    fn as_wire_str(self) -> &'static str {
        match self {
            Self::InProgress => "inProgress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Declined => "declined",
        }
    }
}

impl PatchApplyStatus {
    fn as_wire_str(self) -> &'static str {
        match self {
            Self::InProgress => "inProgress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Declined => "declined",
        }
    }
}

impl ThreadStartOptions {
    pub fn persistent() -> Self {
        Self {
            ephemeral: false,
            developer_instructions: None,
            dynamic_tools: Vec::new(),
        }
    }

    pub fn ephemeral() -> Self {
        Self {
            ephemeral: true,
            developer_instructions: None,
            dynamic_tools: Vec::new(),
        }
    }

    pub fn with_developer_instructions(
        mut self,
        developer_instructions: impl Into<String>,
    ) -> Self {
        self.developer_instructions = Some(developer_instructions.into());
        self
    }

    pub fn with_dynamic_tool(mut self, tool: DynamicToolSpec) -> Self {
        self.dynamic_tools.push(tool);
        self
    }

    pub fn with_dynamic_tools(mut self, tools: Vec<DynamicToolSpec>) -> Self {
        self.dynamic_tools = tools;
        self
    }

    pub fn is_ephemeral(&self) -> bool {
        self.ephemeral
    }

    pub fn dynamic_tools(&self) -> &[DynamicToolSpec] {
        &self.dynamic_tools
    }

    pub fn developer_instructions(&self) -> Option<&str> {
        self.developer_instructions.as_deref()
    }
}

impl TurnStartOptions {
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = non_empty_string(Some(model.into()));
        self
    }

    pub fn with_reasoning_effort(mut self, reasoning_effort: impl Into<String>) -> Self {
        self.reasoning_effort = non_empty_string(Some(reasoning_effort.into()));
        self
    }

    pub fn with_developer_instructions_context(
        mut self,
        developer_instructions: Option<String>,
        model: impl Into<String>,
        reasoning_effort: Option<String>,
    ) -> Self {
        self.developer_instructions_context =
            TurnDeveloperInstructionsContext::new(developer_instructions, model, reasoning_effort);
        self
    }

    pub fn without_developer_instructions_context(mut self) -> Self {
        self.developer_instructions_context = None;
        self
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn reasoning_effort(&self) -> Option<&str> {
        self.reasoning_effort.as_deref()
    }

    pub fn developer_instructions_context(&self) -> Option<&TurnDeveloperInstructionsContext> {
        self.developer_instructions_context.as_ref()
    }
}

impl TurnDeveloperInstructionsContext {
    fn new(
        developer_instructions: Option<String>,
        model: impl Into<String>,
        reasoning_effort: Option<String>,
    ) -> Option<Self> {
        let model = non_empty_string(Some(model.into()))?;
        Some(Self {
            developer_instructions: developer_instructions
                .and_then(|value| (!value.trim().is_empty()).then_some(value)),
            model,
            reasoning_effort: non_empty_string(reasoning_effort),
        })
    }

    pub fn developer_instructions(&self) -> Option<&str> {
        self.developer_instructions.as_deref()
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn reasoning_effort(&self) -> Option<&str> {
        self.reasoning_effort.as_deref()
    }
}

impl UserInput {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn local_image(path: impl Into<String>) -> Self {
        Self::LocalImage { path: path.into() }
    }
}

impl ThreadItem {
    pub fn id(&self) -> &str {
        match self {
            Self::UserMessage(item) => &item.id,
            Self::AgentMessage(item) => &item.id,
            Self::Reasoning(item) => &item.id,
            Self::CommandExecution(item) => &item.id,
            Self::FileChange(item) => &item.id,
            Self::ImageGeneration(item) => &item.id,
            Self::Generic(item) => &item.id,
        }
    }

    pub fn item_type(&self) -> &str {
        match self {
            Self::UserMessage(_) => "userMessage",
            Self::AgentMessage(_) => "agentMessage",
            Self::Reasoning(_) => "reasoning",
            Self::CommandExecution(_) => "commandExecution",
            Self::FileChange(_) => "fileChange",
            Self::ImageGeneration(_) => "imageGeneration",
            Self::Generic(item) => item.item_type.as_str(),
        }
    }

    fn raw_tool_name(&self) -> Option<&str> {
        match self {
            Self::Generic(item) => item.tool.as_deref(),
            _ => None,
        }
    }

    fn raw_tool_server(&self) -> Option<&str> {
        match self {
            Self::Generic(item) => item.server.as_deref(),
            _ => None,
        }
    }

    fn raw_tool_namespace(&self) -> Option<&str> {
        match self {
            Self::Generic(item) => item.namespace.as_deref(),
            _ => None,
        }
    }

    fn raw_resource_uri(&self) -> Option<&str> {
        match self {
            Self::Generic(item) => item.mcp_app_resource_uri.as_deref(),
            _ => None,
        }
    }

    fn raw_command(&self) -> Option<&str> {
        match self {
            Self::CommandExecution(item) => Some(item.command.as_str()),
            _ => None,
        }
    }

    fn command_exec_process_id(&self) -> Option<&str> {
        match self {
            Self::CommandExecution(item) => item.process_id.as_deref(),
            _ => None,
        }
    }

    fn raw_item_status(&self) -> Option<&str> {
        match self {
            Self::CommandExecution(item) => Some(item.status.as_wire_str()),
            Self::FileChange(item) => Some(item.status.as_wire_str()),
            Self::ImageGeneration(item) => item.status.as_deref(),
            Self::Generic(item) => item.status.as_deref(),
            _ => None,
        }
    }

    fn reasoning_summary_text(&self) -> Option<String> {
        match self {
            Self::Reasoning(item) => joined_non_empty_text(&item.summary),
            _ => None,
        }
    }

    fn file_change_summary(&self) -> Option<ToolActivityFileChangeSummary> {
        match self {
            Self::FileChange(item) => Some(file_change_summary(item)),
            _ => None,
        }
    }

    fn agent_label_updates(&self) -> Vec<ToolActivityAgentLabel> {
        match self {
            Self::Generic(item) => item.agent_label_updates(),
            _ => Vec::new(),
        }
    }

    fn receiver_thread_ids(&self) -> Vec<String> {
        match self {
            Self::Generic(item) if item.item_type == "collabAgentToolCall" => {
                item.receiver_thread_ids.clone()
            }
            _ => Vec::new(),
        }
    }

    fn collab_agent_spawn_metadata(&self) -> Option<ToolActivityCollabAgentSpawnMetadata> {
        match self {
            Self::Generic(item) if item.item_type == "collabAgentToolCall" => {
                item.collab_agent_spawn_metadata()
            }
            _ => None,
        }
    }
}

impl GenericThreadItem {
    fn collab_agent_spawn_metadata(&self) -> Option<ToolActivityCollabAgentSpawnMetadata> {
        ToolActivityCollabAgentSpawnMetadata::from_raw(
            self.model.as_deref(),
            self.reasoning_effort.as_deref(),
        )
    }

    fn agent_label_updates(&self) -> Vec<ToolActivityAgentLabel> {
        if self.item_type != "collabAgentToolCall" {
            return Vec::new();
        }

        let mut updates = Vec::new();
        for (key, state) in &self.agents_states {
            let Some(label) = non_empty_string(state.agent_nickname.clone()) else {
                continue;
            };
            let thread_id = non_empty_string(state.thread_id.clone())
                .or_else(|| {
                    self.receiver_thread_ids
                        .iter()
                        .find(|thread_id| thread_id.as_str() == key.as_str())
                        .cloned()
                })
                .or_else(|| {
                    (self.receiver_thread_ids.len() == 1)
                        .then(|| self.receiver_thread_ids[0].clone())
                })
                .or_else(|| non_empty_string(Some(key.clone())));

            push_agent_label_update(&mut updates, thread_id, label);
        }

        if let Some(label) = non_empty_string(self.agent_nickname.clone()) {
            for thread_id in &self.receiver_thread_ids {
                push_agent_label_update(&mut updates, Some(thread_id.clone()), label.clone());
            }
        }

        updates
    }
}

fn file_change_summary(item: &FileChangeItem) -> ToolActivityFileChangeSummary {
    let unique_paths = item
        .changes
        .iter()
        .map(|change| change.path.as_path())
        .collect::<BTreeSet<_>>();
    let file_count = unique_paths.len();
    let single_file_path = unique_paths
        .iter()
        .next()
        .and_then(|path| (file_count == 1).then(|| PathBuf::from(*path)));
    let (additions, deletions) = item
        .changes
        .iter()
        .map(|change| diff_line_counts(change.diff.as_str()))
        .fold(
            (0, 0),
            |(total_additions, total_deletions), (add, delete)| {
                (total_additions + add, total_deletions + delete)
            },
        );

    ToolActivityFileChangeSummary {
        file_count,
        additions,
        deletions,
        single_file_path,
    }
}

fn diff_line_counts(diff: &str) -> (usize, usize) {
    let mut additions = 0;
    let mut deletions = 0;

    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            additions += 1;
        } else if line.starts_with('-') {
            deletions += 1;
        }
    }

    (additions, deletions)
}

impl TurnStreamEvent {
    pub fn activity(&self) -> Option<ToolActivityEvent> {
        let (thread_id, turn_id, item, lifecycle) = match self {
            Self::ItemStarted {
                thread_id,
                turn_id,
                item,
            } => (thread_id, turn_id, item, ToolActivityLifecycle::Started),
            Self::ItemCompleted {
                thread_id,
                turn_id,
                item,
            } => (thread_id, turn_id, item, ToolActivityLifecycle::Completed),
            Self::ReasoningSummaryPartAdded {
                thread_id,
                turn_id,
                item_id,
                summary_index,
            } => {
                return Some(
                    ToolActivityEvent::new(
                        thread_id.as_str(),
                        turn_id.as_str(),
                        item_id.as_str(),
                        "reasoning",
                        ToolActivitySource::Reasoning,
                        ToolActivityLifecycle::Updated,
                    )
                    .with_reasoning_summary_index(Some(*summary_index)),
                );
            }
            Self::ReasoningSummaryTextDelta {
                thread_id,
                turn_id,
                item_id,
                summary_index,
                delta,
            } => {
                return Some(
                    ToolActivityEvent::new(
                        thread_id.as_str(),
                        turn_id.as_str(),
                        item_id.as_str(),
                        "reasoning",
                        ToolActivitySource::Reasoning,
                        ToolActivityLifecycle::Updated,
                    )
                    .with_reasoning_summary_index(Some(*summary_index))
                    .with_reasoning_summary_delta(Some(delta.as_str())),
                );
            }
            _ => return None,
        };
        let item_type = item.item_type();
        let source = ToolActivitySource::from_item_type(item_type)?;

        Some(
            ToolActivityEvent::new(
                thread_id.as_str(),
                turn_id.as_str(),
                item.id(),
                item_type,
                source,
                lifecycle,
            )
            .with_raw_tool_name(item.raw_tool_name())
            .with_raw_tool_server(item.raw_tool_server())
            .with_raw_tool_namespace(item.raw_tool_namespace())
            .with_raw_resource_uri(item.raw_resource_uri())
            .with_raw_command(item.raw_command())
            .with_command_exec_process_id(item.command_exec_process_id())
            .with_raw_item_status(item.raw_item_status())
            .with_reasoning_summary_text(item.reasoning_summary_text())
            .with_file_change_summary(item.file_change_summary())
            .with_collab_agent_spawn_metadata(item.collab_agent_spawn_metadata())
            .with_receiver_thread_ids(item.receiver_thread_ids())
            .with_agent_label_updates(item.agent_label_updates()),
        )
    }

    pub fn tool_activity(&self) -> Option<ToolActivityEvent> {
        self.activity()
            .filter(|activity| activity.source.is_operational_tool())
    }
}

impl<'de> Deserialize<'de> for ThreadItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let item_type = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| serde::de::Error::missing_field("type"))?;

        match item_type {
            "userMessage" => serde_json::from_value(value)
                .map(Self::UserMessage)
                .map_err(serde::de::Error::custom),
            "agentMessage" => serde_json::from_value(value)
                .map(Self::AgentMessage)
                .map_err(serde::de::Error::custom),
            "reasoning" => serde_json::from_value(value)
                .map(Self::Reasoning)
                .map_err(serde::de::Error::custom),
            "commandExecution" => serde_json::from_value(value)
                .map(Self::CommandExecution)
                .map_err(serde::de::Error::custom),
            "fileChange" => serde_json::from_value(value)
                .map(Self::FileChange)
                .map_err(serde::de::Error::custom),
            "imageGeneration" => serde_json::from_value(value)
                .map(Self::ImageGeneration)
                .map_err(serde::de::Error::custom),
            _ => serde_json::from_value(value)
                .map(Self::Generic)
                .map_err(serde::de::Error::custom),
        }
    }
}

impl ThreadStartParams {
    pub(crate) fn for_workspace(path: &Path, options: ThreadStartOptions) -> Self {
        Self {
            cwd: Some(path.display().to_string()),
            ephemeral: options.ephemeral,
            developer_instructions: options.developer_instructions,
            dynamic_tools: options.dynamic_tools,
        }
    }
}

impl ApprovalRequestKind {
    pub fn method(self) -> &'static str {
        match self {
            Self::CommandExecution => COMMAND_EXECUTION_REQUEST_APPROVAL_METHOD,
            Self::FileChange => FILE_CHANGE_REQUEST_APPROVAL_METHOD,
            Self::Permissions => PERMISSIONS_REQUEST_APPROVAL_METHOD,
        }
    }

    pub fn denial_response_interrupts_turn(self) -> bool {
        matches!(self, Self::CommandExecution | Self::FileChange)
    }
}

impl fmt::Display for NonSteerableTurnKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Review => formatter.write_str("review"),
            Self::Compact => formatter.write_str("compact"),
            Self::Other(kind) => formatter.write_str(kind),
        }
    }
}

impl NonSteerableTurnKind {
    fn from_wire(value: &str) -> Self {
        match value {
            "review" => Self::Review,
            "compact" => Self::Compact,
            other => Self::Other(other.to_string()),
        }
    }
}

pub fn active_turn_not_steerable_error(error: &JsonRpcError) -> Option<ActiveTurnNotSteerable> {
    let data = error.data.as_ref()?;
    active_turn_not_steerable_from_value(data)
        .or_else(|| {
            data.get("codexErrorInfo")
                .and_then(active_turn_not_steerable_from_value)
        })
        .or_else(|| {
            data.get("error")
                .and_then(|error| error.get("codexErrorInfo"))
                .and_then(active_turn_not_steerable_from_value)
        })
}

fn active_turn_not_steerable_from_value(value: &Value) -> Option<ActiveTurnNotSteerable> {
    if value.as_str() == Some("activeTurnNotSteerable") {
        return Some(ActiveTurnNotSteerable {
            turn_kind: NonSteerableTurnKind::Other("unknown".to_string()),
        });
    }

    let info = value.get("activeTurnNotSteerable")?;
    let turn_kind = info
        .get("turnKind")
        .and_then(Value::as_str)
        .map(NonSteerableTurnKind::from_wire)
        .unwrap_or_else(|| NonSteerableTurnKind::Other("unknown".to_string()));
    Some(ActiveTurnNotSteerable { turn_kind })
}

impl ApprovalRequest {
    pub fn request_id(&self) -> &Value {
        &self.request_id
    }

    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn kind(&self) -> ApprovalRequestKind {
        self.kind
    }

    pub fn params(&self) -> &Value {
        &self.params
    }

    pub fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }

    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }

    pub fn item_id(&self) -> Option<&str> {
        self.item_id.as_deref()
    }

    pub fn command(&self) -> Option<&str> {
        self.command.as_deref()
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub fn pretty_params(&self) -> String {
        serde_json::to_string_pretty(&self.params).unwrap_or_else(|_| self.params.to_string())
    }

    pub fn summary(&self) -> String {
        let command = self.command.as_deref().unwrap_or("<none>");
        let cwd = self.cwd.as_deref().unwrap_or("<none>");
        let reason = self.reason.as_deref().unwrap_or("<none>");
        format!(
            "method={}, requestId={}, threadId={}, turnId={}, itemId={}, cwd={}, command={}, reason={}",
            self.method,
            self.request_id,
            self.thread_id.as_deref().unwrap_or("<unknown>"),
            self.turn_id.as_deref().unwrap_or("<unknown>"),
            self.item_id.as_deref().unwrap_or("<unknown>"),
            cwd,
            command,
            reason
        )
    }
}

impl TurnStartParams {
    pub(crate) fn text(
        thread_id: impl Into<String>,
        text: impl Into<String>,
        options: TurnStartOptions,
    ) -> Self {
        Self::input(thread_id, vec![UserInput::text(text)], options)
    }

    pub(crate) fn input(
        thread_id: impl Into<String>,
        input: Vec<UserInput>,
        options: TurnStartOptions,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            input,
            model: options.model,
            effort: options.reasoning_effort,
            collaboration_mode: options
                .developer_instructions_context
                .map(TurnStartCollaborationMode::developer_instructions_context),
        }
    }
}

impl TurnStartCollaborationMode {
    fn developer_instructions_context(context: TurnDeveloperInstructionsContext) -> Self {
        Self {
            mode: TurnStartCollaborationModeKind::Default,
            settings: TurnStartCollaborationModeSettings {
                model: context.model,
                reasoning_effort: context.reasoning_effort,
                developer_instructions: context.developer_instructions,
            },
        }
    }
}

impl TurnSteerParams {
    pub(crate) fn input(
        thread_id: impl Into<String>,
        expected_turn_id: impl Into<String>,
        input: Vec<UserInput>,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            expected_turn_id: expected_turn_id.into(),
            input,
        }
    }
}

pub fn parse_approval_request(
    request_id: Value,
    method: &str,
    params: Option<Value>,
) -> Option<ApprovalRequest> {
    let kind = match method {
        COMMAND_EXECUTION_REQUEST_APPROVAL_METHOD => ApprovalRequestKind::CommandExecution,
        FILE_CHANGE_REQUEST_APPROVAL_METHOD => ApprovalRequestKind::FileChange,
        PERMISSIONS_REQUEST_APPROVAL_METHOD => ApprovalRequestKind::Permissions,
        _ => return None,
    };
    let params = params.unwrap_or(Value::Null);
    Some(ApprovalRequest {
        request_id,
        method: method.to_string(),
        kind,
        thread_id: string_field(&params, "threadId"),
        turn_id: string_field(&params, "turnId"),
        item_id: string_field(&params, "itemId"),
        command: string_field(&params, "command"),
        cwd: string_field(&params, "cwd"),
        reason: string_field(&params, "reason"),
        params,
    })
}

pub fn parse_turn_stream_event(
    method: &str,
    params: Option<Value>,
) -> Result<Option<TurnStreamEvent>, serde_json::Error> {
    let Some(params) = params else {
        return Ok(None);
    };

    let event = match method {
        THREAD_STARTED_METHOD => {
            let params: ThreadStartedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ThreadStarted {
                thread: params.thread,
            }
        }
        CODEX_EVENT_COLLAB_AGENT_SPAWN_END_METHOD => {
            let Some(event) = collab_agent_spawn_label_event(&params) else {
                return Ok(None);
            };
            event
        }
        THREAD_STATUS_CHANGED_METHOD => {
            let params: ThreadStatusChangedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ThreadStatusChanged {
                thread_id: params.thread_id,
                status: params.status,
            }
        }
        THREAD_CLOSED_METHOD => {
            let params: ThreadClosedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ThreadClosed {
                thread_id: params.thread_id,
            }
        }
        TURN_STARTED_METHOD => {
            let params: TurnNotification = serde_json::from_value(params)?;
            TurnStreamEvent::TurnStarted {
                thread_id: params.thread_id,
                turn: params.turn,
            }
        }
        TURN_COMPLETED_METHOD => {
            let params: TurnNotification = serde_json::from_value(params)?;
            TurnStreamEvent::TurnCompleted {
                thread_id: params.thread_id,
                turn: params.turn,
            }
        }
        ITEM_STARTED_METHOD => {
            let params: ItemNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ItemStarted {
                thread_id: params.thread_id,
                turn_id: params.turn_id,
                item: params.item,
            }
        }
        ITEM_COMPLETED_METHOD => {
            let params: ItemNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ItemCompleted {
                thread_id: params.thread_id,
                turn_id: params.turn_id,
                item: params.item,
            }
        }
        AGENT_MESSAGE_DELTA_METHOD => {
            let params: ItemDeltaNotification = serde_json::from_value(params)?;
            TurnStreamEvent::AgentMessageDelta {
                thread_id: params.thread_id,
                turn_id: params.turn_id,
                item_id: params.item_id,
                delta: params.delta,
            }
        }
        REASONING_SUMMARY_PART_ADDED_METHOD => {
            let params: ReasoningSummaryPartAddedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ReasoningSummaryPartAdded {
                thread_id: params.thread_id,
                turn_id: params.turn_id,
                item_id: params.item_id,
                summary_index: params.summary_index as usize,
            }
        }
        REASONING_SUMMARY_TEXT_DELTA_METHOD => {
            let params: ReasoningIndexedDeltaNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ReasoningSummaryTextDelta {
                thread_id: params.thread_id,
                turn_id: params.turn_id,
                item_id: params.item_id,
                summary_index: params.index as usize,
                delta: params.delta,
            }
        }
        REASONING_TEXT_DELTA_METHOD => {
            let params: ReasoningIndexedDeltaNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ReasoningTextDelta {
                thread_id: params.thread_id,
                turn_id: params.turn_id,
                item_id: params.item_id,
                content_index: params.index as usize,
                delta: params.delta,
            }
        }
        COMMAND_EXECUTION_OUTPUT_DELTA_METHOD => {
            let params: ItemDeltaNotification = serde_json::from_value(params)?;
            TurnStreamEvent::CommandExecutionOutputDelta {
                thread_id: params.thread_id,
                turn_id: params.turn_id,
                item_id: params.item_id,
                delta: params.delta,
            }
        }
        FILE_CHANGE_OUTPUT_DELTA_METHOD => {
            let params: ItemDeltaNotification = serde_json::from_value(params)?;
            TurnStreamEvent::FileChangeOutputDelta {
                thread_id: params.thread_id,
                turn_id: params.turn_id,
                item_id: params.item_id,
                delta: params.delta,
            }
        }
        THREAD_TOKEN_USAGE_UPDATED_METHOD => {
            let params: ThreadTokenUsageUpdatedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::TokenUsageUpdated {
                thread_id: params.thread_id,
                turn_id: params.turn_id,
                token_usage: params.token_usage,
            }
        }
        ACCOUNT_RATE_LIMITS_UPDATED_METHOD => {
            let params: AccountRateLimitsUpdatedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::AccountRateLimitsUpdated {
                rate_limits: params.rate_limits,
            }
        }
        THREAD_NAME_UPDATED_METHOD => {
            let params: ThreadNameUpdatedNotification = serde_json::from_value(params)?;
            TurnStreamEvent::ThreadNameUpdated {
                thread_id: params.thread_id,
                thread_name: params.thread_name,
            }
        }
        _ => return Ok(None),
    };

    Ok(Some(event))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadStartedNotification {
    thread: ThreadSummary,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadStatusChangedNotification {
    thread_id: String,
    status: ThreadStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadClosedNotification {
    thread_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurnNotification {
    thread_id: String,
    turn: TurnInfo,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemNotification {
    thread_id: String,
    turn_id: String,
    item: ThreadItem,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemDeltaNotification {
    thread_id: String,
    turn_id: String,
    item_id: String,
    delta: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReasoningSummaryPartAddedNotification {
    thread_id: String,
    turn_id: String,
    item_id: String,
    summary_index: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReasoningIndexedDeltaNotification {
    thread_id: String,
    turn_id: String,
    item_id: String,
    #[serde(alias = "summaryIndex", alias = "contentIndex")]
    index: i64,
    delta: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadTokenUsageUpdatedNotification {
    thread_id: String,
    turn_id: String,
    token_usage: ThreadTokenUsage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountRateLimitsUpdatedNotification {
    rate_limits: RateLimitSnapshot,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadNameUpdatedNotification {
    thread_id: String,
    thread_name: Option<String>,
}

fn non_empty_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| (!value.is_empty()).then_some(value))
}

fn joined_non_empty_text(parts: &[String]) -> Option<String> {
    let mut text = String::new();
    for part in parts {
        if !part.is_empty() {
            text.push_str(part);
        }
    }
    non_empty_string(Some(text))
}

fn push_agent_label_update(
    updates: &mut Vec<ToolActivityAgentLabel>,
    thread_id: Option<String>,
    label: String,
) {
    let Some(thread_id) = non_empty_string(thread_id) else {
        return;
    };
    let Some(label) = non_empty_string(Some(label)) else {
        return;
    };

    if let Some(update) = updates
        .iter_mut()
        .find(|update| update.thread_id == thread_id)
    {
        update.label = label;
    } else {
        updates.push(ToolActivityAgentLabel::new(thread_id, label));
    }
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn string_field_any(value: &Value, fields: &[&str]) -> Option<String> {
    fields.iter().find_map(|field| string_field(value, field))
}

fn collab_agent_spawn_label_event(params: &Value) -> Option<TurnStreamEvent> {
    let msg = params.get("msg").unwrap_or(params);
    let thread_id = string_field_any(
        msg,
        &[
            "new_thread_id",
            "newThreadId",
            "new_agent_id",
            "newAgentId",
            "agent_id",
            "agentId",
        ],
    )?;
    let label = string_field_any(
        msg,
        &[
            "new_agent_nickname",
            "newAgentNickname",
            "agent_nickname",
            "agentNickname",
            "nickname",
        ],
    )?;

    Some(TurnStreamEvent::AgentLabelUpdated { thread_id, label })
}
