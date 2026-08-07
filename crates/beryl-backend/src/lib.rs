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
//! use std::time::Duration;
//!
//! use beryl_backend::{
//!     ManagedBackendSession, ThreadListBudget, ThreadListOptions,
//! };
//!
//! # fn collect_workspace_threads(
//! #     session: &mut ManagedBackendSession,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let options = ThreadListOptions::page(100)
//!     .with_cwd(r"C:\work\beryl")
//!     .updated_descending();
//! let budget = ThreadListBudget::new(Duration::from_secs(10), 8, 512)?;
//! let collection = session.list_threads_bounded(options, budget)?;
//! # let _ = collection;
//! # Ok(())
//! # }
//! ```
//!
//! ```no_run
//! use std::time::Duration;
//!
//! use beryl_backend::{ManagedBackendSession, ThreadForkFailure};
//!
//! # fn prepare_child(session: &mut ManagedBackendSession) {
//! match session.fork_thread_with_commitment("thread_root", Duration::from_secs(30)) {
//!     Ok(child) => {
//!         let child_id = child.thread.summary().id;
//!         # let _ = child_id;
//!     }
//!     Err(ThreadForkFailure::NotCommitted { .. }) => {}
//!     Err(ThreadForkFailure::Indeterminate { .. }) => {
//!         // A backend child may exist, but its identity is not recoverable here.
//!     }
//! }
//! # }
//! ```
//!
//! ```no_run
//! use std::time::Duration;
//!
//! use beryl_backend::ManagedBackendServer;
//! use beryl_model::workspace::RuntimeMode;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut server = ManagedBackendServer::launch(RuntimeMode::HostWindows, r"C:\work\beryl")?;
//! let (mut session, report) = match server.connect_and_probe(Duration::from_secs(30)) {
//!     Ok(ready) => ready,
//!     Err(probe_error) => {
//!         // The launched server is still owned here, so cleanup is explicit and verifiable.
//!         server.shutdown()?;
//!         return Err(Box::new(probe_error));
//!     }
//! };
//! # let _ = report;
//! session.shutdown()?;
//! server.shutdown()?;
//! # Ok(())
//! # }
//! ```
//!
//! ```no_run
//! use std::time::Duration;
//!
//! use beryl_backend::ManagedBackendSession;
//!
//! # fn clean_up_task_owned_thread(
//! #     session: &mut ManagedBackendSession,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! session.archive_thread("thread_123", Duration::from_secs(30))?;
//! let restored = session.unarchive_thread("thread_123", Duration::from_secs(30))?;
//! let restored_id = restored.summary().id;
//! session.delete_thread(&restored_id, Duration::from_secs(30))?;
//! # Ok(())
//! # }
//! ```
//!
//! ```no_run
//! use std::time::Duration;
//!
//! use beryl_backend::{ManagedBackendLaunchOptions, ManagedBackendServer};
//! use beryl_model::workspace::RuntimeMode;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let options =
//!     ManagedBackendLaunchOptions::with_exact_host_windows_program(r"C:\Program Files\Codex\codex.exe")?;
//! let (_server, _session, _report) = ManagedBackendServer::launch_and_probe_with_options(
//!     RuntimeMode::HostWindows,
//!     r"C:\work\beryl",
//!     options,
//!     Duration::from_secs(30),
//! )?;
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
mod thread_lifecycle;
mod turn;
mod websocket_transport;

#[cfg(feature = "lifecycle-test-support")]
#[doc(hidden)]
pub mod lifecycle_test_support;

pub use activity::{
    ToolActivityCollabAgentSpawnMetadata, ToolActivityEvent, ToolActivityFileChangeSummary,
    ToolActivityLifecycle, ToolActivitySource,
};
pub use auth::ManagedBackendAuthMaterial;
pub use command::{
    BackendCommandLine, BackendCommandLineError, BackendLaunchSpec, BackendTransport,
    BackendWebSocketConfig, BackendWebSocketEndpoint, ManagedBackendLaunchOptions,
    ManagedBackendLaunchOptionsError,
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
    ModelInfo, ModelListOptions, ModelListResponse, ProtocolPhase, SortDirection, ThreadListBudget,
    ThreadListBudgetError, ThreadListCollection, ThreadListCollectionStatus, ThreadListOptions,
    ThreadListResponse, ThreadListTruncationReason, ThreadLoadedListResponse, ThreadSortKey,
    ThreadSummary,
};
pub use server::{ManagedBackendClientConnector, ManagedBackendServer};
#[cfg(feature = "lifecycle-test-support")]
#[doc(hidden)]
pub use server::{
    combine_lifecycle_test_shutdown_results, launch_and_probe_lifecycle_test_with_options,
    spawn_lifecycle_test_server,
};
pub use session::{
    ManagedBackendClientOptions, ManagedBackendError, ManagedBackendProbeReport,
    ManagedBackendSession, ManagedBackendStartupProgress, ManagedBackendStartupStage,
    ManagedWebSocketError, ProbeMethodSuccess, ThreadForkFailure, ThreadListCollectionError,
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
