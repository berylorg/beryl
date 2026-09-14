use super::*;
use crate::cas_projection::ordinary::{OrdinaryTurnExecutionError, TerminalHistoryCompletion};

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../tests/unit/terminal_completion_flight.rs"]
mod tests;

#[cfg(feature = "test-faults")]
impl CasProjectionCoordinator {
    pub fn terminal_completion_for_test(
        &self,
        thread_id: SyndicThreadId,
    ) -> Result<
        Option<crate::cas_projection::test_faults::TerminalCompletionProbe>,
        ProjectionCoordinatorError,
    > {
        FlightRegistry::terminal_completion(
            self.home_id,
            self.home_generation,
            thread_id,
            FlightRegistry::work_revision()?,
        )
        .map(|observer| observer.map(crate::cas_projection::test_faults::TerminalCompletionProbe))
    }
}

#[derive(Debug)]
struct TerminalCompletionSlot {
    key: ProjectionFlightKey,
    service_generation: crate::cas_projection::ProjectionServiceGeneration,
    turn_id: SyndicTurnId,
    completed: OnceLock<TerminalHistoryCompletion>,
}

#[derive(Clone, Debug)]
pub(crate) struct TerminalCompletionObserver(Arc<TerminalCompletionSlot>);

impl PartialEq for TerminalCompletionObserver {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for TerminalCompletionObserver {}

impl TerminalCompletionObserver {
    pub(crate) fn thread_id(&self) -> SyndicThreadId {
        self.0.key.thread_id
    }

    pub(crate) fn service_generation(&self) -> crate::cas_projection::ProjectionServiceGeneration {
        self.0.service_generation
    }

    pub(crate) fn turn_id(&self) -> SyndicTurnId {
        self.0.turn_id
    }

    pub(crate) fn completion(&self) -> Option<&TerminalHistoryCompletion> {
        self.0.completed.get()
    }

    pub(crate) fn matches(
        &self,
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> bool {
        self.0.key
            == (ProjectionFlightKey {
                home_id,
                home_generation,
                thread_id,
            })
            && self.0.turn_id == turn_id
    }
}

pub(in crate::cas_projection) struct TerminalCompletionPublisher(Arc<TerminalCompletionSlot>);

impl TerminalCompletionPublisher {
    pub(in crate::cas_projection) fn publish(
        &self,
        completion: TerminalHistoryCompletion,
    ) -> Result<(), OrdinaryTurnExecutionError> {
        if completion.home_id() != self.0.key.home_id
            || completion.home_generation() != self.0.key.home_generation
            || completion.thread_id() != self.0.key.thread_id
            || completion.turn_id() != self.0.turn_id
        {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "terminal completion belongs to another execution flight",
            ));
        }
        self.0.completed.set(completion).map_err(|_| {
            OrdinaryTurnExecutionError::Invariant("execution flight completed more than once")
        })
    }
}

impl ProjectionFlight {
    pub(in crate::cas_projection) fn bind_terminal_completion(
        &self,
        turn_id: SyndicTurnId,
    ) -> Result<TerminalCompletionPublisher, OrdinaryTurnExecutionError> {
        let service_generation = self
            .acquisition()
            .ok_or(OrdinaryTurnExecutionError::Invariant(
                "ordinary execution flight has no admitted service",
            ))?
            .service_generation();
        let mut active = PROJECTION_FLIGHTS
            .get_or_init(|| Mutex::new(ProjectionFlightState::default()))
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionFlights,
            })?;
        let entry = active
            .get_mut(&self.key)
            .ok_or(OrdinaryTurnExecutionError::Invariant(
                "ordinary execution flight is missing",
            ))?;
        if entry.terminal_completion.is_some() {
            return Err(OrdinaryTurnExecutionError::Invariant(
                "projection flight already owns an ordinary execution",
            ));
        }
        let slot = Arc::new(TerminalCompletionSlot {
            key: self.key,
            service_generation,
            turn_id,
            completed: OnceLock::new(),
        });
        entry.terminal_completion = Some(TerminalCompletionObserver(Arc::clone(&slot)));
        Ok(TerminalCompletionPublisher(slot))
    }
}

impl FlightRegistry {
    pub(in crate::cas_projection::service) fn terminal_completions(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: crate::cas_projection::ProjectionServiceGeneration,
        revision: u64,
        limit: usize,
    ) -> Result<Vec<TerminalCompletionObserver>, super::super::process_work::ProcessWorkError> {
        let active = PROJECTION_FLIGHTS
            .get_or_init(|| Mutex::new(ProjectionFlightState::default()))
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionFlights,
            })?;
        if active.revision != Some(revision) {
            return Err(super::super::process_work::ProcessWorkError::StaleRevision);
        }
        let mut observers = Vec::new();
        for (key, entry) in active.iter() {
            if key.home_id != home_id || key.home_generation != home_generation {
                continue;
            }
            let Some(observer) = &entry.terminal_completion else {
                continue;
            };
            if observer.service_generation() != service_generation {
                continue;
            }
            if observers.len() == limit {
                return Err(super::super::process_work::ProcessWorkError::SourceBoundExceeded);
            }
            observers.push(observer.clone());
        }
        Ok(observers)
    }

    pub(in crate::cas_projection::service) fn terminal_completion(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        thread_id: SyndicThreadId,
        revision: u64,
    ) -> Result<Option<TerminalCompletionObserver>, ProjectionCoordinatorError> {
        let active = PROJECTION_FLIGHTS
            .get_or_init(|| Mutex::new(ProjectionFlightState::default()))
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionFlights,
            })?;
        if active.revision != Some(revision) {
            return Err(ProjectionCoordinatorError::RegistryWorkSourceChanged {
                registry: ProjectionRegistryKind::ProjectionFlights,
            });
        }
        Ok(active
            .get(&ProjectionFlightKey {
                home_id,
                home_generation,
                thread_id,
            })
            .and_then(|entry| entry.terminal_completion.clone()))
    }
}
