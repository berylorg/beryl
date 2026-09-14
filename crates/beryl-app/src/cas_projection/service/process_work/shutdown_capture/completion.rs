use super::*;
use crate::cas_projection::ordinary::TerminalHistoryCompletion;
use beryl_model::SyndicTurnId;

impl ShutdownTerminalCompletion {
    pub(crate) fn turn_id(&self) -> SyndicTurnId {
        self.observer.turn_id()
    }

    pub(crate) fn completion(
        &self,
        service: &ProjectionConnectionService,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Result<Option<TerminalHistoryCompletion>, ProcessWorkError> {
        if self.service_generation != service.service_generation
            || self.observer.service_generation() != service.service_generation
            || !self
                .observer
                .matches(service.home_id, service.home_generation, thread_id, turn_id)
        {
            return Err(ProcessWorkError::ForeignSources);
        }
        let command = service
            .live_home_command()
            .map_err(|_| ProcessWorkError::Closed)?;
        let coordinator = CasProjectionCoordinator::for_healthy_home(command.home())?;
        if coordinator.home_id() != service.home_id
            || coordinator.home_generation() != service.home_generation
        {
            return Err(ProcessWorkError::ForeignSources);
        }
        let proof = self.observer.completion().copied();
        coordinator.ensure_home(command.home())?;
        Ok(proof)
    }
}
