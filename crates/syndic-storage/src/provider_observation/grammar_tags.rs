use super::{ProviderDeltaKind, ProviderObservationItemKind, ProviderObservationItemLifecycle};

impl ProviderObservationItemLifecycle {
    pub(crate) fn from_tag(tag: u8) -> Option<Self> {
        [Self::Started, Self::Completed]
            .get(usize::from(tag))
            .copied()
    }
}

impl ProviderObservationItemKind {
    pub(crate) const ALL: &'static [Self] = &[
        Self::HookPrompt,
        Self::AgentMessage,
        Self::Plan,
        Self::Reasoning,
        Self::CommandExecution,
        Self::FileChange,
        Self::McpToolCall,
        Self::DynamicToolCall,
        Self::CollabAgentToolCall,
        Self::SubAgentActivity,
        Self::WebSearch,
        Self::ImageView,
        Self::Sleep,
        Self::StandaloneImageGeneration,
        Self::EnteredReviewMode,
        Self::ExitedReviewMode,
        Self::ContextCompaction,
    ];

    pub(crate) fn from_tag(tag: u8) -> Option<Self> {
        Self::ALL.get(usize::from(tag)).copied()
    }
}

impl ProviderDeltaKind {
    pub(crate) const ALL: &'static [Self] = &[
        Self::AgentMessage,
        Self::Plan,
        Self::ReasoningSummaryPartAdded,
        Self::ReasoningSummaryText,
        Self::ReasoningTextObserved,
        Self::CommandExecutionOutput,
        Self::FileChangeOutput,
        Self::FileChangePatchUpdated,
        Self::McpToolCallProgress,
    ];

    pub(crate) fn from_tag(tag: u8) -> Option<Self> {
        Self::ALL.get(usize::from(tag)).copied()
    }
}
