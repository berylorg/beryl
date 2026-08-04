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
//! A provider narrative mismatch instead yields a non-execution
//! [`SameNativeReacquisitionAnchor`]. Reacquisition must resume through a fresh
//! connection to the same managed process before the old subscription is
//! released, so recovered lineage never depends on unacknowledged cold-rollout
//! reconstruction.
//!
//! Typed persistent home failure closes one process-local master command gate
//! before freezing targets. Short mutations linearize through exact scoped
//! permits, while destructor-owned capabilities settle under their connection
//! or router lane and that same gate. A pre-activation loaded projection or
//! same-native reacquisition anchor crosses failure as one complete wrapper;
//! ordinary teardown cannot discard its metadata or retain only a raw lease.
//! A finished cut may then be consumed into one opaque
//! [`PersistentFailurePendingProjectionQuarantine`]. The conversion drains its
//! sealed inventory once, authenticates the complete connection, registry,
//! aggregate target-guard, and connection-barrier topology, exchanges each
//! connection's exact barriers for one retirement-blocking quarantine owner,
//! groups only exact-equal pending wrappers, and retains every candidate lease
//! token and worker hold.
//! All other cut-local authority is settled without backend, storage,
//! unsubscribe, command-admission, publication, or generation-rebind work.
//! Any mismatch installs one inert owning aggregate rather than exposing a
//! partial candidate set. Publication crossing or following checkout is routed
//! into installed inert quarantine ownership, and both success and owning
//! errors expose only bounded content-free metadata.
//!
//! Successful retained-connection adoption remains unpublished and may be consumed only into one
//! exact pending-candidate reauthentication ledger. That ledger stabilizes durable pending-turn
//! facts between exact stable-connection, adopted-epoch, loaded-registry, and recovered-home
//! checks. Accepted candidates remain dormant with their unchanged lease token and replacement
//! worker hold; rejected candidates remain owning and retryable until explicitly disposed. Only a
//! ledger with every candidate accepted or disposed can discharge all connection-quarantine
//! owners and yield candidate-set-converged publication authority.

mod accepted_delivery_recovery;
mod accepted_input_scheduler;
mod active_steering;
mod cancellation;
mod connection;
mod context_compaction;
mod error;
mod execute;
mod execution_error;
mod input_replay;
mod live_source;
mod model;
mod ordinary;
mod persistent_failure;
mod provider_frame;
mod provider_identity;
mod publication;
mod reacquisition;
mod runtime;
mod scheduled_ordinary;
mod service;
mod service_config;
mod service_registry;
mod service_startup;
mod service_supervisor;
mod stop;
#[cfg(feature = "test-faults")]
pub mod test_faults;
mod turn_activation;

pub use accepted_input_scheduler::{AcceptedInputSchedulerDiagnostics, ActiveSteeringRetryState};
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
    PersistentFailureCutCompletion, PersistentFailureCutHandoff, PersistentFailureCutSnapshot,
    PersistentFailureCutState, PersistentFailureGeneration, PersistentFailureNotification,
    PersistentFailureNotificationStatus, PersistentFailurePendingProjectionQuarantine,
    PersistentFailurePendingProjectionQuarantineError,
    PersistentFailurePendingProjectionQuarantineMetadata,
    PersistentFailurePendingProjectionQuarantineReason, PersistentFailureRecoveryInventory,
    PersistentFailureRecoveryInventoryCounts, PersistentFailureRecoveryInventoryError,
    PersistentFailureRecoveryInventoryMetadata, ProjectionServiceGeneration,
};
pub use reacquisition::{
    SameNativeReacquisitionAnchor, SameNativeReacquisitionFailure, SameNativeReacquisitionSuccess,
};
pub use runtime::AdmittedProjectionSession;
pub use scheduled_ordinary::{
    OrdinaryDynamicToolAuthority, ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError,
    ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryExecutionLease,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionProviderFactory,
    ScheduledOrdinaryExecutionUnavailable, ScheduledOrdinaryProviderEpochContext,
    ScheduledOrdinaryRequestPolicy, ScheduledProjectionSessionAuthority,
};
pub use service::{
    AcceptedInputAdmissionExecutionError, AdoptedProjectionCandidateReauthenticationLedger,
    AdoptedUnpublishedProjectionConnectionService,
    CandidateSetConvergedAdoptedProjectionConnectionService, CasProjectionCoordinator,
    HardStopCoordinationOutcome, LiveHomeCommand, PersistentFailureServiceAdoptionError,
    PersistentFailureServiceAdoptionMetadata, PersistentFailureServiceAdoptionReason,
    ProjectionCandidateDispositionOutcome, ProjectionCandidateId,
    ProjectionCandidateLedgerAccessError, ProjectionCandidateLedgerMetadata,
    ProjectionCandidateLedgerSealError, ProjectionCandidateLedgerSealFailure,
    ProjectionCandidateLedgerSealReason, ProjectionCandidateMetadata,
    ProjectionCandidateReauthenticationOutcome, ProjectionCandidateReauthenticationReason,
    ProjectionCandidateReauthenticationStatus, ProjectionConnectionService,
    ProjectionConnectionServiceCloseError, ProjectionConnectionServiceCloseOutcome,
    RecoveredProjectionCandidateMetadata, RecoveredServicePublicationError,
    RecoveredServicePublicationMetadata, RecoveredServicePublicationReason,
    TerminalAdoptedProjectionConnectionService, TerminalAdoptedProjectionConnectionServiceReason,
    UnpublishedProjectionConnectionService, UnpublishedProjectionConnectionServiceBuildError,
    UnpublishedProjectionConnectionServiceMetadata,
};
pub use service_config::{
    ProjectionServiceConfig, ProjectionServiceConfigError, ProjectionWorkerPoolDiagnostics,
};
pub use service_supervisor::{
    RunningProjectionServiceLease, RunningServiceAvailability, RunningSessionRecoveryDiagnostics,
    RunningSessionRecoveryShutdownError, RunningSessionRecoveryStartError,
    RunningSessionRecoverySupervisor,
};
pub use stop::{
    BoundedHardStopResult, HardStopLimitation, HardStopTargetDisposition, HardStopTargetKind,
    HardStopTargetResult, StopCoordinationError, StopCoordinationOutcome, WindowCloseStopBarrier,
    WindowCloseStopBarrierStatus, WindowCloseStopOutcome,
};
pub use turn_activation::PendingTurnActivation;

#[cfg(test)]
pub(in crate::cas_projection) use active_steering::{
    ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome,
};

#[cfg(test)]
mod tests;
