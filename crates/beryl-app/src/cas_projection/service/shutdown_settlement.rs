use beryl_model::SyndicThreadId;
use beryl_model::SyndicTurnId;
use syndic_storage::{PendingDispatchEvidence, SyndicReadError, TerminalHistoryEvidence};

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../tests/unit/shutdown_thread_settlement.rs"]
mod tests;

use super::{ProjectionConnectionService, ProjectionFlight, flight_registry::FlightRegistry};
use crate::{
    cas_projection::{
        ProcessWorkError, ProjectionCancellationToken, ProjectionCoordinatorError,
        ProjectionServiceGeneration, ScheduledExecutionSessions,
    },
    process_admission::{ProcessAdmissionError, ProcessAdmissionFence},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShutdownThreadDisposition {
    Pending(PendingDispatchEvidence),
    Terminal(TerminalHistoryEvidence),
}

pub(crate) struct ShutdownThreadSettlement {
    service_generation: ProjectionServiceGeneration,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    disposition: ShutdownThreadDisposition,
    fence: ProcessAdmissionFence,
    _flight: ProjectionFlight,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ShutdownThreadSettlementError {
    #[error(transparent)]
    Process(#[from] ProcessAdmissionError),
    #[error(transparent)]
    Projection(#[from] ProjectionCoordinatorError),
    #[error(transparent)]
    Work(#[from] ProcessWorkError),
    #[error(transparent)]
    Durable(#[from] SyndicReadError),
    #[error("shutdown thread settlement belongs to another service")]
    ForeignService,
    #[error("shutdown thread settlement source changed")]
    Stale,
}

impl ProjectionConnectionService {
    pub(crate) fn try_shutdown_thread_settlement(
        &self,
        sessions: &ScheduledExecutionSessions,
        fence: &ProcessAdmissionFence,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Option<ShutdownThreadSettlement>, ShutdownThreadSettlementError> {
        if cancellation.is_cancelled() {
            return Err(ProcessWorkError::Cancelled.into());
        }
        let command = self
            .live_home_command()
            .map_err(|_| ProcessWorkError::Closed)?;
        let coordinator =
            crate::cas_projection::CasProjectionCoordinator::for_healthy_home(command.home())?;
        if coordinator.home_id() != self.home_id
            || coordinator.home_generation() != self.home_generation
        {
            return Err(ShutdownThreadSettlementError::ForeignService);
        }
        let source = sessions.work_revision().map_err(ProcessWorkError::from)?;
        if source.home_id() != self.home_id
            || source.home_generation() != self.home_generation
            || source.service_generation() != self.service_generation
        {
            return Err(ProcessWorkError::ForeignSources.into());
        }
        match self
            .command_authorizer
            .validate_process_settlement_fence(fence)
        {
            Ok(()) => {}
            Err(ProcessAdmissionError::Unsettled) => return Ok(None),
            Err(error) => return Err(error.into()),
        }
        let flight = match FlightRegistry::acquire(self.home_id, self.home_generation, thread_id) {
            Ok(flight) => flight,
            Err(ProjectionCoordinatorError::ProjectionInFlight { .. }) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let Some(disposition) = self.work_read().shutdown_thread_disposition(
            sessions,
            thread_id,
            turn_id,
            cancellation,
        )?
        else {
            return Ok(None);
        };
        self.command_authorizer
            .validate_process_settlement_fence(fence)?;
        Ok(Some(ShutdownThreadSettlement {
            service_generation: self.service_generation,
            thread_id,
            turn_id,
            disposition,
            fence: fence.clone(),
            _flight: flight,
        }))
    }
}

impl ShutdownThreadSettlement {
    pub(crate) const fn disposition(&self) -> ShutdownThreadDisposition {
        self.disposition
    }

    pub(crate) fn revalidate(
        &self,
        service: &ProjectionConnectionService,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), ShutdownThreadSettlementError> {
        if service.service_generation != self.service_generation {
            return Err(ShutdownThreadSettlementError::ForeignService);
        }
        service
            .command_authorizer
            .validate_process_settlement_fence(&self.fence)?;
        let current = service.work_read().shutdown_thread_disposition(
            sessions,
            self.thread_id,
            self.turn_id,
            cancellation,
        )?;
        service
            .command_authorizer
            .validate_process_settlement_fence(&self.fence)?;
        if current != Some(self.disposition) {
            return Err(ShutdownThreadSettlementError::Stale);
        }
        Ok(())
    }
}
