//! Backend launch and protocol-facing types for Beryl.
//!
//! ```no_run
//! use std::time::Duration;
//!
//! use beryl_backend::ManagedBackendServer;
//! use beryl_model::workspace::WorkspaceId;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let workspace = WorkspaceId::host_windows(r"C:\work\beryl");
//! let (mut server, mut foreground, _report) =
//!     ManagedBackendServer::launch_and_probe_for_workspace(
//!         workspace,
//!         Duration::from_secs(30),
//!     )?;
//! let connector = server.client_connector();
//! let mut background = connector.connect_client(Duration::from_secs(30))?;
//! # let _ = (&mut server, &mut foreground, &mut background);
//! # Ok(())
//! # }
//! ```
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use beryl_backend::BackendLaunchSpec;
//! use beryl_model::workspace::WorkspaceId;
//!
//! let workspace = WorkspaceId::host_windows(r"C:\work\beryl");
//! let launch = BackendLaunchSpec::managed_stdio_for_workspace(workspace);
//! let command = launch.command_line()?;
//! assert_eq!(command.program(), "codex");
//! # Ok(())
//! # }
//! ```

mod activity;
mod auth;
mod command;
mod discovery;
mod dynamic_tool;
mod hard_stop;
mod managed_process;
mod protocol;
mod response_sanitizer;
mod server;
mod session;
mod thread_branch;
mod thread_history;
mod turn;
mod websocket_transport;

#[cfg(feature = "lifecycle-test-support")]
#[doc(hidden)]
pub mod lifecycle_test_support;

pub use activity::{
    ToolActivityAgentLabel, ToolActivityCollabAgentSpawnMetadata, ToolActivityEvent,
    ToolActivityFileChangeSummary, ToolActivityLifecycle, ToolActivitySource,
};
pub use auth::ManagedBackendAuthMaterial;
pub use command::{
    BackendCommandLine, BackendCommandLineError, BackendLaunchSpec, BackendTransport,
    BackendWebSocketConfig, BackendWebSocketEndpoint,
};
pub use discovery::{
    DiscoveredWorkspace, DiscoveredWorkspaceThread, RuntimeDiscoveryError, RuntimeDiscoveryReport,
    RuntimeDiscoveryStatus, WorkspacePathError, canonicalize_host_path, canonicalize_wsl_home_path,
    canonicalize_wsl_path, discover_host_runtime, discover_wsl_runtime, list_wsl_distros,
    strip_windows_extended_prefix,
};
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
    ModelInfo, ModelListOptions, ModelListResponse, ProtocolPhase, SortDirection,
    ThreadListOptions, ThreadListResponse, ThreadLoadedListResponse, ThreadSortKey, ThreadSummary,
};
pub use server::{ManagedBackendClientConnector, ManagedBackendServer};
pub use session::{
    ManagedBackendClientOptions, ManagedBackendError, ManagedBackendProbeReport,
    ManagedBackendSession, ManagedBackendStartupProgress, ManagedBackendStartupStage,
    ManagedWebSocketError, ProbeMethodSuccess,
};
pub use thread_branch::{
    ThreadBranchCapabilities, ThreadBranchCapabilityProbe, ThreadBranchCapabilityProbeResult,
    ThreadBranchCapabilityReport, ThreadForkOptions, ThreadForkResponse, ThreadRollbackResponse,
};
pub use thread_history::{
    ThreadReadMetadata, ThreadReadOptions, ThreadReadResponse, ThreadResumeOptions,
    ThreadTurnsListOptions, ThreadTurnsListResponse,
};
pub use turn::{
    AccountRateLimitsResponse, ActiveTurnNotSteerable, AgentMessageItem, ApprovalRequest,
    ApprovalRequestKind, CommandExecutionItem, CommandExecutionStatus, FileChangeItem,
    FileUpdateChange, ImageGenerationItem, NonSteerableTurnKind, PatchApplyStatus, PatchChangeKind,
    RateLimitSnapshot, RateLimitWindow, ReasoningItem, ThreadInfo, ThreadItem,
    ThreadSessionMetadata, ThreadSessionResponse, ThreadStartOptions, ThreadStatus,
    ThreadTokenUsage, ThreadUnsubscribeResponse, ThreadUnsubscribeStatus, TokenUsageBreakdown,
    TurnError, TurnInfo, TurnStartOptions, TurnStartResponse, TurnStatus, TurnSteerResponse,
    TurnStreamEvent, UserInput, UserMessageItem, active_turn_not_steerable_error,
    parse_approval_request, parse_turn_stream_event,
};
