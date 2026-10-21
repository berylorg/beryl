use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        Arc, Condvar, Mutex, MutexGuard, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
};

use beryl_home_store::CommandCancellation;
use gpui::{
    ImageResourceConsumer, ImageResourceDomain, ImageResourceDrain, ImageResourceError,
    ImageResourceHandle, ImageResourcePinScope, ImageUploadCancellation, ImageUploadTicket,
};

use super::{
    ImagePreparationError, ImageSourceDemand, ImageSourceError, ImageSourceProvider,
    MediaResourceLimits,
    surfaces::{CpuSurfaceCache, CpuSurfaceDiagnostics, RenditionIdentity},
};

mod lifecycle;
mod original;
mod presentation;
mod signal;
mod worker;

#[cfg(test)]
#[path = "../../tests/unit/image_media_process.rs"]
mod tests;

pub(crate) use original::{
    OriginalCapacityChanged, OriginalImageAvailability, OriginalImageCommand,
    OriginalImageCopyEffect, OriginalImageFailure, OriginalImageRequest, OriginalImageState,
};
pub(crate) use presentation::{ImageMediaInterest, ImageMediaWindow};
pub(crate) use signal::ImageMediaChanged;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImageMediaFailure {
    Cancelled,
    Retired,
    Stale,
    Unavailable,
    Unsupported,
    Capacity,
    Malformed,
    Worker,
    Native(ImageResourceError),
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("image media preparation failed: {0:?}")]
pub(crate) struct ImageMediaError(pub(crate) ImageMediaFailure);

impl From<ImageResourceError> for ImageMediaError {
    fn from(error: ImageResourceError) -> Self {
        Self(ImageMediaFailure::Native(error))
    }
}
impl From<ImageSourceError> for ImageMediaError {
    fn from(error: ImageSourceError) -> Self {
        use ImageSourceError as Source;
        Self(match error {
            Source::Cancelled => ImageMediaFailure::Cancelled,
            Source::Stale => ImageMediaFailure::Stale,
            Source::Identity => ImageMediaFailure::Malformed,
            Source::Unsupported => ImageMediaFailure::Unsupported,
            Source::Capacity => ImageMediaFailure::Capacity,
            Source::Unavailable | Source::Sidecar(_) => ImageMediaFailure::Unavailable,
        })
    }
}
impl From<ImagePreparationError> for ImageMediaError {
    fn from(error: ImagePreparationError) -> Self {
        Self(match error {
            ImagePreparationError::Cancelled => ImageMediaFailure::Cancelled,
            ImagePreparationError::Capacity | ImagePreparationError::Configuration => {
                ImageMediaFailure::Capacity
            }
            ImagePreparationError::Unsupported => ImageMediaFailure::Unsupported,
            ImagePreparationError::Malformed
            | ImagePreparationError::Identity
            | ImagePreparationError::Decode(_) => ImageMediaFailure::Malformed,
            ImagePreparationError::Io(_) => ImageMediaFailure::Unavailable,
        })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ImageMediaDiagnostics {
    pub(crate) native_revision: u64,
    pub(crate) native_revision_exhausted: bool,
    pub(crate) queued: usize,
    pub(crate) active_workers: usize,
    pub(crate) windows: usize,
    pub(crate) flights: usize,
    pub(crate) path_slots: usize,
    pub(crate) verification_slots: usize,
    pub(crate) decodes: u64,
    pub(crate) joins: u64,
    pub(crate) cancellations: u64,
    pub(crate) failures: u64,
    pub(crate) original_commands: usize,
    pub(crate) original_encoded_bytes: usize,
    pub(crate) cpu: CpuSurfaceDiagnostics,
}

pub(crate) struct ImageMediaProcess {
    core: Arc<ProcessCore>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

pub(crate) struct ImageMediaAttachment {
    inner: Arc<AttachmentState>,
}

pub(crate) struct GeneratedSourceStageLease {
    _lease: worker::SourceStageLease,
}

struct ProcessCore {
    generated_capacity: Arc<signal::ChangeEvent>,
    original_capacity: Arc<original::OriginalCapacityEvent>,
    native_progress: Arc<signal::NativeProgress>,
    domain: ImageResourceDomain,
    limits: MediaResourceLimits,
    cpu: CpuSurfaceCache,
    state: Mutex<ProcessState>,
    work: Condvar,
    event: Arc<signal::ChangeEvent>,
}

#[derive(Default)]
struct ProcessState {
    stopping: bool,
    next_attachment: u64,
    next_window: u64,
    next_interest: u64,
    attachment: Weak<AttachmentState>,
    windows: BTreeMap<u64, Weak<WindowState>>,
    queue: VecDeque<QueuedDemand>,
    flights: BTreeMap<RenditionIdentity, Flight>,
    original: Option<Arc<original::OriginalCommandState>>,
    active_workers: usize,
    path_slots: usize,
    verification_slots: usize,
    decodes: u64,
    joins: u64,
    cancellations: u64,
    failures: u64,
}

struct AttachmentState {
    id: u64,
    core: Weak<ProcessCore>,
    source: Mutex<Option<Arc<ImageSourceProvider>>>,
    consumer: ImageResourceConsumer,
    retired: AtomicBool,
    active_workers: AtomicUsize,
    event: Arc<signal::ChangeEvent>,
}

struct WindowState {
    id: u64,
    native_window: gpui::WindowId,
    core: Weak<ProcessCore>,
    attachment: Weak<AttachmentState>,
    scope: ImageResourcePinScope,
    closed: AtomicBool,
    interests: Mutex<BTreeMap<u64, Weak<InterestState>>>,
}

struct InterestState {
    id: u64,
    native_revision: u64,
    window: Weak<WindowState>,
    core: Weak<ProcessCore>,
    cancellation: CommandCancellation,
    status: Mutex<InterestStatus>,
    event: Arc<signal::ChangeEvent>,
}

impl InterestState {
    fn is_live(&self) -> bool {
        !self.cancellation.is_cancelled()
            && self.window.upgrade().is_some_and(|window| {
                !window.closed.load(Ordering::Acquire)
                    && window
                        .attachment
                        .upgrade()
                        .is_some_and(|attachment| !attachment.retired.load(Ordering::Acquire))
            })
    }
}

enum InterestStatus {
    Queued,
    Flight(RenditionIdentity),
    Prepared(RenditionIdentity),
    Ready(ImageResourceHandle),
    Failed(ImageMediaFailure),
}

struct QueuedDemand {
    attachment: Weak<AttachmentState>,
    interest: Weak<InterestState>,
    demand: ImageSourceDemand,
}

struct Flight {
    worker: u64,
    cancellation: CommandCancellation,
    upload_cancellation: Option<ImageUploadCancellation>,
    interests: BTreeMap<u64, Weak<InterestState>>,
    phase: FlightPhase,
}

impl Flight {
    fn has_live_interest(&self) -> bool {
        self.interests
            .values()
            .filter_map(Weak::upgrade)
            .any(|interest| interest.is_live())
    }
}

enum FlightPhase {
    Working,
    Prepared(ImageUploadTicket),
    Publishing,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn next_id(value: &mut u64) -> Result<u64, ImageMediaError> {
    *value = value
        .checked_add(1)
        .ok_or(ImageMediaError(ImageMediaFailure::Capacity))?;
    Ok(*value)
}

impl ImageMediaProcess {
    pub(crate) fn start(
        domain: ImageResourceDomain,
        limits: MediaResourceLimits,
    ) -> Result<Arc<Self>, ImageMediaError> {
        let config = limits.configuration();
        let native = domain.config();
        if native.staging_buffers != config.upload_staging_buffers
            || native.staging_bytes != config.upload_staging_bytes
            || native.texture_entries != config.gpu_texture_entries
            || native.texture_bytes != config.gpu_texture_bytes as u64
            || native.window_pin_entries != config.window_pin_entries
            || native.window_pin_bytes != config.window_pin_gpu_bytes as u64
        {
            return Err(ImageMediaError(ImageMediaFailure::Capacity));
        }
        let core = Arc::new(ProcessCore {
            generated_capacity: Arc::new(signal::ChangeEvent::default()),
            original_capacity: Arc::new(original::OriginalCapacityEvent::new(limits)),
            native_progress: Arc::new(signal::NativeProgress::default()),
            domain,
            limits,
            cpu: CpuSurfaceCache::new(limits),
            state: Mutex::new(ProcessState::default()),
            work: Condvar::new(),
            event: Arc::new(signal::ChangeEvent::default()),
        });
        let mut workers = Vec::with_capacity(config.decoder_slots);
        for index in 0..config.decoder_slots {
            let worker_core = core.clone();
            match thread::Builder::new()
                .name(format!("beryl-image-{index}"))
                .stack_size(32 * 1024 * 1024)
                .spawn(move || worker::run(worker_core))
            {
                Ok(handle) => workers.push(handle),
                Err(_) => {
                    lock(&core.state).stopping = true;
                    core.work.notify_all();
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(ImageMediaError(ImageMediaFailure::Worker));
                }
            }
        }
        Ok(Arc::new(Self {
            core,
            workers: Mutex::new(workers),
        }))
    }

    pub(crate) fn domain(&self) -> &ImageResourceDomain {
        &self.core.domain
    }
    pub(crate) fn limits(&self) -> MediaResourceLimits {
        self.core.limits
    }
    pub(crate) fn changed(&self) -> ImageMediaChanged {
        self.core.event.changed(false)
    }

    pub(crate) fn generated_capacity_changed(&self) -> ImageMediaChanged {
        self.core.generated_capacity.changed(false)
    }

    pub(crate) fn acquire_generated_source_stages(
        &self,
    ) -> Result<GeneratedSourceStageLease, ImageMediaError> {
        Ok(GeneratedSourceStageLease {
            _lease: worker::SourceStageLease::acquire(&self.core)?,
        })
    }

    pub(crate) fn native_resources_changed(&self) -> Result<(), ImageMediaError> {
        let (progress, interests) = {
            let state = lock(&self.core.state);
            let progress = self.core.native_progress.advance();
            let interests = state
                .windows
                .values()
                .filter_map(Weak::upgrade)
                .flat_map(|window| {
                    lock(&window.interests)
                        .values()
                        .filter_map(Weak::upgrade)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            (progress, interests)
        };
        for interest in interests {
            if progress.is_err() {
                presentation::cancel_interest(&interest, ImageMediaFailure::Capacity);
            } else {
                interest.event.notify();
            }
        }
        self.core.event.notify();
        progress
    }

    pub(crate) fn attach(
        self: &Arc<Self>,
        source: ImageSourceProvider,
    ) -> Result<ImageMediaAttachment, ImageMediaError> {
        let mut state = lock(&self.core.state);
        if state.stopping {
            return Err(ImageMediaError(ImageMediaFailure::Retired));
        }
        if let Some(previous) = state.attachment.upgrade() {
            if !previous.retired.load(Ordering::Acquire)
                || previous.active_workers.load(Ordering::Acquire) != 0
                || state
                    .original
                    .as_ref()
                    .is_some_and(|operation| operation.belongs_to(&previous))
                || previous.consumer.retire() != ImageResourceDrain::Drained
            {
                return Err(ImageMediaError(ImageMediaFailure::Retired));
            }
        }
        let consumer = self.core.domain.begin_consumer()?;
        let attachment = Arc::new(AttachmentState {
            id: next_id(&mut state.next_attachment)?,
            core: Arc::downgrade(&self.core),
            source: Mutex::new(Some(Arc::new(source))),
            consumer,
            retired: AtomicBool::new(false),
            active_workers: AtomicUsize::new(0),
            event: Arc::new(signal::ChangeEvent::default()),
        });
        state.attachment = Arc::downgrade(&attachment);
        Ok(ImageMediaAttachment { inner: attachment })
    }

    pub(crate) fn diagnostics(&self) -> Result<ImageMediaDiagnostics, ImageMediaError> {
        let state = lock(&self.core.state);
        Ok(ImageMediaDiagnostics {
            native_revision: self.core.native_progress.revision(),
            native_revision_exhausted: self.core.native_progress.is_exhausted(),
            queued: state.queue.len(),
            active_workers: state.active_workers,
            windows: state.windows.len(),
            flights: state.flights.len(),
            path_slots: state.path_slots,
            verification_slots: state.verification_slots,
            decodes: state.decodes,
            joins: state.joins,
            cancellations: state.cancellations,
            failures: state.failures,
            original_commands: usize::from(state.original.is_some()),
            original_encoded_bytes: state
                .original
                .as_ref()
                .map_or(0, |operation| operation.encoded_bytes()),
            cpu: self.core.cpu.diagnostics()?,
        })
    }
}

impl ProcessCore {
    fn finish_flight(
        &self,
        identity: RenditionIdentity,
        worker: u64,
        result: Result<ImageResourceHandle, ImageMediaFailure>,
    ) {
        let events = {
            let mut state = lock(&self.state);
            if !state
                .flights
                .get(&identity)
                .is_some_and(|flight| flight.worker == worker)
            {
                return;
            }
            let flight = state
                .flights
                .remove(&identity)
                .expect("checked flight owner");
            if result.is_err() {
                state.failures = state.failures.saturating_add(1);
            }
            flight
                .interests
                .values()
                .filter_map(Weak::upgrade)
                .map(|interest| {
                    if interest.is_live() {
                        *lock(&interest.status) = match &result {
                            Ok(handle) => InterestStatus::Ready(handle.clone()),
                            Err(error) => InterestStatus::Failed(*error),
                        };
                    }
                    interest.event.clone()
                })
                .collect::<Vec<_>>()
        };
        for event in events {
            event.notify();
        }
        self.event.notify();
    }
}
