use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, TryLockError,
        atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering},
    },
    time::Instant,
};

use serde::Serialize;
use serde_json::{Value, json};

#[path = "liveness_diagnostics/scheduler.rs"]
mod scheduler;

pub(crate) use scheduler::{
    PollGenerationOutcome, PollGenerationSnapshot, PollScheduleDecision, PollScheduleLane,
    PollSchedulerState,
};

pub(crate) const TRACE_CAPACITY: usize = 64;

pub(crate) const RECEIVER_DISCOVERY: u64 = 1 << 0;
pub(crate) const RECEIVER_WORKSPACE: u64 = 1 << 1;
pub(crate) const RECEIVER_GRAPH: u64 = 1 << 2;
pub(crate) const RECEIVER_GRAPH_THREAD_START: u64 = 1 << 3;
pub(crate) const RECEIVER_TRANSCRIPT_BRANCH: u64 = 1 << 4;
pub(crate) const RECEIVER_TRANSCRIPT_EDIT: u64 = 1 << 5;
pub(crate) const RECEIVER_MEMBER_INVENTORY: u64 = 1 << 6;
pub(crate) const RECEIVER_THREAD_ACTIVATION: u64 = 1 << 7;
pub(crate) const RECEIVER_THREAD_HISTORY: u64 = 1 << 8;
pub(crate) const RECEIVER_IMAGE_LABEL: u64 = 1 << 9;
pub(crate) const RECEIVER_IMAGE_ASSET: u64 = 1 << 10;
pub(crate) const RECEIVER_TURN: u64 = 1 << 11;
pub(crate) const RECEIVER_SHELL_TOOL: u64 = 1 << 12;
pub(crate) const RECEIVER_DIAGNOSTIC_TARGET: u64 = 1 << 13;
pub(crate) const RECEIVER_TURN_STEERING: u64 = 1 << 14;
pub(crate) const RECEIVER_IMAGE_DELIVERY: u64 = 1 << 15;
pub(crate) const RECEIVER_THREAD_TITLE: u64 = 1 << 16;
pub(crate) const RECEIVER_THREAD_TITLE_UPDATE: u64 = 1 << 17;
pub(crate) const RECEIVER_STATUS_OPERATION: u64 = 1 << 18;
pub(crate) const RECEIVER_PHASE_TRANSITION: u64 = 1 << 19;
pub(crate) const RECEIVER_PHASE_DELETION: u64 = 1 << 20;
pub(crate) const RECEIVER_ACCOUNT_RATE_LIMITS: u64 = 1 << 21;
pub(crate) const RECEIVER_TURN_STOP: u64 = 1 << 22;
pub(crate) const RECEIVER_HARD_STOP: u64 = 1 << 23;
pub(crate) const RECEIVER_THEME_CANDIDATE: u64 = 1 << 24;
pub(crate) const RECEIVER_DYNAMIC_THEME: u64 = 1 << 25;
pub(crate) const RECEIVER_PICKER_ACTION: u64 = 1 << 26;
pub(crate) const RECEIVER_RUNTIME_DISTRO: u64 = 1 << 27;
pub(crate) const RECEIVER_WORKSPACE_TITLE: u64 = 1 << 28;
pub(crate) const RECEIVER_SHUTDOWN: u64 = 1 << 29;
pub(crate) const RECEIVER_AUXILIARY_HOLD: u64 = 1 << 30;

const RECEIVER_NAMES: &[(u64, &str)] = &[
    (RECEIVER_DISCOVERY, "discovery"),
    (RECEIVER_WORKSPACE, "workspace"),
    (RECEIVER_GRAPH, "graph"),
    (RECEIVER_GRAPH_THREAD_START, "graph_thread_start"),
    (RECEIVER_TRANSCRIPT_BRANCH, "transcript_branch"),
    (RECEIVER_TRANSCRIPT_EDIT, "transcript_edit"),
    (RECEIVER_MEMBER_INVENTORY, "member_inventory"),
    (RECEIVER_THREAD_ACTIVATION, "thread_activation"),
    (RECEIVER_THREAD_HISTORY, "thread_history"),
    (RECEIVER_IMAGE_LABEL, "image_label"),
    (RECEIVER_IMAGE_ASSET, "image_asset"),
    (RECEIVER_TURN, "turn"),
    (RECEIVER_SHELL_TOOL, "shell_tool"),
    (RECEIVER_DIAGNOSTIC_TARGET, "diagnostic_target"),
    (RECEIVER_TURN_STEERING, "turn_steering"),
    (RECEIVER_IMAGE_DELIVERY, "image_delivery"),
    (RECEIVER_THREAD_TITLE, "thread_title"),
    (RECEIVER_THREAD_TITLE_UPDATE, "thread_title_update"),
    (RECEIVER_STATUS_OPERATION, "status_operation"),
    (RECEIVER_PHASE_TRANSITION, "phase_transition"),
    (RECEIVER_PHASE_DELETION, "phase_deletion"),
    (RECEIVER_ACCOUNT_RATE_LIMITS, "account_rate_limits"),
    (RECEIVER_TURN_STOP, "turn_stop"),
    (RECEIVER_HARD_STOP, "hard_stop"),
    (RECEIVER_THEME_CANDIDATE, "theme_candidate"),
    (RECEIVER_DYNAMIC_THEME, "dynamic_theme"),
    (RECEIVER_PICKER_ACTION, "picker_action"),
    (RECEIVER_RUNTIME_DISTRO, "runtime_distro"),
    (RECEIVER_WORKSPACE_TITLE, "workspace_title"),
    (RECEIVER_SHUTDOWN, "shutdown"),
    (RECEIVER_AUXILIARY_HOLD, "auxiliary_hold"),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LivenessStage {
    None,
    DiagnosticTarget,
    ShellDynamicTool,
    TurnUpdates,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LivenessTransition {
    TimerArm,
    TimerFire,
    TimerRetry,
    TimerRelease,
    TimerStale,
    TimerUnavailable,
    WindowUpdateAttempt,
    WindowUpdateOutcome,
    ViewUpdateAttempt,
    ViewUpdateOutcome,
    PollEnter,
    PollExit,
    StageEnter,
    StageExit,
    DiagnosticEnqueue,
    DiagnosticFull,
    DiagnosticTimeout,
    DiagnosticDequeue,
    DiagnosticExpired,
    DiagnosticClaim,
    DiagnosticHandlerEnter,
    DiagnosticHandlerExit,
    DiagnosticResponse,
    DiagnosticRead,
    ShellToolEnqueue,
    ShellToolBusy,
    ShellToolTimeout,
    ShellToolExpired,
    ShellToolClaim,
    ShellToolHandlerEnter,
    ShellToolHandlerExit,
    ShellToolResponse,
    TurnEventIngress,
    TurnUpdateEnqueue,
    TurnUpdateApplyEnter,
    TurnUpdateApplyExit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LivenessCategory {
    FrameTimer,
    ReadyIdleTimer,
    DiagnosticControl,
    ShellDynamicTool,
    ThreadStarted,
    ThreadArchived,
    ThreadUnarchived,
    ThreadDeleted,
    AgentLabelUpdated,
    ThreadStatusChanged,
    ThreadClosed,
    TurnStarted,
    TurnCompleted,
    ItemStarted,
    ItemCompleted,
    AgentMessageDelta,
    ReasoningSummaryPart,
    ReasoningSummaryDelta,
    ReasoningTextDelta,
    CommandOutputDelta,
    FileChangeOutputDelta,
    TokenUsage,
    AccountRateLimits,
    ThreadName,
    ApprovalRequest,
    DynamicToolCall,
    ProtocolError,
    StreamIdle,
    ThreadActivatedUpdate,
    ThreadTitleEligibleUpdate,
    GraphMutationUpdate,
    LifecycleYieldAcceptedUpdate,
    TurnEventUpdate,
    FinishedUpdate,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LivenessFlags {
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) lifecycle_yield_accepted: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) exact_thread_match: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) exact_turn_match: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) terminal: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) finished: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) success: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LivenessTraceEvent {
    generation: u64,
    sequence: u64,
    elapsed_micros: u64,
    stage: LivenessStage,
    transition: LivenessTransition,
    category: LivenessCategory,
    flags: LivenessFlags,
    diagnostic_queue_occupancy: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LivenessHeartbeatSnapshot {
    generation: u64,
    sequence: u64,
    elapsed_micros: u64,
    stage: LivenessStage,
    stage_sequence: u64,
    stage_since_micros: Option<u64>,
    last_timer_arm_micros: Option<u64>,
    last_timer_fire_micros: Option<u64>,
    last_window_update_attempt_micros: Option<u64>,
    last_window_update_outcome: Option<bool>,
    last_view_update_attempt_micros: Option<u64>,
    last_view_update_outcome: Option<bool>,
    last_poll_entry_micros: Option<u64>,
    last_poll_exit_micros: Option<u64>,
    frame_poll_scheduled: bool,
    frame_poll_generation: Option<u64>,
    frame_poll_outcome: Option<PollGenerationOutcome>,
    frame_poll_last_acknowledged: Option<PollGenerationSnapshot>,
    ready_idle_poll_scheduled: bool,
    ready_idle_poll_generation: Option<u64>,
    ready_idle_poll_outcome: Option<PollGenerationOutcome>,
    ready_idle_poll_last_acknowledged: Option<PollGenerationSnapshot>,
    active_receiver_bits: u64,
    active_receivers: Vec<&'static str>,
    diagnostic_queue_occupancy: usize,
    diagnostic_queue_high_water: usize,
    dropped_trace_events: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LivenessTraceSnapshot {
    capacity: usize,
    trace_available: bool,
    heartbeat: LivenessHeartbeatSnapshot,
    events: Vec<LivenessTraceEvent>,
}

pub(crate) struct ShellLivenessDiagnostics {
    origin: Instant,
    generation: AtomicU64,
    sequence: AtomicU64,
    stage: AtomicU8,
    stage_sequence: AtomicU64,
    stage_since_micros: AtomicU64,
    last_timer_arm_micros: AtomicU64,
    last_timer_fire_micros: AtomicU64,
    last_window_update_attempt_micros: AtomicU64,
    last_window_update_outcome: AtomicU8,
    last_view_update_attempt_micros: AtomicU64,
    last_view_update_outcome: AtomicU8,
    last_poll_entry_micros: AtomicU64,
    last_poll_exit_micros: AtomicU64,
    frame_poll_scheduled: AtomicU8,
    frame_poll_generation: AtomicU64,
    frame_poll_outcome: AtomicU8,
    frame_poll_last_generation: AtomicU64,
    frame_poll_last_outcome: AtomicU8,
    ready_idle_poll_scheduled: AtomicU8,
    ready_idle_poll_generation: AtomicU64,
    ready_idle_poll_outcome: AtomicU8,
    ready_idle_poll_last_generation: AtomicU64,
    ready_idle_poll_last_outcome: AtomicU8,
    active_receiver_bits: AtomicU64,
    diagnostic_queue_accounting: Mutex<()>,
    diagnostic_queue_occupancy: AtomicUsize,
    diagnostic_queue_high_water: AtomicUsize,
    dropped_trace_events: AtomicU64,
    trace: Mutex<VecDeque<LivenessTraceEvent>>,
}

static SHARED_LIVENESS: OnceLock<Arc<ShellLivenessDiagnostics>> = OnceLock::new();

pub(crate) struct DiagnosticEnqueueAccounting<'a> {
    diagnostics: &'a ShellLivenessDiagnostics,
    _guard: MutexGuard<'a, ()>,
}

pub(crate) struct DiagnosticDequeueAccounting<'a> {
    diagnostics: &'a ShellLivenessDiagnostics,
    _guard: MutexGuard<'a, ()>,
}

pub(crate) fn shared_liveness() -> &'static Arc<ShellLivenessDiagnostics> {
    SHARED_LIVENESS.get_or_init(|| Arc::new(ShellLivenessDiagnostics::new()))
}

impl Default for ShellLivenessDiagnostics {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellLivenessDiagnostics {
    pub(crate) fn new() -> Self {
        Self {
            origin: Instant::now(),
            generation: AtomicU64::new(0),
            sequence: AtomicU64::new(0),
            stage: AtomicU8::new(stage_code(LivenessStage::None)),
            stage_sequence: AtomicU64::new(0),
            stage_since_micros: AtomicU64::new(0),
            last_timer_arm_micros: AtomicU64::new(0),
            last_timer_fire_micros: AtomicU64::new(0),
            last_window_update_attempt_micros: AtomicU64::new(0),
            last_window_update_outcome: AtomicU8::new(0),
            last_view_update_attempt_micros: AtomicU64::new(0),
            last_view_update_outcome: AtomicU8::new(0),
            last_poll_entry_micros: AtomicU64::new(0),
            last_poll_exit_micros: AtomicU64::new(0),
            frame_poll_scheduled: AtomicU8::new(0),
            frame_poll_generation: AtomicU64::new(0),
            frame_poll_outcome: AtomicU8::new(0),
            frame_poll_last_generation: AtomicU64::new(0),
            frame_poll_last_outcome: AtomicU8::new(0),
            ready_idle_poll_scheduled: AtomicU8::new(0),
            ready_idle_poll_generation: AtomicU64::new(0),
            ready_idle_poll_outcome: AtomicU8::new(0),
            ready_idle_poll_last_generation: AtomicU64::new(0),
            ready_idle_poll_last_outcome: AtomicU8::new(0),
            active_receiver_bits: AtomicU64::new(0),
            diagnostic_queue_accounting: Mutex::new(()),
            diagnostic_queue_occupancy: AtomicUsize::new(0),
            diagnostic_queue_high_water: AtomicUsize::new(0),
            dropped_trace_events: AtomicU64::new(0),
            trace: Mutex::new(VecDeque::with_capacity(TRACE_CAPACITY)),
        }
    }

    pub(crate) fn heartbeat(&self) -> LivenessHeartbeatSnapshot {
        let receiver_bits = self.active_receiver_bits.load(Ordering::Acquire);
        LivenessHeartbeatSnapshot {
            generation: self.generation.load(Ordering::Acquire),
            sequence: self.sequence.load(Ordering::Acquire),
            elapsed_micros: self.elapsed_micros(),
            stage: stage_from_code(self.stage.load(Ordering::Acquire)),
            stage_sequence: self.stage_sequence.load(Ordering::Acquire),
            stage_since_micros: decode_micros(self.stage_since_micros.load(Ordering::Acquire)),
            last_timer_arm_micros: decode_micros(
                self.last_timer_arm_micros.load(Ordering::Acquire),
            ),
            last_timer_fire_micros: decode_micros(
                self.last_timer_fire_micros.load(Ordering::Acquire),
            ),
            last_window_update_attempt_micros: decode_micros(
                self.last_window_update_attempt_micros
                    .load(Ordering::Acquire),
            ),
            last_window_update_outcome: decode_outcome(
                self.last_window_update_outcome.load(Ordering::Acquire),
            ),
            last_view_update_attempt_micros: decode_micros(
                self.last_view_update_attempt_micros.load(Ordering::Acquire),
            ),
            last_view_update_outcome: decode_outcome(
                self.last_view_update_outcome.load(Ordering::Acquire),
            ),
            last_poll_entry_micros: decode_micros(
                self.last_poll_entry_micros.load(Ordering::Acquire),
            ),
            last_poll_exit_micros: decode_micros(
                self.last_poll_exit_micros.load(Ordering::Acquire),
            ),
            frame_poll_scheduled: self.frame_poll_scheduled.load(Ordering::Acquire) != 0,
            frame_poll_generation: decode_generation(
                self.frame_poll_generation.load(Ordering::Acquire),
            ),
            frame_poll_outcome: decode_poll_outcome(
                self.frame_poll_outcome.load(Ordering::Acquire),
            ),
            frame_poll_last_acknowledged: decode_poll_snapshot(
                self.frame_poll_last_generation.load(Ordering::Acquire),
                self.frame_poll_last_outcome.load(Ordering::Acquire),
            ),
            ready_idle_poll_scheduled: self.ready_idle_poll_scheduled.load(Ordering::Acquire) != 0,
            ready_idle_poll_generation: decode_generation(
                self.ready_idle_poll_generation.load(Ordering::Acquire),
            ),
            ready_idle_poll_outcome: decode_poll_outcome(
                self.ready_idle_poll_outcome.load(Ordering::Acquire),
            ),
            ready_idle_poll_last_acknowledged: decode_poll_snapshot(
                self.ready_idle_poll_last_generation.load(Ordering::Acquire),
                self.ready_idle_poll_last_outcome.load(Ordering::Acquire),
            ),
            active_receiver_bits: receiver_bits,
            active_receivers: RECEIVER_NAMES
                .iter()
                .filter_map(|(bit, name)| (receiver_bits & bit != 0).then_some(*name))
                .collect(),
            diagnostic_queue_occupancy: self.diagnostic_queue_occupancy.load(Ordering::Acquire),
            diagnostic_queue_high_water: self.diagnostic_queue_high_water.load(Ordering::Acquire),
            dropped_trace_events: self.dropped_trace_events.load(Ordering::Acquire),
        }
    }

    pub(crate) fn snapshot(&self) -> LivenessTraceSnapshot {
        let trace = self.trace.try_lock();
        let trace_available = trace.is_ok();
        let events = trace
            .map(|events| events.iter().cloned().collect())
            .unwrap_or_default();
        LivenessTraceSnapshot {
            capacity: TRACE_CAPACITY,
            trace_available,
            heartbeat: self.heartbeat(),
            events,
        }
    }

    pub(crate) fn snapshot_value(&self) -> Value {
        serde_json::to_value(self.snapshot()).unwrap_or_else(|_| json!({ "unavailable": true }))
    }

    pub(crate) fn record(
        &self,
        transition: LivenessTransition,
        category: LivenessCategory,
        flags: LivenessFlags,
    ) -> u64 {
        self.record_generation(
            self.generation.load(Ordering::Acquire),
            transition,
            category,
            flags,
        )
    }

    pub(crate) fn record_generation(
        &self,
        generation: u64,
        transition: LivenessTransition,
        category: LivenessCategory,
        flags: LivenessFlags,
    ) -> u64 {
        let sequence = self.sequence.fetch_add(1, Ordering::AcqRel) + 1;
        let event = LivenessTraceEvent {
            generation,
            sequence,
            elapsed_micros: self.elapsed_micros(),
            stage: stage_from_code(self.stage.load(Ordering::Acquire)),
            transition,
            category,
            flags,
            diagnostic_queue_occupancy: self.diagnostic_queue_occupancy.load(Ordering::Acquire),
        };
        if let Ok(mut trace) = self.trace.try_lock() {
            if trace.len() == TRACE_CAPACITY {
                trace.pop_front();
            }
            trace.push_back(event);
        } else {
            self.dropped_trace_events.fetch_add(1, Ordering::Relaxed);
        }
        sequence
    }

    fn elapsed_micros(&self) -> u64 {
        self.origin.elapsed().as_micros().min(u64::MAX as u128) as u64
    }

    fn store_micros(&self, target: &AtomicU64) {
        target.store(encode_micros(self.elapsed_micros()), Ordering::Release);
    }

    pub(crate) fn timer_arm(&self, category: LivenessCategory, generation: u64) {
        self.generation.fetch_max(generation, Ordering::AcqRel);
        self.store_micros(&self.last_timer_arm_micros);
        self.record_generation(
            generation,
            LivenessTransition::TimerArm,
            category,
            LivenessFlags::default(),
        );
    }

    pub(crate) fn timer_fire(&self, category: LivenessCategory, generation: u64) {
        self.store_micros(&self.last_timer_fire_micros);
        self.record_generation(
            generation,
            LivenessTransition::TimerFire,
            category,
            LivenessFlags::default(),
        );
    }

    pub(crate) fn window_update_attempt(&self, category: LivenessCategory, generation: u64) {
        self.store_micros(&self.last_window_update_attempt_micros);
        self.record_generation(
            generation,
            LivenessTransition::WindowUpdateAttempt,
            category,
            LivenessFlags::default(),
        );
    }

    pub(crate) fn window_update_outcome(
        &self,
        category: LivenessCategory,
        generation: u64,
        success: bool,
    ) {
        self.last_window_update_outcome
            .store(encode_outcome(success), Ordering::Release);
        self.record_generation(
            generation,
            LivenessTransition::WindowUpdateOutcome,
            category,
            LivenessFlags {
                success,
                ..LivenessFlags::default()
            },
        );
    }

    pub(crate) fn view_update_attempt(&self, category: LivenessCategory, generation: u64) {
        self.store_micros(&self.last_view_update_attempt_micros);
        self.record_generation(
            generation,
            LivenessTransition::ViewUpdateAttempt,
            category,
            LivenessFlags::default(),
        );
    }

    pub(crate) fn view_update_outcome(
        &self,
        category: LivenessCategory,
        generation: u64,
        success: bool,
    ) {
        self.last_view_update_outcome
            .store(encode_outcome(success), Ordering::Release);
        self.record_generation(
            generation,
            LivenessTransition::ViewUpdateOutcome,
            category,
            LivenessFlags {
                success,
                ..LivenessFlags::default()
            },
        );
    }

    pub(crate) fn timer_retry(&self, category: LivenessCategory, generation: u64) {
        self.record_generation(
            generation,
            LivenessTransition::TimerRetry,
            category,
            LivenessFlags::default(),
        );
    }

    pub(crate) fn timer_release(&self, category: LivenessCategory, generation: u64) {
        self.record_generation(
            generation,
            LivenessTransition::TimerRelease,
            category,
            LivenessFlags::default(),
        );
    }

    pub(crate) fn timer_stale(&self, category: LivenessCategory, generation: u64) {
        self.record_generation(
            generation,
            LivenessTransition::TimerStale,
            category,
            LivenessFlags::default(),
        );
    }

    pub(crate) fn timer_unavailable(&self, category: LivenessCategory, generation: u64) {
        self.record_generation(
            generation,
            LivenessTransition::TimerUnavailable,
            category,
            LivenessFlags::default(),
        );
    }

    pub(crate) fn shell_state(
        &self,
        frame_poll: Option<PollGenerationSnapshot>,
        frame_last_acknowledged: Option<PollGenerationSnapshot>,
        ready_idle_poll: Option<PollGenerationSnapshot>,
        ready_idle_last_acknowledged: Option<PollGenerationSnapshot>,
        active_receiver_bits: u64,
    ) {
        self.poll_scheduler_state(
            frame_poll,
            frame_last_acknowledged,
            ready_idle_poll,
            ready_idle_last_acknowledged,
        );
        self.active_receiver_bits
            .store(active_receiver_bits, Ordering::Release);
    }

    pub(crate) fn poll_scheduler_state(
        &self,
        frame_poll: Option<PollGenerationSnapshot>,
        frame_last_acknowledged: Option<PollGenerationSnapshot>,
        ready_idle_poll: Option<PollGenerationSnapshot>,
        ready_idle_last_acknowledged: Option<PollGenerationSnapshot>,
    ) {
        self.frame_poll_scheduled
            .store(u8::from(frame_poll.is_some()), Ordering::Release);
        store_poll_snapshot(
            frame_poll,
            &self.frame_poll_generation,
            &self.frame_poll_outcome,
        );
        store_poll_snapshot(
            frame_last_acknowledged,
            &self.frame_poll_last_generation,
            &self.frame_poll_last_outcome,
        );
        self.ready_idle_poll_scheduled
            .store(u8::from(ready_idle_poll.is_some()), Ordering::Release);
        store_poll_snapshot(
            ready_idle_poll,
            &self.ready_idle_poll_generation,
            &self.ready_idle_poll_outcome,
        );
        store_poll_snapshot(
            ready_idle_last_acknowledged,
            &self.ready_idle_poll_last_generation,
            &self.ready_idle_poll_last_outcome,
        );
    }

    pub(crate) fn poll_enter(&self) {
        self.store_micros(&self.last_poll_entry_micros);
        self.record(
            LivenessTransition::PollEnter,
            LivenessCategory::FrameTimer,
            LivenessFlags::default(),
        );
    }

    pub(crate) fn poll_exit(&self) {
        self.store_micros(&self.last_poll_exit_micros);
        self.record(
            LivenessTransition::PollExit,
            LivenessCategory::FrameTimer,
            LivenessFlags::default(),
        );
    }

    pub(crate) fn enter_stage(&self, stage: LivenessStage) -> LivenessStageGuard<'_> {
        self.stage.store(stage_code(stage), Ordering::Release);
        self.store_micros(&self.stage_since_micros);
        let sequence = self.record(
            LivenessTransition::StageEnter,
            category_for_stage(stage),
            LivenessFlags::default(),
        );
        self.stage_sequence.store(sequence, Ordering::Release);
        LivenessStageGuard {
            diagnostics: self,
            stage,
        }
    }

    pub(crate) fn diagnostic_enqueue_begin(&self) -> DiagnosticEnqueueAccounting<'_> {
        let guard = self
            .diagnostic_queue_accounting
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        DiagnosticEnqueueAccounting {
            diagnostics: self,
            _guard: guard,
        }
    }

    pub(crate) fn try_diagnostic_dequeue_begin(&self) -> Option<DiagnosticDequeueAccounting<'_>> {
        let guard = match self.diagnostic_queue_accounting.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return None,
        };
        Some(DiagnosticDequeueAccounting {
            diagnostics: self,
            _guard: guard,
        })
    }

    fn decrement_diagnostic_occupancy(&self) {
        let _ = self.diagnostic_queue_occupancy.fetch_update(
            Ordering::AcqRel,
            Ordering::Acquire,
            |occupancy| Some(occupancy.saturating_sub(1)),
        );
    }

    fn update_diagnostic_high_water(&self, occupancy: usize) {
        let mut high_water = self.diagnostic_queue_high_water.load(Ordering::Acquire);
        while occupancy > high_water {
            match self.diagnostic_queue_high_water.compare_exchange_weak(
                high_water,
                occupancy,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(observed) => high_water = observed,
            }
        }
    }
}

impl DiagnosticEnqueueAccounting<'_> {
    pub(crate) fn commit(self) {
        let occupancy = self
            .diagnostics
            .diagnostic_queue_occupancy
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        self.diagnostics.update_diagnostic_high_water(occupancy);
        self.diagnostics.record(
            LivenessTransition::DiagnosticEnqueue,
            LivenessCategory::DiagnosticControl,
            LivenessFlags::default(),
        );
    }

    pub(crate) fn rollback(self, full: bool) {
        if full {
            self.diagnostics.record(
                LivenessTransition::DiagnosticFull,
                LivenessCategory::DiagnosticControl,
                LivenessFlags::default(),
            );
        }
    }
}

impl DiagnosticDequeueAccounting<'_> {
    pub(crate) fn commit(self) {
        self.diagnostics.decrement_diagnostic_occupancy();
        self.diagnostics.record(
            LivenessTransition::DiagnosticDequeue,
            LivenessCategory::DiagnosticControl,
            LivenessFlags::default(),
        );
    }
}

pub(crate) struct LivenessPollGuard<'a> {
    diagnostics: &'a ShellLivenessDiagnostics,
}

impl<'a> LivenessPollGuard<'a> {
    pub(crate) fn enter(diagnostics: &'a ShellLivenessDiagnostics) -> Self {
        diagnostics.poll_enter();
        Self { diagnostics }
    }
}

impl Drop for LivenessPollGuard<'_> {
    fn drop(&mut self) {
        self.diagnostics.poll_exit();
    }
}

pub(crate) struct LivenessStageGuard<'a> {
    diagnostics: &'a ShellLivenessDiagnostics,
    stage: LivenessStage,
}

impl Drop for LivenessStageGuard<'_> {
    fn drop(&mut self) {
        self.diagnostics.record(
            LivenessTransition::StageExit,
            category_for_stage(self.stage),
            LivenessFlags::default(),
        );
        self.diagnostics
            .stage
            .store(stage_code(LivenessStage::None), Ordering::Release);
        self.diagnostics
            .store_micros(&self.diagnostics.stage_since_micros);
    }
}

pub(crate) struct LivenessTransitionGuard<'a> {
    diagnostics: &'a ShellLivenessDiagnostics,
    exit: LivenessTransition,
    category: LivenessCategory,
    flags: LivenessFlags,
}

impl<'a> LivenessTransitionGuard<'a> {
    pub(crate) fn enter(
        diagnostics: &'a ShellLivenessDiagnostics,
        enter: LivenessTransition,
        exit: LivenessTransition,
        category: LivenessCategory,
        flags: LivenessFlags,
    ) -> Self {
        diagnostics.record(enter, category, flags);
        Self {
            diagnostics,
            exit,
            category,
            flags,
        }
    }
}

impl Drop for LivenessTransitionGuard<'_> {
    fn drop(&mut self) {
        self.diagnostics
            .record(self.exit, self.category, self.flags);
    }
}

fn category_for_stage(stage: LivenessStage) -> LivenessCategory {
    match stage {
        LivenessStage::None => LivenessCategory::FrameTimer,
        LivenessStage::DiagnosticTarget => LivenessCategory::DiagnosticControl,
        LivenessStage::ShellDynamicTool => LivenessCategory::ShellDynamicTool,
        LivenessStage::TurnUpdates => LivenessCategory::TurnEventUpdate,
    }
}

fn stage_code(stage: LivenessStage) -> u8 {
    match stage {
        LivenessStage::None => 0,
        LivenessStage::DiagnosticTarget => 1,
        LivenessStage::ShellDynamicTool => 2,
        LivenessStage::TurnUpdates => 3,
    }
}

fn stage_from_code(code: u8) -> LivenessStage {
    match code {
        1 => LivenessStage::DiagnosticTarget,
        2 => LivenessStage::ShellDynamicTool,
        3 => LivenessStage::TurnUpdates,
        _ => LivenessStage::None,
    }
}

fn encode_micros(micros: u64) -> u64 {
    micros.saturating_add(1)
}

fn decode_micros(encoded: u64) -> Option<u64> {
    encoded.checked_sub(1)
}

fn encode_outcome(success: bool) -> u8 {
    if success { 1 } else { 2 }
}

fn decode_outcome(encoded: u8) -> Option<bool> {
    match encoded {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

fn store_poll_snapshot(
    snapshot: Option<PollGenerationSnapshot>,
    generation: &AtomicU64,
    outcome: &AtomicU8,
) {
    generation.store(
        snapshot.map_or(0, |snapshot| snapshot.generation),
        Ordering::Release,
    );
    outcome.store(
        snapshot.map_or(0, |snapshot| encode_poll_outcome(snapshot.outcome)),
        Ordering::Release,
    );
}

fn decode_generation(generation: u64) -> Option<u64> {
    (generation != 0).then_some(generation)
}

fn decode_poll_snapshot(generation: u64, outcome: u8) -> Option<PollGenerationSnapshot> {
    Some(PollGenerationSnapshot {
        generation: decode_generation(generation)?,
        outcome: decode_poll_outcome(outcome)?,
    })
}

fn encode_poll_outcome(outcome: PollGenerationOutcome) -> u8 {
    match outcome {
        PollGenerationOutcome::Armed => 1,
        PollGenerationOutcome::TimerDelivered => 2,
        PollGenerationOutcome::WindowRetryScheduled => 3,
        PollGenerationOutcome::WindowUpdated => 4,
        PollGenerationOutcome::PollDelivered => 5,
        PollGenerationOutcome::Cancelled => 6,
        PollGenerationOutcome::WindowUnavailable => 7,
        PollGenerationOutcome::ViewUnavailable => 8,
    }
}

fn decode_poll_outcome(outcome: u8) -> Option<PollGenerationOutcome> {
    match outcome {
        1 => Some(PollGenerationOutcome::Armed),
        2 => Some(PollGenerationOutcome::TimerDelivered),
        3 => Some(PollGenerationOutcome::WindowRetryScheduled),
        4 => Some(PollGenerationOutcome::WindowUpdated),
        5 => Some(PollGenerationOutcome::PollDelivered),
        6 => Some(PollGenerationOutcome::Cancelled),
        7 => Some(PollGenerationOutcome::WindowUnavailable),
        8 => Some(PollGenerationOutcome::ViewUnavailable),
        _ => None,
    }
}
