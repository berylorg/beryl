use std::{
    sync::{Arc, Condvar, Mutex, Weak},
    task::{Wake, Waker},
    thread::JoinHandle,
};

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, FrozenHomeRead, HomeServiceReference,
    HomeStore, ReadError, ReconciliationHandle,
};
use beryl_state::BerylState;
use syndic_storage::SyndicStorage;

use crate::cas_projection::initial_start::InitialStartGate;

mod certification;
mod worker;

#[cfg(all(test, feature = "test-faults"))]
#[path = "../tests/unit/catalog_readiness.rs"]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum CatalogSourceReadError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Syndic(#[from] syndic_storage::SyndicReadError),
    #[error(transparent)]
    Catalog(#[from] beryl_state::CatalogReadError),
    #[error(transparent)]
    Runtime(#[from] beryl_state::RuntimeRootCatalogSourceError),
    #[error(transparent)]
    Claim(#[from] beryl_state::ThreadClaimCatalogSourceError),
    #[error(transparent)]
    Projection(#[from] crate::catalog_projection::CatalogProjectionBuildError),
    #[error("catalog source is not ready")]
    NotReady,
    #[error("catalog source graph is retired")]
    Retired,
    #[error("catalog certification was cancelled")]
    Cancelled,
    #[error("catalog coverage counter exhausted")]
    CountExhausted,
    #[error("catalog page has an empty continuation")]
    EmptyContinuation,
    #[error("catalog primary, recency and canonical coverage disagree")]
    CoverageMismatch,
    #[error("catalog projection has no canonical source")]
    CanonicalSourceMissing,
}

#[derive(Debug)]
pub(crate) enum RetainedCatalogRepair {
    NotCommitted(CommandError),
    Committed(CommandOutcome),
    Indeterminate {
        failure: CommandError,
        reconciliation: ReconciliationHandle,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CatalogSourceCoordinatorError {
    #[error("catalog source worker could not start: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("catalog source worker panicked")]
    Panicked,
    #[error(transparent)]
    Source(#[from] CatalogSourceReadError),
    #[error("catalog repair requires outcome settlement: {0:?}")]
    Repair(Box<RetainedCatalogRepair>),
}

#[derive(Default)]
struct SourceState {
    pending: bool,
    stopped: bool,
    published: Option<FrozenHomeRead>,
    threads: usize,
}

struct SourceSignal {
    state: Mutex<SourceState>,
    changed: Condvar,
}

impl SourceSignal {
    fn wait(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while !state.pending && !state.stopped {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.pending = false;
        !state.stopped
    }

    fn stop(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).stopped = true;
        self.changed.notify_all();
    }
}

impl Wake for SourceSignal {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.stopped {
            state.pending = true;
        }
        self.changed.notify_one();
    }
}

#[derive(Clone)]
pub struct CatalogSourceReader {
    signal: Weak<SourceSignal>,
}

impl CatalogSourceReader {
    pub fn retain_source(
        &self,
        home: &HomeStore,
        cancellation: &CommandCancellation,
    ) -> Result<FrozenHomeRead, CatalogSourceReadError> {
        let signal = self
            .signal
            .upgrade()
            .ok_or(CatalogSourceReadError::Retired)?;
        let state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped {
            return Err(CatalogSourceReadError::Retired);
        }
        let source = state
            .published
            .as_ref()
            .ok_or(CatalogSourceReadError::NotReady)?;
        Ok(home.retain_frozen_read(source, cancellation)?)
    }

    pub fn certified_threads(&self) -> Result<usize, CatalogSourceReadError> {
        let signal = self
            .signal
            .upgrade()
            .ok_or(CatalogSourceReadError::Retired)?;
        let state = signal.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped {
            return Err(CatalogSourceReadError::Retired);
        }
        state
            .published
            .as_ref()
            .ok_or(CatalogSourceReadError::NotReady)?;
        Ok(state.threads)
    }
}

pub(crate) struct CatalogSourceCoordinator {
    home: Arc<HomeServiceReference>,
    signal: Arc<SourceSignal>,
    cancellation: CommandCancellation,
    initial_start: Arc<InitialStartGate>,
    worker: Option<JoinHandle<Result<(), CatalogSourceCoordinatorError>>>,
}

impl CatalogSourceCoordinator {
    pub(crate) fn prepare(
        home: Arc<HomeServiceReference>,
        syndic: SyndicStorage,
        state: BerylState,
        initial_start: Arc<InitialStartGate>,
    ) -> Result<Self, CatalogSourceCoordinatorError> {
        let signal = Arc::new(SourceSignal {
            state: Mutex::new(SourceState {
                pending: true,
                ..SourceState::default()
            }),
            changed: Condvar::new(),
        });
        let cancellation = CommandCancellation::new();
        let worker_signal = Arc::clone(&signal);
        let worker_cancel = cancellation.clone();
        let worker_start = Arc::clone(&initial_start);
        let worker_home = Arc::clone(&home);
        let worker = std::thread::Builder::new()
            .name("catalog-source".into())
            .spawn(move || {
                if worker_start.wait() && !worker_cancel.is_cancelled() {
                    worker::run(worker_home, syndic, state, worker_signal, worker_cancel)
                } else {
                    Ok(())
                }
            })?;
        Ok(Self {
            home,
            signal,
            cancellation,
            initial_start,
            worker: Some(worker),
        })
    }

    pub(crate) fn waker(&self) -> Waker {
        Waker::from(Arc::clone(&self.signal))
    }
    pub(crate) fn reader(&self) -> CatalogSourceReader {
        CatalogSourceReader {
            signal: Arc::downgrade(&self.signal),
        }
    }

    pub(crate) fn stop_and_join(&mut self) -> Result<(), CatalogSourceCoordinatorError> {
        self.cancellation.cancel();
        self.signal.stop();
        self.initial_start.cancel();
        let result = match self.worker.take() {
            Some(worker) => worker
                .join()
                .unwrap_or(Err(CatalogSourceCoordinatorError::Panicked)),
            None => Ok(()),
        };
        let published = self
            .signal
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .published
            .take();
        let release = published.map_or(Ok(()), |published| {
            self.home.release_frozen_read(&published)
        });
        match result {
            Err(error) => Err(error),
            Ok(()) => release
                .map_err(CatalogSourceReadError::from)
                .map_err(Into::into),
        }
    }
}

impl Drop for CatalogSourceCoordinator {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}
