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
    persistent_failure::PersistentFailureTerminalDisposer, stop::StopCoordinator,
};

use super::provider_broker::{ProviderBrokerControl, RunningProviderBrokerIngester};

/// Immutable service identity owned by one connection for its complete lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct ConnectionAttachmentIdentity {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    service_generation: ProjectionServiceGeneration,
}

/// Complete immutable execution attachment owned by one connection.
pub(super) struct ConnectionAttachment {
    pub(super) identity: ConnectionAttachmentIdentity,
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
    pub(super) terminal_disposer: PersistentFailureTerminalDisposer,
}

impl ConnectionAttachmentIdentity {
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

impl std::fmt::Debug for ConnectionAttachment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectionAttachment")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl ConnectionAttachment {
    /// Elects ordinary retirement and arms ingester-admission release before cancellation.
    pub(super) fn begin_ordinary_retirement(&self) -> bool {
        if !self.persistent_failure.begin_ordinary_retirement() {
            return false;
        }
        if let Ok(ingester) = self.ingester.lock()
            && let Some(ingester) = ingester.as_ref()
        {
            let _ = ingester.arm_ordinary_worker_release();
        }
        true
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
        let (ingester, poisoned) = match self.ingester.lock() {
            Ok(mut ingester) => (ingester.take(), false),
            Err(poison) => (poison.into_inner().take(), true),
        };
        let ingester = ingester.ok_or(ProjectionCoordinatorError::ProjectionWorkerStopped)?;
        let stopped = ingester.stop_and_join();
        if poisoned {
            drop(stopped);
            return Err(ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionConnection,
            });
        }
        Ok(stopped)
    }

    pub(super) fn stop_and_join_ingester_terminal(&self) -> Result<(), ProjectionCoordinatorError> {
        let (ingester, poisoned) = match self.ingester.lock() {
            Ok(mut ingester) => (ingester.take(), false),
            Err(poison) => (poison.into_inner().take(), true),
        };
        let ingester = ingester.ok_or(ProjectionCoordinatorError::ProjectionWorkerStopped)?;
        let stopped = ingester.stop_and_join();
        let receipt = stopped.receipt();
        drop(stopped.into_worker());
        let receipt_result = receipt
            .validate_exact(
                self.identity.service_generation(),
                self.identity.home_generation(),
            )
            .map_err(|_| ProjectionCoordinatorError::ProjectionWorkerStopped);
        if poisoned {
            return Err(ProjectionCoordinatorError::RegistryPoisoned {
                registry: ProjectionRegistryKind::ProjectionConnection,
            });
        }
        receipt_result
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn poison_ingester_handle_for_test(&self) {
        let attachment = &self.ingester;
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let _guard = attachment
                        .lock()
                        .expect("the ingester handle begins unpoisoned");
                    panic!("poison the exact ingester-handle mutex");
                })
                .join()
                .expect_err("the poison worker must panic");
        });
    }
}
