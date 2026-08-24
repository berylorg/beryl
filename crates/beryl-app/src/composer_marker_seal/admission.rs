use beryl_home_store::{CommandCancellation, HomeStore};
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionReadOutcomeV1,
    SyndicStorage,
};

use super::{
    DraftMarkerSealAdmission, DraftMarkerSealFlight, DraftMarkerSealFlightRequest,
    DraftMarkerSealReleaseIntent, DraftMarkerSealService, DraftMarkerSealServiceError, FlightPhase,
    FlightState, ServiceLifecycle, durability::validate_store, lock_state,
};

impl DraftMarkerSealService {
    pub fn admit(
        &self,
        store: &HomeStore,
        request: DraftMarkerSealFlightRequest,
        cancellation: &CommandCancellation,
    ) -> Result<DraftMarkerSealAdmission, DraftMarkerSealServiceError> {
        if cancellation.is_cancelled() {
            return Ok(DraftMarkerSealAdmission::CancelledBeforeAdmission);
        }
        if !request.is_coherent() {
            return Err(DraftMarkerSealServiceError::InvalidCandidateBinding);
        }
        let storage = {
            let mut state = lock_state(&self.inner);
            validate_store(&mut state, store)?;
            require_active(state.lifecycle)?;
            state.storage
        };
        authenticate_candidate(storage, store, request.candidate)?;

        let mut state = lock_state(&self.inner);
        validate_store(&mut state, store)?;
        require_active(state.lifecycle)?;
        if let Some(existing) = state
            .flights
            .iter()
            .find(|flight| flight.handle.request.operation_id == request.operation_id)
            .copied()
        {
            if existing.handle.request == request {
                state.coalesces = state.coalesces.saturating_add(1);
                return Ok(DraftMarkerSealAdmission::Coalesced(existing.handle));
            }
            state.conflicts = state.conflicts.saturating_add(1);
            state.denials = state.denials.saturating_add(1);
            return Ok(DraftMarkerSealAdmission::Conflict);
        }
        if state
            .flights
            .iter()
            .any(|flight| flight.handle.request.staging == request.staging)
        {
            state.conflicts = state.conflicts.saturating_add(1);
            state.denials = state.denials.saturating_add(1);
            return Ok(DraftMarkerSealAdmission::Conflict);
        }
        if state.flights.len() >= state.limits.max_concurrent_flights.get() {
            state.denials = state.denials.saturating_add(1);
            return Ok(DraftMarkerSealAdmission::Saturated);
        }
        let serial = state.next_serial;
        state.next_serial = state
            .next_serial
            .checked_add(1)
            .ok_or(DraftMarkerSealServiceError::SerialExhausted)?;
        let handle = DraftMarkerSealFlight { serial, request };
        state.flights.push(FlightState {
            handle,
            phase: FlightPhase::PendingBegin,
            driving: false,
            terminal: None,
        });
        state.high_water = state.high_water.max(state.flights.len());
        Ok(DraftMarkerSealAdmission::Admitted(handle))
    }
}

pub(super) fn authenticate_candidate(
    storage: SyndicStorage,
    store: &HomeStore,
    expected: DraftEditorCandidateActivationBindingV1,
) -> Result<(), DraftMarkerSealServiceError> {
    match storage.draft_editor_candidate_session(
        store,
        expected.draft_id(),
        expected.session_id(),
    )? {
        DraftEditorCandidateSessionReadOutcomeV1::Active(head)
            if DraftEditorCandidateActivationBindingV1::from_head(&head) == expected =>
        {
            Ok(())
        }
        DraftEditorCandidateSessionReadOutcomeV1::Active(_) => {
            Err(DraftMarkerSealServiceError::StaleCandidateBinding)
        }
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(_) => {
            Err(DraftMarkerSealServiceError::CandidateSessionDisposed)
        }
        DraftEditorCandidateSessionReadOutcomeV1::Absent => {
            Err(DraftMarkerSealServiceError::CandidateSessionAbsent)
        }
        DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange => {
            Err(DraftMarkerSealServiceError::CandidateSessionConcurrentChange)
        }
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure => {
            Err(DraftMarkerSealServiceError::CandidateSessionInvariant)
        }
    }
}

pub(super) fn authenticate_supersession(
    storage: SyndicStorage,
    store: &HomeStore,
    request: DraftMarkerSealFlightRequest,
    intent: DraftMarkerSealReleaseIntent,
) -> Result<(), DraftMarkerSealServiceError> {
    let DraftMarkerSealReleaseIntent::Superseded {
        successor_operation_id,
        successor,
    } = intent
    else {
        return Ok(());
    };
    if successor_operation_id == request.operation_id
        || successor.draft_id() != request.candidate.draft_id()
        || successor.session_id() != request.candidate.session_id()
    {
        return Err(DraftMarkerSealServiceError::InvalidCandidateBinding);
    }
    authenticate_candidate(storage, store, successor)
}

fn require_active(lifecycle: ServiceLifecycle) -> Result<(), DraftMarkerSealServiceError> {
    match lifecycle {
        ServiceLifecycle::Active => Ok(()),
        ServiceLifecycle::Disposing => Err(DraftMarkerSealServiceError::ServiceDisposing),
        ServiceLifecycle::Retired(super::HomeLoss::Unavailable(state)) => {
            Err(DraftMarkerSealServiceError::HomeUnavailable(state))
        }
        ServiceLifecycle::Retired(super::HomeLoss::GenerationChanged) => {
            Err(DraftMarkerSealServiceError::HomeGenerationChanged)
        }
        ServiceLifecycle::Disposed => Err(DraftMarkerSealServiceError::ServiceDisposed),
    }
}
