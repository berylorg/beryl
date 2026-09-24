use super::*;
use crate::{
    cas_projection::initial_start::InitialStartGate, discussion_handoff_limits::HandoffScanLimits,
};
use std::{
    sync::Condvar,
    task::{Wake, Waker},
    thread::JoinHandle,
};

#[cfg(feature = "test-faults")]
pub(super) mod test_support;
mod worker;

#[derive(Default)]
struct SignalState {
    pending: bool,
    stopped: bool,
    #[cfg(feature = "test-faults")]
    probe: test_support::Probe,
}

pub(crate) struct HandoffSignal {
    state: Mutex<SignalState>,
    changed: Condvar,
}

impl HandoffSignal {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(SignalState {
                pending: true,
                ..SignalState::default()
            }),
            changed: Condvar::new(),
        })
    }

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

impl Wake for HandoffSignal {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).pending = true;
        self.changed.notify_one();
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum HandoffCoordinatorError {
    #[error("handoff coordinator slot configuration disagrees with operation custody")]
    SlotConfigurationMismatch,
    #[error("handoff coordinator worker could not start: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("handoff coordinator worker panicked")]
    Panicked,
    #[error("handoff coordinator could not allocate operation identity or time")]
    Identity,
    #[error(transparent)]
    Read(#[from] beryl_home_store::ReadError),
    #[error(transparent)]
    Page(#[from] beryl_state::DurableJobReadError),
    #[error(transparent)]
    Settlement(#[from] DiscussionSettlementError),
    #[error("handoff coordinator stopped after a command outcome requiring recovery")]
    RecoveryRequired,
}

pub(crate) struct HandoffCoordinator {
    signal: Arc<HandoffSignal>,
    cancellation: CommandCancellation,
    initial_start: Arc<InitialStartGate>,
    operations: DiscussionSettlementOperations,
    worker: Option<JoinHandle<Result<(), HandoffCoordinatorError>>>,
}

impl HandoffCoordinator {
    pub(crate) fn prepare(
        service: DiscussionSettlementService,
        limits: HandoffScanLimits,
        initial_start: Arc<InitialStartGate>,
    ) -> Result<Self, HandoffCoordinatorError> {
        if service.operations.configured_slots() != limits.reconcile_slots() {
            return Err(HandoffCoordinatorError::SlotConfigurationMismatch);
        }
        let signal = HandoffSignal::new();
        let cancellation = CommandCancellation::new();
        let operations = service.operations.clone();
        let worker_signal = Arc::clone(&signal);
        let worker_cancel = cancellation.clone();
        let worker_start = Arc::clone(&initial_start);
        if !operations.set_coordinator_waker(Some(Waker::from(Arc::clone(&signal)))) {
            return Err(DiscussionSettlementError::DuplicateIdentity.into());
        }
        let spawned = std::thread::Builder::new()
            .name("discussion-handoff".into())
            .spawn(move || {
                if worker_start.wait() && !worker_cancel.is_cancelled() {
                    worker::run(service, limits, worker_signal, worker_cancel)
                } else {
                    Ok(())
                }
            });
        let worker = match spawned {
            Ok(worker) => worker,
            Err(error) => {
                operations.set_coordinator_waker(None);
                return Err(error.into());
            }
        };
        Ok(Self {
            signal,
            cancellation,
            initial_start,
            operations,
            worker: Some(worker),
        })
    }

    pub(crate) fn waker(&self) -> Waker {
        Waker::from(Arc::clone(&self.signal))
    }

    pub(crate) fn shutdown(&mut self) -> Result<(), HandoffCoordinatorError> {
        if self.worker.is_none() {
            return Ok(());
        }
        self.cancellation.cancel();
        self.signal.stop();
        self.initial_start.cancel();
        let result = self
            .worker
            .take()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| HandoffCoordinatorError::Panicked)
                    .and_then(|result| result)
            })
            .unwrap_or(Ok(()));
        self.operations.set_coordinator_waker(None);
        result
    }
}

impl Drop for HandoffCoordinator {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
