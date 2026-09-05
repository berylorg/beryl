use super::adapter::ManagedPort;
use super::*;
use beryl_backend::ManagedBackendClientConnector;
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::Instant,
};

#[derive(Clone, Default)]
pub(crate) struct Cancellation(Arc<CancellationState>);

#[derive(Default)]
struct CancellationState {
    cancelled: AtomicBool,
    mutex: Mutex<()>,
    wake: Condvar,
}

impl Cancellation {
    pub(crate) fn cancel(&self) {
        let _guard = self
            .0
            .mutex
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        self.0.cancelled.store(true, Ordering::Release);
        self.0.wake.notify_all();
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    fn wait(&self, duration: Duration) {
        let guard = self
            .0
            .mutex
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let _ = self
            .0
            .wake
            .wait_timeout_while(guard, duration, |_| !self.is_cancelled());
    }
}

pub(crate) struct ObserverTask {
    receiver: Receiver<ObserverUpdate>,
    cancellation: Cancellation,
}

impl ObserverTask {
    pub(crate) fn try_recv(&self) -> Result<ObserverUpdate, TryRecvError> {
        self.receiver.try_recv()
    }

    pub(crate) fn cancel(&self) {
        self.cancellation.cancel();
    }
}

impl Drop for ObserverTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub(crate) struct ChannelSink(SyncSender<ObserverUpdate>);

/// Shared task/channel construction also allows deterministic cancellation tests
/// without opening a backend connection.
pub(crate) fn observer_channel() -> (ObserverTask, ChannelSink, Cancellation) {
    let (sender, receiver) = mpsc::sync_channel(UPDATE_CAPACITY);
    let cancellation = Cancellation::default();
    (
        ObserverTask {
            receiver,
            cancellation: cancellation.clone(),
        },
        ChannelSink(sender),
        cancellation,
    )
}

impl UpdateSink for ChannelSink {
    fn publish(&mut self, mut update: ObserverUpdate, cancellation: &Cancellation) -> bool {
        while !cancellation.is_cancelled() {
            match self.0.try_send(update) {
                Ok(()) => return true,
                Err(TrySendError::Disconnected(_)) => return false,
                Err(TrySendError::Full(retained)) => update = retained,
            }
            cancellation.wait(POLL_INTERVAL);
        }
        false
    }
}

struct SystemClock(Instant);

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn wait(&mut self, duration: Duration, cancellation: &Cancellation) {
        cancellation.wait(duration);
    }
}

/// One task owns one bounded outgoing queue and one independent backend client.
pub(crate) fn spawn_observer(
    connector: ManagedBackendClientConnector,
    target: ObserverTarget,
    request_timeout: Duration,
    warning_threshold: Duration,
    diagnostics: Option<(CompactionDiagnosticHandle, Instant)>,
) -> ObserverTask {
    let (task, mut sink, worker_cancellation) = observer_channel();
    thread::spawn(move || {
        let clock_origin = diagnostics
            .as_ref()
            .map_or_else(Instant::now, |(_, started)| *started);
        let mut backend = ManagedPort {
            connector,
            execution_target: target.execution_target.clone(),
            session: None,
            operation: None,
        };
        run_observer(
            &mut backend,
            &mut SystemClock(clock_origin),
            &mut sink,
            target,
            request_timeout,
            warning_threshold,
            &worker_cancellation,
            diagnostics.map(|(handle, _)| handle),
        );
    });
    task
}
