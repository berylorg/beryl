const HOOK: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!(
        "fragments",
        HookFragments,
        ValueKind::List(ListKind::Object(ObjectSchema::HookFragment)),
        required
    ),
];
const AGENT: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!("text", AgentMessageText, ValueKind::Text, required),
    field!("phase", MessagePhase, ValueKind::Enum(PHASE), optional),
    field!(
        "memoryCitation",
        MemoryCitation,
        ValueKind::Object(ObjectSchema::MemoryCitation),
        optional
    ),
];
const PLAN: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!("text", PlanText, ValueKind::Text, required),
];
const REASONING: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!(
        "summary",
        ReasoningSummaries,
        ValueKind::List(ListKind::Text(F::ReasoningSummary)),
        default
    ),
    field!(
        "content",
        ReasoningSummaries,
        ValueKind::List(ListKind::DiscardText),
        default
    ),
];
const COMMAND: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!("pluginId", McpPluginId, ValueKind::DiscardString, optional),
    field!(
        "scriptPath",
        McpAppContext,
        ValueKind::DiscardString,
        optional
    ),
    field!("command", Command, ValueKind::Text, required),
    field!("cwd", WorkingDirectory, ValueKind::Text, required),
    field!("processId", ProcessId, ValueKind::Text, optional),
    field!(
        "source",
        CommandSource,
        ValueKind::Enum(COMMAND_SOURCE),
        default
    ),
    field!("status", CommandStatus, ValueKind::Enum(STATUS4), required),
    field!(
        "commandActions",
        CommandActions,
        ValueKind::List(ListKind::Object(ObjectSchema::CommandAction)),
        required
    ),
    field!(
        "aggregatedOutput",
        AggregatedOutput,
        ValueKind::Text,
        optional
    ),
    field!("exitCode", ExitCode, ValueKind::Signed32, optional),
    field!("durationMs", DurationMs, ValueKind::Signed, optional),
];
const FILE: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!(
        "status",
        FileChangeStatus,
        ValueKind::Enum(STATUS4),
        required
    ),
    field!(
        "changes",
        FileChanges,
        ValueKind::List(ListKind::Object(ObjectSchema::FileChange)),
        required
    ),
];
const MCP: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!("server", McpServer, ValueKind::Text, required),
    field!("tool", McpTool, ValueKind::Text, required),
    field!("status", McpStatus, ValueKind::Enum(STATUS3), required),
    field!("arguments", McpArguments, ValueKind::Structured, required),
    field!(
        "appContext",
        McpAppContext,
        ValueKind::Object(ObjectSchema::McpAppContext),
        optional
    ),
    field!(
        "mcpAppResourceUri",
        McpResourceUri,
        ValueKind::Text,
        optional
    ),
    field!("pluginId", McpPluginId, ValueKind::Text, optional),
    field!(
        "result",
        McpResult,
        ValueKind::Object(ObjectSchema::McpResult),
        optional
    ),
    field!(
        "error",
        McpError,
        ValueKind::Object(ObjectSchema::McpError),
        optional
    ),
    field!("durationMs", DurationMs, ValueKind::Signed, optional),
];
const DYNAMIC: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!("namespace", DynamicNamespace, ValueKind::Text, optional),
    field!("tool", DynamicTool, ValueKind::Text, required),
    field!(
        "arguments",
        DynamicArguments,
        ValueKind::Structured,
        required
    ),
    field!("status", DynamicStatus, ValueKind::Enum(STATUS3), required),
    field!(
        "contentItems",
        DynamicContentItems,
        ValueKind::List(ListKind::Object(ObjectSchema::DynamicContent)),
        optional
    ),
    field!("success", DynamicSuccess, ValueKind::Boolean, optional),
    field!("durationMs", DurationMs, ValueKind::Signed, optional),
];
const COLLAB: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!("tool", CollabTool, ValueKind::Enum(COLLAB_TOOL), required),
    field!("status", CollabStatus, ValueKind::Enum(STATUS3), required),
    field!(
        "senderThreadId",
        CollabSenderThreadId,
        ValueKind::Text,
        required
    ),
    field!(
        "receiverThreadIds",
        CollabReceiverThreadIds,
        ValueKind::List(ListKind::Text(F::CollabReceiverThreadId)),
        required
    ),
    field!("prompt", CollabPrompt, ValueKind::Text, optional),
    field!("model", CollabModel, ValueKind::Text, optional),
    field!(
        "reasoningEffort",
        CollabReasoningEffort,
        ValueKind::Text,
        optional
    ),
    field!(
        "agentsStates",
        CollabAgentStates,
        ValueKind::AgentStates,
        required
    ),
];
const SUBAGENT: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!(
        "kind",
        SubAgentKind,
        ValueKind::Enum(SUBAGENT_KIND),
        required
    ),
    field!("agentThreadId", SubAgentThreadId, ValueKind::Text, required),
    field!("agentPath", SubAgentPath, ValueKind::Text, required),
];
const WEB: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!("query", WebSearchQuery, ValueKind::Text, required),
    field!(
        "action",
        WebSearchAction,
        ValueKind::Object(ObjectSchema::WebSearchAction),
        optional
    ),
];
const IMAGE_VIEW: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!("path", ImageViewPath, ValueKind::Text, required),
];
const SLEEP: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!("durationMs", SleepDurationMs, ValueKind::Unsigned, required),
];
const IMAGE_GENERATION: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!(
        "status",
        ImageGenerationStatus,
        ValueKind::Enum(STATUS3),
        required
    ),
    field!(
        "revisedPrompt",
        ImageGenerationRevisedPrompt,
        ValueKind::Text,
        optional
    ),
    field!(
        "result",
        ImageGenerationStatus,
        ValueKind::DiscardString,
        required
    ),
    field!(
        "savedPath",
        ImageGenerationSavedPath,
        ValueKind::Text,
        optional
    ),
];
const ENTER_REVIEW: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!("review", EnteredReview, ValueKind::Text, required),
];
const EXIT_REVIEW: &[FieldSpec] = &[
    field!("id", ItemId, ValueKind::ItemId, required),
    field!("review", ExitedReview, ValueKind::Text, required),
];
const COMPACTION: &[FieldSpec] = &[field!("id", ItemId, ValueKind::ItemId, required)];

pub(super) fn item_fields(kind: ProviderItemKind) -> &'static [FieldSpec] {
    match kind {
        ProviderItemKind::HookPrompt => HOOK,
        ProviderItemKind::AgentMessage => AGENT,
        ProviderItemKind::Plan => PLAN,
        ProviderItemKind::Reasoning => REASONING,
        ProviderItemKind::CommandExecution => COMMAND,
        ProviderItemKind::FileChange => FILE,
        ProviderItemKind::McpToolCall => MCP,
        ProviderItemKind::DynamicToolCall => DYNAMIC,
        ProviderItemKind::CollabAgentToolCall => COLLAB,
        ProviderItemKind::SubAgentActivity => SUBAGENT,
        ProviderItemKind::WebSearch => WEB,
        ProviderItemKind::ImageView => IMAGE_VIEW,
        ProviderItemKind::Sleep => SLEEP,
        ProviderItemKind::StandaloneImageGeneration => IMAGE_GENERATION,
        ProviderItemKind::EnteredReviewMode => ENTER_REVIEW,
        ProviderItemKind::ExitedReviewMode => EXIT_REVIEW,
        ProviderItemKind::ContextCompaction => COMPACTION,
    }
}
