use super::*;

impl EventRouter {
    pub(in crate::cas_projection) fn observe_quota(
        &self,
        quota: beryl_backend::AccountQuotaObservation,
    ) -> Result<(), ProjectionCoordinatorError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)?;
        let state = self
            .state
            .lock()
            .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)?;
        command
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
                }
                super::super::registry::observe_quota(self.connection_generation, quota)
            })
            .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)?
    }
    pub(in crate::cas_projection) fn observe_context(
        &self,
        observation: beryl_backend::ThreadContextObservation,
    ) -> Result<(), ProjectionCoordinatorError> {
        let command = self
            .commands
            .authorize()
            .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)?;
        let state = self
            .state
            .lock()
            .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)?;
        command
            .commit_if_current(|| {
                if state.retired.is_some() || state.persistent_failure.is_some() {
                    return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
                }
                let Some(target) = state.targets.get(observation.thread_id()) else {
                    return Ok(());
                };
                if target.turn_state != TargetTurn::Exact
                    || target.turn_id.as_ref() != Some(observation.turn_id())
                    || !target.start_dispatched
                    || !target.activation_durable
                    || target.publication_in_flight.is_some()
                    || target.publication_closing.is_some()
                    || target.loss_requested
                {
                    return Ok(());
                }
                super::super::registry::observe_context(
                    &target.key,
                    self.connection_generation,
                    target.owner,
                    target.loaded_generation,
                    observation,
                )
            })
            .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)?
    }
}
