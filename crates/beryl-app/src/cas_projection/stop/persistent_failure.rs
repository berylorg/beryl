use super::{LocalDispatchState, StopCoordinationError, StopCoordinator};
use crate::cas_projection::persistent_failure::PersistentFailureCutIdentity;

impl StopCoordinator {
    pub(in crate::cas_projection) fn freeze_for_persistent_failure(
        &self,
        identity: PersistentFailureCutIdentity,
    ) -> Result<(), StopCoordinationError> {
        if self.home_id != identity.home_id || self.home_generation != identity.home_generation {
            return Err(StopCoordinationError::HomeAuthorityLost);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| StopCoordinationError::LocalAuthorityMismatch)?;
        if let Some(existing) = state.persistent_failure {
            return if existing == identity {
                Ok(())
            } else {
                Err(StopCoordinationError::LocalAuthorityMismatch)
            };
        }
        for local in state.stops.values_mut() {
            if matches!(
                local.dispatch,
                LocalDispatchState::AdmittedNotClaimed | LocalDispatchState::ClaimedNotDispatched
            ) {
                local.dispatch = LocalDispatchState::FailureFrozenNondispatch;
            }
        }
        state
            .lifecycle_yields
            .retain(|_, outcome| *outcome != crate::LifecycleYieldOutcome::PhaseContinue);
        state.persistent_failure = Some(identity);
        drop(state);
        Ok(())
    }

    pub(super) fn failure_cut_is_active(&self) -> bool {
        self.commands.failure_observed()
            || self
                .state
                .lock()
                .map(|state| state.persistent_failure.is_some())
                .unwrap_or(true)
    }
}
