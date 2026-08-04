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

    /// Commits one bounded authority transition only while two exact service epochs remain open.
    ///
    /// Equal gate identities are validated once. Distinct gates are locked by service generation
    /// and then process address as a defensive tie-breaker, so cross-generation reacquisition has
    /// one deterministic master-gate order.
    pub(in crate::cas_projection) fn commit_pair_if_current<T>(
        &self,
        other: &Self,
        commit: impl FnOnce() -> T,
    ) -> Result<T, LiveCommandAdmissionError> {
        if Arc::ptr_eq(&self.inner, &other.inner) {
            let state = self
                .inner
                .state
                .lock()
                .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
            if !self.matches_open_state(&state) || !other.matches_open_state(&state) {
                return Err(LiveCommandAdmissionError::Closed);
            }
            return Ok(commit());
        }

        let self_order = (self.service_generation, Arc::as_ptr(&self.inner) as usize);
        let other_order = (other.service_generation, Arc::as_ptr(&other.inner) as usize);
        if self_order < other_order {
            let self_state = self
                .inner
                .state
                .lock()
                .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
            let other_state = other
                .inner
                .state
                .lock()
                .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
            if !self.matches_open_state(&self_state) || !other.matches_open_state(&other_state) {
                return Err(LiveCommandAdmissionError::Closed);
            }
            Ok(commit())
        } else {
            let other_state = other
                .inner
                .state
                .lock()
                .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
            let self_state = self
                .inner
                .state
                .lock()
                .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
            if !self.matches_open_state(&self_state) || !other.matches_open_state(&other_state) {
                return Err(LiveCommandAdmissionError::Closed);
            }
            Ok(commit())
        }
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

    /// Returns the exact service generation that admitted this command.
    #[must_use]
    pub const fn service_generation(&self) -> ProjectionServiceGeneration {
        self.service_generation
    }

    /// Captures the notification completion epoch immediately before one store command.
    ///
    /// If that command returns an ambiguous error, the returned witness atomically registers with
    /// the exact supervisor flight before waiting. Capturing first prevents an unrelated earlier
    /// healthy verification from authorizing the ambiguous command.
    pub(in crate::cas_projection) fn verification_join<'permit, 'home>(
        &'permit self,
        home: &'home HomeStore,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
    ) -> Result<LiveCommandVerificationJoin<'permit, 'home>, LiveCommandAdmissionError> {
        if self.status_exact()? != LiveCommandGateStatus::Open {
            return Err(LiveCommandAdmissionError::Closed);
        }
        let notification = self
            .failure_notification
            .clone()
            .ok_or(LiveCommandAdmissionError::Unavailable)?;
        let (ticket, observed_verifying) = notification.verification_completion_ticket(
            home,
            home_id,
            home_generation,
            self.service_generation,
        )?;
        Ok(LiveCommandVerificationJoin {
            permit: self,
            notification,
            home,
            home_id,
            home_generation,
            ticket,
            observed_verifying,
        })
    }

    /// Waits for an exact pre-operation `Verifying` state without polling.
    pub(in crate::cas_projection) fn await_current_or_verification(
        &self,
        home: &HomeStore,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
    ) -> Result<(), LiveCommandAdmissionError> {
        self.verification_join(home, home_id, home_generation)?
            .await_pre_operation()
    }

    pub(in crate::cas_projection) fn observe_persistent_failure(
        &self,
    ) -> crate::cas_projection::PersistentFailureNotificationStatus {
        self.failure_notification.as_ref().map_or(
            crate::cas_projection::PersistentFailureNotificationStatus::Unavailable,
            super::PersistentFailureNotification::notify,
        )
    }
}

impl LiveCommandVerificationJoin<'_, '_> {
    fn await_pre_operation(mut self) -> Result<(), LiveCommandAdmissionError> {
        let state = self.notification.exact_epoch_health_state(
            self.home,
            self.home_id,
            self.home_generation,
            self.permit.service_generation,
        )?;
        if !self.observed_verifying && state == HomeHealthState::Healthy {
            if let Some(completion) = self.wait_for_current_ticket()? {
                return self.await_verified_chain(completion);
            }
            if let Some(completion) = self.completion_after_revalidation()? {
                return self.await_verified_chain(completion);
            }
            return Ok(());
        }
        if !matches!(
            state,
            HomeHealthState::Healthy | HomeHealthState::Verifying | HomeHealthState::Failed
        ) {
            return Err(LiveCommandAdmissionError::Closed);
        }
        let completion = self
            .wait_for_current_ticket()?
            .ok_or(LiveCommandAdmissionError::Closed)?;
        self.await_verified_chain(completion)
    }

    /// Registers with and waits for the exact post-command verification completion.
    ///
    /// `Ok(true)` is an exact `VerifiedCurrent` completion, `Ok(false)` means no verification
    /// started or completed since the pre-command baseline, and `Err` is typed authority loss.
    pub(in crate::cas_projection) fn wait_after_ambiguous(
        mut self,
    ) -> Result<bool, LiveCommandAdmissionError> {
        let completion = match self.wait_for_current_ticket()? {
            Some(completion) => completion,
            None => match self.completion_after_revalidation()? {
                Some(completion) => completion,
                None => return Ok(false),
            },
        };
        self.await_verified_chain(completion)?;
        Ok(true)
    }

    fn wait_for_current_ticket(
        &self,
    ) -> Result<Option<RecoverySupervisorFlightCompletion>, LiveCommandAdmissionError> {
        match self.notification.register_verification_join(
            self.home,
            self.home_id,
            self.home_generation,
            self.permit.service_generation,
            &self.ticket,
        )? {
            VerificationJoinDisposition::Waiting(target) => self
                .notification
                .wait_for_verification_completion(&target)
                .map(Some),
            VerificationJoinDisposition::NotVerification => Ok(None),
            VerificationJoinDisposition::AuthorityLost => Err(LiveCommandAdmissionError::Closed),
        }
    }

    fn await_verified_chain(
        &mut self,
        mut completion: RecoverySupervisorFlightCompletion,
    ) -> Result<(), LiveCommandAdmissionError> {
        loop {
            match completion {
                RecoverySupervisorFlightCompletion::VerifiedCurrent => {
                    if self.permit.status_exact()? != LiveCommandGateStatus::Open {
                        return Err(LiveCommandAdmissionError::Closed);
                    }
                    match self.notification.exact_epoch_health_state(
                        self.home,
                        self.home_id,
                        self.home_generation,
                        self.permit.service_generation,
                    )? {
                        HomeHealthState::Healthy => return Ok(()),
                        HomeHealthState::Verifying => {
                            let (ticket, observed_verifying) =
                                self.notification.verification_completion_ticket(
                                    self.home,
                                    self.home_id,
                                    self.home_generation,
                                    self.permit.service_generation,
                                )?;
                            self.ticket = ticket;
                            self.observed_verifying = observed_verifying;
                            completion = self
                                .wait_for_current_ticket()?
                                .ok_or(LiveCommandAdmissionError::Closed)?;
                        }
                        HomeHealthState::Opening
                        | HomeHealthState::Failed
                        | HomeHealthState::Reopening => {
                            return Err(LiveCommandAdmissionError::Closed);
                        }
                    }
                }
                RecoverySupervisorFlightCompletion::FailedOrStale => {
                    return Err(LiveCommandAdmissionError::Closed);
                }
                RecoverySupervisorFlightCompletion::ShutdownOrUnavailable => {
                    return Err(LiveCommandAdmissionError::Unavailable);
                }
            }
        }
    }

    fn completion_after_revalidation(
        &self,
    ) -> Result<Option<RecoverySupervisorFlightCompletion>, LiveCommandAdmissionError> {
        if self.permit.status_exact()? != LiveCommandGateStatus::Open {
            return Err(LiveCommandAdmissionError::Closed);
        }
        match self.notification.exact_epoch_health_state(
            self.home,
            self.home_id,
            self.home_generation,
            self.permit.service_generation,
        )? {
            HomeHealthState::Healthy => Ok(None),
            HomeHealthState::Verifying | HomeHealthState::Failed => {
                self.wait_for_current_ticket().and_then(|completion| {
                    completion
                        .ok_or(LiveCommandAdmissionError::Closed)
                        .map(Some)
                })
            }
            HomeHealthState::Opening | HomeHealthState::Reopening => {
                Err(LiveCommandAdmissionError::Closed)
            }
        }
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
        {
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
}
