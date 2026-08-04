use beryl_backend::{JsonRpcError, ManagedBackendError, StreamedInputSourceError};
use beryl_home_store::ReadError;
use beryl_state::BerylStateReacquireError;
use thiserror::Error;

use super::super::{
    ProjectionCoordinatorError, ProjectionPublicationFailure,
    connection::{
        ActiveSteeringAttemptAcquireError, ActiveSteeringAttemptFinishError,
        CheckedSteeringLifecycleWaitError, ProviderBrokerLossError, TargetAuthorizationFailure,
    },
    input_replay::AcceptedInputReplayError,
};

/// Result of one bounded active-steering delivery attempt.
#[must_use = "active-steering delivery outcomes require explicit handling of any still-eligible work"]
#[derive(Debug)]
pub(in crate::cas_projection) enum ActiveSteeringDeliveryOutcome {
    NotReady,
    Saturated {
        cause: ActiveSteeringSaturationCause,
    },
    Delivered,
    Retryable {
        cause: ActiveSteeringRetryCause,
    },
    SteeringRejected {
        rejection: JsonRpcError,
    },
    ProjectionLost {
        cause: ActiveSteeringProjectionLossCause,
    },
    DeliveryUnknown {
        cause: ActiveSteeringUnknownCause,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ActiveSteeringSaturationCause {
    WorkerPoolFull,
    ConnectionAttemptBusy,
}

#[derive(Debug)]
pub(in crate::cas_projection) enum ActiveSteeringRetryCause {
    Preparation(ActiveSteeringPreparationFailure),
    ProvenNotDispatched(Box<ManagedBackendError>),
    LifecycleArm(super::super::connection::CheckedSteeringLifecycleArmError),
    TargetAuthorization(TargetAuthorizationFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ActiveSteeringRetryPolicy {
    ParkUntilLifecycleWake,
    FailCloseProjection,
}

impl ActiveSteeringRetryCause {
    pub(super) fn policy(&self) -> ActiveSteeringRetryPolicy {
        match self {
            Self::Preparation(ActiveSteeringPreparationFailure::Replay(
                AcceptedInputReplayError::Cancelled,
            )) => ActiveSteeringRetryPolicy::ParkUntilLifecycleWake,
            Self::ProvenNotDispatched(error)
                if matches!(
                    error.as_ref(),
                    ManagedBackendError::StreamedInputSource {
                        source: StreamedInputSourceError::Cancelled,
                        transport_bytes_written: false,
                        ..
                    }
                ) =>
            {
                ActiveSteeringRetryPolicy::ParkUntilLifecycleWake
            }
            Self::Preparation(
                ActiveSteeringPreparationFailure::State(_)
                | ActiveSteeringPreparationFailure::Asset(_)
                | ActiveSteeringPreparationFailure::Replay(_),
            )
            | Self::ProvenNotDispatched(_)
            | Self::LifecycleArm(_)
            | Self::TargetAuthorization(_) => ActiveSteeringRetryPolicy::FailCloseProjection,
        }
    }
}

#[derive(Debug, Error)]
pub(in crate::cas_projection) enum ActiveSteeringPreparationFailure {
    #[error("Beryl state could not be reacquired")]
    State(#[source] BerylStateReacquireError),
    #[error("accepted-input asset authority could not be read")]
    Asset(#[source] ReadError),
    #[error("accepted input could not be prepared for replay")]
    Replay(#[source] AcceptedInputReplayError),
}

#[derive(Debug)]
pub(in crate::cas_projection) enum ActiveSteeringProjectionLossCause {
    TargetAuthorization(TargetAuthorizationFailure),
    LifecycleArm(super::super::connection::CheckedSteeringLifecycleArmError),
    UnconfirmedRejection(JsonRpcError),
    TargetClosed,
}

#[derive(Debug)]
pub(in crate::cas_projection) enum ActiveSteeringUnknownCause {
    Backend(Box<ManagedBackendError>),
    Coordinator(ProjectionCoordinatorError),
    Lifecycle(CheckedSteeringLifecycleWaitError),
    Disposition(ProjectionPublicationFailure),
    DeliveringRouteRead(syndic_storage::SyndicReadError),
    DeliveringRouteUnavailable,
}

#[derive(Debug, Error)]
pub(in crate::cas_projection) enum ActiveSteeringDeliveryError {
    #[error("the projection service no longer accepts active-steering work")]
    ServiceClosed,
    #[error("persistent Beryl-home failure fenced the active-steering attempt")]
    PersistentFailureCut,
    #[error(transparent)]
    Coordinator(#[from] ProjectionCoordinatorError),
    #[error(transparent)]
    Read(#[from] syndic_storage::SyndicReadError),
    #[error(transparent)]
    Publication(#[from] ProjectionPublicationFailure),
    #[error("the exact active-steering attempt could not be acquired: {0:?}")]
    Attempt(ActiveSteeringAttemptAcquireError),
    #[error("the exact active-steering attempt could not be released: {0:?}")]
    AttemptFinish(ActiveSteeringAttemptFinishError),
    #[error("the checked steering lifecycle could not be released: {0:?}")]
    LifecycleRelease(CheckedSteeringLifecycleWaitError),
    #[error(transparent)]
    Loss(#[from] ProviderBrokerLossError),
}
