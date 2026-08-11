use beryl_home_store::{
    CommandError, CommitReceipt, HomeGeneration, HomeHealthState, ReconciliationDescriptor,
};
use beryl_model::{BerylHomeId, SyndicThreadId};
use beryl_state::AssetReadError;
use syndic_storage::SyndicReadError;
use thiserror::Error;

use crate::cas_projection::{
    LiveEventTargetRegistrationError, ProjectionCoordinatorError, ProjectionPublicationFailure,
    input_replay::InputReplayPrepareError,
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
    #[error("ordinary history convergence command was proven not committed")]
    HomeCommandNotCommitted(#[source] CommandError),
    #[error("ordinary history convergence command committed before a later failure: {later_failure}")]
    HomeCommandCommitted {
        receipt: CommitReceipt,
        #[source]
        later_failure: CommandError,
    },
    #[error("ordinary history convergence command has an indeterminate durable outcome: {failure}")]
    HomeCommandIndeterminate {
        #[source]
        failure: CommandError,
        reconciliation: ReconciliationDescriptor,
    },
    #[error("ordinary input replay requires a healthy Beryl home, got {state:?}")]
    InputReplayHomeNotHealthy {
        state: HomeHealthState,
        expected_home_id: BerylHomeId,
        actual_home_id: BerylHomeId,
        expected_generation: HomeGeneration,
        actual_generation: Option<HomeGeneration>,
    },
    #[error(transparent)]
    Read(#[from] SyndicReadError),
    #[error("Beryl asset authority could not be read while preparing ordinary execution")]
    AssetRead(#[from] AssetReadError),
    #[error(transparent)]
    TargetRegistration(#[from] LiveEventTargetRegistrationError),
    #[error("live-event target could not return its projection authority: {message}")]
    TargetHandoff { message: Box<str> },
    #[error("CAS projection execution failed during ordinary turn capture")]
    ProjectionExecution(#[from] crate::cas_projection::ProjectionExecutionError),
    #[error(transparent)]
    Publication(#[from] ProjectionPublicationFailure),
    #[error("broker-owned source-loss convergence failed during ordinary execution")]
    BrokerLoss {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("system clock precedes the Unix epoch during ordinary execution")]
    SystemClockBeforeUnixEpoch(#[source] std::time::SystemTimeError),
    #[error("system clock milliseconds exceed the durable Syndic timestamp range")]
    SystemClockOutOfRange,
    #[error("loaded projection does not match the exact pending turn for {thread_id}")]
    ProjectionMismatch { thread_id: SyndicThreadId },
    #[error(
        "target Beryl-home generation {current:?} is not strictly newer than loaded projection generation {previous:?}"
    )]
    ProjectionGenerationNotAdvanced {
        previous: HomeGeneration,
        current: HomeGeneration,
    },
    #[error("Syndic thread {thread_id} has no pending ordinary turn")]
    PendingTurnUnavailable { thread_id: SyndicThreadId },
    #[error("Syndic changed while preparing ordinary execution for {thread_id}")]
    ConcurrentChange { thread_id: SyndicThreadId },
    #[error("ordinary input sealed content is unavailable")]
    InputContentUnavailable,
    #[error("ordinary input descriptor evidence is invalid")]
    InputDescriptorInvalid,
    #[error("ordinary input asset-reference set disagrees with sealed content")]
    InputAssetReferenceSetMismatch,
    #[error("ordinary input image sidecar could not be verified")]
    InputAssetSidecar(#[from] beryl_home_store::SidecarError),
    #[error("ordinary input image sidecar path is not Unicode")]
    InputRuntimePathNotUnicode,
    #[error("ordinary input image sidecar path cannot be projected into the selected runtime")]
    InputRuntimePathUnmappable,
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

impl From<crate::cas_projection::connection::LiveEventTargetLossError>
    for OrdinaryTurnExecutionError
{
    fn from(source: crate::cas_projection::connection::LiveEventTargetLossError) -> Self {
        Self::BrokerLoss {
            source: Box::new(source),
        }
    }
}

impl From<InputReplayPrepareError> for OrdinaryTurnExecutionError {
    fn from(error: InputReplayPrepareError) -> Self {
        match error {
            InputReplayPrepareError::Cancelled => {
                crate::cas_projection::ProjectionExecutionError::Cancelled.into()
            }
            InputReplayPrepareError::HomeRead(source) => Self::HomeRead(source),
            InputReplayPrepareError::SyndicRead(source) => Self::Read(source),
            InputReplayPrepareError::AssetRead(source) => Self::AssetRead(source),
            InputReplayPrepareError::Sidecar(source) => Self::InputAssetSidecar(source),
            InputReplayPrepareError::ContentMissing { .. } => Self::InputContentUnavailable,
            InputReplayPrepareError::AssetReferenceSetMissing
            | InputReplayPrepareError::AssetOwnerHeadMissing
            | InputReplayPrepareError::AssetReferenceSetMismatch
            | InputReplayPrepareError::RevisionDrift { .. } => Self::InputAssetReferenceSetMismatch,
            InputReplayPrepareError::DescriptorInvalid => Self::InputDescriptorInvalid,
            InputReplayPrepareError::EmptyInput => Self::EmptyInput,
            InputReplayPrepareError::RuntimePathNotUnicode => Self::InputRuntimePathNotUnicode,
            InputReplayPrepareError::RuntimePathUnmappable => Self::InputRuntimePathUnmappable,
            InputReplayPrepareError::HomeNotHealthy {
                state,
                expected_home_id,
                actual_home_id,
                expected_generation,
                actual_generation,
            } => Self::InputReplayHomeNotHealthy {
                state,
                expected_home_id,
                actual_home_id,
                expected_generation,
                actual_generation,
            },
            InputReplayPrepareError::HealthyHomeGenerationMissing
            | InputReplayPrepareError::HomeIdentityMismatch { .. }
            | InputReplayPrepareError::HomeGenerationMismatch { .. }
            | InputReplayPrepareError::AcceptedInputMissing { .. }
            | InputReplayPrepareError::AcceptedInputChanged { .. }
            | InputReplayPrepareError::AcceptedInputContentMismatch { .. }
            | InputReplayPrepareError::ContentChanged { .. }
            | InputReplayPrepareError::ReadUnavailable
            | InputReplayPrepareError::SourceIdentityMismatch { .. } => {
                Self::Invariant("input replay authority changed during ordinary preparation")
            }
        }
    }
}
