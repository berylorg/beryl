const HOOK: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(
        HookFragments,
        ValueKind::List(ListKind::HookFragments),
        required
    ),
];
const AGENT: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(AgentMessageText, ValueKind::Text, required),
    field!(MessagePhase, ValueKind::Enum(EnumDomain::Phase), optional),
    field!(
        MemoryCitation,
        ValueKind::Object(ObjectSchema::MemoryCitation),
        optional
    ),
];
const PLAN: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(PlanText, ValueKind::Text, required),
];
const REASONING: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(
        ReasoningSummaries,
        ValueKind::List(ListKind::ReasoningSummaries),
        default
    ),
];
const COMMAND: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(Command, ValueKind::Text, required),
    field!(WorkingDirectory, ValueKind::Text, required),
    field!(ProcessId, ValueKind::Text, optional),
    field!(
        CommandSource,
        ValueKind::Enum(EnumDomain::CommandSource),
        default
    ),
    field!(
        CommandStatus,
        ValueKind::Enum(EnumDomain::Status4),
        required
    ),
    field!(
        CommandActions,
        ValueKind::List(ListKind::CommandActions),
        required
    ),
    field!(AggregatedOutput, ValueKind::Text, optional),
    field!(ExitCode, ValueKind::Signed32, optional),
    field!(DurationMs, ValueKind::Signed, optional),
];
const FILE: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(
        FileChangeStatus,
        ValueKind::Enum(EnumDomain::Status4),
        required
    ),
    field!(
        FileChanges,
        ValueKind::List(ListKind::FileChanges),
        required
    ),
];
const MCP: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(McpServer, ValueKind::Text, required),
    field!(McpTool, ValueKind::Text, required),
    field!(McpStatus, ValueKind::Enum(EnumDomain::Status3), required),
    field!(McpArguments, ValueKind::Structured, required),
    field!(
        McpAppContext,
        ValueKind::Object(ObjectSchema::McpAppContext),
        optional
    ),
    field!(McpResourceUri, ValueKind::Text, optional),
    field!(McpPluginId, ValueKind::Text, optional),
    field!(
        McpResult,
        ValueKind::Object(ObjectSchema::McpResult),
        optional
    ),
    field!(
        McpError,
        ValueKind::Object(ObjectSchema::McpError),
        optional
    ),
    field!(DurationMs, ValueKind::Signed, optional),
];
const DYNAMIC: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(DynamicNamespace, ValueKind::Text, optional),
    field!(DynamicTool, ValueKind::Text, required),
    field!(DynamicArguments, ValueKind::Structured, required),
    field!(
        DynamicStatus,
        ValueKind::Enum(EnumDomain::Status3),
        required
    ),
    field!(
        DynamicContentItems,
        ValueKind::List(ListKind::DynamicContentItems),
        optional
    ),
    field!(DynamicSuccess, ValueKind::Boolean, optional),
    field!(DurationMs, ValueKind::Signed, optional),
];
const COLLAB: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(
        CollabTool,
        ValueKind::Enum(EnumDomain::CollabTool),
        required
    ),
    field!(CollabStatus, ValueKind::Enum(EnumDomain::Status3), required),
    field!(CollabSenderThreadId, ValueKind::Identity, required),
    field!(
        CollabReceiverThreadIds,
        ValueKind::List(ListKind::CollabReceiverThreadIds),
        required
    ),
    field!(CollabPrompt, ValueKind::Text, optional),
    field!(CollabModel, ValueKind::Text, optional),
    field!(CollabReasoningEffort, ValueKind::Text, optional),
    field!(CollabAgentStates, ValueKind::AgentStates, required),
];

const SUBAGENT: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(
        SubAgentKind,
        ValueKind::Enum(EnumDomain::SubAgentKind),
        required
    ),
    field!(SubAgentThreadId, ValueKind::Identity, required),
    field!(SubAgentPath, ValueKind::Text, required),
];
const WEB: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(WebSearchQuery, ValueKind::Text, required),
    field!(
        WebSearchAction,
        ValueKind::Object(ObjectSchema::WebSearchAction),
        optional
    ),
];
const IMAGE_VIEW: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(ImageViewPath, ValueKind::Text, required),
];
const SLEEP: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(SleepDurationMs, ValueKind::Unsigned, required),
];
const IMAGE_GENERATION: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(
        ImageGenerationStatus,
        ValueKind::Enum(EnumDomain::Status3),
        required
    ),
    field!(ImageGenerationRevisedPrompt, ValueKind::Text, optional),
    field!(ImageGenerationSavedPath, ValueKind::Text, optional),
];
const ENTER_REVIEW: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(EnteredReview, ValueKind::Text, required),
];
const EXIT_REVIEW: &[FieldSpec] = &[
    field!(ItemId, ValueKind::Identity, required),
    field!(ExitedReview, ValueKind::Text, required),
];
const COMPACTION: &[FieldSpec] = &[field!(ItemId, ValueKind::Identity, required)];

pub(crate) fn item_fields(kind: ProviderObservationItemKind) -> &'static [FieldSpec] {
    match kind {
        ProviderObservationItemKind::HookPrompt => HOOK,
        ProviderObservationItemKind::AgentMessage => AGENT,
        ProviderObservationItemKind::Plan => PLAN,
        ProviderObservationItemKind::Reasoning => REASONING,
        ProviderObservationItemKind::CommandExecution => COMMAND,
        ProviderObservationItemKind::FileChange => FILE,
        ProviderObservationItemKind::McpToolCall => MCP,
        ProviderObservationItemKind::DynamicToolCall => DYNAMIC,
        ProviderObservationItemKind::CollabAgentToolCall => COLLAB,
        ProviderObservationItemKind::SubAgentActivity => SUBAGENT,
        ProviderObservationItemKind::WebSearch => WEB,
        ProviderObservationItemKind::ImageView => IMAGE_VIEW,
        ProviderObservationItemKind::Sleep => SLEEP,
        ProviderObservationItemKind::StandaloneImageGeneration => IMAGE_GENERATION,
        ProviderObservationItemKind::EnteredReviewMode => ENTER_REVIEW,
        ProviderObservationItemKind::ExitedReviewMode => EXIT_REVIEW,
        ProviderObservationItemKind::ContextCompaction => COMPACTION,
    }
}
