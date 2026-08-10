//! Process-owned current-service publication and same-home recovery supervision.

mod provider;
mod recovery;
mod slot;
mod worker;

use std::sync::{Arc, Mutex, mpsc};

use beryl_home_store::{DomainHandleError, HomeGeneration, HomeStore};
use beryl_state::{BerylState, BerylStateReacquireError};
use syndic_storage::SyndicStorage;
use thiserror::Error;

#[cfg(test)]
use self::worker::Phase93AdoptionObservation;
use self::{
    provider::ProviderFactoryOwner,
    worker::{RecoveryWorkerExit, RecoveryWorkerStart},
};
use crate::cas_projection::{
    ProjectionConnectionService, ProjectionConnectionServiceCloseError, ProjectionServiceConfig,
    ProjectionServiceGeneration, ScheduledOrdinaryExecutionProviderFactory,
    ScheduledOrdinaryProviderEpochContext,
};
pub use slot::RunningProjectionServiceLease;
pub(in crate::cas_projection) use slot::{PublishedServiceEpoch, RunningServiceSlot};

/// Why the current process publication slot cannot issue a scoped service lease.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RunningServiceAvailability {
    #[error("same-home service recovery is in progress")]
    Recovering,
    #[error("the running-session supervisor is shutting down")]
    ShuttingDown,
    #[error("the running-session service publication slot is unavailable")]
    Unavailable,
}

/// Bounded content-free state for one process-owned recovery supervisor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunningSessionRecoveryDiagnostics {
    pub(super) current_home_generation: Option<HomeGeneration>,
    pub(super) current_service_generation: Option<ProjectionServiceGeneration>,
    pub(super) active_service_leases: usize,
    pub(super) recovering: bool,
    pub(super) shutting_down: bool,
    pub(super) recovery_cycles: u64,
    pub(super) verification_successes: u64,
    pub(super) terminal_failures: u64,
}

/// Failure before the process recovery worker became the sole service owner.
#[derive(Debug, Error)]
pub enum RunningSessionRecoveryStartError {
    #[error("the exact home's Beryl state handles could not be reacquired: {0}")]
    BerylState(#[source] BerylStateReacquireError),
    #[error("the exact home's Syndic storage handle could not be reacquired: {0}")]
    SyndicStorage(#[source] DomainHandleError),
    #[error("the initial scheduled-ordinary provider epoch could not be created: {0}")]
    Provider(String),
    #[error("the initial projection service could not be constructed: {0}")]
    Service(#[source] crate::cas_projection::ProjectionCoordinatorError),
    #[error("the service recovery notification could not be attached")]
    NotificationAttachment,
    #[error("the same-home recovery worker could not be started: {0}")]
    WorkerSpawn(String),
}

/// Explicit shutdown failure after every reachable process-owned resource was asked to settle.
#[derive(Debug, Error)]
pub enum RunningSessionRecoveryShutdownError {
    #[error("the same-home recovery worker panicked")]
    WorkerPanicked,
    #[error("the current projection service failed to close: {0}")]
    Service(#[source] ProjectionConnectionServiceCloseError),
    #[error("same-home recovery stopped in a terminal unpublished state")]
    TerminalRecovery,
}

/// Sole process owner of current projection-service publication and same-home recovery.
pub struct RunningSessionRecoverySupervisor {
    slot: Arc<RunningServiceSlot>,
    signal: mpsc::SyncSender<()>,
    worker: Mutex<Option<std::thread::JoinHandle<RecoveryWorkerExit>>>,
    #[cfg(test)]
    phase93_observation: Arc<Mutex<Option<Phase93AdoptionObservation>>>,
}

impl RunningSessionRecoverySupervisor {
    /// Mounts the initial healthy service and starts one capacity-one same-home recovery worker.
    pub fn start(
        home: HomeStore,
        config: ProjectionServiceConfig,
        provider_factory: Box<dyn ScheduledOrdinaryExecutionProviderFactory>,
    ) -> Result<Self, RunningSessionRecoveryStartError> {
        let mut provider_factory = ProviderFactoryOwner::new(provider_factory);
        let state = match BerylState::reacquire(&home) {
            Ok(state) => state,
            Err(error) => {
                provider_factory.shutdown();
                return Err(RunningSessionRecoveryStartError::BerylState(error));
            }
        };
        let storage = match SyndicStorage::reacquire(&home) {
            Ok(storage) => storage,
            Err(error) => {
                provider_factory.shutdown();
                return Err(RunningSessionRecoveryStartError::SyndicStorage(error));
            }
        };
        let health = home.health();
        let context = ScheduledOrdinaryProviderEpochContext::new(
            home.home_id(),
            health.generation().ok_or_else(|| {
                RunningSessionRecoveryStartError::Provider(
                    "the initial healthy home has no generation".to_owned(),
                )
            })?,
            state,
        );
        let provider = match provider_factory.create_epoch(context) {
            Ok(provider) => provider,
            Err(error) => {
                provider_factory.shutdown();
                return Err(RunningSessionRecoveryStartError::Provider(
                    error.to_string(),
                ));
            }
        };
        let service = match ProjectionConnectionService::new(home, storage, config, provider) {
            Ok(service) => service,
            Err(error) => {
                provider_factory.shutdown();
                return Err(RunningSessionRecoveryStartError::Service(error));
            }
        };
        let retained_home = service.retained_home_for_recovery();
        let (signal, receiver) = mpsc::sync_channel(1);
        if service.attach_recovery_supervisor(signal.clone()).is_err() {
            drop(retained_home);
            let _ = service.close();
            provider_factory.shutdown();
            return Err(RunningSessionRecoveryStartError::NotificationAttachment);
        }
        let slot = RunningServiceSlot::new(service, state);
        #[cfg(test)]
        let phase93_observation = Arc::new(Mutex::new(None));
        let worker = RecoveryWorkerStart {
            home: retained_home,
            config,
            slot: Arc::clone(&slot),
            signal: signal.clone(),
            receiver,
            provider_factory,
            #[cfg(test)]
            phase93_observation: Arc::clone(&phase93_observation),
        }
        .spawn()?;
        Ok(Self {
            slot,
            signal,
            worker: Mutex::new(Some(worker)),
            #[cfg(test)]
            phase93_observation,
        })
    }

    /// Borrows the pointer-exact current service through a non-cloneable scoped lease.
    pub fn acquire(&self) -> Result<RunningProjectionServiceLease, RunningServiceAvailability> {
        self.slot.acquire()
    }

    /// Returns bounded content-free current publication and recovery state.
    #[must_use]
    pub fn diagnostics(&self) -> RunningSessionRecoveryDiagnostics {
        self.slot.diagnostics()
    }

    #[cfg(test)]
    fn phase93_adoption_observation_for_test(&self) -> Option<Phase93AdoptionObservation> {
        *self
            .phase93_observation
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Explicitly stops recovery, settles the current service, and finally closes the provider
    /// factory's stable admitted-session pool.
    pub fn shutdown(self) -> Result<(), RunningSessionRecoveryShutdownError> {
        self.slot.begin_shutdown();
        let _ = self.signal.try_send(());
        let worker = self
            .worker
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
            .ok_or(RunningSessionRecoveryShutdownError::WorkerPanicked)?;
        worker
            .join()
            .map_err(|_| RunningSessionRecoveryShutdownError::WorkerPanicked)?
            .into_result()
    }
}

impl RunningSessionRecoveryDiagnostics {
    #[must_use]
    pub const fn current_home_generation(self) -> Option<HomeGeneration> {
        self.current_home_generation
    }

    #[must_use]
    pub const fn current_service_generation(self) -> Option<ProjectionServiceGeneration> {
        self.current_service_generation
    }

    #[must_use]
    pub const fn active_service_leases(self) -> usize {
        self.active_service_leases
    }

    #[must_use]
    pub const fn recovering(self) -> bool {
        self.recovering
    }

    #[must_use]
    pub const fn shutting_down(self) -> bool {
        self.shutting_down
    }

    #[must_use]
    pub const fn recovery_cycles(self) -> u64 {
        self.recovery_cycles
    }

    #[must_use]
    pub const fn verification_successes(self) -> u64 {
        self.verification_successes
    }

    #[must_use]
    pub const fn terminal_failures(self) -> u64 {
        self.terminal_failures
    }
}

impl Drop for RunningSessionRecoverySupervisor {
    fn drop(&mut self) {
        self.slot.begin_shutdown();
        let _ = self.signal.try_send(());
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests;
