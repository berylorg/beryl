use super::*;

impl VerificationCompletionCell {
    pub(super) fn new(epoch: u64) -> Arc<Self> {
        Arc::new(Self {
            epoch,
            outcome: Mutex::new(None),
            completed: Condvar::new(),
        })
    }

    pub(super) fn outcome(
        &self,
    ) -> Result<Option<RecoverySupervisorFlightCompletion>, LiveCommandAdmissionError> {
        self.outcome
            .lock()
            .map(|outcome| *outcome)
            .map_err(|_| LiveCommandAdmissionError::Unavailable)
    }

    pub(super) fn publish(
        &self,
        completion: RecoverySupervisorFlightCompletion,
    ) -> Result<bool, LiveCommandAdmissionError> {
        let mut outcome = self
            .outcome
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        if outcome.is_some() {
            return Ok(false);
        }
        *outcome = Some(completion);
        self.completed.notify_all();
        Ok(true)
    }

    pub(super) fn wait(
        &self,
    ) -> Result<RecoverySupervisorFlightCompletion, LiveCommandAdmissionError> {
        let mut outcome = self
            .outcome
            .lock()
            .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        while outcome.is_none() {
            outcome = self
                .completed
                .wait(outcome)
                .map_err(|_| LiveCommandAdmissionError::Unavailable)?;
        }
        (*outcome).ok_or(LiveCommandAdmissionError::Unavailable)
    }
}

impl PersistentFailureNotification {
    pub(in crate::cas_projection) fn attach_recovery_supervisor(
        &self,
        signal: mpsc::SyncSender<()>,
    ) -> Result<(), ()> {
        {
            let mut flight = self.recovery_flight.state.lock().map_err(|_| ())?;
            if flight.signal.is_some() || flight.terminal_completion.is_some() {
                return Err(());
            }
            flight.signal = Some(signal);
        }
        let _ = self.notify();
        Ok(())
    }

    #[must_use]
    pub fn service_generation(&self) -> ProjectionServiceGeneration {
        self.gate.service_generation()
    }

    pub(in crate::cas_projection::persistent_failure) fn wake_worker(&self) {
        let _ = self.signal.try_send(());
    }

    pub(in crate::cas_projection::persistent_failure) fn mark_cut_elected(&self) {
        self.gate.mark_cut_elected();
    }

    pub(in crate::cas_projection::persistent_failure) fn failure_observed(&self) -> bool {
        self.gate.failure_observed()
    }

    pub(in crate::cas_projection::persistent_failure) fn gate_inner(&self) -> Arc<GateInner> {
        Arc::clone(&self.gate)
    }

    pub(super) fn completion_authority_error(
        completion: RecoverySupervisorFlightCompletion,
    ) -> LiveCommandAdmissionError {
        match completion {
            RecoverySupervisorFlightCompletion::FailedOrStale => LiveCommandAdmissionError::Closed,
            RecoverySupervisorFlightCompletion::ShutdownOrUnavailable
            | RecoverySupervisorFlightCompletion::VerifiedCurrent => {
                LiveCommandAdmissionError::Unavailable
            }
        }
    }

    pub(super) fn matches_exact_epoch(
        &self,
        home: &HomeStore,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
    ) -> bool {
        if self.home_id != home_id
            || self.home_generation != home_generation
            || self.service_generation() != service_generation
        {
            return false;
        }
        self.home.upgrade().is_some_and(|retained| {
            std::ptr::eq(Arc::as_ptr(&retained), std::ptr::from_ref(home))
                && home.home_id() == home_id
        })
    }

    pub(in crate::cas_projection::persistent_failure) fn exact_epoch_health_state(
        &self,
        home: &HomeStore,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
    ) -> Result<HomeHealthState, LiveCommandAdmissionError> {
        if !self.matches_exact_epoch(home, home_id, home_generation, service_generation) {
            return Err(LiveCommandAdmissionError::Closed);
        }
        let health = home.health();
        if health.generation() != Some(home_generation) {
            return Err(LiveCommandAdmissionError::Closed);
        }
        Ok(health.state())
    }
}

pub(in crate::cas_projection) fn persistent_failure_notification_channel(
    home: &Arc<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
) -> (PersistentFailureNotification, mpsc::Receiver<()>) {
    let (signal, receiver) = mpsc::sync_channel(1);
    (
        PersistentFailureNotification {
            home: Arc::downgrade(home),
            home_id,
            home_generation,
            signal,
            recovery_flight: Arc::new(RecoverySupervisorFlight {
                state: Mutex::new(RecoverySupervisorFlightState {
                    signal: None,
                    active: None,
                    next: None,
                    last_issued_epoch: 0,
                    followup_requested: false,
                    terminal_completion: None,
                }),
            }),
            gate: GateInner::new(service_generation),
        },
        receiver,
    )
}
