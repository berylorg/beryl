use super::*;
use syndic_storage::SyndicReadError;

pub(super) enum StepError {
    Retry,
    Failed(ShutdownFailure),
}

impl From<ShutdownFailure> for StepError {
    fn from(value: ShutdownFailure) -> Self {
        Self::Failed(value)
    }
}

impl From<StopCoordinationError> for StepError {
    fn from(value: StopCoordinationError) -> Self {
        match value {
            StopCoordinationError::Read(SyndicReadError::ConcurrentChange { .. }) => Self::Retry,
            _ => ShutdownFailure::StopFailed.into(),
        }
    }
}

impl From<ProcessWorkError> for StepError {
    fn from(value: ProcessWorkError) -> Self {
        match value {
            ProcessWorkError::StaleRevision
            | ProcessWorkError::Sessions(ScheduledSessionWorkError::StaleRevision)
            | ProcessWorkError::Connections(ConnectionWorkError::StaleRevision)
            | ProcessWorkError::Controls(ControlWorkError::Stop(StopWorkError::StaleRevision))
            | ProcessWorkError::Controls(ControlWorkError::Compaction(
                CompactionWorkError::StaleRevision,
            )) => Self::Retry,
            ProcessWorkError::Durable(error) => error.into(),
            ProcessWorkError::Cancelled => ShutdownFailure::Cancelled.into(),
            _ => ShutdownFailure::SourceUnavailable.into(),
        }
    }
}

impl From<SyndicReadError> for StepError {
    fn from(value: SyndicReadError) -> Self {
        match value {
            SyndicReadError::StaleNonIdleGateSourceScan
            | SyndicReadError::ConcurrentChange { .. } => Self::Retry,
            _ => ShutdownFailure::SourceUnavailable.into(),
        }
    }
}

impl From<ShutdownExecutionCaptureError> for StepError {
    fn from(value: ShutdownExecutionCaptureError) -> Self {
        match value {
            ShutdownExecutionCaptureError::Work(error) => error.into(),
            ShutdownExecutionCaptureError::Compaction(CompactionWorkError::StaleRevision) => {
                Self::Retry
            }
            ShutdownExecutionCaptureError::Durable(error) => error.into(),
            _ => ShutdownFailure::SourceUnavailable.into(),
        }
    }
}
