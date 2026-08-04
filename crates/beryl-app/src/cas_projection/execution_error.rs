use beryl_backend::{ManagedBackendError, ThreadInjectionPreflightError, ThreadStatus};
use beryl_home_store::{CommandBuildError, CommandError, ReadError};
use beryl_model::{CasProcessGeneration, CasThreadId, RuntimeId};
use syndic_storage::{
    NativeProjectionError, RecoveryProjectionError, SyndicReadError, SyndicRecordError,
    SyndicValueError,
};
use thiserror::Error;

use super::ProjectionCoordinatorError;
use super::{
    LiveEventTargetCloseReason, LoadedProjectionReleaseError, NativeLineageRecoveryDecision,
};
use crate::conversation_tools::ConversationToolRegistryError;

/// Failure to durably publish or reconcile one exact CAS-binding transition.
#[derive(Debug, Error)]
pub enum ProjectionPublicationFailure {
    #[error("Beryl-home revision could not be read before projection publication")]
    HomeRead(#[source] ReadError),
    #[error("projection publication command failed and did not reconcile as exact")]
    Command(#[source] CommandError),
    #[error("projection publication command could not be built")]
    CommandBuild(#[source] CommandBuildError),
    #[error("projection publication left the expected prior binding intact")]
    Prior,
    #[error("projection publication collided with different durable state")]
    Collision,
    #[error("binding revision exhausted while publishing a projection")]
    BindingRevisionExhausted,
    #[error("input-gate revision exhausted while publishing execution state")]
    InputGateRevisionExhausted,
    #[error("projection publication could not be reconciled from Syndic")]
    Reconciliation(#[source] SyndicReadError),
    #[error("projection cleanup lost authority over the exact Beryl-home generation")]
    HomeAuthorityLost(#[source] ProjectionCoordinatorError),
    #[error("projection stale-provenance record could not be constructed")]
    StaleRecord(#[source] SyndicRecordError),
}

/// Closed failures produced while obtaining one native-or-recovered projection.
#[derive(Debug, Error)]
pub enum ProjectionExecutionError {
    #[error(transparent)]
    Coordinator(#[from] ProjectionCoordinatorError),
    #[error(transparent)]
    ConversationTools(#[from] ConversationToolRegistryError),
    #[error("projection runtime {requested} does not match admitted session runtime {admitted}")]
    RuntimeMismatch {
        requested: RuntimeId,
        admitted: RuntimeId,
    },
    /// Replacement connection belongs to another managed CAS process generation.
    #[error("same-native reacquisition requires managed process {expected:?}, not {admitted:?}")]
    ProcessGenerationMismatch {
        expected: CasProcessGeneration,
        admitted: CasProcessGeneration,
    },
    /// Replacement connection is the old connection or already owns another projection.
    #[error("same-native reacquisition for {thread_id} requires a fresh CAS connection")]
    ReacquisitionConnectionNotFresh { thread_id: CasThreadId },
    /// Old subscription anchor disappeared before transfer completed.
    #[error("same-native reacquisition anchor for {thread_id} was lost")]
    ReacquisitionAnchorLost { thread_id: CasThreadId },
    /// Replacement-side resume reservation disappeared before transfer completed.
    #[error("same-native reacquisition reservation for {thread_id} was lost")]
    ReacquisitionReservationLost { thread_id: CasThreadId },
    /// Another operation already owns the quarantined thread handoff.
    #[error("same-native reacquisition is already in progress for {thread_id}")]
    ReacquisitionInProgress { thread_id: CasThreadId },
    /// Durable binding no longer equals the terminal basis captured by the anchor.
    #[error("same-native reacquisition binding changed for {thread_id}")]
    ReacquisitionBindingChanged {
        thread_id: beryl_model::SyndicThreadId,
    },
    /// Anchor belongs to a different Beryl-home identity or generation.
    #[error("same-native reacquisition belongs to another home for {thread_id}")]
    ReacquisitionHomeMismatch {
        thread_id: beryl_model::SyndicThreadId,
    },
    #[error("loaded CAS thread {thread_id} is owned by another exact client connection")]
    LoadedProjectionConnectionMismatch { thread_id: CasThreadId },
    #[error("a loaded-projection lease could not be released cleanly")]
    LeaseRelease(#[source] Box<LoadedProjectionReleaseError>),
    #[error("live-event target {thread_id} closed while a CAS request completed: {reason:?}")]
    LiveEventRouting {
        thread_id: CasThreadId,
        reason: LiveEventTargetCloseReason,
    },
    #[error("durable Syndic projections require persistent CAS threads")]
    EphemeralProjectionThread,
    #[error("CAS projection was cancelled before remote work began")]
    Cancelled,
    #[error("system clock precedes the Unix epoch while observing CAS projection completion")]
    SystemClockBeforeUnixEpoch(#[source] std::time::SystemTimeError),
    #[error("system clock milliseconds exceed the durable Syndic timestamp range")]
    SystemClockOutOfRange,
    #[error(
        "native CAS lineage requires an explicit Retry or Recover from Syndic history decision"
    )]
    NativeLineageRecoveryRequired {
        decision: Box<NativeLineageRecoveryDecision>,
    },
    #[error("native CAS lineage recovery decision for {thread_id} is no longer current")]
    NativeLineageRecoveryDecisionStale {
        thread_id: beryl_model::SyndicThreadId,
    },
    #[error("CAS projection basis changed concurrently for {thread_id}")]
    ProjectionBasisChanged {
        thread_id: beryl_model::SyndicThreadId,
    },
    #[error(transparent)]
    NativePlanning(#[from] NativeProjectionError),
    #[error(transparent)]
    RecoveryProjection(#[from] RecoveryProjectionError),
    #[error(transparent)]
    SyndicRead(#[from] SyndicReadError),
    #[error(transparent)]
    Backend(Box<ManagedBackendError>),
    #[error(transparent)]
    SyndicRecord(#[from] SyndicRecordError),
    #[error(transparent)]
    SyndicValue(#[from] SyndicValueError),
    #[error(transparent)]
    InjectionPreflight(#[from] ThreadInjectionPreflightError),
    #[error("loaded CAS projection thread {thread_id} was not idle: {status:?}")]
    ProjectionThreadNotIdle {
        thread_id: CasThreadId,
        status: ThreadStatus,
    },
    #[error("recovery injection into {thread_id} was rejected ({code}): {message}")]
    InjectionRejected {
        thread_id: CasThreadId,
        code: i64,
        message: Box<str>,
        data_was_present: bool,
    },
    #[error("recovery injection was proven not dispatched for {thread_id}")]
    InjectionNotDispatched {
        thread_id: CasThreadId,
        #[source]
        source: Box<ManagedBackendError>,
    },
    #[error("recovery injection transport was lost for {thread_id}")]
    InjectionTransportLost {
        thread_id: CasThreadId,
        #[source]
        source: Box<ManagedBackendError>,
    },
    #[error("recovery injection completion is unknown for {thread_id}")]
    InjectionCompletionUnknown {
        thread_id: CasThreadId,
        #[source]
        source: Box<ManagedBackendError>,
    },
    #[error("nonempty recovery unexpectedly resolved to an empty native prefix")]
    UnexpectedNativeEmptyRecovery,
    #[error(transparent)]
    Publication(Box<ProjectionPublicationFailure>),
    #[error("projection failed and its abandoned CAS target could not be fully retired")]
    AbandonmentFailed {
        primary: Box<ProjectionExecutionError>,
        release: Option<Box<LoadedProjectionReleaseError>>,
        publication: Option<Box<ProjectionPublicationFailure>>,
    },
}

impl ProjectionExecutionError {
    /// Consumes the error and returns its exact native-lineage recovery decision, when present.
    #[must_use]
    pub fn into_native_lineage_recovery_decision(self) -> Option<NativeLineageRecoveryDecision> {
        match self {
            Self::NativeLineageRecoveryRequired { decision } => Some(*decision),
            _ => None,
        }
    }
}

impl From<ManagedBackendError> for ProjectionExecutionError {
    fn from(error: ManagedBackendError) -> Self {
        Self::Backend(Box::new(error))
    }
}

impl From<ProjectionPublicationFailure> for ProjectionExecutionError {
    fn from(error: ProjectionPublicationFailure) -> Self {
        Self::Publication(Box::new(error))
    }
}
