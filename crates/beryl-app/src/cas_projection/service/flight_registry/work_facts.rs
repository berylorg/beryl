use super::*;

impl FlightRegistry {
    pub(in crate::cas_projection::service) fn try_work_revision()
    -> Result<u64, crate::cas_projection::runtime_work::RuntimeWorkError> {
        use crate::cas_projection::runtime_work::RuntimeWorkError;
        PROJECTION_FLIGHTS
            .get()
            .ok_or(RuntimeWorkError::Unavailable)?
            .try_lock()?
            .revision
            .ok_or(RuntimeWorkError::Unavailable)
    }

    pub(in crate::cas_projection::service) fn work_revision()
    -> Result<u64, ProjectionCoordinatorError> {
        let state = PROJECTION_FLIGHTS
            .get_or_init(|| Mutex::new(ProjectionFlightState::default()))
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionFlights,
            })?;
        state.revision.ok_or(
            ProjectionCoordinatorError::RegistryWorkRevisionUnavailable {
                registry: ProjectionRegistryKind::ProjectionFlights,
            },
        )
    }

    pub(in crate::cas_projection::service) fn work_prefix(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        revision: u64,
        after: Option<SyndicThreadId>,
        limit: usize,
    ) -> Result<Vec<SyndicThreadId>, ProjectionCoordinatorError> {
        let state = PROJECTION_FLIGHTS
            .get_or_init(|| Mutex::new(ProjectionFlightState::default()))
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionFlights,
            })?;
        if state.revision != Some(revision) {
            return Err(ProjectionCoordinatorError::RegistryWorkSourceChanged {
                registry: ProjectionRegistryKind::ProjectionFlights,
            });
        }
        let limit = limit.clamp(1, 257);
        let mut threads = std::collections::BTreeSet::new();
        for key in state.keys() {
            if key.home_id == home_id
                && key.home_generation == home_generation
                && after.is_none_or(|after| key.thread_id > after)
            {
                threads.insert(key.thread_id);
                if threads.len() > limit {
                    threads.pop_last();
                }
            }
        }
        Ok(threads.into_iter().collect())
    }
}
