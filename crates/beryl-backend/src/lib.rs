//! Protocol and transport-facing types for Beryl's Codex App Server boundary.
//!
//! A foreground candidate receives its immutable full-profile ingress configuration before its
//! authenticated WebSocket handshake. Request-only WebSocket and detached stdio sessions are
//! different construction-time policies and cannot be promoted into that profile.
//!
//! Initialization, fixed config read, one-page model discovery, the pinned compatibility sequence,
//! foreground thread start, resume, fork, and unsubscribe, request-only metadata-only thread reads,
//! full-profile streamed `turn/start`, and correlation-bearing streamed `turn/steer` use
//! method-owned writers and the final incremental response lane. Exact foreground
//! `turn/interrupt` exposes non-interchangeable durable-stop and volatile persistent-failure
//! authorization/outcome families over the same writer and ordered driver. Optional coarse thread
//! cleanup accepts only durable-stop authority. Exact foreground `thread/compact/start` uses a
//! distinct idle-thread authorization and returns only its attempt-correlated enqueue disposition
//! while lifecycle remains on the ordered sink;
//! [`ManagedBackendSession::admits_exact_thread_background_terminals_cleanup`] exposes only the
//! locally established pinned-release capability without sending a destructive probe. No generic
//! whole-value request path or aggregate hard-stop facade is present.
//!
//! The response decoder produces bounded facts. Protocol identities, display labels, cursors, and
//! diagnostics use fixed inline storage; a matching `model/list` response creates exactly one
//! fixed-size boxed [`ModelPage`], and callers request at most 64 records at a time. The
//! compatibility sequence validates and releases its page immediately. Thread-lineage responses
//! retain only bounded thread identity, closed status, and bounded model/provider/reasoning facts
//! while structurally discarding history and unrecognized members.
//!
//! ```
//! use beryl_backend::{
//!     DefaultReasoningEffort, InitializePlatform, InitializeResponse, ModelDisplayName,
//!     ModelPage, ModelRecord, ProtocolIdentity, ReasoningEffort, SupportedReasoningEfforts,
//! };
//!
//! let initialize = InitializeResponse::try_new(
//!     "beryl/0.146.0",
//!     InitializePlatform::HostWindows,
//! )?;
//! assert_eq!(initialize.user_agent_product(), "beryl/0.146.0");
//!
//! let mut supported = SupportedReasoningEfforts::empty();
//! supported.insert(ReasoningEffort::Medium);
//! let record = ModelRecord::new(
//!     ProtocolIdentity::try_new("stable-id")?,
//!     ProtocolIdentity::try_new("gpt-model")?,
//!     ModelDisplayName::try_new("GPT Model")?,
//!     false,
//!     true,
//!     supported,
//!     DefaultReasoningEffort::Medium,
//! );
//! let mut page = ModelPage::new();
//! page.try_push(record)?;
//! assert_eq!(page.records().next().map(ModelRecord::model), Some("gpt-model"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Foreground approval classification preserves final pre-bind ordering. Every foreground
//! candidate selects a nonzero local prefix capacity before its first transport read. An unbound
//! command-execution or file-change approval enters that FIFO before automatic denial; a full
//! prefix closes the connection without denying the rejected approval. An unbound permission
//! approval instead closes with its response unexercised because no ordered durable stop owner is
//! available. [`ManagedBackendSession::pre_bind_control_diagnostics`] exposes content-free
//! capacity and occupancy evidence.
//!
//! ```
//! use std::num::NonZeroUsize;
//! use beryl_backend::ForegroundSessionConfig;
//!
//! let config = ForegroundSessionConfig::new(NonZeroUsize::new(128).unwrap());
//! assert_eq!(config.pre_bind_control_capacity().get(), 128);
//! ```
//!
//! Exact interruption consumes one session-minted authorization. The caller issues the
//! non-cloneable fence only while its outer target election prevents a successor; backend IDs do
//! not prove that semantic fact.
//!
//! ```
//! use std::time::Duration;
//! use beryl_backend::{
//!     ApprovalInterruption, CallerNoSuccessorFence, ExactForegroundTurn, ManagedBackendError,
//!     ManagedBackendSession, StopAttemptCorrelation, StopAttemptDisposition,
//!     StopOperationCorrelation, TurnInterruptOutcome,
//! };
//! use beryl_model::{
//!     CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
//!     CasTurnId, RuntimeId,
//! };
//!
//! fn interrupt(
//!     session: &mut ManagedBackendSession,
//!     target: ExactForegroundTurn,
//! ) -> Result<TurnInterruptOutcome, ManagedBackendError> {
//!     session.bind_exact_foreground_turn(target.clone())?;
//!     let authorization = session.authorize_exact_foreground_turn(
//!         target,
//!         StopOperationCorrelation::from_bytes([1; 16]),
//!         StopAttemptCorrelation::from_bytes([2; 16]),
//!         CallerNoSuccessorFence::issue(),
//!     )?;
//!     Ok(session.interrupt_exact_foreground_turn(authorization, Duration::from_secs(30)))
//! }
//!
//! let target = ExactForegroundTurn::new(
//!     RuntimeId::from_bytes([3; 16]),
//!     CasLoadedSessionGeneration::new(
//!         CasProcessGeneration::new(1)?,
//!         CasLoadedThreadGeneration::new(1)?,
//!     ),
//!     CasThreadId::new("thread-1")?,
//!     CasTurnId::new("turn-1")?,
//! );
//! assert_eq!(target.turn_id().as_str(), "turn-1");
//! let approval_interruption = ApprovalInterruption::DurableStopOwned {
//!     operation: StopOperationCorrelation::from_bytes([4; 16]),
//!     target: target.clone(),
//!     attempt_disposition: StopAttemptDisposition::ClaimedNotDispatched(
//!         StopAttemptCorrelation::from_bytes([5; 16]),
//!     ),
//! };
//! assert!(matches!(
//!     approval_interruption,
//!     ApprovalInterruption::DurableStopOwned { target: owned, .. }
//!         if owned == target
//! ));
//! # let _ = interrupt;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Persistent-store-failure interruption uses a separately typed volatile capability. Its
//! correlation is process-local and never appears on the provider wire. The returned outcome is
//! diagnostics only: it supplies no durable stop receipt, retry authority, or lifecycle
//! completion.
//!
//! ```
//! use std::time::Duration;
//! use beryl_backend::{
//!     CallerNoSuccessorFence, ExactForegroundTurn, ManagedBackendError, ManagedBackendSession,
//!     PersistentFailureInterruptCorrelation, PersistentFailureInterruptOutcome,
//!     TurnInterruptDisposition,
//! };
//! use beryl_model::{
//!     CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
//!     CasTurnId, RuntimeId,
//! };
//!
//! fn interrupt_for_persistent_store_failure(
//!     session: &mut ManagedBackendSession,
//!     target: ExactForegroundTurn,
//!     correlation: PersistentFailureInterruptCorrelation,
//! ) -> Result<PersistentFailureInterruptOutcome, ManagedBackendError> {
//!     session.bind_exact_foreground_turn(target.clone())?;
//!     let authorization = session.authorize_persistent_failure_interrupt(
//!         target,
//!         correlation,
//!         CallerNoSuccessorFence::issue(),
//!     )?;
//!     let outcome = session.interrupt_for_persistent_failure(
//!         authorization,
//!         Duration::from_secs(30),
//!     );
//!     debug_assert_eq!(outcome.request().correlation(), correlation);
//!     match outcome.disposition() {
//!         TurnInterruptDisposition::RequestAccepted
//!         | TurnInterruptDisposition::RejectedBeforeCoreInterrupt
//!         | TurnInterruptDisposition::ProvenNotDispatched { .. }
//!         | TurnInterruptDisposition::CompletionUnknown { .. } => {}
//!     }
//!     Ok(outcome)
//! }
//!
//! let target = ExactForegroundTurn::new(
//!     RuntimeId::from_bytes([6; 16]),
//!     CasLoadedSessionGeneration::new(
//!         CasProcessGeneration::new(4)?,
//!         CasLoadedThreadGeneration::new(9)?,
//!     ),
//!     CasThreadId::new("thread-failed-store")?,
//!     CasTurnId::new("turn-in-flight")?,
//! );
//! let correlation = PersistentFailureInterruptCorrelation::from_bytes([7; 16]);
//! assert_eq!(correlation.as_bytes(), &[7; 16]);
//! # let _ = (target, interrupt_for_persistent_store_failure);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Context compaction consumes its own exact idle-thread authorization. The local attempt
//! correlation is returned with the outcome but is never serialized as a provider idempotency
//! key. Only the request acknowledgement timeout belongs to this backend call; feature completion
//! remains on the ordered lifecycle stream.
//!
//! ```
//! use std::time::Duration;
//! use beryl_backend::{
//!     CallerNoSuccessorFence, CompactThreadOutcome, CompactionAttemptCorrelation,
//!     ExactForegroundThread, ManagedBackendError, ManagedBackendSession,
//! };
//! use beryl_model::{
//!     CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
//!     RuntimeId,
//! };
//!
//! fn compact(
//!     session: &mut ManagedBackendSession,
//!     target: ExactForegroundThread,
//! ) -> Result<CompactThreadOutcome, ManagedBackendError> {
//!     session.bind_exact_foreground_thread(target.clone())?;
//!     let authorization = session.authorize_exact_foreground_thread(
//!         target,
//!         CompactionAttemptCorrelation::from_bytes([6; 16]),
//!         CallerNoSuccessorFence::issue(),
//!     )?;
//!     Ok(session.compact_exact_foreground_thread(
//!         authorization,
//!         Duration::from_secs(30),
//!     ))
//! }
//!
//! let target = ExactForegroundThread::new(
//!     RuntimeId::from_bytes([7; 16]),
//!     CasLoadedSessionGeneration::new(
//!         CasProcessGeneration::new(2)?,
//!         CasLoadedThreadGeneration::new(3)?,
//!     ),
//!     CasThreadId::new("thread-idle")?,
//! );
//! assert_eq!(target.thread_id().as_str(), "thread-idle");
//! # let _ = compact;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Recovery context crosses the boundary through a compact preflight and a
//! revision-bound sequential source; no caller-owned item collection or raw
//! app-server response item is accepted.
//!
//! ```
//! use std::num::NonZeroUsize;
//!
//! use beryl_backend::{
//!     THREAD_INJECTION_MAX_PAGE_BYTES, ThreadInjectionPreflight, ThreadInjectionRole,
//!     ThreadInjectionSource, ThreadInjectionSourceError, ThreadInjectionSourceIdentity,
//!     ThreadInjectionSourcePage, ThreadInjectionSourceRevision,
//! };
//! use beryl_model::{RecoveryItemSequenceAccumulator, RecoveryItemSequenceRole};
//! use beryl_stream::{PageLease, PagePool};
//!
//! const TEXT: &str = "recovered question";
//! let identity = ThreadInjectionSourceIdentity::new([7; 32]);
//! let revision = ThreadInjectionSourceRevision::new(4);
//! let mut digest = RecoveryItemSequenceAccumulator::new(1, TEXT.len() as u64);
//! digest.begin_item(1, RecoveryItemSequenceRole::UserInputText, TEXT.len() as u64)?;
//! digest.update_text(TEXT.as_bytes())?;
//! digest.finish_item()?;
//! let preflight = ThreadInjectionPreflight::new(
//!     identity,
//!     revision,
//!     1,
//!     TEXT.len() as u64,
//!     digest.finish()?,
//! )?;
//! assert_eq!(preflight.item_count(), 1);
//!
//! let pool = PagePool::new(
//!     NonZeroUsize::new(THREAD_INJECTION_MAX_PAGE_BYTES).unwrap(),
//!     NonZeroUsize::new(1).unwrap(),
//! )?;
//! let mut page = pool.try_lease()?;
//! page.buffer_mut()[..TEXT.len()].copy_from_slice(TEXT.as_bytes());
//! page.set_len(TEXT.len())?;
//!
//! struct OnePageSource {
//!     page: Option<PageLease>,
//! }
//! impl ThreadInjectionSource for OnePageSource {
//!     fn next_page(
//!         &mut self,
//!         max_utf8_bytes: usize,
//!     ) -> Result<Option<ThreadInjectionSourcePage>, ThreadInjectionSourceError> {
//!         let Some(page) = self.page.take() else { return Ok(None); };
//!         assert!(max_utf8_bytes >= TEXT.len());
//!         ThreadInjectionSourcePage::new(
//!             ThreadInjectionSourceIdentity::new([7; 32]),
//!             ThreadInjectionSourceRevision::new(4),
//!             1,
//!             ThreadInjectionRole::UserInputText,
//!             TEXT.len() as u64,
//!             0,
//!             page,
//!             true,
//!             true,
//!         ).map(Some)
//!     }
//! }
//! let _source = OnePageSource { page: Some(page) };
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Submitted input crosses the outbound boundary through one owned non-cloneable
//! descriptor source. Request encoding and both lifecycle echoes independently
//! replay its count-and-digest-bound descriptor sequence.
//!
//! ```
//! use std::time::Duration;
//!
//! use beryl_backend::{
//!     ClientUserMessageId, ManagedBackendSession, StreamedInputSequenceDigestAccumulator,
//!     StreamedInputSource, TextSourceProof, TurnStartOutcome, TurnSteerOutcome,
//! };
//! use beryl_model::{CasThreadId, CasTurnId};
//!
//! fn start_from_broker(
//!     session: &mut ManagedBackendSession,
//!     thread_id: &CasThreadId,
//!     source: Box<dyn StreamedInputSource>,
//! ) -> TurnStartOutcome {
//!     session.start_turn_with_streamed_input(thread_id, source, Duration::from_secs(30))
//! }
//!
//! fn steer_from_broker(
//!     session: &mut ManagedBackendSession,
//!     thread_id: &CasThreadId,
//!     expected_turn_id: &CasTurnId,
//!     correlation: &ClientUserMessageId,
//!     source: Box<dyn StreamedInputSource>,
//! ) -> TurnSteerOutcome {
//!     session.steer_turn_with_streamed_input(
//!         thread_id,
//!         expected_turn_id,
//!         correlation,
//!         source,
//!         Duration::from_secs(30),
//!     )
//! }
//!
//! let proof = TextSourceProof::new([0x42; 32]);
//! let mut digest = StreamedInputSequenceDigestAccumulator::new(1);
//! digest.push_text(1, proof, "hello".len() as u64)?;
//! let sequence_digest = digest.finish()?;
//! assert_ne!(sequence_digest.as_bytes(), &[0; 32]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
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
//!
//! Dynamic-tool calls cross the ordered stream as a compact begin, structural argument controls,
//! same-page scalar fragments, and one seal or abandonment. A sink selects its feature-owned
//! builder from `DynamicBegin` before accepting argument bytes.
//!
//! ```
//! use beryl_backend::OrderedTurnStreamOperation;
//!
//! fn observe_dynamic_operation(operation: &OrderedTurnStreamOperation) {
//!     match operation {
//!         OrderedTurnStreamOperation::DynamicBegin(call) => {
//!             assert_eq!(call.method(), "item/tool/call");
//!         }
//!         OrderedTurnStreamOperation::DynamicArgumentControl(_)
//!         | OrderedTurnStreamOperation::DynamicArgumentFragment(_)
//!         | OrderedTurnStreamOperation::DynamicAcquirePage
//!         | OrderedTurnStreamOperation::DynamicSeal
//!         | OrderedTurnStreamOperation::DynamicAbandon(_) => {}
//!         _ => {}
//!     }
//! }
//! ```
//!
//! Loaded-thread closure crosses that same synchronous ordered boundary as one validated CAS
//! thread identity and carries no consumer policy.
//!
//! ```
//! use beryl_backend::OrderedTurnStreamOperation;
//!
//! fn closed_thread_id(operation: &OrderedTurnStreamOperation) -> Option<&str> {
//!     match operation {
//!         OrderedTurnStreamOperation::ThreadClosed(closed) => Some(closed.thread_id().as_str()),
//!         _ => None,
//!     }
//! }
//! ```

mod auth;
mod command;
mod dynamic_tool;
mod exact_interruption;
mod foreground;
mod hard_stop;
mod incoming_json;
mod managed_process;
mod ordered_turn_stream;
mod persistent_failure_interrupt;
mod protocol;
mod provider_observation;
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

#[cfg(feature = "lifecycle-test-support")]
#[doc(hidden)]
pub use websocket_transport::diagnostics::{WebSocketDiagnostics, WebSocketDiagnosticsSnapshot};

pub use command::{
    BackendCommandLine, BackendCommandLineError, BackendWebSocketEndpoint,
    ManagedBackendLaunchSpec, ManagedBackendLaunchSpecError,
};
pub use dynamic_tool::{
    DYNAMIC_TOOL_CALL_REQUEST_ID_MAX_BYTES, DYNAMIC_TOOL_NAMESPACE_MAX_BYTES,
    DynamicToolArgumentContainer, DynamicToolArgumentControl, DynamicToolArgumentFragment,
    DynamicToolArgumentScalarKind, DynamicToolCall, DynamicToolCallAbandonReason,
    DynamicToolCallError, DynamicToolCallOutputContentItem, DynamicToolCallRequestId,
    DynamicToolCallResponse, DynamicToolCallResponseDisposition, DynamicToolCallSchemaError,
    DynamicToolFunctionSpec, DynamicToolNamespaceSpec, DynamicToolSpec,
};
pub use foreground::{ForegroundSessionConfig, PreBindControlDiagnostics};
pub use hard_stop::{
    CallerNoSuccessorFence, CoarseThreadCleanupDisposition, CoarseThreadCleanupOutcome,
    ExactForegroundTurn, ExactForegroundTurnAuthorization, ExactForegroundTurnRequest,
    ExactHardStopLimitation, SameSessionCleanupOrdering, StopAttemptCorrelation,
    StopAttemptDisposition, StopOperationCorrelation, TurnInterruptDisposition,
    TurnInterruptOutcome,
};
pub use incoming_json::ForegroundIngressError;
pub use ordered_turn_stream::{
    ApprovalInterruption, ApprovalOperationCompletion, OrderedTurnStreamBindingError,
    OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamProgress,
    OrderedTurnStreamRejection, OrderedTurnStreamSink, OrderedTurnStreamSubmitCause,
    OrderedTurnStreamSubmitError,
};
pub use persistent_failure_interrupt::{
    PersistentFailureInterruptAuthorization, PersistentFailureInterruptCorrelation,
    PersistentFailureInterruptOutcome, PersistentFailureInterruptRequest,
};
pub use protocol::{
    BackendConfigDefaults, BoundedResponseResult, BoundedResponseTextError, CompatibilityError,
    CompatibilityProbe, CompatibilityProbeResult, CompatibilityProbeSet, ConfigReadResponse,
    DefaultReasoningEffort, EmptyAcknowledgement, InitializePlatform, InitializeResponse,
    JSON_RPC_DIAGNOSTIC_MAX_BYTES, JsonRpcError, JsonRpcErrorVerdict, JsonRpcTurnKind,
    MODEL_CURSOR_MAX_BYTES, MODEL_DISPLAY_NAME_MAX_BYTES, MODEL_PAGE_MAX_RECORDS, ModelDisplayName,
    ModelListOptions, ModelPage, ModelPageCapacityError, ModelPageCursor, ModelPageLimit,
    ModelPageLimitError, ModelRecord, PROTOCOL_IDENTITY_MAX_BYTES, ProtocolIdentity,
    REQUIRED_CODEX_APP_SERVER_VERSION, ReasoningEffort, SupportedReasoningEfforts,
};
pub use provider_observation::{
    ProviderContainer, ProviderDeltaKind, ProviderEnumValue, ProviderField, ProviderFiniteF64,
    ProviderItemKind, ProviderItemLifecycle, ProviderObservationAbandonReason,
    ProviderObservationBegin, ProviderObservationControl, ProviderObservationError,
    ProviderObservationFragment, ProviderObservationRoute, ProviderObservationSchemaError,
    ProviderScalar, ProviderStructuredPosition, ProviderValueContext,
};
pub use server::{
    ManagedBackendClientConnector, ManagedBackendLaunchIdentity, ManagedBackendServer,
};
pub use session::{
    CompactThreadDisposition, CompactThreadOutcome, CompactThreadRequest,
    CompactionAttemptCorrelation, ExactForegroundThread, ExactForegroundThreadAuthorization,
    ManagedBackendError, ManagedBackendProbeReport, ManagedBackendSession, ManagedWebSocketError,
    NonIdempotentRequestOutcome, TurnStartOutcome, TurnSteerOutcome,
};
pub use thread_branch::ThreadBranchCapabilities;
pub use thread_injection::{
    THREAD_INJECTION_MAX_ITEMS, THREAD_INJECTION_MAX_PAGE_BYTES, THREAD_INJECTION_MAX_TEXT_BYTES,
    ThreadInjectionOutcome, ThreadInjectionPreflight, ThreadInjectionPreflightError,
    ThreadInjectionRejection, ThreadInjectionRole, ThreadInjectionSource,
    ThreadInjectionSourceError, ThreadInjectionSourceIdentity, ThreadInjectionSourcePage,
    ThreadInjectionSourceRevision,
};
pub use thread_lineage::{
    FreshIdleThread, FreshLoadedThreadSession, FreshThreadNotIdle, LoadedThreadSession,
    ThreadApprovalPolicy, ThreadLineageResponse, ThreadLoadOptions, ThreadSandboxMode,
};
pub use thread_metadata::{THREAD_AGENT_NICKNAME_MAX_BYTES, ThreadReadMetadata};
pub use turn::{
    APPROVAL_REQUEST_ID_MAX_BYTES, ApprovalRequest, ApprovalRequestId, ApprovalRequestKind,
    ApprovalRequestSchemaError, ApprovalResponseDisposition, CLIENT_USER_MESSAGE_ID_MAX_BYTES,
    CheckedSteeringUserMessage, CheckedSteeringUserMessageSubmitError, CheckedUserMessage,
    ClientUserMessageId, CodexErrorInfo, ImageDetail, ItemLifecycleTimestampMs, LoadedThreadStatus,
    NORMAL_TURN_TERMINAL_DIAGNOSTIC_MAX_BYTES, NormalTurnTerminal, NormalTurnTerminalStatus,
    STREAMED_TEXT_MAX_PAGE_BYTES, StartedTurn, SteeredTurn, SteeringUserMessageAbandonReason,
    SteeringUserMessageError, SteeringUserMessageSelection, SteeringUserMessageSelectionError,
    SteeringUserMessageSource, StreamedInputDescriptor, StreamedInputDescriptorKind,
    StreamedInputHeader, StreamedInputSequenceDigest, StreamedInputSequenceDigestAccumulator,
    StreamedInputSequenceDigestError, StreamedInputSource, StreamedInputSourceError,
    StreamedInputSourceIdentity, StreamedInputSourceRevision, StreamedLocalImageDescriptor,
    StreamedTextDescriptor, StreamedTextPage, StreamedTextSourceId, StreamedUserMessageCorrelation,
    StreamedUserMessageCorrelationError, TerminalNonSteerableTurnKind, TextSourceProof,
    ThreadActiveFlags, ThreadClosed, ThreadSessionMetadata, ThreadStartOptions, ThreadStatus,
    ThreadStatusChanged, ThreadTokenUsage, ThreadUnsubscribeResponse, ThreadUnsubscribeStatus,
    TokenUsageBreakdown, TurnStartOptions, TurnStarted, TurnStatus, TurnSteerResponseWire,
    UserMessageEchoLifecycle,
};
