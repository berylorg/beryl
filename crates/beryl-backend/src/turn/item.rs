mod auxiliary;
mod collaboration;
mod execution;
mod kind;
mod message;

use beryl_model::CasItemId;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

pub use auxiliary::*;
pub use collaboration::*;
pub use execution::*;
pub use kind::*;
pub use message::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadItem {
    UserMessage(UserMessageItem),
    HookPrompt(HookPromptItem),
    AgentMessage(AgentMessageItem),
    Plan(PlanItem),
    Reasoning(ReasoningItem),
    CommandExecution(CommandExecutionItem),
    FileChange(FileChangeItem),
    McpToolCall(McpToolCallItem),
    DynamicToolCall(DynamicToolCallItem),
    CollabAgentToolCall(CollabAgentToolCallItem),
    SubAgentActivity(SubAgentActivityItem),
    WebSearch(WebSearchItem),
    ImageView(ImageViewItem),
    Sleep(SleepItem),
    ImageGeneration(ImageGenerationItem),
    EnteredReviewMode(EnteredReviewModeItem),
    ExitedReviewMode(ExitedReviewModeItem),
    ContextCompaction(ContextCompactionItem),
}

impl ThreadItem {
    #[must_use]
    pub const fn id(&self) -> &CasItemId {
        match self {
            Self::UserMessage(item) => &item.id,
            Self::HookPrompt(item) => &item.id,
            Self::AgentMessage(item) => &item.id,
            Self::Plan(item) => &item.id,
            Self::Reasoning(item) => &item.id,
            Self::CommandExecution(item) => &item.id,
            Self::FileChange(item) => &item.id,
            Self::McpToolCall(item) => &item.id,
            Self::DynamicToolCall(item) => &item.id,
            Self::CollabAgentToolCall(item) => &item.id,
            Self::SubAgentActivity(item) => &item.id,
            Self::WebSearch(item) => &item.id,
            Self::ImageView(item) => &item.id,
            Self::Sleep(item) => &item.id,
            Self::ImageGeneration(item) => &item.id,
            Self::EnteredReviewMode(item) => &item.id,
            Self::ExitedReviewMode(item) => &item.id,
            Self::ContextCompaction(item) => &item.id,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> ThreadItemKind {
        match self {
            Self::UserMessage(_) => ThreadItemKind::UserMessage,
            Self::HookPrompt(_) => ThreadItemKind::HookPrompt,
            Self::AgentMessage(_) => ThreadItemKind::AgentMessage,
            Self::Plan(_) => ThreadItemKind::Plan,
            Self::Reasoning(_) => ThreadItemKind::Reasoning,
            Self::CommandExecution(_) => ThreadItemKind::CommandExecution,
            Self::FileChange(_) => ThreadItemKind::FileChange,
            Self::McpToolCall(_) => ThreadItemKind::McpToolCall,
            Self::DynamicToolCall(_) => ThreadItemKind::DynamicToolCall,
            Self::CollabAgentToolCall(_) => ThreadItemKind::CollabAgentToolCall,
            Self::SubAgentActivity(_) => ThreadItemKind::SubAgentActivity,
            Self::WebSearch(_) => ThreadItemKind::WebSearch,
            Self::ImageView(_) => ThreadItemKind::ImageView,
            Self::Sleep(_) => ThreadItemKind::Sleep,
            Self::ImageGeneration(_) => ThreadItemKind::ImageGeneration,
            Self::EnteredReviewMode(_) => ThreadItemKind::EnteredReviewMode,
            Self::ExitedReviewMode(_) => ThreadItemKind::ExitedReviewMode,
            Self::ContextCompaction(_) => ThreadItemKind::ContextCompaction,
        }
    }

    #[must_use]
    pub const fn item_type(&self) -> &'static str {
        self.kind().item_type()
    }

    #[must_use]
    pub const fn lifecycle_contract(&self) -> ThreadItemLifecycleContract {
        self.kind().lifecycle_contract()
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

        macro_rules! parse_item {
            ($variant:ident, $ty:ty) => {
                serde_json::from_value::<$ty>(value)
                    .map(Self::$variant)
                    .map_err(serde::de::Error::custom)
            };
        }

        match item_type {
            "userMessage" => parse_item!(UserMessage, UserMessageItem),
            "hookPrompt" => parse_item!(HookPrompt, HookPromptItem),
            "agentMessage" => parse_item!(AgentMessage, AgentMessageItem),
            "plan" => parse_item!(Plan, PlanItem),
            "reasoning" => parse_item!(Reasoning, ReasoningItem),
            "commandExecution" => parse_item!(CommandExecution, CommandExecutionItem),
            "fileChange" => parse_item!(FileChange, FileChangeItem),
            "mcpToolCall" => parse_item!(McpToolCall, McpToolCallItem),
            "dynamicToolCall" => parse_item!(DynamicToolCall, DynamicToolCallItem),
            "collabAgentToolCall" => parse_item!(CollabAgentToolCall, CollabAgentToolCallItem),
            "subAgentActivity" => parse_item!(SubAgentActivity, SubAgentActivityItem),
            "webSearch" => parse_item!(WebSearch, WebSearchItem),
            "imageView" => parse_item!(ImageView, ImageViewItem),
            "sleep" => parse_item!(Sleep, SleepItem),
            "imageGeneration" => parse_item!(ImageGeneration, ImageGenerationItem),
            "enteredReviewMode" => parse_item!(EnteredReviewMode, EnteredReviewModeItem),
            "exitedReviewMode" => parse_item!(ExitedReviewMode, ExitedReviewModeItem),
            "contextCompaction" => parse_item!(ContextCompaction, ContextCompactionItem),
            unknown => Err(serde::de::Error::custom(format_args!(
                "unknown pinned CAS thread item type {unknown:?}"
            ))),
        }
    }
}
