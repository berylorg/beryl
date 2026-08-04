/// Exact semantic identity of every streamed string or key in the pinned grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

/// Location of a value inside one structured-value root.
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

/// Container controls are semantic JSON structure, independent of transport pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderContainer {
    List,
    Object,
}

/// Validated exact finite IEEE-754 provider value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProviderFiniteF64(u64);

impl ProviderFiniteF64 {
    pub fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value.to_bits()))
    }

    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    #[must_use]
    pub fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

/// Exact JSON number classes retained without converting integers through `f64`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderScalar {
    Null,
    Boolean(bool),
    Signed(i64),
    Unsigned(u64),
    FiniteFloat(ProviderFiniteF64),
}

/// Every bounded enum outcome in the pinned provider grammar.
///
/// `Other` is reserved solely for a nonempty unknown Web-search action discriminator. Its
/// unsupported payload is structurally validated and discarded without exposing provider bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    /// A nonempty unknown Web-search action; no other enum domain may produce this value.
    Other,
}

/// Closed typed token/control grammar delivered independently of page boundaries.
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
