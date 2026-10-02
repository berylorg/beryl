use super::*;
use beryl_home_store::HomeRecoveryCandidate;

pub struct DraftMarkerSealRetainedFlights {
    inner: Arc<Mutex<ServiceState>>,
    home_id: BerylHomeId,
    original_generation: HomeGeneration,
    generation: HomeGeneration,
    canonical_home: std::path::PathBuf,
    live: Option<Vec<FlightState>>,
    orphans: Option<Vec<FlightState>>,
    captured: Vec<DraftMarkerSealFlight>,
    bound: bool,
}

pub(super) struct MarkerOperationBorrow(Arc<Mutex<ServiceState>>);

impl Drop for MarkerOperationBorrow {
    fn drop(&mut self) {
        let mut state = lock_state(&self.0);
        state.active_borrows = state
            .active_borrows
            .checked_sub(1)
            .expect("marker operation borrow was charged");
    }
}

impl DraftMarkerSealService {
    pub(super) fn borrow_operations(
        &self,
    ) -> Result<MarkerOperationBorrow, DraftMarkerSealServiceError> {
        self.borrow_for_access(false)
    }
    pub(super) fn borrow_for_access(
        &self,
        candidate: bool,
    ) -> Result<MarkerOperationBorrow, DraftMarkerSealServiceError> {
        let mut state = lock_state(&self.inner);
        if self.generation != state.home_generation || state.recovery_owned != candidate {
            return Err(DraftMarkerSealServiceError::HomeGenerationChanged);
        }
        state.active_borrows = state
            .active_borrows
            .checked_add(1)
            .ok_or(DraftMarkerSealServiceError::FlightBusy)?;
        Ok(MarkerOperationBorrow(self.inner.clone()))
    }

    pub fn capture_failed_home(
        &self,
        store: &HomeStore,
    ) -> Result<DraftMarkerSealRetainedFlights, DraftMarkerSealServiceError> {
        let mut state = lock_state(&self.inner);
        if store.home_id() != self.home_id {
            return Err(DraftMarkerSealServiceError::ForeignHome);
        }
        if store.health().state() != HomeHealthState::Failed
            || store.health().generation() != Some(self.generation)
        {
            return Err(DraftMarkerSealServiceError::HomeUnavailable(
                store.health().state(),
            ));
        }
        if state.home_generation != self.generation || state.recovery_owned {
            return Err(DraftMarkerSealServiceError::HomeGenerationChanged);
        }
        if state.active_borrows != 0
            || state
                .flights
                .iter()
                .chain(state.orphans.iter())
                .any(|flight| flight.driving || Arc::strong_count(&flight.custody) != 1)
        {
            return Err(DraftMarkerSealServiceError::FlightBusy);
        }
        let live = std::mem::take(&mut state.flights);
        let orphans = std::mem::take(&mut state.orphans);
        let captured = live
            .iter()
            .chain(orphans.iter())
            .map(|flight| flight.handle)
            .collect();
        state.reserved = live.len() + orphans.len();
        state.recovery_owned = true;
        state.lifecycle = ServiceLifecycle::Retired(HomeLoss::Unavailable(HomeHealthState::Failed));
        state.backends.take();
        state.command_fault = CommandFault::default();
        state.reconcile_fault = ReconcileFault::default();
        Ok(DraftMarkerSealRetainedFlights {
            inner: self.inner.clone(),
            home_id: self.home_id,
            original_generation: self.generation,
            generation: self.generation,
            canonical_home: store.canonical_path().to_owned(),
            live: Some(live),
            orphans: Some(orphans),
            captured,
            bound: false,
        })
    }
}

impl DraftMarkerSealRetainedFlights {
    pub fn captured_flights(&self) -> impl Iterator<Item = DraftMarkerSealFlight> + '_ {
        self.captured.iter().copied()
    }

    pub(crate) fn authenticates(
        &self,
        service: &DraftMarkerSealService,
        flight: DraftMarkerSealFlight,
    ) -> bool {
        self.home_id == service.home_id
            && self.original_generation == service.generation
            && Arc::ptr_eq(&self.inner, &service.inner)
            && self.captured.contains(&flight)
    }

    pub(crate) fn authenticates_request(
        &self,
        home_id: BerylHomeId,
        generation: HomeGeneration,
        request: DraftMarkerSealFlightRequest,
    ) -> bool {
        self.home_id == home_id
            && self.original_generation == generation
            && self.captured.iter().any(|flight| flight.request == request)
    }

    pub fn bind_candidate(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        storage: SyndicStorage,
        assets: AssetState,
    ) -> Result<(), DraftMarkerSealServiceError> {
        if candidate.home_id() != self.home_id
            || candidate.service_reference().canonical_path() != self.canonical_home
        {
            return Err(DraftMarkerSealServiceError::ForeignHome);
        }
        if candidate.generation() == self.original_generation
            || (self.bound && candidate.generation() == self.generation)
        {
            return Err(DraftMarkerSealServiceError::HomeGenerationChanged);
        }
        let access = candidate
            .recovery_access()
            .map_err(|_| DraftMarkerSealServiceError::HomeGenerationChanged)?;
        storage.revision_candidate(&access)?;
        assets.revision_candidate(&access)?;
        let mut state = lock_state(&self.inner);
        if state.active_borrows != 0
            || !state.recovery_owned
            || state.home_generation != self.generation
            || state
                .flights
                .iter()
                .chain(state.orphans.iter())
                .any(|flight| flight.driving || Arc::strong_count(&flight.custody) != 1)
        {
            return Err(DraftMarkerSealServiceError::FlightBusy);
        }
        state.backends = Some(MarkerBackends { storage, assets });
        if let Some(live) = self.live.take() {
            state.flights = live;
        }
        if let Some(orphans) = self.orphans.take() {
            state.orphans = orphans;
        }
        let mut index = 0;
        while index < state.orphans.len() {
            if state.orphans[index].collision {
                index += 1;
            } else {
                let flight = state.orphans.swap_remove(index);
                state.flights.push(flight);
            }
        }
        state.reserved = 0;
        state.home_generation = access.generation();
        state.lifecycle = ServiceLifecycle::Recovering;
        self.generation = access.generation();
        self.bound = true;
        Ok(())
    }

    fn bound_service(&self) -> Result<DraftMarkerSealService, DraftMarkerSealServiceError> {
        let state = lock_state(&self.inner);
        if !self.bound || !state.recovery_owned || state.home_generation != self.generation {
            return Err(DraftMarkerSealServiceError::HomeGenerationChanged);
        }
        Ok(DraftMarkerSealService {
            inner: self.inner.clone(),
            home_id: self.home_id,
            generation: self.generation,
        })
    }

    pub fn admit_candidate(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        request: DraftMarkerSealFlightRequest,
        cancellation: &beryl_home_store::CommandCancellation,
    ) -> Result<DraftMarkerSealAdmission, DraftMarkerSealServiceError> {
        let service = self.bound_service()?;
        let access = candidate
            .recovery_access()
            .map_err(|_| DraftMarkerSealServiceError::HomeGenerationChanged)?;
        service.admit_with_access(
            super::access::MarkerAccess::Candidate(&access),
            request,
            cancellation,
        )
    }

    pub fn drive_candidate(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        flight: DraftMarkerSealFlight,
    ) -> Result<DraftMarkerSealDriveOutcome, DraftMarkerSealServiceError> {
        let service = self.bound_service()?;
        let access = candidate
            .recovery_access()
            .map_err(|_| DraftMarkerSealServiceError::HomeGenerationChanged)?;
        service.drive_with_access(super::access::MarkerAccess::Candidate(&access), flight)
    }

    pub fn release_candidate(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        flight: DraftMarkerSealFlight,
        intent: DraftMarkerSealReleaseIntent,
    ) -> Result<DraftMarkerSealReleaseOutcome, DraftMarkerSealServiceError> {
        let service = self.bound_service()?;
        let access = candidate
            .recovery_access()
            .map_err(|_| DraftMarkerSealServiceError::HomeGenerationChanged)?;
        service.release_with_access(
            super::access::MarkerAccess::Candidate(&access),
            flight,
            intent,
        )
    }
}

impl Drop for DraftMarkerSealRetainedFlights {
    fn drop(&mut self) {
        let mut state = lock_state(&self.inner);
        if state.home_generation != self.generation || state.active_borrows != 0 {
            return;
        }
        if let Some(live) = self.live.take() {
            state.flights = live;
        }
        if let Some(orphans) = self.orphans.take() {
            state.orphans = orphans;
        }
        state.reserved = 0;
        state.recovery_owned = false;
        state.lifecycle = ServiceLifecycle::Retired(HomeLoss::GenerationChanged);
        state.backends.take();
    }
}
