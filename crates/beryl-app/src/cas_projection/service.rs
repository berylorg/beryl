use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};

use beryl_home_store::{HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::{BerylHomeId, SyndicThreadId};

use super::{ProjectionCoordinatorError, ProjectionRegistryKind};

/// App-owned coordinator for projection work against one exact healthy home generation.
///
/// The coordinator serializes only competing work for the same Syndic thread.
/// Different threads remain independent. Its mutexes protect bounded in-memory
/// registry mutations only and are never retained across backend or storage
/// work.
pub struct CasProjectionCoordinator {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
}

impl CasProjectionCoordinator {
    /// Binds a coordinator to the exact identity and current healthy generation of `home`.
    pub fn for_healthy_home(home: &HomeStore) -> Result<Self, ProjectionCoordinatorError> {
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
            });
        }
        let Some(home_generation) = health.generation() else {
            return Err(ProjectionCoordinatorError::HealthyHomeGenerationMissing);
        };
        Ok(Self {
            home_id: home.home_id(),
            home_generation,
        })
    }

    /// Returns the exact durable home identity bound at construction.
    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    /// Returns the exact healthy home generation bound at construction.
    #[must_use]
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    pub(crate) fn ensure_home(&self, home: &HomeStore) -> Result<(), ProjectionCoordinatorError> {
        if home.home_id() != self.home_id {
            return Err(ProjectionCoordinatorError::HomeIdentityMismatch {
                expected: self.home_id,
                actual: home.home_id(),
            });
        }
        let health = home.health();
        if health.state() != HomeHealthState::Healthy
            || health.generation() != Some(self.home_generation)
        {
            return Err(ProjectionCoordinatorError::HomeGenerationMismatch {
                expected: self.home_generation,
                actual: health.generation(),
                state: health.state(),
            });
        }
        Ok(())
    }

    /// Acquires the one process-wide projection flight for `thread_id` in this home generation.
    ///
    /// The returned non-cloneable guard releases the flight on drop. The
    /// registry mutex has already been released when this method returns.
    pub(super) fn begin_projection(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        FlightRegistry::acquire(self.home_id, self.home_generation, thread_id)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ProjectionFlightKey {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    thread_id: SyndicThreadId,
}

static PROJECTION_FLIGHTS: OnceLock<Mutex<HashSet<ProjectionFlightKey>>> = OnceLock::new();

pub(super) struct FlightRegistry;

impl FlightRegistry {
    pub(super) fn acquire(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        thread_id: SyndicThreadId,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        let registry = PROJECTION_FLIGHTS.get_or_init(|| Mutex::new(HashSet::new()));
        let mut active =
            registry
                .lock()
                .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                    registry: ProjectionRegistryKind::ProjectionFlights,
                })?;
        let key = ProjectionFlightKey {
            home_id,
            home_generation,
            thread_id,
        };
        if !active.insert(key) {
            return Err(ProjectionCoordinatorError::ProjectionInFlight { thread_id });
        }
        drop(active);
        Ok(ProjectionFlight { key })
    }

    fn release(key: ProjectionFlightKey) {
        let registry = PROJECTION_FLIGHTS.get_or_init(|| Mutex::new(HashSet::new()));
        let mut active = match registry.lock() {
            Ok(active) => active,
            Err(poisoned) => poisoned.into_inner(),
        };
        active.remove(&key);
    }
}

/// Non-cloneable RAII ownership of one Syndic thread's projection flight.
///
/// Holding this guard excludes another projection operation for the same
/// Syndic thread without holding a mutex during backend or storage work.
#[must_use = "dropping the guard releases the thread's projection flight"]
pub(super) struct ProjectionFlight {
    key: ProjectionFlightKey,
}

impl Drop for ProjectionFlight {
    fn drop(&mut self) {
        FlightRegistry::release(self.key);
    }
}
