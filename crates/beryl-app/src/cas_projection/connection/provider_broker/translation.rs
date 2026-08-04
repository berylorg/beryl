use backend::{
    ProviderContainer as BackendContainer, ProviderDeltaKind as BackendDeltaKind,
    ProviderEnumValue as BackendEnumValue, ProviderField as BackendField,
    ProviderItemKind as BackendItemKind, ProviderItemLifecycle as BackendItemLifecycle,
};
use beryl_backend as backend;
use storage::{
    ProviderContainer as StorageContainer, ProviderDeltaKind as StorageDeltaKind,
    ProviderEnumValue as StorageEnumValue, ProviderField as StorageField,
    ProviderObservationItemKind as StorageItemKind,
    ProviderObservationItemLifecycle as StorageItemLifecycle,
};
use syndic_storage as storage;

macro_rules! map_unit_enum {
    ($value:expr, $source:ident, $target:ident, [$($variant:ident),+ $(,)?]) => {
        match $value {
            $($source::$variant => $target::$variant,)+
        }
    };
}

pub(super) fn begin(value: backend::ProviderObservationBegin) -> storage::ProviderObservationBegin {
    match value {
        backend::ProviderObservationBegin::Item { lifecycle, kind } => {
            storage::ProviderObservationBegin::Item {
                lifecycle: map_unit_enum!(
                    lifecycle,
                    BackendItemLifecycle,
                    StorageItemLifecycle,
                    [Started, Completed]
                ),
                kind: item_kind(kind),
            }
        }
        backend::ProviderObservationBegin::Delta { kind } => {
            storage::ProviderObservationBegin::Delta {
                kind: map_unit_enum!(
                    kind,
                    BackendDeltaKind,
                    StorageDeltaKind,
                    [
                        AgentMessage,
                        Plan,
                        ReasoningSummaryPartAdded,
                        ReasoningSummaryText,
                        ReasoningTextObserved,
                        CommandExecutionOutput,
                        FileChangeOutput,
                        FileChangePatchUpdated,
                        McpToolCallProgress,
                    ]
                ),
            }
        }
    }
}

fn item_kind(value: backend::ProviderItemKind) -> storage::ProviderObservationItemKind {
    map_unit_enum!(
        value,
        BackendItemKind,
        StorageItemKind,
        [
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
        ]
    )
}

pub(super) fn context(value: backend::ProviderValueContext) -> storage::ProviderValueContext {
    match value {
        backend::ProviderValueContext::Field(value) => {
            storage::ProviderValueContext::Field(field(value))
        }
        backend::ProviderValueContext::Structured {
            root,
            depth,
            position,
        } => storage::ProviderValueContext::Structured {
            root: field(root),
            depth,
            position: match position {
                backend::ProviderStructuredPosition::ListElement { index } => {
                    storage::ProviderStructuredPosition::ListElement { index }
                }
                backend::ProviderStructuredPosition::ObjectKey { entry } => {
                    storage::ProviderStructuredPosition::ObjectKey { entry }
                }
                backend::ProviderStructuredPosition::ObjectValue { entry } => {
                    storage::ProviderStructuredPosition::ObjectValue { entry }
                }
            },
        },
    }
}

fn field(value: backend::ProviderField) -> storage::ProviderField {
    map_unit_enum!(
        value,
        BackendField,
        StorageField,
        [
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
        ]
    )
}

pub(super) fn control(
    value: backend::ProviderObservationControl,
) -> storage::ProviderObservationControl {
    use backend::ProviderObservationControl as Source;
    use storage::ProviderObservationControl as Target;
    match value {
        Source::BeginField(value) => Target::BeginField(context(value)),
        Source::EndField(value) => Target::EndField(context(value)),
        Source::BeginContainer {
            context: value,
            container,
        } => Target::BeginContainer {
            context: context(value),
            container: map_container(container),
        },
        Source::EndContainer {
            context: value,
            container,
        } => Target::EndContainer {
            context: context(value),
            container: map_container(container),
        },
        Source::BeginElement {
            context: value,
            index,
        } => Target::BeginElement {
            context: context(value),
            index,
        },
        Source::EndElement {
            context: value,
            index,
        } => Target::EndElement {
            context: context(value),
            index,
        },
        Source::BeginObjectEntry { root, depth, entry } => Target::BeginObjectEntry {
            root: field(root),
            depth,
            entry,
        },
        Source::EndObjectEntry { root, depth, entry } => Target::EndObjectEntry {
            root: field(root),
            depth,
            entry,
        },
        Source::Enum {
            context: value,
            value: token,
        } => Target::Enum {
            context: context(value),
            value: enum_value(token),
        },
        Source::Scalar {
            context: value,
            value: scalar,
        } => Target::Scalar {
            context: context(value),
            value: map_scalar(scalar),
        },
    }
}

fn map_container(value: backend::ProviderContainer) -> storage::ProviderContainer {
    map_unit_enum!(value, BackendContainer, StorageContainer, [List, Object])
}

fn map_scalar(value: backend::ProviderScalar) -> storage::ProviderScalar {
    match value {
        backend::ProviderScalar::Null => storage::ProviderScalar::Null,
        backend::ProviderScalar::Boolean(value) => storage::ProviderScalar::Boolean(value),
        backend::ProviderScalar::Signed(value) => storage::ProviderScalar::Signed(value),
        backend::ProviderScalar::Unsigned(value) => storage::ProviderScalar::Unsigned(value),
        backend::ProviderScalar::FiniteFloat(value) => storage::ProviderScalar::FiniteFloat(
            storage::ProviderFiniteF64::new(value.get())
                .expect("backend finite float stays finite"),
        ),
    }
}

fn enum_value(value: backend::ProviderEnumValue) -> storage::ProviderEnumValue {
    map_unit_enum!(
        value,
        BackendEnumValue,
        StorageEnumValue,
        [
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
            Other,
        ]
    )
}
