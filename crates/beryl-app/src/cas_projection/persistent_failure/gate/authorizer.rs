use super::*;
impl LiveCommandAuthorizer {
    /// Admits one scoped command only while this exact service generation remains open.
    pub fn authorize(&self) -> Result<LiveCommandPermit, LiveCommandAdmissionError> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        if self.inner.status(&state, None) != LiveCommandGateStatus::Open {
            return Err(LiveCommandAdmissionError::Closed);
        }
        state.active = state
            .active
            .checked_add(1)
            .ok_or(LiveCommandAdmissionError::Unavailable)?;
        Ok(LiveCommandPermit {
            inner: Arc::clone(&self.inner),
            failure_notification: self.failure_notification.clone(),
            service_generation: self.inner.service_generation,
            epoch: state.epoch,
            released: false,
        })
    }

    /// Returns the exact service generation represented by this authorizer.
    #[must_use]
    pub fn service_generation(&self) -> ProjectionServiceGeneration {
        self.inner.service_generation
    }

    pub(in crate::cas_projection) fn persistent_failure_frontier(
        &self,
        service_generation: ProjectionServiceGeneration,
        failure_generation: PersistentFailureGeneration,
    ) -> Result<PersistentFailureCommandFrontier, LiveCommandAdmissionError> {
        if service_generation != self.inner.service_generation {
            return Err(LiveCommandAdmissionError::Closed);
        }
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        if state.election != GateElection::PersistentFailure(failure_generation) {
            return Err(LiveCommandAdmissionError::Closed);
        }
        Ok(PersistentFailureCommandFrontier {
            service_generation,
            failure_generation,
            gate_epoch: state.epoch,
        })
    }

    /// Reports whether this service generation still admits new live commands.
    #[must_use]
    pub fn is_open(&self) -> bool {
        matches!(
            self.inner.status_exact(None),
            Ok(LiveCommandGateStatus::Open)
        )
    }

    pub(in crate::cas_projection) fn is_persistent_failure_cut(&self) -> bool {
        self.inner
            .state
            .lock()
            .map(|state| {
                matches!(
                    state.election,
                    GateElection::FailureObserved | GateElection::PersistentFailure(_)
                )
            })
            .unwrap_or(true)
    }

    pub(in crate::cas_projection) fn status_exact(
        &self,
    ) -> Result<LiveCommandGateStatus, LiveCommandAdmissionError> {
        self.inner.status_exact(None)
    }

    pub(in crate::cas_projection) fn observe_persistent_failure(
        &self,
    ) -> crate::cas_projection::PersistentFailureNotificationStatus {
        self.failure_notification.as_ref().map_or(
            crate::cas_projection::PersistentFailureNotificationStatus::Unavailable,
            super::PersistentFailureNotification::notify,
        )
    }

    pub(in crate::cas_projection) fn failure_observed(&self) -> bool {
        self.inner.failure_observed()
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn active_command_count_for_test(&self) -> usize {
        self.inner
            .state
            .lock()
            .map(|state| state.active)
            .unwrap_or(usize::MAX)
    }

    /// Settles a long-lived in-memory authority owner on the exact side of the service cut.
    ///
    /// Unlike command admission this does not increment the drain count: the bounded transition is
    /// linearized by the gate mutex itself. Callers must acquire connection/router authority before
    /// entering and callbacks must not perform I/O, join, wait, or reacquire those locks.
    pub(in crate::cas_projection) fn settle_authority<T>(
        &self,
        current: impl FnOnce() -> T,
        persistent_failure: impl FnOnce(PersistentFailureGeneration) -> T,
        closed: impl FnOnce() -> T,
    ) -> Result<T, LiveCommandAdmissionError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        Ok(match state.election {
            GateElection::Open if !state.local_failure => current(),
            GateElection::FailureObserved => persistent_failure(PersistentFailureGeneration::FIRST),
            GateElection::PersistentFailure(generation) => persistent_failure(generation),
            GateElection::Open | GateElection::OrdinaryShutdown => closed(),
        })
    }
}
