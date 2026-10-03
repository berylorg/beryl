use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[path = "notification_audio/playback.rs"]
pub(crate) mod playback;
#[path = "notification_audio/wav.rs"]
pub(crate) mod wav;

pub(crate) const MAX_PATH_BYTES: usize = 32 * 1024;
pub(crate) const MAX_ENCODED_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_DECODED_BYTES: usize = 16 * 1024 * 1024;
const POLL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SoundKind {
    EndTurn,
    OperatorAttention,
    PlanComplete,
}

pub(crate) struct SoundEvent {
    pub(crate) kind: SoundKind,
    pub(crate) path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Admission {
    Accepted,
    ReplacedWaiting,
    Busy,
    Closed,
    InvalidPath,
}

fn ordinary_file_path(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    if !path.is_absolute() || path.as_os_str().len() > MAX_PATH_BYTES / 2 {
        return false;
    }
    for component in path.components() {
        match component {
            Component::Prefix(prefix)
                if !matches!(
                    prefix.kind(),
                    Prefix::Disk(_)
                        | Prefix::VerbatimDisk(_)
                        | Prefix::UNC(_, _)
                        | Prefix::VerbatimUNC(_, _)
                ) =>
            {
                return false;
            }
            Component::Normal(name) => {
                if let Some(name) = name.to_str() {
                    let stem = name.split('.').next().unwrap().trim_end_matches([' ', '.']);
                    if ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
                        .iter()
                        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
                    {
                        return false;
                    }
                    if stem.get(..3).is_some_and(|prefix| {
                        prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT")
                    }) && matches!(
                        stem.get(3..),
                        Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
                    ) {
                        return false;
                    }
                }
            }
            _ => {}
        }
    }
    true
}

struct State {
    waiting: Option<SoundEvent>,
    closed: bool,
}

pub(crate) struct Control {
    state: Mutex<State>,
    wake: Condvar,
    cancelled: AtomicBool,
    encoded_charge: AtomicUsize,
    decoded_charge: AtomicUsize,
}

impl Control {
    pub(crate) fn admit_output(&self, append: impl FnOnce()) -> Result<(), String> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return Err("notification audio cancelled".into());
        }
        append();
        Ok(())
    }
    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> Result<(), String> {
        if self.cancelled() {
            Err("notification audio cancelled".into())
        } else {
            Ok(())
        }
    }

    pub(crate) fn wait_tick(&self) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.closed {
            drop(
                self.wake
                    .wait_timeout(state, POLL)
                    .unwrap_or_else(|e| e.into_inner()),
            );
        }
    }
}

pub(crate) trait Backend: Send + 'static {
    fn attempt(&mut self, event: SoundEvent, control: &Arc<Control>) -> Result<(), String>;
}

struct ProductionBackend;

impl Backend for ProductionBackend {
    fn attempt(&mut self, event: SoundEvent, control: &Arc<Control>) -> Result<(), String> {
        let samples = wav::acquire(&event.path, control)?;
        control.check()?;
        playback::play(samples, control)
    }
}

pub(crate) struct NotificationAudioLane {
    control: Arc<Control>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub(crate) struct AudioIngress {
    control: Arc<Control>,
}

impl AudioIngress {
    pub(crate) fn offer(&self, kind: SoundKind, path: &Path) -> Admission {
        if self.control.cancelled() {
            return Admission::Closed;
        }
        if !ordinary_file_path(path) {
            tracing::warn!("notification audio rejected invalid or oversized sound path");
            return Admission::InvalidPath;
        }
        let Ok(mut state) = self.control.state.try_lock() else {
            tracing::warn!("notification audio ingress busy");
            return Admission::Busy;
        };
        if state.closed {
            return Admission::Closed;
        }
        let replaced = state
            .waiting
            .replace(SoundEvent {
                kind,
                path: path.to_owned(),
            })
            .is_some();
        self.control.wake.notify_one();
        if replaced {
            Admission::ReplacedWaiting
        } else {
            Admission::Accepted
        }
    }

    #[cfg(test)]
    pub(crate) fn same_lane(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.control, &other.control)
    }

    #[cfg(test)]
    pub(crate) fn is_closed(&self) -> bool {
        self.control.cancelled()
    }
}

struct Reservation(Arc<Control>);

impl Reservation {
    fn acquire(control: &Arc<Control>) -> Self {
        control
            .encoded_charge
            .store(MAX_ENCODED_BYTES, Ordering::Release);
        control
            .decoded_charge
            .store(MAX_DECODED_BYTES, Ordering::Release);
        Self(control.clone())
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.0.encoded_charge.store(0, Ordering::Release);
        self.0.decoded_charge.store(0, Ordering::Release);
    }
}

impl NotificationAudioLane {
    pub(crate) fn new() -> Self {
        Self::with_backend(ProductionBackend)
    }

    pub(crate) fn with_backend(mut backend: impl Backend) -> Self {
        let control = Arc::new(Control {
            state: Mutex::new(State {
                waiting: None,
                closed: false,
            }),
            wake: Condvar::new(),
            cancelled: AtomicBool::new(false),
            encoded_charge: AtomicUsize::new(0),
            decoded_charge: AtomicUsize::new(0),
        });
        let worker_control = control.clone();
        let worker = thread::Builder::new()
            .name("notification-audio".into())
            .spawn(move || {
                loop {
                    let event = {
                        let mut state = worker_control
                            .state
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        while !state.closed && state.waiting.is_none() {
                            state = worker_control
                                .wake
                                .wait(state)
                                .unwrap_or_else(|e| e.into_inner());
                        }
                        if state.closed {
                            break;
                        }
                        state.waiting.take().unwrap()
                    };
                    let _reservation = Reservation::acquire(&worker_control);
                    let kind = event.kind;
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        worker_control.check()?;
                        backend.attempt(event, &worker_control)
                    }));
                    match result {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) if !worker_control.cancelled() => {
                            tracing::warn!(?kind, %error, "notification audio attempt failed");
                        }
                        Err(_) => tracing::warn!(?kind, "notification audio backend panicked"),
                        _ => {}
                    }
                }
            });
        let worker = match worker {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(%error, "notification audio worker unavailable");
                control.state.lock().unwrap().closed = true;
                control.cancelled.store(true, Ordering::Release);
                None
            }
        };
        Self { control, worker }
    }

    pub(crate) fn ingress(&self) -> AudioIngress {
        AudioIngress {
            control: self.control.clone(),
        }
    }

    pub(crate) fn close(&self) {
        let mut state = self.control.state.lock().unwrap_or_else(|e| e.into_inner());
        state.closed = true;
        state.waiting.take();
        self.control.cancelled.store(true, Ordering::Release);
        self.control.wake.notify_all();
    }

    pub(crate) fn finish(&mut self) {
        self.close();
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::warn!("notification audio worker terminated unexpectedly");
            }
        }
    }

    pub(crate) fn take_worker(&mut self) -> Self {
        self.close();
        Self {
            control: self.control.clone(),
            worker: self.worker.take(),
        }
    }

    #[cfg(test)]
    pub(crate) fn charges(&self) -> (usize, usize) {
        (
            self.control.encoded_charge.load(Ordering::Acquire),
            self.control.decoded_charge.load(Ordering::Acquire),
        )
    }

    #[cfg(test)]
    pub(crate) fn is_finished(&self) -> bool {
        self.worker.is_none()
    }
}

impl Drop for NotificationAudioLane {
    fn drop(&mut self) {
        self.finish();
    }
}
