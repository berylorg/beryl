use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Condvar, Mutex, Weak, atomic::{AtomicBool, Ordering}},
    task::{Context, Wake, Waker},
    thread::JoinHandle,
};

use beryl_home_store::{CommandCancellation, HomeServiceReference};
use beryl_model::SyndicResourceId;
use beryl_state::AssetState;
use syndic_storage::{GeneratedMediaSweep, SyndicStorage};

use crate::{cas_projection::{GeneratedSourceAuthority, initial_start::InitialStartGate}, image_processing::{ImageMediaChanged, ImageMediaProcess}};
use super::scheduler::{Scheduler, Work};

#[derive(Debug, thiserror::Error)]
pub(crate) enum GeneratedMediaCoordinatorError {
    #[error("generated media worker could not start: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("generated media finalization failed: {0}")]
    Worker(String),
    #[error("generated media worker panicked")]
    Panicked,
    #[error("generated media read changed concurrently")]
    Deferred,
}

pub(super) type Result<T> = std::result::Result<T, GeneratedMediaCoordinatorError>;

pub(super) fn failure(error: impl std::fmt::Display) -> GeneratedMediaCoordinatorError {
    GeneratedMediaCoordinatorError::Worker(error.to_string())
}

pub(super) fn source_read_failure(error: syndic_storage::SyndicReadError) -> GeneratedMediaCoordinatorError {
    match error {
        syndic_storage::SyndicReadError::ConcurrentChange { .. } => GeneratedMediaCoordinatorError::Deferred,
        error => failure(error),
    }
}

pub(super) fn admission_read_failure(error: beryl_state::GeneratedMediaError) -> GeneratedMediaCoordinatorError {
    match error {
        beryl_state::GeneratedMediaError::ConcurrentChange => GeneratedMediaCoordinatorError::Deferred,
        error => failure(error),
    }
}

pub(super) fn settle_resource(result: Result<bool>) -> Result<bool> {
    match result { Err(GeneratedMediaCoordinatorError::Deferred) => Ok(false), result => result }
}

pub(super) struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    pub(super) cancelled: Arc<AtomicBool>,
    pub(super) command_cancel: CommandCancellation,
    #[cfg(feature = "test-faults")]
    pause_preparation: AtomicBool,
    #[cfg(feature = "test-faults")]
    prepared_reached: AtomicBool,
}

struct State {
    scheduler: Scheduler<SyndicResourceId, GeneratedMediaSweep>,
    capacity: ImageMediaChanged,
    capacity_waiting: bool,
}

#[derive(Clone)]
pub(super) struct Services {
    pub(super) home: Arc<HomeServiceReference>,
    pub(super) syndic: SyndicStorage,
    pub(super) assets: AssetState,
    pub(super) authority: GeneratedSourceAuthority,
    pub(super) process: Arc<ImageMediaProcess>,
    pub(super) shared: Arc<Shared>,
}

pub(crate) struct GeneratedMediaCoordinator {
    services: Services,
    start: Arc<InitialStartGate>,
    workers: Vec<JoinHandle<Result<()>>>,
    failure: Option<GeneratedMediaCoordinatorError>,
}

impl Wake for Shared {
    fn wake(self: Arc<Self>) { self.wake_by_ref(); }
    fn wake_by_ref(self: &Arc<Self>) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).scheduler.notify();
        self.changed.notify_all();
    }
}

struct CapacityWake(Weak<Shared>);
impl Wake for CapacityWake {
    fn wake(self: Arc<Self>) { self.wake_by_ref(); }
    fn wake_by_ref(self: &Arc<Self>) {
        if let Some(shared) = self.0.upgrade() {
            let _state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            shared.changed.notify_all();
        }
    }
}

impl GeneratedMediaCoordinator {
    pub(crate) fn prepare(
        home: Arc<HomeServiceReference>, syndic: SyndicStorage, assets: AssetState,
        authority: GeneratedSourceAuthority, process: Arc<ImageMediaProcess>,
        start: Arc<InitialStartGate>,
    ) -> Result<Self> {
        let limits = process.limits().configuration();
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                scheduler: Scheduler::new(limits.generated_media_queue_items, limits.generated_media_worker_slots),
                capacity: process.generated_capacity_changed(), capacity_waiting: false,
            }),
            changed: Condvar::new(), cancelled: Arc::new(AtomicBool::new(false)),
            command_cancel: CommandCancellation::new(),
            #[cfg(feature = "test-faults")]
            pause_preparation: AtomicBool::new(false),
            #[cfg(feature = "test-faults")]
            prepared_reached: AtomicBool::new(false),
        });
        let services = Services { home, syndic, assets, authority, process, shared };
        let mut result = Self { services, start, workers: Vec::new(), failure: None };
        for slot in 0..limits.generated_media_worker_slots {
            let services = result.services.clone();
            let start = result.start.clone();
            match std::thread::Builder::new().name(format!("generated-media-{slot}")).spawn(move || {
                if !start.wait() || services.shared.cancelled.load(Ordering::Acquire) { return Ok(()); }
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&services)))
                    .unwrap_or(Err(GeneratedMediaCoordinatorError::Panicked));
                if outcome.is_err() { services.shared.stop(); }
                let outcome = if outcome.is_err() && !matches!(&outcome, Err(GeneratedMediaCoordinatorError::Panicked))
                    && services.home.health().state() == beryl_home_store::HomeHealthState::Failed
                { Ok(()) } else { outcome };
                outcome
            }) {
                Ok(worker) => result.workers.push(worker),
                Err(error) => { result.begin_retirement(); let _ = result.join_retirement(); return Err(error.into()); }
            }
        }
        Ok(result)
    }

    pub(crate) fn waker(&self) -> Waker { Waker::from(self.services.shared.clone()) }

    #[cfg(feature = "test-faults")]
    pub(crate) fn diagnostics(&self) -> (usize, usize, bool, usize) {
        let state = self.services.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let (queue, active) = state.scheduler.counts();
        (queue, active, state.scheduler.idle(), self.workers.len())
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn pause_preparation(&self) { self.services.shared.pause_preparation.store(true, Ordering::Release); }

    #[cfg(feature = "test-faults")]
    pub(super) fn preparation_reached(&self) -> bool { self.services.shared.prepared_reached.load(Ordering::Acquire) }

    #[cfg(feature = "test-faults")]
    pub(super) fn release_preparation(&self) {
        self.services.shared.pause_preparation.store(false, Ordering::Release);
        self.services.shared.changed.notify_all();
    }

    pub(crate) fn begin_retirement(&self) {
        self.services.shared.stop();
        self.start.cancel();
    }

    pub(crate) fn join_retirement(&mut self) -> Result<()> {
        self.begin_retirement();
        for worker in self.workers.drain(..) {
            let outcome = worker.join().unwrap_or(Err(GeneratedMediaCoordinatorError::Panicked));
            if let Err(error) = outcome { self.failure.get_or_insert(error); }
        }
        match self.failure.as_ref() {
            Some(error) => Err(failure(error)),
            None => Ok(()),
        }
    }
}

impl Drop for GeneratedMediaCoordinator {
    fn drop(&mut self) { let _ = self.join_retirement(); }
}

impl Shared {
    #[cfg(feature = "test-faults")]
    pub(super) fn preparation_checkpoint(&self) {
        if !self.pause_preparation.load(Ordering::Acquire) { return; }
        self.prepared_reached.store(true, Ordering::Release);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        self.changed.notify_all();
        while self.pause_preparation.load(Ordering::Acquire) && !self.cancelled.load(Ordering::Acquire) {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }
    pub(super) fn stop(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.command_cancel.cancel();
        self.state.lock().unwrap_or_else(|e| e.into_inner()).scheduler.stop();
        self.changed.notify_all();
    }

    pub(super) fn capacity_wait(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).capacity_waiting = true;
    }

    pub(super) fn cleanup_wait(&self) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self.changed.wait_timeout(state, std::time::Duration::from_millis(50));
    }
}

fn run(services: &Services) -> Result<()> {
    let capacity_waker = Waker::from(Arc::new(CapacityWake(Arc::downgrade(&services.shared))));
    loop {
        let work = {
            let mut state = services.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if Pin::new(&mut state.capacity).poll(&mut Context::from_waker(&capacity_waker)).is_ready() {
                    state.capacity = services.process.generated_capacity_changed();
                    if std::mem::take(&mut state.capacity_waiting) { state.scheduler.notify(); }
                    continue;
                }
                match state.scheduler.next() {
                    Work::Wait => state = services.shared.changed.wait(state).unwrap_or_else(|e| e.into_inner()),
                    work => break work,
                }
            }
        };
        match work {
            Work::Stop => return Ok(()),
            Work::Wait => unreachable!(),
            Work::Resource(resource) => {
                let result = settle_resource(super::publication::finalize(services, resource));
                let mut state = services.shared.state.lock().unwrap_or_else(|e| e.into_inner());
                state.scheduler.finished(resource);
                if matches!(result, Ok(true)) { state.scheduler.notify(); }
                drop(state);
                services.shared.changed.notify_all();
                result?;
            }
            Work::Discover { sweep, available } => {
                let mut retry_sweep = sweep.clone();
                let result = (|| {
                    let sweep = match sweep { Some(sweep) => sweep, None => services.syndic.begin_generated_media_sweep(&services.home).map_err(source_read_failure)? };
                    retry_sweep = Some(sweep.clone());
                    let page = services.syndic.pending_generated_media_page(&services.home, &sweep, available.min(256), services.process.limits().configuration().generated_media_page_bytes.min(65_536)).map_err(source_read_failure)?;
                    let mut resources = Vec::with_capacity(page.entries().len());
                    for entry in page.entries() {
                        match super::publication::discoverable(services, entry.resource_id()) {
                            Ok(true) => resources.push(entry.resource_id()),
                            Ok(false) | Err(GeneratedMediaCoordinatorError::Deferred) => {},
                            Err(error) => return Err(error),
                        }
                    }
                    Ok((resources, page.into_continuation()))
                })();
                let mut state = services.shared.state.lock().unwrap_or_else(|e| e.into_inner());
                match result {
                    Ok((resources, continuation)) => {
                        state.scheduler.discovered(resources, continuation);
                    }
                    Err(GeneratedMediaCoordinatorError::Deferred) => {
                        state.scheduler.discovery_deferred(retry_sweep);
                    }
                    Err(error) => {
                        state.scheduler.discovery_failed();
                        if services.shared.cancelled.load(Ordering::Acquire) { return Ok(()); }
                        return Err(error);
                    }
                }
                drop(state);
                services.shared.changed.notify_all();
            }
        }
    }
}
