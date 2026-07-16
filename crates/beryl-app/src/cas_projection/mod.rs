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
//! CAS turn identity and exposes only events routed from its exact connection,
//! thread, and loaded-session generation. [`AdmittedProjectionSession`] keeps
//! process/account facts separate through a process-wide bounded snapshot.

mod cancellation;
mod connection;
mod error;
mod execute;
mod execution_error;
mod model;
mod ordinary;
mod publication;
mod runtime;
mod service;

pub use cancellation::ProjectionCancellationToken;
pub use connection::{
    LIVE_EVENT_TARGET_QUEUE_BYTE_LIMIT, LIVE_EVENT_TARGET_QUEUE_COUNT_LIMIT,
    LiveEventConnectionFact, LiveEventConnectionState, LiveEventPoll, LiveEventProcessSnapshot,
    LiveEventRouterSnapshot, LiveEventTarget, LiveEventTargetCloseReason, LiveEventTargetError,
    LiveEventTargetRegistrationError, LoadedProjectionReleaseError, LoadedProjectionReleaseOutcome,
    RoutedLiveEvent,
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
    OrdinaryDynamicToolContext, OrdinaryDynamicToolHandler, OrdinaryNotStartedProjection,
    OrdinaryTurnCaptureLoss, OrdinaryTurnExecutionError, OrdinaryTurnExecutionOutcome,
    OrdinaryTurnExecutionRequest, OrdinaryTurnNotStarted,
};
pub use runtime::AdmittedProjectionSession;
pub use service::CasProjectionCoordinator;

#[cfg(test)]
mod tests;
