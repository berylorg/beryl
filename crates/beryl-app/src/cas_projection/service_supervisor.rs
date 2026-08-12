//! Process-owned current-service access and terminal failed-service disposal.

mod slot;
mod worker;

use std::sync::{Arc, Mutex, mpsc};

use beryl_home_store::{DomainHandleError, HomeGeneration, HomeStore};
use beryl_state::{BerylState, BerylStateReacquireError};
use syndic_storage::SyndicStorage;
use thiserror::Error;

use self::worker::{TerminalWorkerExit, TerminalWorkerStart};
use crate::cas_projection::{
    ProjectionConnectionService, ProjectionConnectionServiceCloseError, ProjectionServiceConfig,
    ProjectionServiceGeneration, ScheduledOrdinaryExecutionProvider,
};
use slot::RunningServiceLease;
pub(in crate::cas_projection) use slot::RunningServiceSlot;

/// Why the current process service slot cannot issue a scoped lease.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
enum ServiceAvailability {
    #[error("terminal failed-service disposal is in progress")]
    Disposing,
    #[error("the service supervisor is shutting down")]
    ShuttingDown,
    #[error("the running service is unavailable")]
    Unavailable,
}

/// Bounded content-free state for one process-owned terminal service supervisor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalServiceDiagnostics {
    pub(super) current_home_generation: Option<HomeGeneration>,
    pub(super) current_service_generation: Option<ProjectionServiceGeneration>,
    pub(super) active_service_leases: usize,
    pub(super) disposing: bool,
    pub(super) shutting_down: bool,
    pub(super) terminal_failures: u64,
    pub(super) terminal_settled: bool,
}

/// Failure before the terminal-disposal worker became the sole service owner.
#[derive(Debug, Error)]
enum TerminalServiceStartError {
    #[error("the exact home's Beryl state handles could not be reacquired: {0}")]
    BerylState(#[source] BerylStateReacquireError),
    #[error("the exact home's Syndic storage handle could not be reacquired: {0}")]
    SyndicStorage(#[source] DomainHandleError),
    #[error("the initial projection service could not be constructed: {0}")]
    Service(#[source] crate::cas_projection::ProjectionCoordinatorError),
    #[error("the terminal service notification could not be attached")]
    NotificationAttachment,
    #[error("the terminal service worker could not be started: {0}")]
    WorkerSpawn(String),
}

/// Explicit shutdown failure after every reachable process-owned resource was asked to settle.
#[derive(Debug, Error)]
enum TerminalServiceShutdownError {
    #[error("the terminal service worker panicked")]
    WorkerPanicked,
    #[error("the current projection service failed to close: {0}")]
    Service(#[source] ProjectionConnectionServiceCloseError),
    #[error("failed-service disposal made the service terminally unavailable")]
    TerminalUnavailable,
}

/// Sole process owner of current projection-service access and terminal disposal.
struct TerminalServiceSupervisor {
    slot: Arc<RunningServiceSlot>,
    signal: mpsc::SyncSender<()>,
    worker: Mutex<Option<std::thread::JoinHandle<TerminalWorkerExit>>>,
}

impl TerminalServiceSupervisor {
    /// Mounts the initial healthy service and starts one capacity-one terminal-disposal worker.
    fn start(
        home: HomeStore,
        config: ProjectionServiceConfig,
        mut provider: Box<dyn ScheduledOrdinaryExecutionProvider>,
    ) -> Result<Self, TerminalServiceStartError> {
        let state = match BerylState::reacquire(&home) {
            Ok(state) => state,
            Err(error) => {
                provider.shutdown();
                return Err(TerminalServiceStartError::BerylState(error));
            }
        };
        let storage = match SyndicStorage::reacquire(&home) {
            Ok(storage) => storage,
            Err(error) => {
                provider.shutdown();
                return Err(TerminalServiceStartError::SyndicStorage(error));
            }
        };
        let service = match ProjectionConnectionService::new(home, storage, config, provider) {
            Ok(service) => service,
            Err(error) => {
                return Err(TerminalServiceStartError::Service(error));
            }
        };
        let (signal, receiver) = mpsc::sync_channel(1);
        if service.attach_terminal_disposer(signal.clone()).is_err() {
            let _ = service.close();
            return Err(TerminalServiceStartError::NotificationAttachment);
        }
        let slot = RunningServiceSlot::new(service, state);
        let worker = TerminalWorkerStart {
            slot: Arc::clone(&slot),
            receiver,
        }
        .spawn()?;
        Ok(Self {
            slot,
            signal,
            worker: Mutex::new(Some(worker)),
        })
    }

    /// Borrows the pointer-exact current service through a non-cloneable scoped lease.
    fn acquire(&self) -> Result<RunningServiceLease, ServiceAvailability> {
        self.slot.acquire()
    }

    /// Returns bounded content-free current publication and recovery state.
    #[must_use]
    fn diagnostics(&self) -> TerminalServiceDiagnostics {
        self.slot.diagnostics()
    }

    /// Explicitly stops supervision and settles the current service.
    fn shutdown(self) -> Result<(), TerminalServiceShutdownError> {
        self.slot.begin_shutdown();
        let _ = self.signal.try_send(());
        let worker = self
            .worker
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
            .ok_or(TerminalServiceShutdownError::WorkerPanicked)?;
        worker
            .join()
            .map_err(|_| TerminalServiceShutdownError::WorkerPanicked)?
            .into_result()
    }
}

impl TerminalServiceDiagnostics {
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
    pub const fn disposing(self) -> bool {
        self.disposing
    }

    #[must_use]
    pub const fn shutting_down(self) -> bool {
        self.shutting_down
    }

    #[must_use]
    pub const fn terminal_failures(self) -> u64 {
        self.terminal_failures
    }

    /// Reports that terminal failed-service disposal has finished.
    #[must_use]
    pub const fn terminal_settled(self) -> bool {
        self.terminal_settled
    }
}

impl Drop for TerminalServiceSupervisor {
    fn drop(&mut self) {
        self.slot.begin_shutdown();
        let _ = self.signal.try_send(());
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod tests;
