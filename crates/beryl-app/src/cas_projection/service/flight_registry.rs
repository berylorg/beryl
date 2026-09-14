use super::*;

mod work_facts;

impl CasProjectionCoordinator {
    /// Binds a coordinator to the exact identity and current healthy generation of `home`.
    pub fn for_healthy_home(home: &HomeStore) -> Result<Self, ProjectionCoordinatorError> {
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
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
    pub(in crate::cas_projection) fn begin_projection(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        FlightRegistry::acquire(self.home_id, self.home_generation, thread_id)
    }

    pub(in crate::cas_projection) fn begin_scheduled_projection(
        &self,
        thread_id: SyndicThreadId,
        scheduler_signal: &AcceptedInputSchedulerSignal,
        commands: &LiveCommandAuthorizer,
        command: &super::super::LiveCommandPermit,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        let acquisition =
            super::super::acquisition::ProjectionAcquisition::admit_from(commands, command)?;
        FlightRegistry::acquire_or_arm(
            self.home_id,
            self.home_generation,
            thread_id,
            scheduler_signal,
        )
        .map(|flight| flight.with_acquisition(acquisition))
    }

    pub(in crate::cas_projection) fn ensure_projection_flight(
        &self,
        flight: &ProjectionFlight,
        thread_id: SyndicThreadId,
    ) -> Result<(), ProjectionCoordinatorError> {
        if flight.key
            == (ProjectionFlightKey {
                home_id: self.home_id,
                home_generation: self.home_generation,
                thread_id,
            })
        {
            Ok(())
        } else {
            Err(ProjectionCoordinatorError::ProjectionFlightMismatch { thread_id })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ProjectionFlightKey {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    thread_id: SyndicThreadId,
}

static PROJECTION_FLIGHTS: OnceLock<Mutex<ProjectionFlightState>> = OnceLock::new();

struct ProjectionFlightState {
    revision: Option<u64>,
    entries: HashMap<ProjectionFlightKey, Option<AcceptedInputSchedulerSignal>>,
}

impl Default for ProjectionFlightState {
    fn default() -> Self {
        Self {
            revision: Some(0),
            entries: HashMap::new(),
        }
    }
}

impl std::ops::Deref for ProjectionFlightState {
    type Target = HashMap<ProjectionFlightKey, Option<AcceptedInputSchedulerSignal>>;

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl std::ops::DerefMut for ProjectionFlightState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.revision = self.revision.and_then(|value| value.checked_add(1));
        &mut self.entries
    }
}

pub(super) struct FlightRegistry;

impl FlightRegistry {
    pub(super) fn acquire(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        thread_id: SyndicThreadId,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        let registry =
            PROJECTION_FLIGHTS.get_or_init(|| Mutex::new(ProjectionFlightState::default()));
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
        if active.contains_key(&key) {
            return Err(ProjectionCoordinatorError::ProjectionInFlight { thread_id });
        }
        active.insert(key, None);
        drop(active);
        Ok(ProjectionFlight {
            key,
            acquisition: None,
        })
    }

    pub(super) fn acquire_or_arm(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        thread_id: SyndicThreadId,
        scheduler_signal: &AcceptedInputSchedulerSignal,
    ) -> Result<ProjectionFlight, ProjectionCoordinatorError> {
        let registry =
            PROJECTION_FLIGHTS.get_or_init(|| Mutex::new(ProjectionFlightState::default()));
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
        if let Some(release_signal) = active.get_mut(&key) {
            if release_signal.is_none() {
                *release_signal = Some(scheduler_signal.clone());
            }
            return Err(ProjectionCoordinatorError::ProjectionInFlight { thread_id });
        }
        active.insert(key, None);
        drop(active);
        Ok(ProjectionFlight {
            key,
            acquisition: None,
        })
    }

    fn release(key: ProjectionFlightKey) {
        let registry =
            PROJECTION_FLIGHTS.get_or_init(|| Mutex::new(ProjectionFlightState::default()));
        let mut active = match registry.lock() {
            Ok(active) => active,
            Err(poisoned) => poisoned.into_inner(),
        };
        let release_signal = active.remove(&key).flatten();
        drop(active);
        if let Some(signal) = release_signal {
            signal.wake(AcceptedInputWakeReason::ProjectionFlightReleased);
        }
    }
}

/// Non-cloneable RAII ownership of one Syndic thread's projection flight.
///
/// Holding this guard excludes another projection operation for the same
/// Syndic thread without holding a mutex during backend or storage work.
#[must_use = "dropping the guard releases the thread's projection flight"]
pub(in crate::cas_projection) struct ProjectionFlight {
    key: ProjectionFlightKey,
    acquisition: Option<super::super::acquisition::ProjectionAcquisition>,
}

impl ProjectionFlight {
    pub(in crate::cas_projection) fn with_acquisition(
        mut self,
        acquisition: super::super::acquisition::ProjectionAcquisition,
    ) -> Self {
        self.acquisition = Some(acquisition);
        self
    }

    pub(in crate::cas_projection) fn acquisition(
        &self,
    ) -> Option<&super::super::acquisition::ProjectionAcquisition> {
        self.acquisition.as_ref()
    }
}

impl Drop for ProjectionFlight {
    fn drop(&mut self) {
        FlightRegistry::release(self.key);
    }
}
