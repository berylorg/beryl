use std::sync::{Arc, Mutex};

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
use syndic_storage::SyndicStorage;

use super::{EventRouter, persistent_failure::PersistentFailureDriverSlot};
use crate::cas_projection::{
    LiveCommandAuthorizer, PersistentFailureNotification, ProjectionCoordinatorError,
    ProjectionRegistryKind, ProjectionServiceGeneration,
    accepted_input_scheduler::AcceptedInputSchedulerSignal,
    context_compaction::ContextCompactionCoordinator,
    persistent_failure::{PersistentFailureCutIdentity, PersistentFailureProjectionRetainer},
    stop::StopCoordinator,
};

use super::provider_broker::{ProviderBrokerControl, RunningProviderBrokerIngester};

/// Exact replaceable service-generation identity attached to one stable connection core.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct ConnectionEpochIdentity {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
}

/// Complete service-owned execution attachment for one stable connection core.
pub(super) struct ConnectionServiceEpoch {
    pub(super) identity: ConnectionEpochIdentity,
    pub(super) home: Arc<HomeStore>,
    pub(super) storage: SyndicStorage,
    pub(super) router: Arc<EventRouter>,
    pub(super) broker: Arc<ProviderBrokerControl>,
    pub(super) ingester: Mutex<Option<RunningProviderBrokerIngester>>,
    pub(super) commands: LiveCommandAuthorizer,
    pub(super) persistent_failure: Arc<PersistentFailureDriverSlot>,
    pub(super) stop_coordinator: Arc<StopCoordinator>,
    pub(super) context_compaction: Arc<ContextCompactionCoordinator>,
    pub(super) scheduler_signal: AcceptedInputSchedulerSignal,
    pub(super) failure_notification: PersistentFailureNotification,
    pub(super) projection_retainer: PersistentFailureProjectionRetainer,
}

impl ConnectionEpochIdentity {
    pub(in crate::cas_projection) const fn new(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        service_generation: ProjectionServiceGeneration,
    ) -> Self {
        Self {
            home_id,
            home_generation,
            service_generation,
        }
    }

    pub(in crate::cas_projection) const fn home_id(self) -> BerylHomeId {
        self.home_id
    }

    pub(in crate::cas_projection) const fn home_generation(self) -> HomeGeneration {
        self.home_generation
    }

    pub(in crate::cas_projection) const fn service_generation(self) -> ProjectionServiceGeneration {
        self.service_generation
    }
}

impl std::fmt::Debug for ConnectionServiceEpoch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectionServiceEpoch")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl ConnectionServiceEpoch {
    /// Elects ordinary retirement and arms ingester-admission release before any caller may
    /// request cancellation.
    pub(super) fn begin_ordinary_retirement(&self) -> bool {
        if !self.persistent_failure.begin_ordinary_retirement() {
            return false;
        }
        if let Ok(ingester) = self.ingester.lock()
            && let Some(ingester) = ingester.as_ref()
        {
            // Poison or an impossible conflicting disposition retains the exact permit for the
            // later consuming join. Ordinary retirement still owns the sticky epoch closure.
            let _ = ingester.arm_ordinary_worker_release();
        }
        true
    }

    /// Arms exact-cut worker retention before adoption may cancel or join the old ingester.
    pub(super) fn arm_ingester_worker_retention_for_adoption(
        &self,
        cut: PersistentFailureCutIdentity,
    ) -> Result<(), ProjectionCoordinatorError> {
        if !self
            .persistent_failure
            .matches_finished_cut(cut)
            .unwrap_or(false)
        {
            return Err(ProjectionCoordinatorError::ProjectionWorkerStopped);
        }
        let ingester =
            self.ingester
                .lock()
                .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                    registry: ProjectionRegistryKind::ProjectionConnection,
                })?;
        let ingester = ingester
            .as_ref()
            .ok_or(ProjectionCoordinatorError::ProjectionWorkerStopped)?;
        ingester
            .arm_worker_retention_for_adoption(cut)
            .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped)
    }

    pub(super) fn request_ingester_cancel(&self) {
        self.broker.request_cancel();
        if let Ok(ingester) = self.ingester.lock()
            && let Some(ingester) = ingester.as_ref()
        {
            ingester.request_cancel();
        }
    }

    pub(super) fn ingester_is_finished(&self) -> bool {
        self.ingester.lock().is_ok_and(|ingester| {
            ingester
                .as_ref()
                .is_none_or(|ingester| ingester.is_finished())
        })
    }

    pub(super) fn stop_and_join_ingester(
        &self,
    ) -> Result<super::provider_broker::ProviderBrokerStopped, ProjectionCoordinatorError> {
        let ingester = self
            .ingester
            .lock()
            .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionConnection,
            })?
            .take()
            .ok_or(ProjectionCoordinatorError::ProjectionWorkerStopped)?;
        Ok(ingester.stop_and_join())
    }

    /// Consumes an ordinary-retirement join into terminal proof only. Any permit retained because
    /// disposition coordination was unavailable is released here by the consuming lifecycle owner.
    pub(super) fn stop_and_join_ingester_after_ordinary_retirement(
        &self,
    ) -> Result<super::provider_broker::ProviderBrokerTerminalReceipt, ProjectionCoordinatorError>
    {
        let stopped = self.stop_and_join_ingester()?;
        let receipt = stopped.receipt();
        drop(stopped.into_worker());
        Ok(receipt)
    }
}
