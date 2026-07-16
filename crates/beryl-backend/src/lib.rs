//! Protocol and transport-facing types for Beryl's Codex App Server boundary.
//!
//! Foreground connection workers can prove that their initialized session kept
//! the complete notification profile, then account normalized events by their
//! pre-event-parse retained bytes.
//!
//! ```
//! use std::time::Duration;
//!
//! use beryl_backend::{ManagedBackendError, ManagedBackendSession};
//!
//! fn poll(
//!     session: &mut ManagedBackendSession,
//! ) -> Result<Option<usize>, ManagedBackendError> {
//!     assert!(session.has_full_turn_stream());
//!     Ok(session
//!         .poll_turn_stream_envelope(Duration::from_millis(50))?
//!         .map(|envelope| envelope.approximate_retained_bytes()))
//! }
//! ```
//!
//! Recovery context crosses the boundary only as a validated, closed item
//! sequence; callers cannot provide arbitrary app-server response items.
//!
//! ```
//! use beryl_backend::{ThreadInjectionBatch, ThreadInjectionItem};
//!
//! let batch = ThreadInjectionBatch::new(vec![
//!     ThreadInjectionItem::user_input_text("question")?,
//!     ThreadInjectionItem::assistant_output_text("answer")?,
//! ])?;
//! assert_eq!(batch.item_count(), 2);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Non-idempotent turn delivery exposes exact dispatch evidence instead of a
//! retry-oriented error result.
//!
//! ```
//! use beryl_backend::{
//!     NonIdempotentRequestOutcome, TurnSteerOutcome,
//!     active_turn_not_steerable_error,
//! };
//!
//! fn classify(outcome: TurnSteerOutcome) {
//!     match outcome {
//!         NonIdempotentRequestOutcome::ExactResponse { response } => {
//!             let _ = response.turn_id();
//!         }
//!         NonIdempotentRequestOutcome::ExactRejection { error } => {
//!             let _ = active_turn_not_steerable_error(&error);
//!         }
//!         NonIdempotentRequestOutcome::ProvenNotDispatched { error } => {
//!             eprintln!("safe to reconsider delivery: {error}");
//!         }
//!         NonIdempotentRequestOutcome::CompletionUnknown { error } => {
//!             eprintln!("must not replay automatically: {error}");
//!         }
//!     }
//! }
//! ```
//!
//! Foreground lifecycle events use the pinned closed item vocabulary. Every
//! item-specific delta carries the item kind it is valid for; terminal events
//! carry status and typed error authority rather than an item snapshot.
//!
//! ```
//! use beryl_backend::{
//!     ItemDeltaPayload, ThreadItemKind, TurnStreamEvent,
//!     parse_turn_stream_event,
//! };
//! use serde_json::json;
//!
//! let event = parse_turn_stream_event(
//!     "item/plan/delta",
//!     Some(json!({
//!         "threadId": "thread-1",
//!         "turnId": "turn-1",
//!         "itemId": "plan-1",
//!         "delta": "Implement the closed boundary."
//!     })),
//! )?
//! .expect("known notification");
//!
//! let TurnStreamEvent::ItemDelta(delta) = event else {
//!     unreachable!("plan deltas normalize as item deltas");
//! };
//! assert_eq!(delta.expected_item_kind(), ThreadItemKind::Plan);
//! assert!(matches!(delta.payload(), ItemDeltaPayload::Plan { .. }));
//! # Ok::<(), serde_json::Error>(())
//! ```
//!
//! Item lifecycle events retain their required exact millisecond timestamp.
//! Standalone image generation retains only non-binary metadata; its upstream
//! base64 result is removed at transport JSON ingress before normalization.
//!
//! ```
//! use beryl_backend::{ThreadItem, TurnStreamEvent, parse_turn_stream_event};
//! use serde_json::json;
//!
//! let event = parse_turn_stream_event(
//!     "item/completed",
//!     Some(json!({
//!         "threadId": "thread-1",
//!         "turnId": "turn-1",
//!         "completedAtMs": 1_752_689_600_123_u64,
//!         "item": {
//!             "id": "image-1",
//!             "type": "imageGeneration",
//!             "status": "completed",
//!             "savedPath": "generated/image-1.png"
//!         }
//!     })),
//! )?
//! .expect("known notification");
//!
//! let TurnStreamEvent::ItemCompleted {
//!     completed_at_ms,
//!     item: ThreadItem::ImageGeneration(image),
//!     ..
//! } = event else {
//!     unreachable!("expected completed generated media");
//! };
//! assert_eq!(completed_at_ms.get(), 1_752_689_600_123);
//! assert!(image.saved_path.is_some());
//! # Ok::<(), serde_json::Error>(())
//! ```
//!
//! Dynamic-tool registration uses Codex App Server's canonical tagged schema.
//! Functions are grouped under an explicit namespace when they belong to one
//! application-owned registry.
//!
//! ```
//! use beryl_backend::{
//!     DynamicToolFunctionSpec, DynamicToolNamespaceSpec, DynamicToolSpec,
//! };
//! use serde_json::json;
//!
//! let tools = vec![DynamicToolFunctionSpec::new(
//!     "inspect",
//!     "Inspect bounded application state.",
//!     json!({"type": "object"}),
//! )];
//! let registry = DynamicToolSpec::from(DynamicToolNamespaceSpec::new(
//!     "beryl",
//!     "Beryl-owned tools.",
//!     tools,
//! ));
//! assert!(matches!(registry, DynamicToolSpec::Namespace(_)));
//! ```

mod activity;
mod auth;
mod command;
mod discovery;
mod dynamic_tool;
mod hard_stop;
mod incoming_json;
mod managed_process;
mod protocol;
mod server;
mod session;
mod thread_branch;
mod thread_injection;
mod thread_lineage;
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
    DynamicToolFunctionSpec, DynamicToolNamespaceSpec, DynamicToolSpec,
    parse_dynamic_tool_call_request,
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
    ManagedBackendSession, ManagedWebSocketError, NonIdempotentRequestOutcome, ProbeMethodSuccess,
    TurnStartOutcome, TurnSteerOutcome, TurnStreamEnvelope,
};
pub use thread_branch::{
    ThreadBranchCapabilities, ThreadBranchCapabilityProbe, ThreadBranchCapabilityProbeResult,
    ThreadBranchCapabilityReport,
};
pub use thread_injection::{
    THREAD_INJECTION_MAX_ITEMS, THREAD_INJECTION_MAX_TEXT_BYTES, ThreadInjectionBatch,
    ThreadInjectionBatchError, ThreadInjectionItem, ThreadInjectionMessageText,
    ThreadInjectionMessageTextError, ThreadInjectionOutcome, ThreadInjectionRejection,
};
pub use thread_lineage::{
    FreshIdleThread, FreshLoadedThreadSession, FreshThreadNotIdle, LoadedThreadSession,
    ThreadApprovalPolicy, ThreadLoadOptions, ThreadSandboxMode,
};
pub use thread_metadata::ThreadReadMetadata;
pub use turn::{
    AccountRateLimitsResponse, ActiveTurnNotSteerable, AgentMessageItem, ApprovalRequest,
    ApprovalRequestKind, ApprovalResponseDisposition, ByteRange, CodexErrorInfo, CollabAgentState,
    CollabAgentStatus, CollabAgentTool, CollabAgentToolCallItem, CollabAgentToolCallStatus,
    CommandAction, CommandExecutionItem, CommandExecutionSource, CommandExecutionStatus,
    CompletedTurn, CompletedTurnStatus, ContextCompactionItem, DynamicToolCallItem,
    DynamicToolCallStatus, EnteredReviewModeItem, ExitedReviewModeItem, FileChangeItem,
    FileUpdateChange, HookPromptFragment, HookPromptItem, ImageDetail, ImageGenerationItem,
    ImageViewItem, ItemDelta, ItemDeltaPayload, ItemLifecycleTimestampMs, McpToolCallAppContext,
    McpToolCallError, McpToolCallItem, McpToolCallResult, McpToolCallStatus, MemoryCitation,
    MemoryCitationEntry, NonSteerableTurnKind, PatchApplyStatus, PatchChangeKind, PlanItem,
    RateLimitSnapshot, RateLimitWindow, ReasoningItem, SleepItem, StartedTurn, SteeredTurn,
    SubAgentActivityItem, SubAgentActivityKind, TerminalNonSteerableTurnKind, TextElement,
    ThreadInfo, ThreadItem, ThreadItemKind, ThreadItemLifecycleContract, ThreadSessionMetadata,
    ThreadSessionResponse, ThreadStartOptions, ThreadStatus, ThreadTokenUsage,
    ThreadUnsubscribeResponse, ThreadUnsubscribeStatus, TokenUsageBreakdown, TurnError,
    TurnStartOptions, TurnStatus, TurnStreamEvent, UserInput, UserMessageItem, WebSearchAction,
    WebSearchItem, active_turn_not_steerable_error, parse_approval_request,
    parse_turn_stream_event,
};
