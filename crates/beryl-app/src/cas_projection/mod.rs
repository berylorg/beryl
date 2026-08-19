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
mod ordinary;
mod persistent_failure;
mod provider_frame;
mod provider_identity;
mod publication;
mod runtime;
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
    ContextCompactionStagingPauseController, ContextCompactionTerminalResponseTestOutcome,
    ContextCompactionWaitTestHarness,
};
pub use context_compaction::{
    ContextCompactionDiagnostics, ContextCompactionError, ContextCompactionOutcome,
    ContextCompactionRequest,
};
pub use error::{
    ProjectionCoordinatorError, ProjectionRegistryKind, ProjectionSessionAdmissionError,
};
pub use execution_error::{ProjectionExecutionError, ProjectionPublicationFailure};
pub use model::{
    CasProjectionRequest, LoadedCasProjection, NativeLineageOperation,
    NativeLineageRecoveryDecision,
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
pub use runtime::AdmittedProjectionSession;
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
    StopCoordinationError, StopCoordinationOutcome, WindowCloseStopBarrier,
    WindowCloseStopBarrierStatus, WindowCloseStopOutcome,
};
pub use turn_activation::PendingTurnActivation;

#[cfg(test)]
pub(in crate::cas_projection) use active_steering::{
    ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome,
};
