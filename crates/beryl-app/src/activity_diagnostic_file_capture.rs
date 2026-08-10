//! Bounded, content-free Activity diagnostic JSONL capture.

use std::{
    collections::hash_map::RandomState,
    fs::{self, File, OpenOptions},
    hash::{BuildHasher, Hash, Hasher},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::BerylHomeDir;

pub const ACTIVITY_CAPTURE_SCHEMA_VERSION: u32 = 1;
pub const ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY: u64 = 10 * 1024 * 1024;
pub const ACTIVITY_CAPTURE_TOTAL_DATA_BYTE_CAPACITY: u64 =
    2 * ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY;
pub const ACTIVITY_CAPTURE_QUEUE_CAPACITY: usize = 256;

const IDENTITY_BYTE_LIMIT: usize = 512;
const PROTOCOL_STRING_BYTE_LIMIT: usize = 512;
const BUILD_IDENTITY_BYTE_LIMIT: usize = 256;
const RENDER_SAMPLE_ROW_LIMIT: usize = 32;
const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(5);

const CAPTURE_DIRECTORY: &str = "diagnostics/activity-capture";
const CURRENT_FILE: &str = "activity.jsonl";
const PREVIOUS_FILE: &str = "activity.previous.jsonl";
const LOCK_FILE: &str = "activity.lock";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityDiagnosticCaptureRuntimeState {
    Disabled,
    Starting,
    Active,
    Stopping,
    Unavailable,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityDiagnosticCaptureErrorCategory {
    LockUnavailable,
    Directory,
    Lock,
    Recovery,
    Rotation,
    Serialization,
    Write,
    WriterDisconnected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityDiagnosticCaptureStatus {
    pub configured: bool,
    pub runtime_state: ActivityDiagnosticCaptureRuntimeState,
    pub capture_generation: u64,
    pub written_record_count: u64,
    pub dropped_record_count: u64,
    pub queue_full_drop_count: u64,
    pub queue_disconnected_drop_count: u64,
    pub schema_rejection_drop_count: u64,
    pub oversized_record_count: u64,
    pub repair_count: u64,
    pub rotation_count: u64,
    pub error_category: Option<ActivityDiagnosticCaptureErrorCategory>,
}

impl Default for ActivityDiagnosticCaptureStatus {
    fn default() -> Self {
        Self {
            configured: false,
            runtime_state: ActivityDiagnosticCaptureRuntimeState::Disabled,
            capture_generation: 0,
            written_record_count: 0,
            dropped_record_count: 0,
            queue_full_drop_count: 0,
            queue_disconnected_drop_count: 0,
            schema_rejection_drop_count: 0,
            oversized_record_count: 0,
            repair_count: 0,
            rotation_count: 0,
            error_category: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityDiagnosticCaptureSubmitOutcome {
    Enqueued,
    Disabled,
    QueueFull,
    Disconnected,
    SchemaRejected,
    SequenceExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityDiagnosticCaptureControllerError {
    WriterStart,
    WriterDisconnected,
    GenerationExhausted,
}

impl std::fmt::Display for ActivityDiagnosticCaptureControllerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::WriterStart => "Activity diagnostic capture writer could not start",
            Self::WriterDisconnected => "Activity diagnostic capture writer is unavailable",
            Self::GenerationExhausted => "Activity diagnostic capture generation is exhausted",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ActivityDiagnosticCaptureControllerError {}

#[derive(Clone)]
pub struct ActivityDiagnosticCaptureSink {
    shared: Arc<CaptureShared>,
    event_sender: SyncSender<EventEnvelope>,
}

pub struct ActivityDiagnosticFileCaptureController {
    shared: Arc<CaptureShared>,
    control_sender: mpsc::Sender<ControlCommand>,
    sink: ActivityDiagnosticCaptureSink,
}

struct StatusCore {
    configured: bool,
    runtime_state: ActivityDiagnosticCaptureRuntimeState,
    capture_generation: u64,
    error_category: Option<ActivityDiagnosticCaptureErrorCategory>,
}

struct CaptureShared {
    status: Mutex<StatusCore>,
    active_generation: AtomicU64,
    generation_counter: AtomicU64,
    next_capture_sequence: AtomicU64,
    written_record_count: AtomicU64,
    dropped_record_count: AtomicU64,
    queue_full_drop_count: AtomicU64,
    queue_disconnected_drop_count: AtomicU64,
    schema_rejection_drop_count: AtomicU64,
    oversized_record_count: AtomicU64,
    repair_count: AtomicU64,
    rotation_count: AtomicU64,
    pending_gap: Mutex<PendingGap>,
}

enum ControlCommand {
    Enable {
        generation: u64,
        build_identity: Option<String>,
    },
    Disable {
        generation: u64,
    },
    Shutdown,
}

struct EventEnvelope {
    generation: u64,
    capture_sequence: u64,
    payload: EventPayload,
}

enum EventPayload {
    Event(ActivityDiagnosticCaptureEventV1),
    Gap {
        first_dropped_capture_sequence: u64,
        last_dropped_capture_sequence: u64,
        queue_full_drop_count: u64,
        queue_disconnected_drop_count: u64,
    },
}

impl ActivityDiagnosticFileCaptureController {
    pub fn new(beryl_home: BerylHomeDir) -> Result<Self, ActivityDiagnosticCaptureControllerError> {
        Self::with_queue_capacity(beryl_home, ACTIVITY_CAPTURE_QUEUE_CAPACITY)
    }

    pub fn with_queue_capacity(
        beryl_home: BerylHomeDir,
        queue_capacity: usize,
    ) -> Result<Self, ActivityDiagnosticCaptureControllerError> {
        let shared = Arc::new(CaptureShared::new());
        let (control_sender, control_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::sync_channel(queue_capacity.max(1));
        let worker_shared = Arc::clone(&shared);
        let paths = CapturePaths::from_beryl_home(&beryl_home);
        thread::Builder::new()
            .name("activity-diagnostic-capture".to_string())
            .spawn(move || writer_loop(paths, worker_shared, control_receiver, event_receiver))
            .map_err(|_| ActivityDiagnosticCaptureControllerError::WriterStart)?;
        let sink = ActivityDiagnosticCaptureSink {
            shared: Arc::clone(&shared),
            event_sender,
        };
        Ok(Self {
            shared,
            control_sender,
            sink,
        })
    }

    pub fn sink(&self) -> ActivityDiagnosticCaptureSink {
        self.sink.clone()
    }

    pub fn enable(
        &self,
        build_identity: Option<&str>,
    ) -> Result<u64, ActivityDiagnosticCaptureControllerError> {
        self.shared.active_generation.store(0, Ordering::Release);
        let generation = self.shared.next_generation()?;
        self.shared
            .next_capture_sequence
            .store(1, Ordering::Release);
        self.shared.clear_pending_gap(generation);
        self.shared.set_status(
            true,
            ActivityDiagnosticCaptureRuntimeState::Starting,
            generation,
            None,
        );
        self.control_sender
            .send(ControlCommand::Enable {
                generation,
                build_identity: bounded_build_identity(build_identity),
            })
            .map_err(|_| {
                self.shared.fail_generation(
                    generation,
                    ActivityDiagnosticCaptureRuntimeState::Failed,
                    ActivityDiagnosticCaptureErrorCategory::WriterDisconnected,
                );
                ActivityDiagnosticCaptureControllerError::WriterDisconnected
            })?;
        Ok(generation)
    }

    pub fn disable(&self) -> Result<(), ActivityDiagnosticCaptureControllerError> {
        let generation = self.shared.active_generation.swap(0, Ordering::AcqRel);
        let status_generation = if generation == 0 {
            self.shared.status().capture_generation
        } else {
            generation
        };
        self.shared.set_status(
            false,
            ActivityDiagnosticCaptureRuntimeState::Stopping,
            status_generation,
            None,
        );
        self.control_sender
            .send(ControlCommand::Disable {
                generation: status_generation,
            })
            .map_err(|_| {
                self.shared.fail_disable(
                    status_generation,
                    ActivityDiagnosticCaptureErrorCategory::WriterDisconnected,
                );
                ActivityDiagnosticCaptureControllerError::WriterDisconnected
            })
    }

    pub fn status(&self) -> ActivityDiagnosticCaptureStatus {
        self.shared.status()
    }
}

impl Drop for ActivityDiagnosticFileCaptureController {
    fn drop(&mut self) {
        self.shared.active_generation.store(0, Ordering::Release);
        if let Ok(mut status) = self.shared.status.try_lock() {
            status.configured = false;
            status.runtime_state = ActivityDiagnosticCaptureRuntimeState::Stopping;
            status.error_category = None;
        }
        let _ = self.control_sender.send(ControlCommand::Shutdown);
    }
}

impl CaptureShared {
    fn new() -> Self {
        Self {
            status: Mutex::new(StatusCore {
                configured: false,
                runtime_state: ActivityDiagnosticCaptureRuntimeState::Disabled,
                capture_generation: 0,
                error_category: None,
            }),
            active_generation: AtomicU64::new(0),
            generation_counter: AtomicU64::new(0),
            next_capture_sequence: AtomicU64::new(1),
            written_record_count: AtomicU64::new(0),
            dropped_record_count: AtomicU64::new(0),
            queue_full_drop_count: AtomicU64::new(0),
            queue_disconnected_drop_count: AtomicU64::new(0),
            schema_rejection_drop_count: AtomicU64::new(0),
            oversized_record_count: AtomicU64::new(0),
            repair_count: AtomicU64::new(0),
            rotation_count: AtomicU64::new(0),
            pending_gap: Mutex::new(PendingGap::default()),
        }
    }

    fn next_generation(&self) -> Result<u64, ActivityDiagnosticCaptureControllerError> {
        self.generation_counter
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(1)
            })
            .map(|previous| previous + 1)
            .map_err(|_| ActivityDiagnosticCaptureControllerError::GenerationExhausted)
    }

    fn next_sequence(&self) -> Option<u64> {
        self.next_capture_sequence
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(1)
            })
            .ok()
    }

    fn set_status(
        &self,
        configured: bool,
        runtime_state: ActivityDiagnosticCaptureRuntimeState,
        generation: u64,
        error_category: Option<ActivityDiagnosticCaptureErrorCategory>,
    ) {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        status.configured = configured;
        status.runtime_state = runtime_state;
        status.capture_generation = generation;
        status.error_category = error_category;
    }

    fn fail_generation(
        &self,
        generation: u64,
        state: ActivityDiagnosticCaptureRuntimeState,
        category: ActivityDiagnosticCaptureErrorCategory,
    ) {
        let _ = self.active_generation.compare_exchange(
            generation,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if status.configured
            && status.capture_generation == generation
            && matches!(
                status.runtime_state,
                ActivityDiagnosticCaptureRuntimeState::Starting
                    | ActivityDiagnosticCaptureRuntimeState::Active
            )
        {
            status.runtime_state = state;
            status.error_category = Some(category);
        }
    }

    fn fail_disable(&self, generation: u64, category: ActivityDiagnosticCaptureErrorCategory) {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !status.configured
            && status.capture_generation == generation
            && status.runtime_state == ActivityDiagnosticCaptureRuntimeState::Stopping
        {
            status.runtime_state = ActivityDiagnosticCaptureRuntimeState::Failed;
            status.error_category = Some(category);
        }
    }

    fn status(&self) -> ActivityDiagnosticCaptureStatus {
        let status = self
            .status
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        ActivityDiagnosticCaptureStatus {
            configured: status.configured,
            runtime_state: status.runtime_state,
            capture_generation: status.capture_generation,
            written_record_count: self.written_record_count.load(Ordering::Relaxed),
            dropped_record_count: self.dropped_record_count.load(Ordering::Relaxed),
            queue_full_drop_count: self.queue_full_drop_count.load(Ordering::Relaxed),
            queue_disconnected_drop_count: self
                .queue_disconnected_drop_count
                .load(Ordering::Relaxed),
            schema_rejection_drop_count: self.schema_rejection_drop_count.load(Ordering::Relaxed),
            oversized_record_count: self.oversized_record_count.load(Ordering::Relaxed),
            repair_count: self.repair_count.load(Ordering::Relaxed),
            rotation_count: self.rotation_count.load(Ordering::Relaxed),
            error_category: status.error_category,
        }
    }

    fn generation_is_desired(&self, generation: u64) -> bool {
        let status = self
            .status
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        status.configured
            && status.capture_generation == generation
            && matches!(
                status.runtime_state,
                ActivityDiagnosticCaptureRuntimeState::Starting
                    | ActivityDiagnosticCaptureRuntimeState::Active
            )
    }

    fn disable_is_desired(&self, generation: u64) -> bool {
        let status = self
            .status
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        !status.configured
            && status.capture_generation == generation
            && status.runtime_state == ActivityDiagnosticCaptureRuntimeState::Stopping
    }

    fn publish_active_if_desired(&self, generation: u64) -> bool {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if status.configured
            && status.capture_generation == generation
            && status.runtime_state == ActivityDiagnosticCaptureRuntimeState::Starting
        {
            self.active_generation.store(generation, Ordering::Release);
            status.runtime_state = ActivityDiagnosticCaptureRuntimeState::Active;
            status.error_category = None;
            true
        } else {
            false
        }
    }

    fn clear_pending_gap(&self, generation: u64) {
        if let Some(mut pending) = self.try_pending_gap() {
            pending.reset(generation);
        }
    }
}

impl ActivityDiagnosticCaptureSink {
    pub(crate) fn is_active(&self) -> bool {
        self.shared.active_generation.load(Ordering::Acquire) != 0
    }

    pub(crate) fn note_schema_rejection(&self) -> ActivityDiagnosticCaptureSubmitOutcome {
        if !self.is_active() {
            return ActivityDiagnosticCaptureSubmitOutcome::Disabled;
        }
        atomic_saturating_increment(&self.shared.schema_rejection_drop_count);
        atomic_saturating_increment(&self.shared.dropped_record_count);
        ActivityDiagnosticCaptureSubmitOutcome::SchemaRejected
    }

    pub fn try_record(
        &self,
        event: ActivityDiagnosticCaptureEventV1,
    ) -> ActivityDiagnosticCaptureSubmitOutcome {
        let generation = self.shared.active_generation.load(Ordering::Acquire);
        if generation == 0 {
            return ActivityDiagnosticCaptureSubmitOutcome::Disabled;
        }
        if !event.validate() {
            atomic_saturating_increment(&self.shared.schema_rejection_drop_count);
            atomic_saturating_increment(&self.shared.dropped_record_count);
            return ActivityDiagnosticCaptureSubmitOutcome::SchemaRejected;
        }
        match self.try_emit_pending_gap(generation) {
            GapAttempt::Continue => {}
            GapAttempt::SequenceExhausted => {
                atomic_saturating_increment(&self.shared.dropped_record_count);
                return ActivityDiagnosticCaptureSubmitOutcome::SequenceExhausted;
            }
        }
        let Some(capture_sequence) = self.shared.next_sequence() else {
            atomic_saturating_increment(&self.shared.dropped_record_count);
            return ActivityDiagnosticCaptureSubmitOutcome::SequenceExhausted;
        };
        let envelope = EventEnvelope {
            generation,
            capture_sequence,
            payload: EventPayload::Event(event),
        };
        match self.event_sender.try_send(envelope) {
            Ok(()) => {
                self.shared.allow_gap_retry(generation);
                ActivityDiagnosticCaptureSubmitOutcome::Enqueued
            }
            Err(TrySendError::Full(envelope)) => {
                self.shared
                    .record_drop(envelope.capture_sequence, DropKind::QueueFull, generation);
                ActivityDiagnosticCaptureSubmitOutcome::QueueFull
            }
            Err(TrySendError::Disconnected(envelope)) => {
                self.shared.record_drop(
                    envelope.capture_sequence,
                    DropKind::QueueDisconnected,
                    generation,
                );
                self.shared.fail_generation(
                    generation,
                    ActivityDiagnosticCaptureRuntimeState::Failed,
                    ActivityDiagnosticCaptureErrorCategory::WriterDisconnected,
                );
                ActivityDiagnosticCaptureSubmitOutcome::Disconnected
            }
        }
    }

    fn try_emit_pending_gap(&self, generation: u64) -> GapAttempt {
        let Some(mut gap) = self.shared.take_pending_gap(generation) else {
            return GapAttempt::Continue;
        };
        let Some(capture_sequence) = self.shared.next_sequence() else {
            self.shared.restore_pending_gap(generation, gap, false);
            return GapAttempt::SequenceExhausted;
        };
        let envelope = EventEnvelope {
            generation,
            capture_sequence,
            payload: EventPayload::Gap {
                first_dropped_capture_sequence: gap.first_sequence,
                last_dropped_capture_sequence: gap.last_sequence,
                queue_full_drop_count: gap.queue_full_count,
                queue_disconnected_drop_count: gap.queue_disconnected_count,
            },
        };
        match self.event_sender.try_send(envelope) {
            Ok(()) => GapAttempt::Continue,
            Err(TrySendError::Full(envelope)) => {
                gap.include_sequence(envelope.capture_sequence);
                self.shared.restore_pending_gap(generation, gap, true);
                GapAttempt::Continue
            }
            Err(TrySendError::Disconnected(envelope)) => {
                gap.include_sequence(envelope.capture_sequence);
                self.shared.restore_pending_gap(generation, gap, true);
                GapAttempt::Continue
            }
        }
    }
}

enum GapAttempt {
    Continue,
    SequenceExhausted,
}

#[derive(Clone, Copy)]
enum DropKind {
    QueueFull,
    QueueDisconnected,
}

#[derive(Default)]
struct PendingGap {
    generation: u64,
    suppressed: bool,
    snapshot: Option<PendingGapSnapshot>,
}

#[derive(Clone, Copy)]
struct PendingGapSnapshot {
    first_sequence: u64,
    last_sequence: u64,
    queue_full_count: u64,
    queue_disconnected_count: u64,
}

impl PendingGap {
    fn reset(&mut self, generation: u64) {
        self.generation = generation;
        self.suppressed = false;
        self.snapshot = None;
    }

    fn ensure_generation(&mut self, generation: u64) {
        if self.generation != generation {
            self.reset(generation);
        }
    }

    fn record(&mut self, generation: u64, sequence: u64, kind: DropKind) {
        self.ensure_generation(generation);
        let snapshot = self.snapshot.get_or_insert(PendingGapSnapshot {
            first_sequence: sequence,
            last_sequence: sequence,
            queue_full_count: 0,
            queue_disconnected_count: 0,
        });
        snapshot.include_sequence(sequence);
        match kind {
            DropKind::QueueFull => {
                snapshot.queue_full_count = snapshot.queue_full_count.saturating_add(1)
            }
            DropKind::QueueDisconnected => {
                snapshot.queue_disconnected_count =
                    snapshot.queue_disconnected_count.saturating_add(1)
            }
        }
    }

    fn restore(&mut self, generation: u64, restored: PendingGapSnapshot, suppress: bool) {
        self.ensure_generation(generation);
        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.first_sequence = snapshot.first_sequence.min(restored.first_sequence);
            snapshot.last_sequence = snapshot.last_sequence.max(restored.last_sequence);
            snapshot.queue_full_count = snapshot
                .queue_full_count
                .saturating_add(restored.queue_full_count);
            snapshot.queue_disconnected_count = snapshot
                .queue_disconnected_count
                .saturating_add(restored.queue_disconnected_count);
        } else {
            self.snapshot = Some(restored);
        }
        self.suppressed |= suppress;
    }
}

impl PendingGapSnapshot {
    fn include_sequence(&mut self, sequence: u64) {
        self.first_sequence = self.first_sequence.min(sequence);
        self.last_sequence = self.last_sequence.max(sequence);
    }
}

impl CaptureShared {
    fn record_drop(&self, sequence: u64, kind: DropKind, generation: u64) {
        if self.active_generation.load(Ordering::Acquire) != generation {
            return;
        }
        atomic_saturating_increment(&self.dropped_record_count);
        match kind {
            DropKind::QueueFull => atomic_saturating_increment(&self.queue_full_drop_count),
            DropKind::QueueDisconnected => {
                atomic_saturating_increment(&self.queue_disconnected_drop_count)
            }
        }
        if let Some(mut pending) = self.try_pending_gap() {
            pending.record(generation, sequence, kind);
        }
    }

    fn take_pending_gap(&self, generation: u64) -> Option<PendingGapSnapshot> {
        let mut pending = self.try_pending_gap()?;
        pending.ensure_generation(generation);
        if pending.suppressed {
            return None;
        }
        pending.snapshot.take()
    }

    fn restore_pending_gap(&self, generation: u64, snapshot: PendingGapSnapshot, suppress: bool) {
        if let Some(mut pending) = self.try_pending_gap() {
            pending.restore(generation, snapshot, suppress);
        }
    }

    fn allow_gap_retry(&self, generation: u64) {
        if let Some(mut pending) = self.try_pending_gap() {
            pending.ensure_generation(generation);
            pending.suppressed = false;
        }
    }

    fn try_pending_gap(&self) -> Option<std::sync::MutexGuard<'_, PendingGap>> {
        match self.pending_gap.try_lock() {
            Ok(pending) => Some(pending),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }
}

fn atomic_saturating_increment(value: &AtomicU64) {
    atomic_saturating_add(value, 1);
}

fn atomic_saturating_add(value: &AtomicU64, amount: u64) {
    let _ = value.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(amount))
    });
}

struct ActiveWriterGeneration {
    generation: u64,
    journal: ActivityCaptureJournal,
    reported_rotation_count: u64,
}

fn writer_loop(
    paths: CapturePaths,
    shared: Arc<CaptureShared>,
    control_receiver: Receiver<ControlCommand>,
    event_receiver: Receiver<EventEnvelope>,
) {
    let mut active: Option<ActiveWriterGeneration> = None;
    loop {
        let mut handled_control = false;
        loop {
            match control_receiver.try_recv() {
                Ok(ControlCommand::Enable {
                    generation,
                    build_identity,
                }) => {
                    handled_control = true;
                    active.take();
                    if !shared.generation_is_desired(generation) {
                        continue;
                    }
                    let header_context = new_header_context(build_identity.as_deref(), generation);
                    match ActivityCaptureJournal::activate(paths.clone(), header_context) {
                        Ok(journal) => {
                            if !shared.publish_active_if_desired(generation) {
                                drop(journal);
                                continue;
                            }
                            atomic_saturating_add(&shared.repair_count, journal.repair_count);
                            atomic_saturating_add(&shared.rotation_count, journal.rotation_count);
                            let reported_rotation_count = journal.rotation_count;
                            active = Some(ActiveWriterGeneration {
                                generation,
                                journal,
                                reported_rotation_count,
                            });
                        }
                        Err(category) => {
                            let state = if category
                                == ActivityDiagnosticCaptureErrorCategory::LockUnavailable
                            {
                                ActivityDiagnosticCaptureRuntimeState::Unavailable
                            } else {
                                ActivityDiagnosticCaptureRuntimeState::Failed
                            };
                            shared.fail_generation(generation, state, category);
                        }
                    }
                }
                Ok(ControlCommand::Disable { generation }) => {
                    handled_control = true;
                    if !shared.disable_is_desired(generation) {
                        continue;
                    }
                    active.take();
                    shared.active_generation.store(0, Ordering::Release);
                    shared.set_status(
                        false,
                        ActivityDiagnosticCaptureRuntimeState::Disabled,
                        generation,
                        None,
                    );
                }
                Ok(ControlCommand::Shutdown) => {
                    active.take();
                    shared.active_generation.store(0, Ordering::Release);
                    return;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    active.take();
                    shared.active_generation.store(0, Ordering::Release);
                    return;
                }
            }
        }
        if handled_control {
            continue;
        }

        let envelope = match event_receiver.recv_timeout(CONTROL_POLL_INTERVAL) {
            Ok(envelope) => envelope,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                match control_receiver.recv_timeout(CONTROL_POLL_INTERVAL) {
                    Ok(command) => {
                        if matches!(command, ControlCommand::Shutdown) {
                            active.take();
                            shared.active_generation.store(0, Ordering::Release);
                            return;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                continue;
            }
        };
        let Some(writer) = active.as_mut() else {
            continue;
        };
        if writer.generation != envelope.generation
            || shared.active_generation.load(Ordering::Acquire) != envelope.generation
        {
            continue;
        }
        let record = match envelope.payload {
            EventPayload::Event(event) => {
                durable_event_record(envelope.generation, envelope.capture_sequence, event)
            }
            EventPayload::Gap {
                first_dropped_capture_sequence,
                last_dropped_capture_sequence,
                queue_full_drop_count,
                queue_disconnected_drop_count,
            } => DurableRecordV1::CaptureGap(CaptureGapRecordV1 {
                schema_version: ACTIVITY_CAPTURE_SCHEMA_VERSION,
                capture_generation: envelope.generation,
                capture_sequence: envelope.capture_sequence,
                first_dropped_capture_sequence,
                last_dropped_capture_sequence,
                queue_full_drop_count,
                queue_disconnected_drop_count,
            }),
        };
        match writer.journal.append_record(&record) {
            Ok(JournalWriteOutcome::Written) => {
                atomic_saturating_increment(&shared.written_record_count);
                if writer.journal.rotation_count > writer.reported_rotation_count {
                    atomic_saturating_add(
                        &shared.rotation_count,
                        writer
                            .journal
                            .rotation_count
                            .saturating_sub(writer.reported_rotation_count),
                    );
                    writer.reported_rotation_count = writer.journal.rotation_count;
                }
            }
            Ok(JournalWriteOutcome::Oversized) => {
                atomic_saturating_increment(&shared.oversized_record_count);
                atomic_saturating_increment(&shared.dropped_record_count);
            }
            Err(category) => {
                let generation = writer.generation;
                active.take();
                shared.fail_generation(
                    generation,
                    ActivityDiagnosticCaptureRuntimeState::Failed,
                    category,
                );
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityDiagnosticIdentityValidityV1 {
    Valid,
    Missing,
    Blank,
    OverBound,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityDiagnosticIdentityV1 {
    validity: ActivityDiagnosticIdentityValidityV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    original_byte_count: u64,
}

impl ActivityDiagnosticIdentityV1 {
    pub fn capture(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self {
                validity: ActivityDiagnosticIdentityValidityV1::Missing,
                value: None,
                original_byte_count: 0,
            };
        };
        let original_byte_count = usize_to_u64_saturated(value.len());
        if value.trim().is_empty() {
            return Self {
                validity: ActivityDiagnosticIdentityValidityV1::Blank,
                value: None,
                original_byte_count,
            };
        }
        if value.len() > IDENTITY_BYTE_LIMIT {
            return Self {
                validity: ActivityDiagnosticIdentityValidityV1::OverBound,
                value: None,
                original_byte_count,
            };
        }
        Self {
            validity: ActivityDiagnosticIdentityValidityV1::Valid,
            value: Some(value.to_string()),
            original_byte_count,
        }
    }

    pub(crate) fn try_from_normalized(
        validity: ActivityDiagnosticIdentityValidityV1,
        value: Option<&str>,
        original_byte_count: usize,
    ) -> Option<Self> {
        let identity = Self {
            validity,
            value: value.map(str::to_string),
            original_byte_count: u64::try_from(original_byte_count).ok()?,
        };
        identity.validate().then_some(identity)
    }

    fn validate(&self) -> bool {
        match self.validity {
            ActivityDiagnosticIdentityValidityV1::Valid => {
                self.value.as_ref().is_some_and(|value| {
                    !value.trim().is_empty()
                        && value.len() <= IDENTITY_BYTE_LIMIT
                        && self.original_byte_count == usize_to_u64_saturated(value.len())
                })
            }
            ActivityDiagnosticIdentityValidityV1::Missing => {
                self.value.is_none() && self.original_byte_count == 0
            }
            ActivityDiagnosticIdentityValidityV1::Blank
            | ActivityDiagnosticIdentityValidityV1::OverBound => self.value.is_none(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityDiagnosticProtocolStringV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    original_byte_count: u64,
    truncated: bool,
}

impl ActivityDiagnosticProtocolStringV1 {
    pub fn capture(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self {
                value: None,
                original_byte_count: 0,
                truncated: false,
            };
        };
        let end = utf8_prefix_end(value, PROTOCOL_STRING_BYTE_LIMIT);
        Self {
            value: Some(value[..end].to_string()),
            original_byte_count: usize_to_u64_saturated(value.len()),
            truncated: end < value.len(),
        }
    }

    pub(crate) fn try_from_normalized(
        value: Option<&str>,
        original_byte_count: usize,
        truncated: bool,
    ) -> Option<Self> {
        let protocol_string = Self {
            value: value.map(str::to_string),
            original_byte_count: u64::try_from(original_byte_count).ok()?,
            truncated,
        };
        protocol_string.validate().then_some(protocol_string)
    }

    fn validate(&self) -> bool {
        match &self.value {
            None => self.original_byte_count == 0 && !self.truncated,
            Some(value) => {
                value.len() <= PROTOCOL_STRING_BYTE_LIMIT
                    && self.original_byte_count >= usize_to_u64_saturated(value.len())
                    && self.truncated
                        == (self.original_byte_count > usize_to_u64_saturated(value.len()))
            }
        }
    }
}

macro_rules! closed_string_enum {
    ($name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
        pub enum $name {
            $(#[serde(rename = $wire)] $variant),+
        }
    };
}

closed_string_enum!(ActivityDiagnosticLifecycleStageV1 {
    ActivityIngress => "activity_ingress",
    Fallback => "fallback",
    StreamFailure => "stream_failure",
});
closed_string_enum!(ActivityDiagnosticLifecycleCategoryV1 {
    Lifecycle => "lifecycle",
    Fallback => "fallback",
    StreamFailure => "stream_failure",
});
closed_string_enum!(ActivityDiagnosticLifecycleKindV1 {
    Started => "started",
    Updated => "updated",
    Completed => "completed",
    TurnCompleted => "turn_completed",
    ThreadClosed => "thread_closed",
    ThreadArchived => "thread_archived",
    ThreadDeleted => "thread_deleted",
    ProtocolError => "protocol_error",
    LocalTurnFailure => "local_turn_failure",
});
closed_string_enum!(ActivityDiagnosticProjectionOutcomeV1 {
    InsertedRunning => "inserted_running",
    MatchedRunning => "matched_running",
    ReactivatedExisting => "reactivated_existing",
    MatchedExisting => "matched_existing",
    InsertedCompleted => "inserted_completed",
    NoRunningMatch => "no_running_match",
    FinishedRunningRows => "finished_running_rows",
});
closed_string_enum!(ActivityDiagnosticRowStatusV1 {
    Running => "running",
    FinishedOk => "finished_ok",
    FinishedError => "finished_error",
});
closed_string_enum!(ActivityDiagnosticIndicatorRoleV1 {
    Running => "activity.indicator.running",
    Ok => "activity.indicator.ok",
    Error => "activity.indicator.error",
});
closed_string_enum!(ActivityDiagnosticColorSourceV1 {
    ThemeRole => "theme_role",
    RendererFallback => "renderer_fallback",
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityDiagnosticLifecycleEventV1 {
    pub source_sequence: u64,
    pub elapsed_micros: u64,
    pub stage: ActivityDiagnosticLifecycleStageV1,
    pub category: ActivityDiagnosticLifecycleCategoryV1,
    pub kind: ActivityDiagnosticLifecycleKindV1,
    pub thread_identity: ActivityDiagnosticIdentityV1,
    pub turn_identity: ActivityDiagnosticIdentityV1,
    pub item_identity: ActivityDiagnosticIdentityV1,
    pub item_type: ActivityDiagnosticProtocolStringV1,
    pub item_status: ActivityDiagnosticProtocolStringV1,
    pub projection_outcome: ActivityDiagnosticProjectionOutcomeV1,
    pub before_row_status: Option<ActivityDiagnosticRowStatusV1>,
    pub after_row_status: Option<ActivityDiagnosticRowStatusV1>,
    pub affected_row_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityDiagnosticProjectionChangedV1 {
    pub source_sequence: u64,
    pub elapsed_micros: u64,
    pub projection_revision: u64,
    pub newest_lifecycle_sequence: Option<u64>,
    pub total_row_count: u64,
    pub running_row_count: u64,
    pub finished_ok_row_count: u64,
    pub finished_error_row_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityDiagnosticShellNotifiedV1 {
    pub source_sequence: u64,
    pub elapsed_micros: u64,
    pub projection_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityDiagnosticRenderRowV1 {
    pub rendered_index: u64,
    pub thread_identity: ActivityDiagnosticIdentityV1,
    pub turn_identity: ActivityDiagnosticIdentityV1,
    pub item_identity: ActivityDiagnosticIdentityV1,
    pub row_status: ActivityDiagnosticRowStatusV1,
    pub status_indicator_theme_role: ActivityDiagnosticIndicatorRoleV1,
    pub color_source: ActivityDiagnosticColorSourceV1,
    pub resolved_rgba: [u8; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityDiagnosticRenderSampleV1 {
    pub source_sequence: u64,
    pub elapsed_micros: u64,
    pub render_revision: u64,
    pub projection_revision: u64,
    pub newest_notified_projection_revision: u64,
    pub panel_visible: bool,
    pub selected_thread_identity: Option<ActivityDiagnosticIdentityV1>,
    pub selected_thread_row_count: u64,
    pub rendered_range_start: u64,
    pub rendered_range_end: u64,
    pub overscan_row_count: u64,
    pub sampled_rows: Vec<ActivityDiagnosticRenderRowV1>,
    pub row_sample_truncated: bool,
    pub event_bytes: u64,
    pub event_bytes_truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivityDiagnosticCaptureEventV1 {
    Lifecycle(ActivityDiagnosticLifecycleEventV1),
    ProjectionChanged(ActivityDiagnosticProjectionChangedV1),
    ShellNotified(ActivityDiagnosticShellNotifiedV1),
    RenderSample(ActivityDiagnosticRenderSampleV1),
}

impl ActivityDiagnosticCaptureEventV1 {
    fn validate(&self) -> bool {
        match self {
            Self::Lifecycle(event) => event.validate(),
            Self::ProjectionChanged(event) => {
                event
                    .running_row_count
                    .saturating_add(event.finished_ok_row_count)
                    .saturating_add(event.finished_error_row_count)
                    == event.total_row_count
            }
            Self::ShellNotified(_) => true,
            Self::RenderSample(event) => event.validate(),
        }
    }
}

impl ActivityDiagnosticLifecycleEventV1 {
    fn validate(&self) -> bool {
        let stage_kind_valid = match (self.stage, self.category, self.kind) {
            (
                ActivityDiagnosticLifecycleStageV1::ActivityIngress,
                ActivityDiagnosticLifecycleCategoryV1::Lifecycle,
                ActivityDiagnosticLifecycleKindV1::Started
                | ActivityDiagnosticLifecycleKindV1::Updated
                | ActivityDiagnosticLifecycleKindV1::Completed,
            ) => true,
            (
                ActivityDiagnosticLifecycleStageV1::Fallback,
                ActivityDiagnosticLifecycleCategoryV1::Fallback,
                ActivityDiagnosticLifecycleKindV1::TurnCompleted
                | ActivityDiagnosticLifecycleKindV1::ThreadClosed
                | ActivityDiagnosticLifecycleKindV1::ThreadArchived
                | ActivityDiagnosticLifecycleKindV1::ThreadDeleted
                | ActivityDiagnosticLifecycleKindV1::ProtocolError,
            ) => true,
            (
                ActivityDiagnosticLifecycleStageV1::StreamFailure,
                ActivityDiagnosticLifecycleCategoryV1::StreamFailure,
                ActivityDiagnosticLifecycleKindV1::LocalTurnFailure,
            ) => true,
            _ => false,
        };
        stage_kind_valid
            && self.thread_identity.validate()
            && self.turn_identity.validate()
            && self.item_identity.validate()
            && self.item_type.validate()
            && self.item_status.validate()
    }
}

impl ActivityDiagnosticRenderSampleV1 {
    fn validate(&self) -> bool {
        self.rendered_range_start <= self.rendered_range_end
            && self.sampled_rows.len() <= RENDER_SAMPLE_ROW_LIMIT
            && self
                .selected_thread_identity
                .as_ref()
                .is_none_or(ActivityDiagnosticIdentityV1::validate)
            && self.sampled_rows.iter().all(|row| {
                row.thread_identity.validate()
                    && row.turn_identity.validate()
                    && row.item_identity.validate()
            })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "recordKind")]
enum DurableRecordV1 {
    #[serde(rename = "segment_header")]
    SegmentHeader(SegmentHeaderRecordV1),
    #[serde(rename = "lifecycle_event")]
    LifecycleEvent(LifecycleRecordV1),
    #[serde(rename = "projection_changed")]
    ProjectionChanged(ProjectionRecordV1),
    #[serde(rename = "shell_notified")]
    ShellNotified(ShellNotifiedRecordV1),
    #[serde(rename = "render_sample")]
    RenderSample(RenderSampleRecordV1),
    #[serde(rename = "capture_gap")]
    CaptureGap(CaptureGapRecordV1),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SegmentHeaderRecordV1 {
    schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    build_identity: Option<String>,
    process_id: u32,
    session_id: String,
    capture_generation: u64,
    segment_sequence: u64,
    started_unix_millis: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LifecycleRecordV1 {
    schema_version: u32,
    capture_generation: u64,
    capture_sequence: u64,
    source_sequence: u64,
    elapsed_micros: u64,
    stage: ActivityDiagnosticLifecycleStageV1,
    category: ActivityDiagnosticLifecycleCategoryV1,
    kind: ActivityDiagnosticLifecycleKindV1,
    thread_identity: ActivityDiagnosticIdentityV1,
    turn_identity: ActivityDiagnosticIdentityV1,
    item_identity: ActivityDiagnosticIdentityV1,
    item_type: ActivityDiagnosticProtocolStringV1,
    item_status: ActivityDiagnosticProtocolStringV1,
    projection_outcome: ActivityDiagnosticProjectionOutcomeV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    before_row_status: Option<ActivityDiagnosticRowStatusV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    after_row_status: Option<ActivityDiagnosticRowStatusV1>,
    affected_row_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionRecordV1 {
    schema_version: u32,
    capture_generation: u64,
    capture_sequence: u64,
    source_sequence: u64,
    elapsed_micros: u64,
    projection_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    newest_lifecycle_sequence: Option<u64>,
    total_row_count: u64,
    running_row_count: u64,
    finished_ok_row_count: u64,
    finished_error_row_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShellNotifiedRecordV1 {
    schema_version: u32,
    capture_generation: u64,
    capture_sequence: u64,
    source_sequence: u64,
    elapsed_micros: u64,
    projection_revision: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RenderRowRecordV1 {
    rendered_index: u64,
    thread_identity: ActivityDiagnosticIdentityV1,
    turn_identity: ActivityDiagnosticIdentityV1,
    item_identity: ActivityDiagnosticIdentityV1,
    row_status: ActivityDiagnosticRowStatusV1,
    status_indicator_theme_role: ActivityDiagnosticIndicatorRoleV1,
    color_source: ActivityDiagnosticColorSourceV1,
    resolved_rgba: [u8; 4],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RenderSampleRecordV1 {
    schema_version: u32,
    capture_generation: u64,
    capture_sequence: u64,
    source_sequence: u64,
    elapsed_micros: u64,
    render_revision: u64,
    projection_revision: u64,
    newest_notified_projection_revision: u64,
    panel_visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_thread_identity: Option<ActivityDiagnosticIdentityV1>,
    selected_thread_row_count: u64,
    rendered_range_start: u64,
    rendered_range_end: u64,
    overscan_row_count: u64,
    sampled_rows: Vec<RenderRowRecordV1>,
    row_sample_truncated: bool,
    event_bytes: u64,
    event_bytes_truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CaptureGapRecordV1 {
    schema_version: u32,
    capture_generation: u64,
    capture_sequence: u64,
    first_dropped_capture_sequence: u64,
    last_dropped_capture_sequence: u64,
    queue_full_drop_count: u64,
    queue_disconnected_drop_count: u64,
}

impl DurableRecordV1 {
    fn validate(&self) -> bool {
        match self {
            Self::SegmentHeader(header) => {
                header.schema_version == ACTIVITY_CAPTURE_SCHEMA_VERSION
                    && header.capture_generation > 0
                    && header.segment_sequence > 0
                    && !header.session_id.is_empty()
                    && header.session_id.len() <= 64
                    && header
                        .build_identity
                        .as_ref()
                        .is_none_or(|value| value.len() <= BUILD_IDENTITY_BYTE_LIMIT)
            }
            Self::LifecycleEvent(record) => {
                record.schema_version == ACTIVITY_CAPTURE_SCHEMA_VERSION
                    && ActivityDiagnosticLifecycleEventV1::from(record.clone()).validate()
            }
            Self::ProjectionChanged(record) => {
                record.schema_version == ACTIVITY_CAPTURE_SCHEMA_VERSION
                    && ActivityDiagnosticCaptureEventV1::ProjectionChanged(record.clone().into())
                        .validate()
            }
            Self::ShellNotified(record) => record.schema_version == ACTIVITY_CAPTURE_SCHEMA_VERSION,
            Self::RenderSample(record) => {
                record.schema_version == ACTIVITY_CAPTURE_SCHEMA_VERSION
                    && ActivityDiagnosticCaptureEventV1::RenderSample(record.clone().into())
                        .validate()
            }
            Self::CaptureGap(record) => {
                record.schema_version == ACTIVITY_CAPTURE_SCHEMA_VERSION
                    && record.first_dropped_capture_sequence <= record.last_dropped_capture_sequence
                    && (record.queue_full_drop_count > 0
                        || record.queue_disconnected_drop_count > 0)
            }
        }
    }
}

impl From<LifecycleRecordV1> for ActivityDiagnosticLifecycleEventV1 {
    fn from(record: LifecycleRecordV1) -> Self {
        Self {
            source_sequence: record.source_sequence,
            elapsed_micros: record.elapsed_micros,
            stage: record.stage,
            category: record.category,
            kind: record.kind,
            thread_identity: record.thread_identity,
            turn_identity: record.turn_identity,
            item_identity: record.item_identity,
            item_type: record.item_type,
            item_status: record.item_status,
            projection_outcome: record.projection_outcome,
            before_row_status: record.before_row_status,
            after_row_status: record.after_row_status,
            affected_row_count: record.affected_row_count,
        }
    }
}

impl From<ProjectionRecordV1> for ActivityDiagnosticProjectionChangedV1 {
    fn from(record: ProjectionRecordV1) -> Self {
        Self {
            source_sequence: record.source_sequence,
            elapsed_micros: record.elapsed_micros,
            projection_revision: record.projection_revision,
            newest_lifecycle_sequence: record.newest_lifecycle_sequence,
            total_row_count: record.total_row_count,
            running_row_count: record.running_row_count,
            finished_ok_row_count: record.finished_ok_row_count,
            finished_error_row_count: record.finished_error_row_count,
        }
    }
}

impl From<RenderSampleRecordV1> for ActivityDiagnosticRenderSampleV1 {
    fn from(record: RenderSampleRecordV1) -> Self {
        Self {
            source_sequence: record.source_sequence,
            elapsed_micros: record.elapsed_micros,
            render_revision: record.render_revision,
            projection_revision: record.projection_revision,
            newest_notified_projection_revision: record.newest_notified_projection_revision,
            panel_visible: record.panel_visible,
            selected_thread_identity: record.selected_thread_identity,
            selected_thread_row_count: record.selected_thread_row_count,
            rendered_range_start: record.rendered_range_start,
            rendered_range_end: record.rendered_range_end,
            overscan_row_count: record.overscan_row_count,
            sampled_rows: record
                .sampled_rows
                .into_iter()
                .map(|row| ActivityDiagnosticRenderRowV1 {
                    rendered_index: row.rendered_index,
                    thread_identity: row.thread_identity,
                    turn_identity: row.turn_identity,
                    item_identity: row.item_identity,
                    row_status: row.row_status,
                    status_indicator_theme_role: row.status_indicator_theme_role,
                    color_source: row.color_source,
                    resolved_rgba: row.resolved_rgba,
                })
                .collect(),
            row_sample_truncated: record.row_sample_truncated,
            event_bytes: record.event_bytes,
            event_bytes_truncated: record.event_bytes_truncated,
        }
    }
}

fn durable_event_record(
    generation: u64,
    capture_sequence: u64,
    event: ActivityDiagnosticCaptureEventV1,
) -> DurableRecordV1 {
    match event {
        ActivityDiagnosticCaptureEventV1::Lifecycle(event) => {
            DurableRecordV1::LifecycleEvent(LifecycleRecordV1 {
                schema_version: ACTIVITY_CAPTURE_SCHEMA_VERSION,
                capture_generation: generation,
                capture_sequence,
                source_sequence: event.source_sequence,
                elapsed_micros: event.elapsed_micros,
                stage: event.stage,
                category: event.category,
                kind: event.kind,
                thread_identity: event.thread_identity,
                turn_identity: event.turn_identity,
                item_identity: event.item_identity,
                item_type: event.item_type,
                item_status: event.item_status,
                projection_outcome: event.projection_outcome,
                before_row_status: event.before_row_status,
                after_row_status: event.after_row_status,
                affected_row_count: event.affected_row_count,
            })
        }
        ActivityDiagnosticCaptureEventV1::ProjectionChanged(event) => {
            DurableRecordV1::ProjectionChanged(ProjectionRecordV1 {
                schema_version: ACTIVITY_CAPTURE_SCHEMA_VERSION,
                capture_generation: generation,
                capture_sequence,
                source_sequence: event.source_sequence,
                elapsed_micros: event.elapsed_micros,
                projection_revision: event.projection_revision,
                newest_lifecycle_sequence: event.newest_lifecycle_sequence,
                total_row_count: event.total_row_count,
                running_row_count: event.running_row_count,
                finished_ok_row_count: event.finished_ok_row_count,
                finished_error_row_count: event.finished_error_row_count,
            })
        }
        ActivityDiagnosticCaptureEventV1::ShellNotified(event) => {
            DurableRecordV1::ShellNotified(ShellNotifiedRecordV1 {
                schema_version: ACTIVITY_CAPTURE_SCHEMA_VERSION,
                capture_generation: generation,
                capture_sequence,
                source_sequence: event.source_sequence,
                elapsed_micros: event.elapsed_micros,
                projection_revision: event.projection_revision,
            })
        }
        ActivityDiagnosticCaptureEventV1::RenderSample(event) => {
            DurableRecordV1::RenderSample(RenderSampleRecordV1 {
                schema_version: ACTIVITY_CAPTURE_SCHEMA_VERSION,
                capture_generation: generation,
                capture_sequence,
                source_sequence: event.source_sequence,
                elapsed_micros: event.elapsed_micros,
                render_revision: event.render_revision,
                projection_revision: event.projection_revision,
                newest_notified_projection_revision: event.newest_notified_projection_revision,
                panel_visible: event.panel_visible,
                selected_thread_identity: event.selected_thread_identity,
                selected_thread_row_count: event.selected_thread_row_count,
                rendered_range_start: event.rendered_range_start,
                rendered_range_end: event.rendered_range_end,
                overscan_row_count: event.overscan_row_count,
                sampled_rows: event
                    .sampled_rows
                    .into_iter()
                    .map(|row| RenderRowRecordV1 {
                        rendered_index: row.rendered_index,
                        thread_identity: row.thread_identity,
                        turn_identity: row.turn_identity,
                        item_identity: row.item_identity,
                        row_status: row.row_status,
                        status_indicator_theme_role: row.status_indicator_theme_role,
                        color_source: row.color_source,
                        resolved_rgba: row.resolved_rgba,
                    })
                    .collect(),
                row_sample_truncated: event.row_sample_truncated,
                event_bytes: event.event_bytes,
                event_bytes_truncated: event.event_bytes_truncated,
            })
        }
    }
}

#[derive(Clone, Debug)]
struct CapturePaths {
    directory: PathBuf,
    current: PathBuf,
    previous: PathBuf,
    lock: PathBuf,
}

impl CapturePaths {
    fn from_beryl_home(beryl_home: &BerylHomeDir) -> Self {
        let directory = beryl_home.root_dir().join(CAPTURE_DIRECTORY);
        Self {
            current: directory.join(CURRENT_FILE),
            previous: directory.join(PREVIOUS_FILE),
            lock: directory.join(LOCK_FILE),
            directory,
        }
    }
}

struct ExclusiveCaptureLock {
    _file: File,
}

impl ExclusiveCaptureLock {
    #[cfg(target_os = "windows")]
    fn acquire(path: &Path) -> io::Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .share_mode(0)
            .open(path)?;
        Ok(Self { _file: file })
    }

    #[cfg(not(target_os = "windows"))]
    fn acquire(_path: &Path) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Activity capture ownership is supported only on Windows",
        ))
    }
}

#[derive(Clone, Debug)]
struct HeaderContext {
    build_identity: Option<String>,
    process_id: u32,
    session_id: String,
    capture_generation: u64,
    started_unix_millis: u64,
}

impl HeaderContext {
    fn record(&self, segment_sequence: u64) -> DurableRecordV1 {
        DurableRecordV1::SegmentHeader(SegmentHeaderRecordV1 {
            schema_version: ACTIVITY_CAPTURE_SCHEMA_VERSION,
            build_identity: self.build_identity.clone(),
            process_id: self.process_id,
            session_id: self.session_id.clone(),
            capture_generation: self.capture_generation,
            segment_sequence,
            started_unix_millis: self.started_unix_millis,
        })
    }
}

enum SegmentInspection {
    Missing,
    Usable {
        byte_len: u64,
        maximum_segment_sequence: u64,
        repaired_tail: bool,
    },
    Unusable {
        repaired_tail: bool,
    },
    Oversized,
}

struct ActivityCaptureJournal {
    _lock: ExclusiveCaptureLock,
    paths: CapturePaths,
    current: Option<File>,
    current_byte_len: u64,
    next_segment_sequence: u64,
    header_context: HeaderContext,
    repair_count: u64,
    rotation_count: u64,
}

enum JournalWriteOutcome {
    Written,
    Oversized,
}

impl ActivityCaptureJournal {
    fn activate(
        paths: CapturePaths,
        header_context: HeaderContext,
    ) -> Result<Self, ActivityDiagnosticCaptureErrorCategory> {
        fs::create_dir_all(&paths.directory)
            .map_err(|_| ActivityDiagnosticCaptureErrorCategory::Directory)?;
        let lock = ExclusiveCaptureLock::acquire(&paths.lock).map_err(|error| {
            let unavailable = error.kind() == io::ErrorKind::WouldBlock
                || matches!(error.raw_os_error(), Some(32 | 33));
            if unavailable {
                ActivityDiagnosticCaptureErrorCategory::LockUnavailable
            } else {
                ActivityDiagnosticCaptureErrorCategory::Lock
            }
        })?;

        let previous = inspect_segment(&paths.previous)
            .map_err(|_| ActivityDiagnosticCaptureErrorCategory::Recovery)?;
        let current = inspect_segment(&paths.current)
            .map_err(|_| ActivityDiagnosticCaptureErrorCategory::Recovery)?;
        let mut repair_count = 0_u64;
        let mut maximum_segment_sequence = 0_u64;

        match previous {
            SegmentInspection::Missing => {}
            SegmentInspection::Usable {
                maximum_segment_sequence: sequence,
                repaired_tail,
                ..
            } => {
                maximum_segment_sequence = maximum_segment_sequence.max(sequence);
                repair_count = repair_count.saturating_add(u64::from(repaired_tail));
            }
            SegmentInspection::Unusable { repaired_tail } => {
                repair_count = repair_count
                    .saturating_add(u64::from(repaired_tail))
                    .saturating_add(1);
                fs::remove_file(&paths.previous)
                    .map_err(|_| ActivityDiagnosticCaptureErrorCategory::Recovery)?;
            }
            SegmentInspection::Oversized => {
                repair_count = repair_count.saturating_add(1);
                fs::remove_file(&paths.previous)
                    .map_err(|_| ActivityDiagnosticCaptureErrorCategory::Recovery)?;
            }
        }

        let mut current_is_usable = false;
        let mut current_byte_len = 0_u64;
        match current {
            SegmentInspection::Missing => {}
            SegmentInspection::Usable {
                byte_len,
                maximum_segment_sequence: sequence,
                repaired_tail,
            } => {
                current_is_usable = true;
                current_byte_len = byte_len;
                maximum_segment_sequence = maximum_segment_sequence.max(sequence);
                repair_count = repair_count.saturating_add(u64::from(repaired_tail));
            }
            SegmentInspection::Unusable { repaired_tail } => {
                repair_count = repair_count
                    .saturating_add(u64::from(repaired_tail))
                    .saturating_add(1);
            }
            SegmentInspection::Oversized => {
                repair_count = repair_count.saturating_add(1);
            }
        }

        let current_file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(current_is_usable)
            .write(!current_is_usable)
            .truncate(!current_is_usable)
            .open(&paths.current)
            .map_err(|_| ActivityDiagnosticCaptureErrorCategory::Recovery)?;
        let mut journal = Self {
            _lock: lock,
            paths,
            current: Some(current_file),
            current_byte_len,
            next_segment_sequence: maximum_segment_sequence.saturating_add(1).max(1),
            header_context,
            repair_count,
            rotation_count: 0,
        };
        journal
            .append_activation_header(current_is_usable)
            .map_err(|_| ActivityDiagnosticCaptureErrorCategory::Recovery)?;
        Ok(journal)
    }

    fn append_activation_header(&mut self, existing_current: bool) -> io::Result<()> {
        let header = self.take_next_header_line()?;
        if existing_current
            && self.current_byte_len.saturating_add(byte_len(&header))
                > ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY
        {
            self.rotate_with_header(header)
        } else {
            self.write_current(&header)
        }
    }

    fn append_record(
        &mut self,
        record: &DurableRecordV1,
    ) -> Result<JournalWriteOutcome, ActivityDiagnosticCaptureErrorCategory> {
        let line = encode_jsonl(record)
            .map_err(|_| ActivityDiagnosticCaptureErrorCategory::Serialization)?;
        let next_header_sequence = self.next_segment_sequence;
        let next_header = encode_jsonl(&self.header_context.record(next_header_sequence))
            .map_err(|_| ActivityDiagnosticCaptureErrorCategory::Serialization)?;
        if byte_len(&next_header).saturating_add(byte_len(&line))
            > ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY
        {
            return Ok(JournalWriteOutcome::Oversized);
        }
        if self.current_byte_len.saturating_add(byte_len(&line))
            > ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY
        {
            self.next_segment_sequence = self.next_segment_sequence.saturating_add(1);
            self.rotate_with_header(next_header)
                .map_err(|_| ActivityDiagnosticCaptureErrorCategory::Rotation)?;
        }
        self.write_current(&line)
            .map_err(|_| ActivityDiagnosticCaptureErrorCategory::Write)?;
        Ok(JournalWriteOutcome::Written)
    }

    fn take_next_header_line(&mut self) -> io::Result<Vec<u8>> {
        let sequence = self.next_segment_sequence;
        self.next_segment_sequence = self.next_segment_sequence.saturating_add(1);
        encode_jsonl(&self.header_context.record(sequence))
    }

    fn rotate_with_header(&mut self, header: Vec<u8>) -> io::Result<()> {
        self.current.take();
        match fs::remove_file(&self.paths.previous) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(&self.paths.current, &self.paths.previous)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(&self.paths.current)?;
        self.current = Some(file);
        self.current_byte_len = 0;
        self.write_current(&header)?;
        self.rotation_count = self.rotation_count.saturating_add(1);
        Ok(())
    }

    fn write_current(&mut self, line: &[u8]) -> io::Result<()> {
        let file = self
            .current
            .as_mut()
            .ok_or_else(|| io::Error::other("capture current segment is closed"))?;
        file.write_all(line)?;
        file.flush()?;
        self.current_byte_len = self.current_byte_len.saturating_add(byte_len(line));
        Ok(())
    }
}

fn inspect_segment(path: &Path) -> io::Result<SegmentInspection> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(SegmentInspection::Missing);
        }
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.len() > ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY {
        return Ok(SegmentInspection::Oversized);
    }

    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)?;
    let mut repaired_tail = false;
    if !bytes.is_empty() && bytes.last() != Some(&b'\n') {
        let repaired_len = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        file.set_len(repaired_len as u64)?;
        file.seek(SeekFrom::Start(repaired_len as u64))?;
        bytes.truncate(repaired_len);
        repaired_tail = true;
    }
    let byte_len = byte_len(&bytes);
    let mut records = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty());
    let Some(first_line) = records.next() else {
        return Ok(SegmentInspection::Unusable { repaired_tail });
    };
    let first = match parse_supported_record(first_line) {
        Some(DurableRecordV1::SegmentHeader(header)) => header,
        _ => return Ok(SegmentInspection::Unusable { repaired_tail }),
    };
    let mut maximum_segment_sequence = first.segment_sequence;
    for line in records {
        let Some(record) = parse_supported_record(line) else {
            return Ok(SegmentInspection::Unusable { repaired_tail });
        };
        if let DurableRecordV1::SegmentHeader(header) = record {
            maximum_segment_sequence = maximum_segment_sequence.max(header.segment_sequence);
        }
    }
    Ok(SegmentInspection::Usable {
        byte_len,
        maximum_segment_sequence,
        repaired_tail,
    })
}

fn parse_supported_record(line: &[u8]) -> Option<DurableRecordV1> {
    let record: DurableRecordV1 = serde_json::from_slice(line).ok()?;
    record.validate().then_some(record)
}

fn encode_jsonl(record: &DurableRecordV1) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(record).map_err(io::Error::other)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn bounded_build_identity(value: Option<&str>) -> Option<String> {
    value.map(|value| {
        let end = utf8_prefix_end(value, BUILD_IDENTITY_BYTE_LIMIT);
        value[..end].to_string()
    })
}

fn new_header_context(build_identity: Option<&str>, capture_generation: u64) -> HeaderContext {
    let started_unix_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    let random_state = RandomState::new();
    let mut first = random_state.build_hasher();
    (
        std::process::id(),
        capture_generation,
        started_unix_millis,
        &random_state as *const RandomState as usize,
    )
        .hash(&mut first);
    let mut second = random_state.build_hasher();
    (first.finish(), thread::current().id()).hash(&mut second);
    HeaderContext {
        build_identity: bounded_build_identity(build_identity),
        process_id: std::process::id(),
        session_id: format!("{:016x}{:016x}", first.finish(), second.finish()),
        capture_generation,
        started_unix_millis,
    }
}

fn utf8_prefix_end(value: &str, byte_limit: usize) -> usize {
    let mut end = value.len().min(byte_limit);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    end
}

fn byte_len(bytes: &[u8]) -> u64 {
    usize_to_u64_saturated(bytes.len())
}

fn usize_to_u64_saturated(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn journal_omits_a_record_larger_than_an_empty_segment() {
        let temp = tempfile::tempdir().unwrap();
        let home = BerylHomeDir::from_explicit_path(temp.path()).unwrap();
        let paths = CapturePaths::from_beryl_home(&home);
        let header_context = new_header_context(None, 1);
        let mut journal = ActivityCaptureJournal::activate(paths.clone(), header_context).unwrap();
        let before = fs::read(&paths.current).unwrap();
        let oversized = DurableRecordV1::SegmentHeader(SegmentHeaderRecordV1 {
            schema_version: ACTIVITY_CAPTURE_SCHEMA_VERSION,
            build_identity: None,
            process_id: 1,
            session_id: "x".repeat(ACTIVITY_CAPTURE_SEGMENT_BYTE_CAPACITY as usize),
            capture_generation: 1,
            segment_sequence: 2,
            started_unix_millis: 0,
        });

        assert!(matches!(
            journal.append_record(&oversized),
            Ok(JournalWriteOutcome::Oversized)
        ));
        assert_eq!(fs::read(&paths.current).unwrap(), before);
        assert!(!paths.previous.exists());
    }
}
