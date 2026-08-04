/// Closed lifecycle method selected before item fields are staged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ProviderObservationItemLifecycle {
    Started,
    Completed,
}

/// Closed captured item vocabulary. Submitted-user correlation is separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ProviderObservationItemKind {
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
    StandaloneImageGeneration,
    EnteredReviewMode,
    ExitedReviewMode,
    ContextCompaction,
}

/// All pinned provider delta methods.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ProviderDeltaKind {
    AgentMessage,
    Plan,
    ReasoningSummaryPartAdded,
    ReasoningSummaryText,
    ReasoningTextObserved,
    CommandExecutionOutput,
    FileChangeOutput,
    FileChangePatchUpdated,
    McpToolCallProgress,
}

/// Schema selected before any size-unbounded public field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderObservationBegin {
    Item {
        lifecycle: ProviderObservationItemLifecycle,
        kind: ProviderObservationItemKind,
    },
    Delta {
        kind: ProviderDeltaKind,
    },
}

/// Exact semantic identity of every string or structured root in the pinned grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ProviderField {
    LifecycleObservedAt,
    ItemId,
    ClientId,
    HookFragments,
    HookFragmentText,
    HookRunId,
    AgentMessageText,
    MessagePhase,
    MemoryCitation,
    MemoryCitationEntries,
    MemoryCitationPath,
    MemoryCitationLineStart,
    MemoryCitationLineEnd,
    MemoryCitationNote,
    MemoryCitationThreadIds,
    MemoryCitationThreadId,
    PlanText,
    ReasoningSummaries,
    ReasoningSummary,
    Command,
    WorkingDirectory,
    ProcessId,
    CommandSource,
    CommandStatus,
    CommandActions,
    CommandActionKind,
    CommandActionCommand,
    CommandActionName,
    CommandActionPath,
    CommandActionQuery,
    AggregatedOutput,
    ExitCode,
    DurationMs,
    FileChangeStatus,
    FileChanges,
    FileChangePath,
    FileChangeDiff,
    FileChangeKind,
    FileChangeMovePath,
    McpServer,
    McpTool,
    McpStatus,
    McpArguments,
    McpAppContext,
    McpConnectorId,
    McpLinkId,
    McpResourceUri,
    McpAppName,
    McpTemplateId,
    McpActionName,
    McpPluginId,
    McpResult,
    McpResultContents,
    McpResultContent,
    McpStructuredContent,
    McpMeta,
    McpError,
    McpErrorMessage,
    DynamicNamespace,
    DynamicTool,
    DynamicArguments,
    DynamicStatus,
    DynamicContentItems,
    DynamicContentItemKind,
    DynamicOutputText,
    DynamicOutputImageLocator,
    DynamicSuccess,
    CollabTool,
    CollabStatus,
    CollabSenderThreadId,
    CollabReceiverThreadIds,
    CollabReceiverThreadId,
    CollabPrompt,
    CollabModel,
    CollabReasoningEffort,
    CollabAgentStates,
    CollabAgentStateKey,
    CollabAgentStateStatus,
    CollabAgentStateMessage,
    SubAgentKind,
    SubAgentThreadId,
    SubAgentPath,
    WebSearchQuery,
    WebSearchAction,
    WebSearchActionKind,
    WebSearchActionQuery,
    WebSearchActionQueryList,
    WebSearchActionQueries,
    WebSearchUrl,
    WebSearchPattern,
    ImageViewPath,
    SleepDurationMs,
    ImageGenerationStatus,
    ImageGenerationRevisedPrompt,
    ImageGenerationSavedPath,
    EnteredReview,
    ExitedReview,
    DeltaSummaryIndex,
    DeltaContentIndex,
    DeltaChanges,
    DeltaText,
    McpProgressMessage,
}

impl ProviderField {
    pub(crate) const ALL: &'static [Self] = &[
        Self::LifecycleObservedAt,
        Self::ItemId,
        Self::ClientId,
        Self::HookFragments,
        Self::HookFragmentText,
        Self::HookRunId,
        Self::AgentMessageText,
        Self::MessagePhase,
        Self::MemoryCitation,
        Self::MemoryCitationEntries,
        Self::MemoryCitationPath,
        Self::MemoryCitationLineStart,
        Self::MemoryCitationLineEnd,
        Self::MemoryCitationNote,
        Self::MemoryCitationThreadIds,
        Self::MemoryCitationThreadId,
        Self::PlanText,
        Self::ReasoningSummaries,
        Self::ReasoningSummary,
        Self::Command,
        Self::WorkingDirectory,
        Self::ProcessId,
        Self::CommandSource,
        Self::CommandStatus,
        Self::CommandActions,
        Self::CommandActionKind,
        Self::CommandActionCommand,
        Self::CommandActionName,
        Self::CommandActionPath,
        Self::CommandActionQuery,
        Self::AggregatedOutput,
        Self::ExitCode,
        Self::DurationMs,
        Self::FileChangeStatus,
        Self::FileChanges,
        Self::FileChangePath,
        Self::FileChangeDiff,
        Self::FileChangeKind,
        Self::FileChangeMovePath,
        Self::McpServer,
        Self::McpTool,
        Self::McpStatus,
        Self::McpArguments,
        Self::McpAppContext,
        Self::McpConnectorId,
        Self::McpLinkId,
        Self::McpResourceUri,
        Self::McpAppName,
        Self::McpTemplateId,
        Self::McpActionName,
        Self::McpPluginId,
        Self::McpResult,
        Self::McpResultContents,
        Self::McpResultContent,
        Self::McpStructuredContent,
        Self::McpMeta,
        Self::McpError,
        Self::McpErrorMessage,
        Self::DynamicNamespace,
        Self::DynamicTool,
        Self::DynamicArguments,
        Self::DynamicStatus,
        Self::DynamicContentItems,
        Self::DynamicContentItemKind,
        Self::DynamicOutputText,
        Self::DynamicOutputImageLocator,
        Self::DynamicSuccess,
        Self::CollabTool,
        Self::CollabStatus,
        Self::CollabSenderThreadId,
        Self::CollabReceiverThreadIds,
        Self::CollabReceiverThreadId,
        Self::CollabPrompt,
        Self::CollabModel,
        Self::CollabReasoningEffort,
        Self::CollabAgentStates,
        Self::CollabAgentStateKey,
        Self::CollabAgentStateStatus,
        Self::CollabAgentStateMessage,
        Self::SubAgentKind,
        Self::SubAgentThreadId,
        Self::SubAgentPath,
        Self::WebSearchQuery,
        Self::WebSearchAction,
        Self::WebSearchActionKind,
        Self::WebSearchActionQuery,
        Self::WebSearchActionQueryList,
        Self::WebSearchActionQueries,
        Self::WebSearchUrl,
        Self::WebSearchPattern,
        Self::ImageViewPath,
        Self::SleepDurationMs,
        Self::ImageGenerationStatus,
        Self::ImageGenerationRevisedPrompt,
        Self::ImageGenerationSavedPath,
        Self::EnteredReview,
        Self::ExitedReview,
        Self::DeltaSummaryIndex,
        Self::DeltaContentIndex,
        Self::DeltaChanges,
        Self::DeltaText,
        Self::McpProgressMessage,
    ];

    pub(crate) const fn tag(self) -> u8 {
        self as u8
    }

    pub(crate) fn from_tag(tag: u8) -> Option<Self> {
        Self::ALL.get(usize::from(tag)).copied()
    }
}

/// Location of a value inside one structured root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderStructuredPosition {
    ListElement { index: u64 },
    ObjectKey { entry: u64 },
    ObjectValue { entry: u64 },
}

/// Allocation-free semantic ownership for a scalar, string, or container.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderValueContext {
    Field(ProviderField),
    Structured {
        root: ProviderField,
        depth: u8,
        position: ProviderStructuredPosition,
    },
}

impl ProviderValueContext {
    pub(crate) const fn root(self) -> ProviderField {
        match self {
            Self::Field(field) | Self::Structured { root: field, .. } => field,
        }
    }
}

/// Container controls are independent of transport and durable page boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ProviderContainer {
    List,
    Object,
}

/// Exact finite IEEE-754 provider value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProviderFiniteF64(u64);

impl ProviderFiniteF64 {
    pub fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value.to_bits()))
    }

    pub(crate) fn from_bits(bits: u64) -> Option<Self> {
        Self::new(f64::from_bits(bits))
    }

    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

/// Exact JSON number classes without integer conversion through `f64`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderScalar {
    Null,
    Boolean(bool),
    Signed(i64),
    Unsigned(u64),
    FiniteFloat(ProviderFiniteF64),
}

/// Every bounded enum token in the pinned provider grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ProviderEnumValue {
    Commentary,
    FinalAnswer,
    Agent,
    UserShell,
    UnifiedExecStartup,
    UnifiedExecInteraction,
    InProgress,
    Completed,
    Failed,
    Declined,
    Add,
    Delete,
    Update,
    SpawnAgent,
    SendInput,
    ResumeAgent,
    Wait,
    CloseAgent,
    PendingInit,
    Running,
    Interrupted,
    Errored,
    Shutdown,
    NotFound,
    SubAgentStarted,
    SubAgentInteracted,
    SubAgentInterrupted,
    Search,
    OpenPage,
    FindInPage,
    InputText,
    InputImage,
    Read,
    ListFiles,
    Unknown,
    /// Pinned Web-search `serde(other)` marker with no retained unknown payload.
    Other,
}

impl ProviderEnumValue {
    pub(crate) const ALL: &'static [Self] = &[
        Self::Commentary,
        Self::FinalAnswer,
        Self::Agent,
        Self::UserShell,
        Self::UnifiedExecStartup,
        Self::UnifiedExecInteraction,
        Self::InProgress,
        Self::Completed,
        Self::Failed,
        Self::Declined,
        Self::Add,
        Self::Delete,
        Self::Update,
        Self::SpawnAgent,
        Self::SendInput,
        Self::ResumeAgent,
        Self::Wait,
        Self::CloseAgent,
        Self::PendingInit,
        Self::Running,
        Self::Interrupted,
        Self::Errored,
        Self::Shutdown,
        Self::NotFound,
        Self::SubAgentStarted,
        Self::SubAgentInteracted,
        Self::SubAgentInterrupted,
        Self::Search,
        Self::OpenPage,
        Self::FindInPage,
        Self::InputText,
        Self::InputImage,
        Self::Read,
        Self::ListFiles,
        Self::Unknown,
        Self::Other,
    ];

    pub(crate) const fn tag(self) -> u8 {
        self as u8
    }

    pub(crate) fn from_tag(tag: u8) -> Option<Self> {
        Self::ALL.get(usize::from(tag)).copied()
    }
}

/// Closed typed control grammar delivered independently of page boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderObservationControl {
    BeginField(ProviderValueContext),
    EndField(ProviderValueContext),
    BeginContainer {
        context: ProviderValueContext,
        container: ProviderContainer,
    },
    EndContainer {
        context: ProviderValueContext,
        container: ProviderContainer,
    },
    BeginElement {
        context: ProviderValueContext,
        index: u64,
    },
    EndElement {
        context: ProviderValueContext,
        index: u64,
    },
    BeginObjectEntry {
        root: ProviderField,
        depth: u8,
        entry: u64,
    },
    EndObjectEntry {
        root: ProviderField,
        depth: u8,
        entry: u64,
    },
    Enum {
        context: ProviderValueContext,
        value: ProviderEnumValue,
    },
    Scalar {
        context: ProviderValueContext,
        value: ProviderScalar,
    },
}
