use std::sync::Arc;

use beryl_home_store::{DomainHandleError, HomeGeneration, HomeHealthState, HomeStore};
use beryl_model::BerylHomeId;
use beryl_state::{BerylState, BerylStateReacquireError};
use syndic_storage::SyndicStorage;
use thiserror::Error;

use super::super::super::{
    ProjectionConnectionServiceCloseError, error::ProjectionCoordinatorError,
    persistent_failure::ProjectionServiceGeneration,
    scheduled_ordinary::ScheduledOrdinaryExecutionProvider,
    service_config::ProjectionServiceConfig, service_startup::ServiceStartupGate,
};
use super::ProjectionConnectionService;

/// Content-free identity of one never-published replacement service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnpublishedProjectionConnectionServiceMetadata {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
}

/// Construction failure before a replacement service can become adoption authority.
#[derive(Debug, Error)]
pub enum UnpublishedProjectionConnectionServiceBuildError {
    #[error("the recovered Beryl handle set could not be reacquired")]
    BerylState(#[source] BerylStateReacquireError),
    #[error("the recovered Syndic handle could not be reacquired")]
    SyndicStorage(#[source] DomainHandleError),
    #[error(transparent)]
    Service(#[from] ProjectionCoordinatorError),
}

/// Non-cloneable service typestate whose workers are all held behind one startup fence.
#[must_use = "the unpublished service owns its recovered home and dormant worker topology"]
pub struct UnpublishedProjectionConnectionService {
    pub(super) service: Option<ProjectionConnectionService>,
    beryl_state: Option<BerylState>,
    pub(super) startup_gate: Option<Arc<ServiceStartupGate>>,
    #[cfg(test)]
    replacement_resource_failure: Option<ReplacementResourceFailureSelectorForTest>,
}

/// One exact replacement resource failure selected for a stable connection in an adoption test.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ReplacementResourceFailureForTest {
    WorkerCapacity { connection_generation: u64 },
    BrokerSpawn { connection_generation: u64 },
}

/// Attempt-owned, one-shot selection transferred from dormant replacement construction.
#[cfg(test)]
pub(super) struct ReplacementResourceFailureSelectorForTest {
    failure: Option<ReplacementResourceFailureForTest>,
}

struct UnpublishedServiceStartupGuard {
    gate: Option<Arc<ServiceStartupGate>>,
}

impl UnpublishedProjectionConnectionServiceMetadata {
    #[must_use]
    pub const fn home_id(self) -> BerylHomeId {
        self.home_id
    }

    #[must_use]
    pub const fn home_generation(self) -> HomeGeneration {
        self.home_generation
    }

    #[must_use]
    pub const fn service_generation(self) -> ProjectionServiceGeneration {
        self.service_generation
    }
}

impl UnpublishedServiceStartupGuard {
    fn new() -> Self {
        Self {
            gate: Some(ServiceStartupGate::closed_gate()),
        }
    }

    fn gate(&self) -> &Arc<ServiceStartupGate> {
        self.gate
            .as_ref()
            .expect("an armed unpublished-service constructor retains its startup gate")
    }

    fn disarm(mut self) -> Arc<ServiceStartupGate> {
        self.gate
            .take()
            .expect("a complete unpublished service takes its startup gate once")
    }
}

impl Drop for UnpublishedServiceStartupGuard {
    fn drop(&mut self) {
        if let Some(gate) = self.gate.take() {
            gate.cancel();
        }
    }
}

impl UnpublishedProjectionConnectionService {
    /// Reacquires the complete current-generation handle set from one retained same-home store and
    /// constructs every service worker behind one shared closed startup fence.
    pub(in crate::cas_projection) fn from_recovered_home(
        home: Arc<HomeStore>,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Result<Self, UnpublishedProjectionConnectionServiceBuildError> {
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
            }
            .into());
        }
        let beryl_state = BerylState::reacquire(&home)
            .map_err(UnpublishedProjectionConnectionServiceBuildError::BerylState)?;
        let storage = SyndicStorage::reacquire(&home)
            .map_err(UnpublishedProjectionConnectionServiceBuildError::SyndicStorage)?;
        Self::from_recovered_handles(
            home,
            beryl_state,
            storage,
            config,
            scheduled_ordinary_provider,
        )
    }

    /// Constructs one dormant replacement from the complete handles reacquired by the process
    /// supervisor for the exact recovered generation.
    pub(in crate::cas_projection) fn from_recovered_handles(
        home: Arc<HomeStore>,
        beryl_state: BerylState,
        storage: SyndicStorage,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Result<Self, UnpublishedProjectionConnectionServiceBuildError> {
        let health = home.health();
        if health.state() != HomeHealthState::Healthy {
            return Err(ProjectionCoordinatorError::HomeNotHealthy {
                state: health.state(),
                generation: health.generation(),
            }
            .into());
        }
        let startup_guard = UnpublishedServiceStartupGuard::new();
        let service = match ProjectionConnectionService::new_dormant_with_startup_gate(
            home,
            storage,
            config,
            scheduled_ordinary_provider,
            Arc::clone(startup_guard.gate()),
        ) {
            Ok(service) => service,
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            service: Some(service),
            beryl_state: Some(beryl_state),
            startup_gate: Some(startup_guard.disarm()),
            #[cfg(test)]
            replacement_resource_failure: None,
        })
    }

    /// Constructs a dormant replacement that carries one deterministic resource failure only for
    /// the consuming adoption attempt.
    #[cfg(test)]
    pub(in crate::cas_projection) fn from_recovered_home_with_replacement_resource_failure_for_test(
        home: Arc<HomeStore>,
        config: ProjectionServiceConfig,
        scheduled_ordinary_provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
        failure: ReplacementResourceFailureForTest,
    ) -> Result<Self, UnpublishedProjectionConnectionServiceBuildError> {
        let mut replacement = Self::from_recovered_home(home, config, scheduled_ordinary_provider)?;
        replacement.replacement_resource_failure =
            Some(ReplacementResourceFailureSelectorForTest {
                failure: Some(failure),
            });
        Ok(replacement)
    }

    #[must_use]
    pub fn metadata(&self) -> UnpublishedProjectionConnectionServiceMetadata {
        let service = self
            .service
            .as_ref()
            .expect("an unpublished service retains its dormant service topology");
        UnpublishedProjectionConnectionServiceMetadata {
            home_id: service.home_id(),
            home_generation: service.home_generation(),
            service_generation: service.service_generation(),
        }
    }

    pub(in super::super) fn service(&self) -> &ProjectionConnectionService {
        self.service
            .as_ref()
            .expect("an unpublished service retains its dormant service topology")
    }

    pub(super) fn service_mut(&mut self) -> &mut ProjectionConnectionService {
        self.service
            .as_mut()
            .expect("an unpublished service retains its dormant service topology")
    }

    pub(in super::super) fn take_service(&mut self) -> ProjectionConnectionService {
        self.service
            .take()
            .expect("an unpublished service transfers its dormant service topology once")
    }

    pub(in super::super) fn take_beryl_state(&mut self) -> BerylState {
        self.beryl_state
            .take()
            .expect("an unpublished service transfers its recovered Beryl handles once")
    }

    pub(in super::super) fn startup_gate(&self) -> &Arc<ServiceStartupGate> {
        self.startup_gate
            .as_ref()
            .expect("an unpublished service retains its shared startup fence")
    }

    pub(in super::super) fn take_startup_gate(&mut self) -> Arc<ServiceStartupGate> {
        self.startup_gate
            .take()
            .expect("an unpublished service transfers its shared startup fence once")
    }

    #[cfg(test)]
    pub(super) fn take_replacement_resource_failure_for_test(
        &mut self,
    ) -> Option<ReplacementResourceFailureSelectorForTest> {
        self.replacement_resource_failure.take()
    }

    pub(in crate::cas_projection) fn attach_recovery_supervisor(
        &self,
        signal: std::sync::mpsc::SyncSender<()>,
    ) -> Result<(), ()> {
        self.service().attach_recovery_supervisor(signal)
    }

    /// Consumes a never-published replacement through its nonpublishing terminal lifecycle.
    ///
    /// The startup fence closes before the service joins any dormant workers. The retained home
    /// belongs to the supervisor and is deliberately not closed here.
    pub(in crate::cas_projection) fn dispose_for_supervisor_terminal(
        mut self,
    ) -> Result<(), ProjectionConnectionServiceCloseError> {
        if let Some(gate) = self.startup_gate.take() {
            gate.cancel();
        }
        let result = self.service.as_mut().map_or(
            Ok(()),
            ProjectionConnectionService::dispose_unpublished_inert,
        );
        drop(self.service.take());
        let _ = self.beryl_state.take();
        result
    }
}

impl Drop for UnpublishedProjectionConnectionService {
    fn drop(&mut self) {
        if let Some(gate) = self.startup_gate.take() {
            gate.cancel();
        }
    }
}

#[cfg(test)]
impl ReplacementResourceFailureSelectorForTest {
    pub(super) fn is_consumed(&self) -> bool {
        self.failure.is_none()
    }

    pub(super) fn take_worker_capacity_for_connection(
        &mut self,
        connection_generation: u64,
    ) -> bool {
        matches!(
            self.failure,
            Some(ReplacementResourceFailureForTest::WorkerCapacity {
                connection_generation: expected,
            }) if expected == connection_generation
        ) && self.failure.take().is_some()
    }

    pub(super) fn take_broker_spawn_for_connection(&mut self, connection_generation: u64) -> bool {
        matches!(
            self.failure,
            Some(ReplacementResourceFailureForTest::BrokerSpawn {
                connection_generation: expected,
            }) if expected == connection_generation
        ) && self.failure.take().is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{ServiceStartupGate, UnpublishedProjectionConnectionService};

    #[test]
    fn implicit_unpublished_owner_drop_cancels_and_wakes_the_startup_fence() {
        let gate = ServiceStartupGate::closed_gate();
        let blocked_gate = Arc::clone(&gate);
        let waiter = std::thread::spawn(move || blocked_gate.wait());
        let owner = UnpublishedProjectionConnectionService {
            service: None,
            beryl_state: None,
            startup_gate: Some(gate),
            replacement_resource_failure: None,
        };

        drop(owner);
        assert!(!waiter.join().unwrap());
    }
}
