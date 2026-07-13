//! Protocol and transport-facing types for Beryl's Codex App Server boundary.

mod activity;
mod auth;
mod command;
mod discovery;
mod dynamic_tool;
mod hard_stop;
mod managed_process;
mod protocol;
mod server;
mod session;
mod thread_branch;
mod thread_metadata;
mod turn;
mod websocket_transport;

#[cfg(feature = "lifecycle-test-support")]
#[doc(hidden)]
pub mod lifecycle_test_support;

pub use activity::{
    ToolActivityAgentLabel, ToolActivityCollabAgentSpawnMetadata, ToolActivityEvent,
    ToolActivityFileChangeSummary, ToolActivityLifecycle, ToolActivitySource,
};
pub use command::{BackendCommandLine, BackendCommandLineError, BackendWebSocketEndpoint};
pub use discovery::strip_windows_extended_prefix;
pub use dynamic_tool::{
    DynamicToolCallOutputContentItem, DynamicToolCallRequest, DynamicToolCallResponse,
    DynamicToolSpec, parse_dynamic_tool_call_request,
};
pub use hard_stop::{
    HardStopCapabilities, HardStopCapabilityProbe, HardStopCapabilityProbeResult,
    HardStopCapabilityReport, HardStopTarget, HardStopTargetKind, HardStopTargetOutcome,
};
pub use protocol::{
    BackendConfigDefaults, BackendEvent, CompatibilityError, CompatibilityProbe,
    CompatibilitySnapshot, ConfigReadOptions, ConfigReadResponse, InitializeResponse, JsonRpcError,
    ModelInfo, ModelListOptions, ModelListResponse, ProtocolPhase,
    REQUIRED_CODEX_APP_SERVER_VERSION, ThreadSummary,
};
pub use server::ManagedBackendClientConnector;
pub use session::{
    ManagedBackendClientOptions, ManagedBackendError, ManagedBackendProbeReport,
    ManagedBackendSession, ManagedWebSocketError, ProbeMethodSuccess,
};
pub use thread_branch::{
    ThreadBranchCapabilities, ThreadBranchCapabilityProbe, ThreadBranchCapabilityProbeResult,
    ThreadBranchCapabilityReport, ThreadForkOptions, ThreadForkResponse, ThreadRollbackResponse,
};
pub use thread_metadata::ThreadReadMetadata;
pub use turn::{
    AccountRateLimitsResponse, ActiveTurnNotSteerable, AgentMessageItem, ApprovalRequest,
    ApprovalRequestKind, CommandExecutionItem, CommandExecutionStatus, FileChangeItem,
    FileUpdateChange, GenericThreadItem, ImageGenerationItem, NonSteerableTurnKind,
    PatchApplyStatus, PatchChangeKind, RateLimitSnapshot, RateLimitWindow, ReasoningItem,
    ThreadInfo, ThreadItem, ThreadSessionMetadata, ThreadSessionResponse, ThreadStartOptions,
    ThreadStatus, ThreadTokenUsage, ThreadUnsubscribeResponse, ThreadUnsubscribeStatus,
    TokenUsageBreakdown, TurnError, TurnInfo, TurnItemsView, TurnStartOptions, TurnStartResponse,
    TurnStatus, TurnSteerResponse, TurnStreamEvent, UserInput, UserMessageItem,
    active_turn_not_steerable_error, parse_approval_request, parse_turn_stream_event,
};
