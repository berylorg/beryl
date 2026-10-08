use super::*;

impl RuntimeSetupService {
    #[cfg(test)]
    pub(crate) fn test_first_flight(&self) -> Arc<RuntimeSetupFlight> {
        let flights = self.flights.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(flights.len(), 1);
        flights[0].clone()
    }

    #[cfg(test)]
    pub(crate) fn test_execute_first(
        &self,
        command: beryl_home_store::HomeCommand,
        runtime: beryl_model::RuntimeId,
        root: beryl_model::RootId,
        onboarding: crate::runtime_admission::OnboardingFacts,
    ) -> Arc<RuntimeSetupFlight> {
        let outcome = self
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .unwrap()
            .test_execute_first(command, runtime, root, onboarding);
        self.retain_test_outcome(outcome)
    }

    #[cfg(test)]
    pub(crate) fn test_execute_later(
        &self,
        command: beryl_home_store::HomeCommand,
        window: beryl_model::WindowId,
        runtime: beryl_model::RuntimeId,
        root: beryl_model::RootId,
    ) -> Arc<RuntimeSetupFlight> {
        let outcome = self
            .admission
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .unwrap()
            .test_execute_later(command, window, runtime, root);
        self.retain_test_outcome(outcome)
    }

    #[cfg(test)]
    pub(crate) fn retain_test_outcome(
        &self,
        outcome: RuntimeAdmissionOutcome,
    ) -> Arc<RuntimeSetupFlight> {
        let flight = Arc::new(RuntimeSetupFlight {
            home: Mutex::new(self.home.lock().unwrap_or_else(|e| e.into_inner()).clone()),
            retired: self.retired.clone(),
            publication_gate: self.publication_gate.clone(),
            cancellation: CommandCancellation::new(),
            pending: AtomicBool::new(false),
            worker: Mutex::new(None),
            outcome: Mutex::new(Some(outcome)),
            first: Mutex::new(None),
            failure: Mutex::new(None),
            preparation_error: Mutex::new(None),
            unavailable: Mutex::new(None),
            revalidation_ready: AtomicBool::new(false),
        });
        self.flights
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(flight.clone());
        flight
    }
}
