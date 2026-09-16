use std::sync::{Arc, Mutex};

use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::BerylHomeId;
use beryl_state::{AssetReferenceSetStagingAuthority, AssetState};
use syndic_storage::{DraftMarkerSealFailureReasonV1, DraftMarkerSealProofV1, SyndicStorage};

mod admission;
mod drive;
mod durability;
pub(crate) mod initial_preparation;
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
    #[cfg(feature = "test-faults")]
    pub fn test_new(
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
        validate_construction_authority(store, &storage, &assets)?;
        let inner = new_shared_home_state(home_id, home_generation, storage, assets, limits);
        Ok(Self { inner, home_id })
    }

    pub fn drive(
        &self,
        store: &HomeStore,
        flight: DraftMarkerSealFlight,
    ) -> Result<DraftMarkerSealDriveOutcome, DraftMarkerSealServiceError> {
        let (storage, assets, page_limit, phase, command_fault, reconcile_fault, injected_failure) = {
            let mut state = lock_state(&self.inner);
            validate_store(&mut state, store)?;
            let storage = state.storage.clone();
            let assets = state.assets.clone();
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
            let injected_failure = std::mem::take(&mut state.fail_next_drive_operationally)
                .then_some(DraftMarkerSealServiceError::InjectedOperationalFailure);
            #[cfg(not(feature = "test-faults"))]
            let injected_failure: Option<DraftMarkerSealServiceError> = None;
            (
                storage,
                assets,
                page_limit,
                phase,
                command_fault,
                reconcile_fault,
                injected_failure,
            )
        };

        let update = if let Some(error) = injected_failure {
            Err(error)
        } else {
            match phase {
                FlightPhase::PendingBegin => drive_begin(
                    store,
                    &storage,
                    &assets,
                    flight.request,
                    command_fault,
                    reconcile_fault,
                ),
                FlightPhase::Streaming { staging } => drive_page(
                    store,
                    &storage,
                    &assets,
                    flight.request,
                    staging,
                    page_limit,
                    command_fault,
                    reconcile_fault,
                ),
                FlightPhase::SealingAsset { staging, syndic } => drive_asset_seal(
                    store,
                    &storage,
                    &assets,
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

#[cfg(feature = "test-faults")]
fn validate_construction_authority(
    store: &HomeStore,
    storage: &SyndicStorage,
    assets: &AssetState,
) -> Result<(), DraftMarkerSealServiceConstructionError> {
    if storage.revision(store).is_err() || assets.revision(store).is_err() {
        return Err(DraftMarkerSealServiceConstructionError::DomainAuthorityMismatch);
    }
    Ok(())
}
