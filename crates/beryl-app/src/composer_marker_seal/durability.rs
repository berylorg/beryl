use std::sync::Mutex;

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeHealthState, HomeStore, ReconciliationResolution,
};
use syndic_storage::DraftMarkerSealProofV1;

use super::{
    CommandFault, DraftMarkerSealFlightRequest, DraftMarkerSealServiceError, DurableCommandResult,
    HomeLoss, ReconcileFault, ServiceLifecycle, ServiceState,
};
use syndic_storage::SyndicStorage;

pub(super) fn require_matching_frontier(
    manifest: &beryl_state::AssetReferenceSetManifest,
    syndic: DraftMarkerSealProofV1,
) -> Result<(), DraftMarkerSealServiceError> {
    if manifest.entry_frontier() != syndic.sequential().marker_count()
        || manifest.sequential() != syndic.sequential()
        || manifest.ordered_assets() != syndic.ordered_assets()
    {
        return Err(DraftMarkerSealServiceError::FrontierMismatch);
    }
    Ok(())
}

pub(super) fn settle_command(
    store: &HomeStore,
    outcome: CommandOutcome,
    storage: &SyndicStorage,
    request: DraftMarkerSealFlightRequest,
    fault: ReconcileFault,
) -> Result<DurableCommandResult, DraftMarkerSealServiceError> {
    match outcome {
        CommandOutcome::NotCommitted { .. } => Ok(DurableCommandResult::ExactOld),
        CommandOutcome::Committed { .. } => Ok(DurableCommandResult::ExactNew),
        CommandOutcome::Indeterminate { reconciliation, .. } => {
            let handle = reconciliation.install_and_handle();
            fault.run(store, storage, request);
            match store.reconcile(&handle)? {
                ReconciliationResolution::ExactOld => Ok(DurableCommandResult::ExactOld),
                ReconciliationResolution::ExactNew { .. } => Ok(DurableCommandResult::ExactNew),
                ReconciliationResolution::ExactSuccessor { .. }
                | ReconciliationResolution::Collision => {
                    Err(DraftMarkerSealServiceError::ReconciliationCollision)
                }
            }
        }
    }
}

pub(super) fn execute_command(
    store: &HomeStore,
    command: HomeCommand,
    fault: CommandFault,
) -> CommandOutcome {
    fault.run(store);
    store.execute(command)
}

pub(super) fn validate_store(
    state: &mut ServiceState,
    store: &HomeStore,
) -> Result<(), DraftMarkerSealServiceError> {
    if store.home_id() != state.home_id {
        return Err(DraftMarkerSealServiceError::ForeignHome);
    }
    let health = store.health();
    if health.state() != HomeHealthState::Healthy {
        retire(state, HomeLoss::Unavailable(health.state()));
        return Err(DraftMarkerSealServiceError::HomeUnavailable(health.state()));
    }
    if health.generation() != Some(state.home_generation) {
        retire(state, HomeLoss::GenerationChanged);
        return Err(DraftMarkerSealServiceError::HomeGenerationChanged);
    }
    match state.lifecycle {
        ServiceLifecycle::Active | ServiceLifecycle::Disposing => Ok(()),
        ServiceLifecycle::Retired(HomeLoss::Unavailable(state)) => {
            Err(DraftMarkerSealServiceError::HomeUnavailable(state))
        }
        ServiceLifecycle::Retired(HomeLoss::GenerationChanged) => {
            Err(DraftMarkerSealServiceError::HomeGenerationChanged)
        }
        ServiceLifecycle::Disposed => Err(DraftMarkerSealServiceError::ServiceDisposed),
    }
}

pub(super) fn lock_state(inner: &Mutex<ServiceState>) -> std::sync::MutexGuard<'_, ServiceState> {
    inner
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn retire(state: &mut ServiceState, loss: HomeLoss) -> (usize, usize) {
    state.lifecycle = ServiceLifecycle::Retired(loss);
    let before = state.flights.len();
    state.flights.retain(|flight| flight.driving);
    let settling = state.flights.len();
    (before - settling, settling)
}

impl ReconcileFault {
    #[cfg(feature = "test-faults")]
    pub(super) fn take(&mut self) -> Self {
        Self(self.0.take())
    }

    #[cfg(not(feature = "test-faults"))]
    pub(super) const fn take(&mut self) -> Self {
        Self
    }

    #[cfg(feature = "test-faults")]
    fn run(
        self,
        store: &HomeStore,
        storage: &SyndicStorage,
        request: DraftMarkerSealFlightRequest,
    ) {
        if let Some(fault) = self.0 {
            fault(store, storage.clone(), request);
        }
    }

    #[cfg(not(feature = "test-faults"))]
    fn run(
        self,
        _store: &HomeStore,
        _storage: &SyndicStorage,
        _request: DraftMarkerSealFlightRequest,
    ) {
    }
}

impl CommandFault {
    #[cfg(feature = "test-faults")]
    pub(super) fn take(&mut self) -> Self {
        Self(self.0.take())
    }

    #[cfg(not(feature = "test-faults"))]
    pub(super) const fn take(&mut self) -> Self {
        Self
    }

    #[cfg(feature = "test-faults")]
    fn run(self, store: &HomeStore) {
        if let Some(fault) = self.0 {
            fault(store);
        }
    }

    #[cfg(not(feature = "test-faults"))]
    fn run(self, _store: &HomeStore) {}
}
