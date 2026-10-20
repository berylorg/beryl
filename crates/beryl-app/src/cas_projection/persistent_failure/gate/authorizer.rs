use super::*;
impl LiveCommandAuthorizer {
    pub(crate) fn applied_settings(
        &self,
    ) -> Option<crate::settings_service::AppliedSettingsReader> {
        self.inner.applied_settings.try_lock().ok()?.clone()
    }

    pub(crate) fn bind_applied_settings(
        &self,
        reader: crate::settings_service::AppliedSettingsReader,
    ) -> Result<(), crate::cas_projection::LiveCommandAdmissionError> {
        let mut current = self
            .inner
            .applied_settings
            .lock()
            .map_err(|_| crate::cas_projection::LiveCommandAdmissionError::Closed)?;
        if current
            .as_ref()
            .is_some_and(|old| !old.same_source(&reader))
        {
            return Err(crate::cas_projection::LiveCommandAdmissionError::Closed);
        }
        *current = Some(reader);
        Ok(())
    }

    pub(crate) fn try_with_work_open<T>(&self, publish: impl FnOnce() -> T) -> Option<T> {
        self.process.try_admit(|| {
            let _commands = self.try_hold_work_open().ok()?;
            Some(publish())
        })?
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn parent_sound_sink(
        &self,
    ) -> Option<crate::parent_completion_sound::ParentSoundSink> {
        self.inner.parent_sound.try_lock().ok()?.clone()
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn bind_parent_sound(&self, sink: crate::parent_completion_sound::ParentSoundSink) {
        if let Ok(mut current) = self.inner.parent_sound.lock() {
            *current = Some(sink);
        }
    }
    pub(in crate::cas_projection) fn prepare_process_closing(
        &self,
    ) -> Result<
        crate::process_admission::ProcessAdmissionClosing<'_>,
        crate::process_admission::ProcessAdmissionError,
    > {
        self.process.prepare_closing()
    }

    pub(in crate::cas_projection) fn try_check_work_open(
        &self,
    ) -> Result<(), crate::cas_projection::runtime_work::RuntimeWorkError> {
        self.try_hold_work_open().map(drop)
    }

    pub(crate) fn try_hold_work_open(
        &self,
    ) -> Result<impl Sized + '_, crate::cas_projection::runtime_work::RuntimeWorkError> {
        use crate::cas_projection::runtime_work::RuntimeWorkError;
        let state = self.inner.state.try_lock()?;
        if self.inner.status(&state, None) != LiveCommandGateStatus::Open {
            return Err(RuntimeWorkError::Closed);
        }
        Ok(state)
    }

    pub(crate) fn validate_process_settlement_fence(
        &self,
        fence: &crate::process_admission::ProcessAdmissionFence,
    ) -> Result<(), crate::process_admission::ProcessAdmissionError> {
        fence.validate_settled_for(&self.process)
    }

    pub(in crate::cas_projection) fn with_process_settlement_fence<T>(
        &self,
        fence: &crate::process_admission::ProcessAdmissionFence,
        publish: impl FnOnce() -> T,
    ) -> Result<T, crate::process_admission::ProcessAdmissionError> {
        fence.with_settled_for(&self.process, publish)
    }

    pub(crate) fn execution_candidate(
        &self,
    ) -> Result<LiveExecutionCandidate, crate::process_admission::ProcessExecutionAdmissionError>
    {
        let permit = self.authorize()?;
        self.execution_candidate_from(&permit)
    }

    pub(crate) fn execution_candidate_from(
        &self,
        permit: &LiveCommandPermit,
    ) -> Result<LiveExecutionCandidate, crate::process_admission::ProcessExecutionAdmissionError>
    {
        if !Arc::ptr_eq(&self.inner, &permit.inner) {
            return Err(LiveCommandAdmissionError::Closed.into());
        }
        permit.commit_execution_if_current(|| LiveExecutionCandidate {
            authorizer: self.clone(),
            execution: permit.execution.clone(),
        })
    }

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
            execution: self.process.execution_permit(),
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

    pub(in crate::cas_projection) fn try_is_open(&self) -> bool {
        self.inner
            .state
            .try_lock()
            .is_ok_and(|state| self.inner.status(&state, None) == LiveCommandGateStatus::Open)
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::cas_projection) fn test_with_gate_lock<T>(
        &self,
        callback: impl FnOnce() -> T,
    ) -> T {
        let _state = self.inner.state.lock().unwrap();
        callback()
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

    #[cfg(any(test, feature = "test-faults"))]
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
