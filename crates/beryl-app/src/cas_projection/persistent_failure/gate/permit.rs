use super::*;
impl LiveCommandPermit {
    fn matches_open_state(&self, state: &GateState) -> bool {
        self.inner
            .status(state, Some((self.service_generation, self.epoch)))
            == LiveCommandGateStatus::Open
    }

    /// Revalidates that the command still belongs to the open gate epoch.
    #[must_use]
    pub fn is_current(&self) -> bool {
        matches!(self.status_exact(), Ok(LiveCommandGateStatus::Open))
    }

    pub(in crate::cas_projection) fn status_exact(
        &self,
    ) -> Result<LiveCommandGateStatus, LiveCommandAdmissionError> {
        self.inner
            .status_exact(Some((self.service_generation, self.epoch)))
    }

    /// Commits one short authority transition only if this exact permit still precedes the
    /// persistent-failure election. The callback must not perform I/O, join, or wait.
    pub(in crate::cas_projection) fn commit_if_current<T>(
        &self,
        commit: impl FnOnce() -> T,
    ) -> Result<T, LiveCommandAdmissionError> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        if !self.matches_open_state(&state) {
            return Err(LiveCommandAdmissionError::Closed);
        }
        Ok(commit())
    }

    /// Settles one exact authority owner while holding the same mutex that elects the cut.
    ///
    /// Exactly one callback runs. The callbacks must perform only bounded in-memory authority
    /// transitions; they must not perform I/O, join, wait, or acquire a connection/router lock.
    pub(in crate::cas_projection) fn commit_or_transfer<T>(
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
        let outcome = if state.local_failure || state.epoch != self.epoch {
            match state.election {
                GateElection::FailureObserved => {
                    persistent_failure(PersistentFailureGeneration::FIRST)
                }
                GateElection::PersistentFailure(generation) => persistent_failure(generation),
                GateElection::Open | GateElection::OrdinaryShutdown => closed(),
            }
        } else {
            match state.election {
                GateElection::Open => current(),
                GateElection::FailureObserved => {
                    persistent_failure(PersistentFailureGeneration::FIRST)
                }
                GateElection::PersistentFailure(generation) => persistent_failure(generation),
                GateElection::OrdinaryShutdown => closed(),
            }
        };
        Ok(outcome)
    }

    /// Settles a pre-writer admission without converting process-local failure into persistent
    /// failure provenance.
    pub(in crate::cas_projection) fn commit_or_transfer_persistent_only<T>(
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
        let outcome = if state.local_failure {
            closed()
        } else if state.epoch != self.epoch {
            match state.election {
                GateElection::FailureObserved => {
                    persistent_failure(PersistentFailureGeneration::FIRST)
                }
                GateElection::PersistentFailure(generation) => persistent_failure(generation),
                GateElection::Open | GateElection::OrdinaryShutdown => closed(),
            }
        } else {
            match state.election {
                GateElection::Open => current(),
                GateElection::FailureObserved => {
                    persistent_failure(PersistentFailureGeneration::FIRST)
                }
                GateElection::PersistentFailure(generation) => persistent_failure(generation),
                GateElection::OrdinaryShutdown => closed(),
            }
        };
        Ok(outcome)
    }

    /// Returns the exact service generation that admitted this command.
    #[must_use]
    pub const fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    /// Enters one store operation only while the exact service and home remain healthy.
    pub(in crate::cas_projection) fn enter_current_home<'permit, 'home>(
        &'permit self,
        home: &'home HomeStore,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
    ) -> Result<LiveCommandHealthFence<'permit, 'home>, LiveCommandAdmissionError> {
        if self.status_exact()? != LiveCommandGateStatus::Open {
            return Err(LiveCommandAdmissionError::Closed);
        }
        let health = home.health();
        if home.home_id() != home_id
            || health.generation() != Some(home_generation)
            || health.state() != HomeHealthState::Healthy
        {
            return Err(LiveCommandAdmissionError::Closed);
        }
        Ok(LiveCommandHealthFence {
            permit: self,
            home,
            home_id,
            home_generation,
        })
    }

    pub(in crate::cas_projection) fn observe_persistent_failure(
        &self,
    ) -> crate::cas_projection::PersistentFailureNotificationStatus {
        self.failure_notification.as_ref().map_or(
            crate::cas_projection::PersistentFailureNotificationStatus::Unavailable,
            super::PersistentFailureNotification::notify,
        )
    }

    /// Releases this drain-counted permit from an already-typed verification authority loss.
    ///
    /// The completed flight is the immutable disposition. Re-notifying here would resample mutable
    /// home health and could reconstruct a different failure after the typed settlement.
    pub(in crate::cas_projection) fn release_after_authority_loss(mut self) {
        self.release_active();
    }

    fn release_active(&mut self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.active = state
            .active
            .checked_sub(1)
            .expect("every live-command permit releases one admission");
        self.released = true;
        if state.active == 0 {
            self.inner.drained.notify_all();
        }
    }
}

impl LiveCommandHealthFence<'_, '_> {
    /// Revalidates the exact service and healthy home after the operation returns.
    pub(in crate::cas_projection) fn settle_after_operation(
        self,
    ) -> Result<LiveCommandHealthSettlement, LiveCommandAdmissionError> {
        if self.permit.status_exact()? != LiveCommandGateStatus::Open {
            return Err(LiveCommandAdmissionError::Closed);
        }
        let health = self.home.health();
        if self.home.home_id() != self.home_id
            || health.generation() != Some(self.home_generation)
            || health.state() != HomeHealthState::Healthy
        {
            return Err(LiveCommandAdmissionError::Closed);
        }
        Ok(LiveCommandHealthSettlement)
    }
}

impl Drop for LiveCommandPermit {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        if let Some(notification) = &self.failure_notification {
            let status = notification.notify();
            if status == crate::cas_projection::PersistentFailureNotificationStatus::Unavailable
                && !notification.unavailable_allows_command_drain()
            {
                // Keep the active count closed when terminal waiter publication cannot be proven.
                return;
            }
        }
        self.release_active();
    }
}
