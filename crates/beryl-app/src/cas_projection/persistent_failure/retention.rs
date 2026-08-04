use std::{
    collections::{HashMap, hash_map::Entry},
    fmt,
    sync::{Arc, Mutex, OnceLock},
};

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, DomainRevision};
use syndic_storage::SyndicStorage;

use super::{
    LiveCommandAuthorizer, MasterCommandGate, PersistentFailureCoordinator,
    PersistentFailureCutIdentity, PersistentFailureCutSnapshot, PersistentFailureGeneration,
    ProjectionServiceGeneration,
};
use crate::cas_projection::{
    ProjectionCoordinatorError,
    accepted_input_scheduler::{AcceptedInputScheduler, AcceptedInputSchedulerSignal},
    connection::ProjectionConnection,
    context_compaction::ContextCompactionCoordinator,
    scheduled_ordinary::ScheduledOrdinaryExecutionProvider,
    service_config::{ProjectionServiceConfig, ProjectionWorkerPool},
    service_registry::ProjectionServiceConnectionRegistry,
    stop::StopCoordinator,
};

mod inventory;

pub(in crate::cas_projection) use inventory::{
    PersistentFailureOldServiceEpochRetirementError,
    PersistentFailureOldServiceEpochRetirementReason,
};
pub use inventory::{
    PersistentFailureRecoveryInventory, PersistentFailureRecoveryInventoryError,
    PersistentFailureRecoveryInventoryMetadata,
};
pub(in crate::cas_projection::persistent_failure) use inventory::{
    PersistentFailureTerminalDispositionWitness, PersistentFailureTerminalRetirementError,
};

/// Non-cloneable process handoff for one exact retained persistent-failure cut.
#[must_use = "the persistent-failure handoff remains the process recovery authority"]
pub struct PersistentFailureCutHandoff {
    retained: Arc<PersistentFailureRetainedService>,
    _escrow: Arc<PersistentFailureServiceEscrowCell>,
}

/// Exclusive reservation for one service generation's first persistent-failure cut.
pub(in crate::cas_projection) struct PersistentFailureServiceEscrowReservation {
    identity: PersistentFailureCutIdentity,
    cell: Arc<PersistentFailureServiceEscrowCell>,
    active: bool,
}

struct PersistentFailureServiceEscrowCell {
    retained: Mutex<Vec<Arc<PersistentFailureRetainedService>>>,
}

/// Stability of the one-shot cut retained by a failure handoff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistentFailureCutCompletion {
    /// The cut worker and command drain reached their stable retained boundary.
    Finished,
    /// Worker or gate synchronization failed; conservative resources remain retained.
    Incomplete,
}

impl PersistentFailureCutHandoff {
    #[must_use]
    pub fn home_id(&self) -> BerylHomeId {
        self.retained.home_id
    }

    #[must_use]
    pub fn home_generation(&self) -> HomeGeneration {
        self.retained.home_generation
    }

    #[must_use]
    pub fn service_generation(&self) -> ProjectionServiceGeneration {
        self.retained.service_generation
    }

    #[must_use]
    pub fn failure_generation(&self) -> PersistentFailureGeneration {
        self.retained.failure_generation
    }

    #[must_use]
    pub fn cut_snapshot(&self) -> PersistentFailureCutSnapshot {
        self.retained.persistent_failure.snapshot()
    }

    #[must_use]
    pub fn completion(&self) -> PersistentFailureCutCompletion {
        self.retained.completion
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn escrow_registered_for_test(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
        failure_generation: PersistentFailureGeneration,
    ) -> bool {
        let identity = PersistentFailureCutIdentity::new(
            home_id,
            home_generation,
            service_generation,
            failure_generation,
        );
        retained_services()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .contains_key(&identity)
    }
}

impl fmt::Debug for PersistentFailureCutHandoff {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentFailureCutHandoff")
            .field("home_id", &self.home_id())
            .field("home_generation", &self.home_generation())
            .field("service_generation", &self.service_generation())
            .field("failure_generation", &self.failure_generation())
            .field("cut_snapshot", &self.cut_snapshot())
            .field("completion", &self.completion())
            .finish_non_exhaustive()
    }
}

#[allow(
    dead_code,
    reason = "Phase 78 consumes the passive retained-service escrow"
)]
pub(in crate::cas_projection) struct PersistentFailureRetainedService {
    home: Arc<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    storage_revision: DomainRevision,
    config: ProjectionServiceConfig,
    workers: ProjectionWorkerPool,
    service_generation: ProjectionServiceGeneration,
    failure_generation: PersistentFailureGeneration,
    command_gate: MasterCommandGate,
    command_authorizer: LiveCommandAuthorizer,
    persistent_failure: PersistentFailureCoordinator,
    connections: Arc<ProjectionServiceConnectionRegistry>,
    retained_connections: Vec<Arc<ProjectionConnection>>,
    stop_coordinator: Arc<StopCoordinator>,
    context_compaction: Option<Arc<ContextCompactionCoordinator>>,
    scheduler: Mutex<Option<AcceptedInputScheduler>>,
    scheduler_signal: AcceptedInputSchedulerSignal,
    scheduled_ordinary_provider: Option<Arc<Mutex<Box<dyn ScheduledOrdinaryExecutionProvider>>>>,
    completion: PersistentFailureCutCompletion,
}

static RETAINED_SERVICES: OnceLock<
    Mutex<HashMap<PersistentFailureCutIdentity, Arc<PersistentFailureServiceEscrowCell>>>,
> = OnceLock::new();

fn retained_services()
-> &'static Mutex<HashMap<PersistentFailureCutIdentity, Arc<PersistentFailureServiceEscrowCell>>> {
    RETAINED_SERVICES.get_or_init(|| Mutex::new(HashMap::new()))
}

impl PersistentFailureServiceEscrowReservation {
    pub(in crate::cas_projection) fn reserve(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
    ) -> Result<Self, ProjectionCoordinatorError> {
        let identity = PersistentFailureCutIdentity::new(
            home_id,
            home_generation,
            service_generation,
            PersistentFailureGeneration::FIRST,
        );
        let cell = Arc::new(PersistentFailureServiceEscrowCell {
            retained: Mutex::new(Vec::new()),
        });
        let mut services = retained_services()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match services.entry(identity) {
            Entry::Vacant(entry) => {
                entry.insert(Arc::clone(&cell));
            }
            Entry::Occupied(_) => {
                return Err(
                    ProjectionCoordinatorError::PersistentFailureEscrowIdentityAlreadyReserved,
                );
            }
        }
        Ok(Self {
            identity,
            cell,
            active: true,
        })
    }
}

impl PersistentFailureRetainedService {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection) fn new(
        home: Arc<HomeStore>,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        storage: SyndicStorage,
        storage_revision: DomainRevision,
        config: ProjectionServiceConfig,
        workers: ProjectionWorkerPool,
        service_generation: ProjectionServiceGeneration,
        failure_generation: PersistentFailureGeneration,
        command_gate: MasterCommandGate,
        command_authorizer: LiveCommandAuthorizer,
        persistent_failure: PersistentFailureCoordinator,
        connections: Arc<ProjectionServiceConnectionRegistry>,
        retained_connections: Vec<Arc<ProjectionConnection>>,
        stop_coordinator: Arc<StopCoordinator>,
        context_compaction: Option<Arc<ContextCompactionCoordinator>>,
        scheduler: Option<AcceptedInputScheduler>,
        scheduler_signal: AcceptedInputSchedulerSignal,
        scheduled_ordinary_provider: Option<
            Arc<Mutex<Box<dyn ScheduledOrdinaryExecutionProvider>>>,
        >,
        completion: PersistentFailureCutCompletion,
    ) -> Self {
        Self {
            home,
            home_id,
            home_generation,
            storage,
            storage_revision,
            config,
            workers,
            service_generation,
            failure_generation,
            command_gate,
            command_authorizer,
            persistent_failure,
            connections,
            retained_connections,
            stop_coordinator,
            context_compaction,
            scheduler: Mutex::new(scheduler),
            scheduler_signal,
            scheduled_ordinary_provider,
            completion,
        }
    }

    pub(in crate::cas_projection) fn escrow(
        self,
        mut reservation: PersistentFailureServiceEscrowReservation,
    ) -> PersistentFailureCutHandoff {
        let identity = PersistentFailureCutIdentity::new(
            self.home_id,
            self.home_generation,
            self.service_generation,
            self.failure_generation,
        );
        debug_assert_eq!(identity, reservation.identity);
        let retained = Arc::new(self);
        reservation
            .cell
            .retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(Arc::clone(&retained));
        reservation.active = false;
        PersistentFailureCutHandoff {
            retained,
            _escrow: Arc::clone(&reservation.cell),
        }
    }
}

impl Drop for PersistentFailureServiceEscrowReservation {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        let mut services = retained_services()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let is_empty = self
            .cell
            .retained
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .is_empty();
        if is_empty
            && services
                .get(&self.identity)
                .is_some_and(|cell| Arc::ptr_eq(cell, &self.cell))
        {
            services.remove(&self.identity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion};

    #[test]
    fn exact_escrow_reservation_rejects_duplicates_and_drop_reopens_identity() {
        let directory = tempfile::tempdir().unwrap();
        let home = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let home_id = home.home_id();
        let home_generation = home.health().generation().unwrap();
        let service_generation = ProjectionServiceGeneration::allocate().unwrap();
        let first = PersistentFailureServiceEscrowReservation::reserve(
            home_id,
            home_generation,
            service_generation,
        )
        .unwrap();

        assert!(matches!(
            PersistentFailureServiceEscrowReservation::reserve(
                home_id,
                home_generation,
                service_generation,
            ),
            Err(ProjectionCoordinatorError::PersistentFailureEscrowIdentityAlreadyReserved)
        ));

        drop(first);
        let replacement = PersistentFailureServiceEscrowReservation::reserve(
            home_id,
            home_generation,
            service_generation,
        )
        .unwrap();
        drop(replacement);
        home.close().unwrap();
    }

    #[test]
    fn old_epoch_retirement_removes_only_the_exact_unaliased_escrow_cell() {
        let directory = tempfile::tempdir().unwrap();
        let home = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let home_id = home.home_id();
        let home_generation = home.health().generation().unwrap();
        let service_generation = ProjectionServiceGeneration::allocate().unwrap();
        let reservation = PersistentFailureServiceEscrowReservation::reserve(
            home_id,
            home_generation,
            service_generation,
        )
        .unwrap();
        let identity = reservation.identity;
        let foreign = Arc::new(PersistentFailureServiceEscrowCell {
            retained: Mutex::new(Vec::new()),
        });

        assert_eq!(
            inventory::remove_exact_retirement_escrow(identity, &foreign),
            Err(PersistentFailureOldServiceEpochRetirementReason::EscrowIdentityMismatch)
        );
        let alias = Arc::clone(&reservation.cell);
        assert_eq!(
            inventory::remove_exact_retirement_escrow(identity, &reservation.cell),
            Err(PersistentFailureOldServiceEpochRetirementReason::EscrowOwnerAliased)
        );
        drop(alias);
        assert_eq!(
            inventory::remove_exact_retirement_escrow(identity, &reservation.cell),
            Ok(())
        );
        assert!(
            !retained_services()
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .contains_key(&identity)
        );

        drop(reservation);
        let replacement = PersistentFailureServiceEscrowReservation::reserve(
            home_id,
            home_generation,
            service_generation,
        )
        .unwrap();
        drop(replacement);
        home.close().unwrap();
    }
}
