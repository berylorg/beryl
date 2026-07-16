#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadItemLifecycleContract {
    Paired,
    CompletionOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadItemKind {
    UserMessage,
    HookPrompt,
    AgentMessage,
    Plan,
    Reasoning,
    CommandExecution,
    FileChange,
    McpToolCall,
    DynamicToolCall,
    CollabAgentToolCall,
    SubAgentActivity,
    WebSearch,
    ImageView,
    Sleep,
    ImageGeneration,
    EnteredReviewMode,
    ExitedReviewMode,
    ContextCompaction,
}

impl ThreadItemKind {
    #[must_use]
    pub const fn item_type(self) -> &'static str {
        match self {
            Self::UserMessage => "userMessage",
            Self::HookPrompt => "hookPrompt",
            Self::AgentMessage => "agentMessage",
            Self::Plan => "plan",
            Self::Reasoning => "reasoning",
            Self::CommandExecution => "commandExecution",
            Self::FileChange => "fileChange",
            Self::McpToolCall => "mcpToolCall",
            Self::DynamicToolCall => "dynamicToolCall",
            Self::CollabAgentToolCall => "collabAgentToolCall",
            Self::SubAgentActivity => "subAgentActivity",
            Self::WebSearch => "webSearch",
            Self::ImageView => "imageView",
            Self::Sleep => "sleep",
            Self::ImageGeneration => "imageGeneration",
            Self::EnteredReviewMode => "enteredReviewMode",
            Self::ExitedReviewMode => "exitedReviewMode",
            Self::ContextCompaction => "contextCompaction",
        }
    }

    #[must_use]
    pub const fn lifecycle_contract(self) -> ThreadItemLifecycleContract {
        match self {
            Self::SubAgentActivity => ThreadItemLifecycleContract::CompletionOnly,
            _ => ThreadItemLifecycleContract::Paired,
        }
    }
}
