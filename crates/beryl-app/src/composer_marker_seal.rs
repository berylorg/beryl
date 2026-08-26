use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, Weak},
};

use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::BerylHomeId;
use beryl_state::{AssetReferenceSetStagingAuthority, AssetState};
use syndic_storage::{DraftMarkerSealFailureReasonV1, DraftMarkerSealProofV1, SyndicStorage};

mod admission;
mod drive;
mod durability;
mod terminal;
mod types;

pub use types::*;

use drive::{drive_asset_seal, drive_begin, drive_page};
use durability::{lock_state, validate_store};
use terminal::finish_disposal;

pub struct DraftMarkerSealService {
    inner: Arc<Mutex<ServiceState>>,
    home_id: BerylHomeId,
}

#[derive(Default)]
struct SharedHomeRegistry {
    homes: HashMap<BerylHomeId, Weak<Mutex<ServiceState>>>,
}

static SHARED_HOME_STATES: OnceLock<Mutex<SharedHomeRegistry>> = OnceLock::new();

struct ServiceState {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    assets: AssetState,
    limits: DraftMarkerSealServiceLimits,
    flights: Vec<FlightState>,
    next_serial: u64,
    high_water: usize,
    denials: u64,
    coalesces: u64,
    conflicts: u64,
    lifecycle: ServiceLifecycle,
    command_fault: CommandFault,
    reconcile_fault: ReconcileFault,
    #[cfg(feature = "test-faults")]
    fail_next_drive_operationally: bool,
}

#[derive(Clone, Copy)]
struct FlightState {
    handle: DraftMarkerSealFlight,
    phase: FlightPhase,
    driving: bool,
    terminal: Option<DraftMarkerSealReleaseIntent>,
}

#[derive(Clone, Copy)]
enum ServiceLifecycle {
    Active,
    Disposing,
    Retired(HomeLoss),
    Disposed,
}

#[derive(Clone, Copy)]
enum HomeLoss {
    Unavailable(HomeHealthState),
    GenerationChanged,
}

#[derive(Clone, Copy)]
enum FlightPhase {
    PendingBegin,
    Streaming {
        staging: Option<AssetReferenceSetStagingAuthority>,
    },
    SealingAsset {
        staging: AssetReferenceSetStagingAuthority,
        syndic: DraftMarkerSealProofV1,
    },
}

enum DriveUpdate {
    Keep(FlightPhase, DraftMarkerSealDriveOutcome),
    Complete(DraftMarkerSealDriveOutcome),
}

enum DurableCommandResult {
    ExactOld,
    ExactNew,
}

#[cfg(feature = "test-faults")]
#[derive(Default)]
struct CommandFault(Option<Box<dyn FnOnce(&HomeStore) + Send + 'static>>);

#[cfg(not(feature = "test-faults"))]
#[derive(Default)]
struct CommandFault;

#[cfg(feature = "test-faults")]
#[derive(Default)]
struct ReconcileFault(
    Option<
        Box<dyn FnOnce(&HomeStore, SyndicStorage, DraftMarkerSealFlightRequest) + Send + 'static>,
    >,
);

#[cfg(not(feature = "test-faults"))]
#[derive(Default)]
struct ReconcileFault;

impl DraftMarkerSealService {
    pub fn new(
        store: &HomeStore,
        home_generation: HomeGeneration,
        storage: SyndicStorage,
        assets: AssetState,
        limits: DraftMarkerSealServiceLimits,
    ) -> Result<Self, DraftMarkerSealServiceConstructionError> {
        if store.health().generation() != Some(home_generation) {
            return Err(DraftMarkerSealServiceConstructionError::HomeGenerationMismatch);
        }
        let home_id = store.home_id();
        let inner =
            acquire_shared_home_state(store, home_id, home_generation, storage, assets, limits)?;
        Ok(Self { inner, home_id })
    }

    pub fn drive(
        &self,
        store: &HomeStore,
        flight: DraftMarkerSealFlight,
    ) -> Result<DraftMarkerSealDriveOutcome, DraftMarkerSealServiceError> {
        let (
            storage,
            assets,
            page_limit,
            phase,
            command_fault,
            reconcile_fault,
            fail_operationally,
        ) = {
            let mut state = lock_state(&self.inner);
            validate_store(&mut state, store)?;
            let storage = state.storage;
            let assets = state.assets;
            let page_limit = state.limits.markers_per_page.get();
            let current = state
                .flights
                .iter_mut()
                .find(|current| current.handle == flight)
                .ok_or(DraftMarkerSealServiceError::StaleFlight)?;
            if current.driving {
                return Err(DraftMarkerSealServiceError::FlightBusy);
            }
            if current.terminal.is_some() {
                return Err(DraftMarkerSealServiceError::TerminalSettlementRequired);
            }
            current.driving = true;
            let phase = current.phase;
            let command_fault = state.command_fault.take();
            let reconcile_fault = state.reconcile_fault.take();
            #[cfg(feature = "test-faults")]
            let fail_operationally = std::mem::take(&mut state.fail_next_drive_operationally);
            #[cfg(not(feature = "test-faults"))]
            let fail_operationally = false;
            (
                storage,
                assets,
                page_limit,
                phase,
                command_fault,
                reconcile_fault,
                fail_operationally,
            )
        };

        let update = if fail_operationally {
            Err(DraftMarkerSealServiceError::InjectedOperationalFailure)
        } else {
            match phase {
                FlightPhase::PendingBegin => drive_begin(
                    store,
                    storage,
                    assets,
                    flight.request,
                    command_fault,
                    reconcile_fault,
                ),
                FlightPhase::Streaming { staging } => drive_page(
                    store,
                    storage,
                    assets,
                    flight.request,
                    staging,
                    page_limit,
                    command_fault,
                    reconcile_fault,
                ),
                FlightPhase::SealingAsset { staging, syndic } => drive_asset_seal(
                    store,
                    storage,
                    assets,
                    flight.request,
                    staging,
                    syndic,
                    command_fault,
                    reconcile_fault,
                ),
            }
        };

        let mut state = lock_state(&self.inner);
        let Some(index) = state
            .flights
            .iter()
            .position(|current| current.handle == flight)
        else {
            return Err(DraftMarkerSealServiceError::HomeGenerationChanged);
        };
        if let ServiceLifecycle::Retired(loss) = state.lifecycle {
            state.flights.swap_remove(index);
            return Err(match loss {
                HomeLoss::Unavailable(state) => DraftMarkerSealServiceError::HomeUnavailable(state),
                HomeLoss::GenerationChanged => DraftMarkerSealServiceError::HomeGenerationChanged,
            });
        }
        match update {
            Ok(DriveUpdate::Keep(next, outcome)) => {
                state.flights[index].phase = next;
                state.flights[index].driving = false;
                Ok(match state.flights[index].terminal {
                    Some(intent) => DraftMarkerSealDriveOutcome::TerminalSettlementPending(intent),
                    None => outcome,
                })
            }
            Ok(DriveUpdate::Complete(outcome)) => {
                state.flights.swap_remove(index);
                finish_disposal(&mut state);
                Ok(outcome)
            }
            Err(DraftMarkerSealServiceError::ReconciliationCollision) => {
                state.flights.swap_remove(index);
                finish_disposal(&mut state);
                Err(DraftMarkerSealServiceError::ReconciliationCollision)
            }
            Err(error) => {
                state.flights[index].driving = false;
                if state.flights[index].terminal.is_none() {
                    state.flights[index].terminal = Some(DraftMarkerSealReleaseIntent::Failed(
                        DraftMarkerSealFailureReasonV1::Operational,
                    ));
                }
                Err(error)
            }
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_before_command_fault(&self, fault: impl FnOnce(&HomeStore) + Send + 'static) {
        let mut state = lock_state(&self.inner);
        assert!(state.command_fault.0.is_none());
        state.command_fault.0 = Some(Box::new(fault));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_before_reconcile_fault(
        &self,
        fault: impl FnOnce(&HomeStore, SyndicStorage, DraftMarkerSealFlightRequest) + Send + 'static,
    ) {
        let mut state = lock_state(&self.inner);
        assert!(state.reconcile_fault.0.is_none());
        state.reconcile_fault.0 = Some(Box::new(fault));
    }

    #[cfg(feature = "test-faults")]
    pub fn test_fail_next_drive_operationally(&self) {
        let mut state = lock_state(&self.inner);
        assert!(!state.fail_next_drive_operationally);
        state.fail_next_drive_operationally = true;
    }

    pub fn diagnostics(&self) -> DraftMarkerSealServiceDiagnostics {
        let state = lock_state(&self.inner);
        DraftMarkerSealServiceDiagnostics {
            configured_flight_limit: state.limits.max_concurrent_flights.get(),
            current_flights: state.flights.len(),
            high_water_flights: state.high_water,
            admission_denials: state.denials,
            coalesced_admissions: state.coalesces,
            conflicts: state.conflicts,
            driving_flights: state.flights.iter().filter(|flight| flight.driving).count(),
            terminalizing_flights: state
                .flights
                .iter()
                .filter(|flight| flight.terminal.is_some())
                .count(),
            retained_draft_sized_bytes: 0,
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn test_hold_state_lock(
        &self,
        reached: std::sync::mpsc::Sender<()>,
        release: std::sync::mpsc::Receiver<()>,
    ) {
        let _state = lock_state(&self.inner);
        reached.send(()).unwrap();
        release.recv().unwrap();
    }
}

impl Clone for DraftMarkerSealService {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            home_id: self.home_id,
        }
    }
}

impl Drop for DraftMarkerSealService {
    fn drop(&mut self) {
        let registry = SHARED_HOME_STATES.get_or_init(|| Mutex::new(SharedHomeRegistry::default()));
        let mut registry = registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if Arc::strong_count(&self.inner) == 1 {
            let is_current = registry
                .homes
                .get(&self.home_id)
                .is_some_and(|current| Weak::ptr_eq(current, &Arc::downgrade(&self.inner)));
            if is_current {
                registry.homes.remove(&self.home_id);
            }
        }
    }
}

fn acquire_shared_home_state(
    store: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    assets: AssetState,
    limits: DraftMarkerSealServiceLimits,
) -> Result<Arc<Mutex<ServiceState>>, DraftMarkerSealServiceConstructionError> {
    validate_construction_authority(store, storage, assets)?;
    loop {
        let existing = {
            let registry =
                SHARED_HOME_STATES.get_or_init(|| Mutex::new(SharedHomeRegistry::default()));
            let mut registry = registry
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            registry.homes.retain(|_, state| state.strong_count() > 0);
            registry.homes.get(&home_id).and_then(Weak::upgrade)
        };
        let Some(existing) = existing else {
            let state = new_shared_home_state(home_id, home_generation, storage, assets, limits);
            let registry =
                SHARED_HOME_STATES.get_or_init(|| Mutex::new(SharedHomeRegistry::default()));
            let mut registry = registry
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            registry
                .homes
                .retain(|_, current| current.strong_count() > 0);
            if registry
                .homes
                .get(&home_id)
                .and_then(Weak::upgrade)
                .is_none()
            {
                registry.homes.insert(home_id, Arc::downgrade(&state));
                return Ok(state);
            }
            continue;
        };

        let (existing_generation, existing_storage, existing_assets, existing_limits, terminal) = {
            let state = lock_state(&existing);
            (
                state.home_generation,
                state.storage,
                state.assets,
                state.limits,
                state.flights.is_empty()
                    && matches!(
                        state.lifecycle,
                        ServiceLifecycle::Disposed | ServiceLifecycle::Retired(_)
                    ),
            )
        };
        if !terminal {
            if existing_generation != home_generation {
                return Err(DraftMarkerSealServiceConstructionError::HomeGenerationMismatch);
            }
            if existing_limits != limits {
                return Err(DraftMarkerSealServiceConstructionError::LimitsMismatch);
            }
            validate_construction_authority(store, existing_storage, existing_assets)?;
            return Ok(existing);
        }

        let replacement = new_shared_home_state(home_id, home_generation, storage, assets, limits);
        let registry = SHARED_HOME_STATES.get_or_init(|| Mutex::new(SharedHomeRegistry::default()));
        let mut registry = registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registry
            .homes
            .retain(|_, current| current.strong_count() > 0);
        let is_current = registry
            .homes
            .get(&home_id)
            .is_some_and(|current| Weak::ptr_eq(current, &Arc::downgrade(&existing)));
        if is_current {
            registry.homes.insert(home_id, Arc::downgrade(&replacement));
            return Ok(replacement);
        }
    }
}

fn new_shared_home_state(
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    assets: AssetState,
    limits: DraftMarkerSealServiceLimits,
) -> Arc<Mutex<ServiceState>> {
    Arc::new(Mutex::new(ServiceState {
        home_id,
        home_generation,
        storage,
        assets,
        limits,
        flights: Vec::new(),
        next_serial: 1,
        high_water: 0,
        denials: 0,
        coalesces: 0,
        conflicts: 0,
        lifecycle: ServiceLifecycle::Active,
        command_fault: CommandFault::default(),
        reconcile_fault: ReconcileFault::default(),
        #[cfg(feature = "test-faults")]
        fail_next_drive_operationally: false,
    }))
}

fn validate_construction_authority(
    store: &HomeStore,
    storage: SyndicStorage,
    assets: AssetState,
) -> Result<(), DraftMarkerSealServiceConstructionError> {
    if storage.revision(store).is_err() || assets.revision(store).is_err() {
        return Err(DraftMarkerSealServiceConstructionError::DomainAuthorityMismatch);
    }
    Ok(())
}
