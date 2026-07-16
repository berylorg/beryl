use beryl_model::SyndicThreadId;
use syndic_storage::SyndicReadError;
use thiserror::Error;

use crate::cas_projection::{
    LiveEventTargetError, LiveEventTargetRegistrationError, ProjectionCoordinatorError,
    ProjectionPublicationFailure,
};

/// Closed failures while starting or capturing one ordinary Syndic turn.
#[derive(Debug, Error)]
pub enum OrdinaryTurnExecutionError {
    #[error(transparent)]
    Coordinator(#[from] ProjectionCoordinatorError),
    #[error("Beryl-home state could not be read while preparing ordinary execution")]
    HomeRead(#[from] beryl_home_store::ReadError),
    #[error("ordinary history convergence command could not be built")]
    HomeCommandBuild(#[from] beryl_home_store::CommandBuildError),
    #[error("ordinary history convergence command failed")]
    HomeCommand(#[from] beryl_home_store::CommandError),
    #[error(transparent)]
    Read(#[from] SyndicReadError),
    #[error(transparent)]
    TargetRegistration(#[from] LiveEventTargetRegistrationError),
    #[error(transparent)]
    Target(#[from] LiveEventTargetError),
    #[error("live-event target could not return its projection authority: {message}")]
    TargetHandoff { message: Box<str> },
    #[error("CAS projection execution failed during ordinary turn capture")]
    ProjectionExecution(#[from] crate::cas_projection::ProjectionExecutionError),
    #[error(transparent)]
    Publication(#[from] ProjectionPublicationFailure),
    #[error(transparent)]
    SyndicRecord(#[from] syndic_storage::SyndicRecordError),
    #[error(transparent)]
    SyndicValue(#[from] syndic_storage::SyndicValueError),
    #[error("provider supplied an invalid bounded identity")]
    ProviderIdentity(#[from] beryl_model::ValueError),
    #[error("system clock precedes the Unix epoch during ordinary execution")]
    SystemClockBeforeUnixEpoch(#[source] std::time::SystemTimeError),
    #[error("system clock milliseconds exceed the durable Syndic timestamp range")]
    SystemClockOutOfRange,
    #[error("loaded projection does not match the exact pending turn for {thread_id}")]
    ProjectionMismatch { thread_id: SyndicThreadId },
    #[error("Syndic thread {thread_id} has no pending ordinary turn")]
    PendingTurnUnavailable { thread_id: SyndicThreadId },
    #[error("Syndic changed while preparing ordinary execution for {thread_id}")]
    ConcurrentChange { thread_id: SyndicThreadId },
    #[error("ordinary turn input contains image markers before image projection is implemented")]
    ImageInputNotImplemented,
    #[error("ordinary turn input is empty")]
    EmptyInput,
    #[error("ordinary execution invariant failed: {0}")]
    Invariant(&'static str),
}

impl From<crate::cas_projection::connection::LiveEventTargetHandoffError>
    for OrdinaryTurnExecutionError
{
    fn from(error: crate::cas_projection::connection::LiveEventTargetHandoffError) -> Self {
        Self::TargetHandoff {
            message: error.to_string().into_boxed_str(),
        }
    }
}
