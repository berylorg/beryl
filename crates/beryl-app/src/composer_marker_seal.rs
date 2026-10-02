use std::sync::{Arc, Mutex};

use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::BerylHomeId;
use beryl_state::{AssetReferenceSetStagingAuthority, AssetState};
use syndic_storage::{DraftMarkerSealFailureReasonV1, DraftMarkerSealProofV1, SyndicStorage};

mod access;
mod admission;
mod custody;
mod drive;
mod durability;
pub(crate) mod initial_preparation;
mod recovery;
mod terminal;
mod types;

pub use recovery::DraftMarkerSealRetainedFlights;
pub use types::*;

use drive::{drive_asset_seal, drive_begin, drive_page};
use durability::lock_state;
use terminal::finish_disposal;

pub struct DraftMarkerSealService {
    inner: Arc<Mutex<ServiceState>>,
    home_id: BerylHomeId,
    generation: HomeGeneration,
}

struct ServiceState {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    backends: Option<MarkerBackends>,
    limits: DraftMarkerSealServiceLimits,
    flights: Vec<FlightState>,
    orphans: Vec<FlightState>,
    reserved: usize,
    recovery_owned: bool,
    active_borrows: usize,
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
    #[cfg(feature = "test-faults")]
    fail_next_drive_as_collision: bool,
}

struct FlightState {
    handle: DraftMarkerSealFlight,
    phase: FlightPhase,
    driving: bool,
    terminal: Option<DraftMarkerSealReleaseIntent>,
    custody: Arc<Mutex<custody::SealCommandCustody>>,
    collision: bool,
}

#[derive(Clone)]
struct MarkerBackends {
    storage: SyndicStorage,
    assets: AssetState,
}

impl ServiceState {
    fn storage(&self) -> SyndicStorage {
        self.backends
            .as_ref()
            .expect("validated marker backend custody")
            .storage
            .clone()
    }
    fn assets(&self) -> AssetState {
        self.backends
            .as_ref()
            .expect("validated marker backend custody")
            .assets
            .clone()
    }
}

#[derive(Clone, Copy)]
enum ServiceLifecycle {
    Active,
    Recovering,
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

#[derive(Clone, Copy)]
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
        Ok(Self {
            inner,
            home_id,
            generation: home_generation,
        })
    }

    pub fn drive(
        &self,
        store: &HomeStore,
        flight: DraftMarkerSealFlight,
    ) -> Result<DraftMarkerSealDriveOutcome, DraftMarkerSealServiceError> {
        self.drive_with_access(access::MarkerAccess::Ordinary(store), flight)
    }
    fn drive_with_access(
        &self,
        access: access::MarkerAccess<'_>,
        flight: DraftMarkerSealFlight,
    ) -> Result<DraftMarkerSealDriveOutcome, DraftMarkerSealServiceError> {
        let _borrow = self.borrow_for_access(access.is_candidate())?;
        let (
            storage,
            assets,
            page_limit,
            phase,
            command_fault,
            reconcile_fault,
            injected_failure,
            custody,
        ) = {
            let mut state = lock_state(&self.inner);
            access.validate(&mut state)?;
            let storage = state.storage();
            let assets = state.assets();
            let page_limit = state.limits.markers_per_page.get();
            if state
                .orphans
                .iter()
                .any(|orphan| orphan.handle == flight && orphan.collision)
            {
                return Err(DraftMarkerSealServiceError::ReconciliationCollision);
            }
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
            let custody = current.custody.clone();
            let command_fault = state.command_fault.take();
            let reconcile_fault = state.reconcile_fault.take();
            #[cfg(feature = "test-faults")]
            let injected_failure = if std::mem::take(&mut state.fail_next_drive_as_collision) {
                Some(DraftMarkerSealServiceError::ReconciliationCollision)
            } else {
                std::mem::take(&mut state.fail_next_drive_operationally)
                    .then_some(DraftMarkerSealServiceError::InjectedOperationalFailure)
            };
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
                custody,
            )
        };

        let settled = lock_state(&custody).require_settled(access);
        let update = if let Err(error) = settled {
            Err(error)
        } else if let Some(error) = injected_failure {
            Err(error)
        } else {
            match phase {
                FlightPhase::PendingBegin => drive_begin(
                    access,
                    &storage,
                    &assets,
                    flight.request,
                    command_fault,
                    reconcile_fault,
                    &custody,
                ),
                FlightPhase::Streaming { staging } => drive_page(
                    access,
                    &storage,
                    &assets,
                    flight.request,
                    staging,
                    page_limit,
                    command_fault,
                    reconcile_fault,
                    &custody,
                ),
                FlightPhase::SealingAsset { staging, syndic } => drive_asset_seal(
                    access,
                    &storage,
                    &assets,
                    flight.request,
                    staging,
                    syndic,
                    command_fault,
                    reconcile_fault,
                    &custody,
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
            let mut orphan = state.flights.swap_remove(index);
            orphan.driving = false;
            if matches!(
                update,
                Err(DraftMarkerSealServiceError::ReconciliationCollision)
            ) {
                orphan.collision = true;
            }
            state.orphans.push(orphan);
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
                let mut orphan = state.flights.swap_remove(index);
                orphan.driving = false;
                orphan.collision = true;
                state.orphans.push(orphan);
                finish_disposal(&mut state);
                Err(DraftMarkerSealServiceError::ReconciliationCollision)
            }
            Err(error) => {
                state.flights[index].driving = false;
                if matches!(access, access::MarkerAccess::Ordinary(store) if store.health().state() == HomeHealthState::Healthy)
                    && state.flights[index].terminal.is_none()
                {
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

    #[cfg(feature = "test-faults")]
    pub fn test_fail_next_drive_as_collision(&self) {
        let mut state = lock_state(&self.inner);
        assert!(!state.fail_next_drive_as_collision);
        state.fail_next_drive_as_collision = true;
    }

    pub fn diagnostics(&self) -> DraftMarkerSealServiceDiagnostics {
        let state = lock_state(&self.inner);
        DraftMarkerSealServiceDiagnostics {
            configured_flight_limit: state.limits.max_concurrent_flights.get(),
            current_flights: state.flights.len(),
            retained_flights: state.orphans.len() + state.reserved,
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

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_generation_retired(&self) -> bool {
        matches!(
            lock_state(&self.inner).lifecycle,
            ServiceLifecycle::Retired(_)
        )
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
            generation: self.generation,
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
        backends: Some(MarkerBackends { storage, assets }),
        limits,
        flights: Vec::new(),
        orphans: Vec::new(),
        reserved: 0,
        recovery_owned: false,
        active_borrows: 0,
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
        #[cfg(feature = "test-faults")]
        fail_next_drive_as_collision: false,
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
