mod auxiliary;
mod collaboration;
mod execution;
mod message;

pub use auxiliary::*;
pub use collaboration::*;
pub use execution::*;
pub use message::*;

use crate::{ProviderItemKind, UnsupportedHistoryReason};

use super::ProviderFrameHistorySupportV1;

/// Closed V1 value union for every pinned normalized public item family.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ProviderItemV1 {
    UserMessage(ProviderUserMessageV1),
    HookPrompt(ProviderHookPromptV1),
    AgentMessage(ProviderAgentMessageV1),
    Plan(ProviderPlanV1),
    Reasoning(ProviderReasoningV1),
    CommandExecution(ProviderCommandExecutionV1),
    FileChange(ProviderFileChangeV1),
    McpToolCall(ProviderMcpToolCallV1),
    DynamicToolCall(ProviderDynamicToolCallV1),
    CollabAgentToolCall(ProviderCollabAgentToolCallV1),
    SubAgentActivity(ProviderSubAgentActivityV1),
    WebSearch(ProviderWebSearchV1),
    ImageView(ProviderImageViewV1),
    Sleep(ProviderSleepV1),
    StandaloneImageGeneration(ProviderImageGenerationV1),
    EnteredReviewMode(ProviderEnteredReviewModeV1),
    ExitedReviewMode(ProviderExitedReviewModeV1),
    ContextCompaction,
}

impl ProviderItemV1 {
    #[must_use]
    pub const fn kind(&self) -> ProviderItemKind {
        match self {
            Self::UserMessage(_) => ProviderItemKind::UserMessage,
            Self::HookPrompt(_) => ProviderItemKind::HookPrompt,
            Self::AgentMessage(_) => ProviderItemKind::AgentMessage,
            Self::Plan(_) => ProviderItemKind::Plan,
            Self::Reasoning(_) => ProviderItemKind::Reasoning,
            Self::CommandExecution(_) => ProviderItemKind::CommandExecution,
            Self::FileChange(_) => ProviderItemKind::FileChange,
            Self::McpToolCall(_) => ProviderItemKind::McpToolCall,
            Self::DynamicToolCall(_) => ProviderItemKind::DynamicToolCall,
            Self::CollabAgentToolCall(_) => ProviderItemKind::CollabAgentToolCall,
            Self::SubAgentActivity(_) => ProviderItemKind::SubAgentActivity,
            Self::WebSearch(_) => ProviderItemKind::WebSearch,
            Self::ImageView(_) => ProviderItemKind::ImageView,
            Self::Sleep(_) => ProviderItemKind::Sleep,
            Self::StandaloneImageGeneration(_) => ProviderItemKind::StandaloneImageGeneration,
            Self::EnteredReviewMode(_) => ProviderItemKind::EnteredReviewMode,
            Self::ExitedReviewMode(_) => ProviderItemKind::ExitedReviewMode,
            Self::ContextCompaction => ProviderItemKind::ContextCompaction,
        }
    }

    /// Returns the typed complete-history support carried by this exact item snapshot.
    #[must_use]
    pub const fn history_support(&self) -> ProviderFrameHistorySupportV1 {
        match self {
            Self::WebSearch(ProviderWebSearchV1 {
                action: Some(ProviderWebSearchActionV1::Other),
                ..
            }) => ProviderFrameHistorySupportV1::Unsupported(
                UnsupportedHistoryReason::UnsupportedRequiredPayload,
            ),
            _ => ProviderFrameHistorySupportV1::Supported,
        }
    }

    pub fn validate(&self, prior_frontier: u64) -> Result<(), super::ProviderItemValidationError> {
        super::validate::validate_item(self, prior_frontier)
    }
}
