//! Non-GPUI coordination primitives for one exclusive CAS projection.
//!
//! This module correlates exact home, runtime, managed-process, loaded-thread,
//! Syndic-thread, and durable binding facts. Durable binding authority remains
//! in `syndic-storage`, while backend transport and protocol authority remain
//! in `beryl-backend`.
//!
//! Projection acquisition performs storage, protocol, and bounded retry waits
//! synchronously and must run on a non-GPUI worker.
//!
//! A loaded projection is consumed into exactly one [`LiveEventTarget`] before
//! an ordinary turn starts. The target binds once to the returned or observed
//! CAS turn identity and exposes only feature-owned operations routed from its
//! exact connection, thread, and loaded-session generation. [`AdmittedProjectionSession`] keeps
//! process/account facts separate through a process-wide bounded snapshot.
//!
//! Typed persistent home failure closes one process-local master command gate
//! before freezing targets. Short mutations linearize through exact scoped
//! permits, while destructor-owned capabilities settle under their connection
//! or router lane and that same gate. The terminal close path joins the cut,
//! settles every retained local registry and connection authority, shuts down
//! the old scheduler, compaction worker, connections, and execution provider,
//! and returns only bounded content-free [`PersistentFailureTerminalEvidence`].
//! No failed-generation service, connection, worker, or publication authority
//! crosses that boundary. Running-session recovery remains unavailable after
//! this terminal disposition.

mod accepted_delivery_recovery;
mod accepted_input_scheduler;
mod active_steering;
mod cancellation;
mod connection;
mod context_compaction;
mod error;
mod execute;
mod execution_error;
mod initial_start;
mod input_replay;
mod live_source;
mod model;
mod native_lineage_recovery;
mod ordinary;
mod persistent_failure;
mod process_sessions;
mod process_tools;
mod provider_frame;
mod provider_identity;
mod publication;
mod runtime;
mod runtime_interest;
mod scheduled_ordinary;
mod service;
mod service_config;
mod service_registry;
mod service_supervisor;
mod stop;
#[cfg(feature = "test-faults")]
pub mod test_faults;
mod turn_activation;

pub use accepted_input_scheduler::{AcceptedInputSchedulerDiagnostics, ActiveSteeringRetryState};
pub use beryl_home_store::{
    DURABLE_START_ADMISSION_BUDGET_BYTES, MinimumTurnCaptureReserve, TurnStartAdmissionRequirement,
    TurnStartAdmissionRequirementError,
};
pub use cancellation::ProjectionCancellationToken;
pub use connection::{
    LiveEventConnectionFact, LiveEventConnectionState, LiveEventPoll, LiveEventProcessSnapshot,
    LiveEventRouterSnapshot, LiveEventTarget, LiveEventTargetCloseReason,
    LiveEventTargetRegistrationError, LoadedProjectionReleaseError, LoadedProjectionReleaseOutcome,
    RecoveryReplayCapacityDiagnostics, RecoveryReplayDiagnosticsObserver,
    RecoveryReplayDiagnosticsSnapshot, RoutedApproval, RoutedDynamicToolCall,
};
#[cfg(feature = "test-faults")]
#[doc(hidden)]
pub use context_compaction::{
    ContextCompactionCapacityTestGuard, ContextCompactionLifecycleTestHarness,
    ContextCompactionSettlementPauseController, ContextCompactionStagingPauseController,
    ContextCompactionTerminalResponseTestOutcome, ContextCompactionWaitTestHarness,
};
pub use context_compaction::{
    ContextCompactionDiagnostics, ContextCompactionError, ContextCompactionOutcome,
    ContextCompactionRequest, ContextCompactionTimeoutPolicy, ContextCompactionTimeoutSource,
    ResolvedContextCompactionTimeout,
};
pub use error::{
    ProjectionCoordinatorError, ProjectionRegistryKind, ProjectionSessionAdmissionError,
};
pub use execution_error::{ProjectionExecutionError, ProjectionPublicationFailure};
pub use model::{
    CasProjectionRequest, LoadedCasProjection, NativeLineageOperation,
    NativeLineageRecoveryDecision,
};
pub use native_lineage_recovery::{
    NativeLineageRecoveryCommand, NativeLineageRecoveryCommandError, NativeLineageRecoveryControl,
    NativeLineageRecoveryKey, NativeLineageRecoverySnapshot, NativeLineageRecoveryStatus,
};
pub use ordinary::{
    OrdinaryDynamicToolContext, OrdinaryDynamicToolHandlers, OrdinaryNotStartedProjection,
    OrdinaryTurnCaptureLoss, OrdinaryTurnExecutionError, OrdinaryTurnExecutionFailure,
    OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionRequest, OrdinaryTurnNotStarted,
};
#[cfg(feature = "test-faults")]
#[doc(hidden)]
pub use ordinary::{
    OrdinaryInputReplayDiagnostics, OrdinaryInputReplayDiagnosticsSnapshot,
    SourcePageHandoffBarrierController,
};
pub use persistent_failure::{
    LiveCommandAdmissionError, LiveCommandAuthorizer, LiveCommandPermit,
    PersistentFailureCutCompletion, PersistentFailureCutSnapshot, PersistentFailureCutState,
    PersistentFailureGeneration, PersistentFailureNotification,
    PersistentFailureNotificationStatus, PersistentFailureTerminalEvidence,
    ProjectionServiceGeneration,
};
pub use process_sessions::{
    ProcessScheduledExecutionProvider, RuntimeSessionPreparationConfig,
    RuntimeSessionPreparationError, RuntimeTokenDirectories, ScheduledExecutionProviderContext,
    ScheduledExecutionSessions, ScheduledSessionDiagnostics, ScheduledSessionFact,
    ScheduledSessionPreparationFact, ScheduledSessionRegistration,
    ScheduledSessionRegistrationError, ScheduledSessionWorkCursor, ScheduledSessionWorkError,
    ScheduledSessionWorkPage, ScheduledSessionWorkPageLimits, ScheduledSessionWorkRecord,
    ScheduledSessionWorkRevision, ScheduledSessionWorkState,
};
pub use process_tools::ProcessOrdinaryDynamicToolAuthority;
pub use runtime::AdmittedProjectionSession;
pub use runtime_interest::{
    RuntimeActivityPeriod, RuntimeFailure, RuntimeFailureSnapshot, RuntimeInterest,
    RuntimeInterestConfig, RuntimeInterestError, RuntimeInterestKind, RuntimeInterestStatus,
    RuntimeReadiness, RuntimeSessionAdmissionError,
};
#[cfg(feature = "test-faults")]
pub use runtime_interest::{RuntimeInterestTestHarness, RuntimeInterestTestProbe};
pub use scheduled_ordinary::{
    OrdinaryDynamicToolAuthority, ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError,
    ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionLease,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
    ScheduledOrdinaryRequestPolicy, ScheduledProjectionSessionAuthority,
};
pub use service::{
    CasProjectionCoordinator, LiveHomeCommand, ProjectionConnectionService,
    ProjectionConnectionServiceCloseError, ProjectionConnectionServiceCloseOutcome,
};
pub use service_config::{
    ProjectionServiceConfig, ProjectionServiceConfigError, ProjectionWorkerPoolDiagnostics,
};
pub use stop::{
    ProcessLifecycleYieldHandler, StopCoordinationError, StopCoordinationOutcome,
    WindowCloseStopBarrier, WindowCloseStopBarrierStatus, WindowCloseStopOutcome,
};
pub use turn_activation::PendingTurnActivation;

#[cfg(test)]
pub(in crate::cas_projection) use active_steering::{
    ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome,
};
