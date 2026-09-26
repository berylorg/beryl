use std::collections::BTreeMap;

use super::{
    ProjectionConnectionService, ShutdownExecutionCapture, ShutdownExecutionCaptureError,
    ShutdownWorkError, ShutdownWorkObservation,
};
use crate::{
    cas_projection::*,
    process_admission::{ProcessAdmissionError, ProcessAdmissionFence},
};
use beryl_model::{SyndicThreadId, SyndicTurnId};

mod errors;
#[cfg(feature = "test-faults")]
mod probe;
mod progress;
mod settlement;
use errors::StepError;
#[cfg(feature = "test-faults")]
pub use probe::GracefulShutdownProbe;

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../tests/unit/graceful_shutdown.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownAttemptId {
    service: ProjectionServiceGeneration,
    serial: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShutdownFailure {
    Cancelled,
    SourceUnavailable,
    StopFailed,
    CleanupFailed,
    UnprovenExecution {
        thread: SyndicThreadId,
        turn: SyndicTurnId,
    },
    UnprovenCompaction {
        operation: syndic_storage::CompactionOperationId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShutdownProgress {
    Waiting,
    Ready,
    Failed {
        reason: ShutdownFailure,
        reopened: bool,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ShutdownCoordinatorError {
    #[error("shutdown attempt belongs to another service or attempt")]
    StaleAttempt,
    #[error("shutdown coordinator is unavailable")]
    Unavailable,
    #[error(transparent)]
    Capture(#[from] ShutdownExecutionCaptureError),
    #[error(transparent)]
    Work(#[from] ShutdownWorkError),
}

#[derive(Default)]
pub(super) struct ShutdownCoordinator {
    serial: u64,
    attempt: Option<ShutdownAttempt>,
    completed_failure: Option<(ShutdownAttemptId, ShutdownFailure)>,
}

struct ShutdownAttempt {
    id: ShutdownAttemptId,
    fence: ProcessAdmissionFence,
    execution: ShutdownExecutionCapture,
    stops: BTreeMap<(SyndicThreadId, SyndicTurnId), WindowCloseStopBarrier>,
    progress_after: Option<SyndicThreadId>,
    failure: Option<ShutdownFailure>,
}

impl ProjectionConnectionService {
    pub(crate) fn try_begin_observed_shutdown(
        &self,
        sessions: &ScheduledExecutionSessions,
        observation: &ShutdownWorkObservation,
    ) -> Result<ShutdownAttemptId, ShutdownCoordinatorError> {
        let mut coordinator = self
            .graceful_shutdown
            .try_lock()
            .map_err(|_| ShutdownCoordinatorError::Unavailable)?;
        if coordinator.attempt.is_some() {
            return Err(ShutdownCoordinatorError::StaleAttempt);
        }
        let serial = coordinator
            .serial
            .checked_add(1)
            .ok_or(ShutdownCoordinatorError::Unavailable)?;
        let prepared = self.prepare_shutdown_execution_capture()?;
        let fence = self.try_admit_observed_shutdown(sessions, observation)?;
        let execution = prepared.bind(&fence);
        let id = ShutdownAttemptId {
            service: self.service_generation,
            serial,
        };
        coordinator.serial = serial;
        coordinator.attempt = Some(ShutdownAttempt {
            id,
            fence,
            execution,
            stops: BTreeMap::new(),
            progress_after: None,
            failure: None,
        });
        Ok(id)
    }

    pub(crate) fn begin_graceful_shutdown(
        &self,
        fence: &ProcessAdmissionFence,
    ) -> Result<ShutdownAttemptId, ShutdownCoordinatorError> {
        let mut coordinator = self
            .graceful_shutdown
            .lock()
            .map_err(|_| ShutdownCoordinatorError::Unavailable)?;
        if let Some(attempt) = &coordinator.attempt {
            if !attempt.fence.same_attempt(fence) {
                return Err(ShutdownCoordinatorError::StaleAttempt);
            }
            match self
                .command_authorizer
                .validate_process_settlement_fence(fence)
            {
                Ok(()) | Err(ProcessAdmissionError::Unsettled) => {}
                Err(_) => return Err(ShutdownCoordinatorError::StaleAttempt),
            }
            return Ok(attempt.id);
        }
        let execution = self.begin_shutdown_execution_capture(fence)?;
        let serial = coordinator
            .serial
            .checked_add(1)
            .ok_or(ShutdownCoordinatorError::Unavailable)?;
        let id = ShutdownAttemptId {
            service: self.service_generation,
            serial,
        };
        coordinator.serial = serial;
        coordinator.attempt = Some(ShutdownAttempt {
            id,
            fence: fence.clone(),
            execution,
            stops: BTreeMap::new(),
            progress_after: None,
            failure: None,
        });
        Ok(id)
    }

    pub(crate) fn poll_graceful_shutdown(
        &self,
        sessions: &ScheduledExecutionSessions,
        id: ShutdownAttemptId,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ShutdownProgress, ShutdownCoordinatorError> {
        let source = sessions
            .work_revision()
            .map_err(|_| ShutdownCoordinatorError::StaleAttempt)?;
        if source.home_id() != self.home_id
            || source.home_generation() != self.home_generation
            || source.service_generation() != self.service_generation
        {
            return Err(ShutdownCoordinatorError::StaleAttempt);
        }
        let mut coordinator = self
            .graceful_shutdown
            .lock()
            .map_err(|_| ShutdownCoordinatorError::Unavailable)?;
        if let Some((completed, reason)) = coordinator.completed_failure
            && completed == id
        {
            return Ok(ShutdownProgress::Failed {
                reason,
                reopened: true,
            });
        }
        let attempt = coordinator
            .attempt
            .as_mut()
            .filter(|attempt| attempt.id == id && id.service == self.service_generation)
            .ok_or(ShutdownCoordinatorError::StaleAttempt)?;
        if cancellation.is_cancelled() {
            attempt.failure.get_or_insert(ShutdownFailure::Cancelled);
        }
        let progress = if attempt.failure.is_none() {
            match attempt.progress(self, sessions, cancellation) {
                Ok(progress) => progress,
                Err(StepError::Retry) => ShutdownProgress::Waiting,
                Err(StepError::Failed(reason)) => {
                    attempt.failure = Some(reason);
                    ShutdownProgress::Failed {
                        reason,
                        reopened: false,
                    }
                }
            }
        } else {
            ShutdownProgress::Failed {
                reason: attempt.failure.expect("failed attempt"),
                reopened: false,
            }
        };
        if let Some(reason) = attempt.failure
            && self.reopen_failed_shutdown(&attempt.fence)
        {
            coordinator.attempt = None;
            coordinator.completed_failure = Some((id, reason));
            return Ok(ShutdownProgress::Failed {
                reason,
                reopened: true,
            });
        }
        Ok(progress)
    }

    fn reopen_failed_shutdown(&self, fence: &ProcessAdmissionFence) -> bool {
        self.try_reopen_shutdown_admission(fence).is_ok()
    }
}

impl ShutdownAttempt {
    fn progress(
        &mut self,
        service: &ProjectionConnectionService,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ShutdownProgress, StepError> {
        self.execution.refresh(service, cancellation)?;
        self.progress_executions(service)?;
        self.progress_generic(service, sessions, cancellation)?;
        let cleanup_ready = service.poll_shutdown_cleanup(sessions, cancellation)?;
        match service
            .command_authorizer
            .validate_process_settlement_fence(&self.fence)
        {
            Ok(()) => {}
            Err(ProcessAdmissionError::Unsettled) => return Ok(ShutdownProgress::Waiting),
            Err(_) => return Err(ShutdownFailure::SourceUnavailable.into()),
        }
        self.execution.refresh(service, cancellation)?;
        if !cleanup_ready {
            return Ok(ShutdownProgress::Waiting);
        }
        self.final_settlement(service, sessions, cancellation)
    }
}
