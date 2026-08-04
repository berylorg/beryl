use std::sync::{Arc, Condvar, Mutex};

mod diagnostics;

const ACCEPTED_READY: u16 = 1 << 0;
const TARGET_READY: u16 = 1 << 1;
const WORKER_RELEASED: u16 = 1 << 2;
const ATTEMPT_RELEASED: u16 = 1 << 3;
const CANCELLATION_LIFECYCLE: u16 = 1 << 4;
const RECOVERY: u16 = 1 << 5;
const CANCELLATION_REQUESTED: u16 = 1 << 6;
const SHUTDOWN: u16 = 1 << 7;
const ACCEPTED_NEXT_READY: u16 = 1 << 8;
const PROJECTION_FLIGHT_RELEASED: u16 = 1 << 9;
const EXECUTION_READY: u16 = 1 << 10;
const WORKER_COMPLETED: u16 = 1 << 11;
const NEXT_WORKER_CAPACITY_RELEASED: u16 = 1 << 12;
const RECOVERED_PENDING_CONTINUE: u16 = 1 << 13;
const SAME_GENERATION_VERIFIED: u16 = 1 << 14;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum AcceptedInputWakeReason {
    AcceptedReady,
    TargetReady,
    WorkerReleased,
    AttemptReleased,
    CancellationLifecycle,
    Recovery,
    CancellationRequested,
    Shutdown,
    AcceptedNextReady,
    ProjectionFlightReleased,
    ExecutionReady,
    WorkerCompleted,
    NextWorkerCapacityReleased,
    RecoveredPendingContinue,
    SameGenerationVerified,
}

impl AcceptedInputWakeReason {
    const fn bit(self) -> u16 {
        match self {
            Self::AcceptedReady => ACCEPTED_READY,
            Self::TargetReady => TARGET_READY,
            Self::WorkerReleased => WORKER_RELEASED,
            Self::AttemptReleased => ATTEMPT_RELEASED,
            Self::CancellationLifecycle => CANCELLATION_LIFECYCLE,
            Self::Recovery => RECOVERY,
            Self::CancellationRequested => CANCELLATION_REQUESTED,
            Self::Shutdown => SHUTDOWN,
            Self::AcceptedNextReady => ACCEPTED_NEXT_READY,
            Self::ProjectionFlightReleased => PROJECTION_FLIGHT_RELEASED,
            Self::ExecutionReady => EXECUTION_READY,
            Self::WorkerCompleted => WORKER_COMPLETED,
            Self::NextWorkerCapacityReleased => NEXT_WORKER_CAPACITY_RELEASED,
            Self::RecoveredPendingContinue => RECOVERED_PENDING_CONTINUE,
            Self::SameGenerationVerified => SAME_GENERATION_VERIFIED,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveSteeringRetryState {
    Ineligible,
    Eligible,
    Parked,
}

/// Content-free bounded-state diagnostics for automatic active steering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedInputSchedulerDiagnostics {
    pub(super) wake_count: u64,
    pub(super) coalesced_wake_count: u64,
    pub(super) startup_recovery_page_reads: u64,
    pub(super) startup_recovery_cases: u64,
    pub(super) startup_active_convergences: u64,
    pub(super) startup_terminal_convergences: u64,
    pub(super) startup_pending_turns: u64,
    pub(super) startup_deferred_compactions: u64,
    pub(super) recovery_handed_off: bool,
    pub(super) verification_pauses: u64,
    pub(super) steering_pass_count: u64,
    pub(super) recovered_pending_pass_count: u64,
    pub(super) recovered_projection_pass_count: u64,
    pub(super) next_pass_count: u64,
    pub(super) steering_source_page_reads: u64,
    pub(super) recovered_pending_source_page_reads: u64,
    pub(super) next_source_page_reads: u64,
    pub(super) steering_candidate_page_reads: u64,
    pub(super) next_candidate_page_reads: u64,
    pub(super) point_reads: u64,
    pub(super) target_misses: u64,
    pub(super) steering_stale_scans: u64,
    pub(super) recovered_pending_stale_scans: u64,
    pub(super) next_stale_scans: u64,
    pub(super) steering_capacity_waits: u64,
    pub(super) recovered_pending_capacity_waits: u64,
    pub(super) next_capacity_waits: u64,
    pub(super) attempt_waits: u64,
    pub(super) recovered_pending_flight_waits: u64,
    pub(super) recovered_projection_flight_waits: u64,
    pub(super) next_flight_waits: u64,
    pub(super) recovered_pending_execution_unavailable: u64,
    pub(super) recovered_projection_execution_unavailable: u64,
    pub(super) recovered_projection_staged: u64,
    pub(super) recovered_projection_retained: usize,
    pub(super) recovered_projection_high_water: usize,
    pub(super) recovered_projection_requeues: u64,
    pub(super) next_execution_unavailable: u64,
    pub(super) workers_active: usize,
    pub(super) workers_high_water: usize,
    pub(super) workers_started: u64,
    pub(super) workers_joined: u64,
    pub(super) steering_retained_source_cursor: bool,
    pub(super) recovered_pending_retained_source_cursor: bool,
    pub(super) steering_retained_candidate_cursor: bool,
    pub(super) next_retained_source_cursor: bool,
    pub(super) next_retained_candidate_cursor: bool,
    pub(super) retry_state: ActiveSteeringRetryState,
    pub(super) stopped: bool,
    pub(super) fatal: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::cas_projection) struct StartupRecoveryDiagnostics {
    pub(in crate::cas_projection) page_reads: u64,
    pub(in crate::cas_projection) cases: u64,
    pub(in crate::cas_projection) active_convergences: u64,
    pub(in crate::cas_projection) terminal_convergences: u64,
    pub(in crate::cas_projection) pending_turns: u64,
    pub(in crate::cas_projection) deferred_compactions: u64,
}

#[derive(Clone)]
pub(in crate::cas_projection) struct AcceptedInputSchedulerSignal {
    inner: Arc<SignalInner>,
}

impl std::fmt::Debug for AcceptedInputSchedulerSignal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AcceptedInputSchedulerSignal")
            .finish_non_exhaustive()
    }
}

struct SignalInner {
    state: Mutex<SignalState>,
    changed: Condvar,
}

struct SignalState {
    pending: u16,
    shutdown: bool,
    diagnostics: AcceptedInputSchedulerDiagnostics,
}

#[derive(Clone, Copy)]
pub(super) struct WakeBatch {
    bits: u16,
    shutdown: bool,
}

impl WakeBatch {
    pub(super) const fn opens_steering_pass(self) -> bool {
        self.bits
            & (ACCEPTED_READY
                | TARGET_READY
                | WORKER_RELEASED
                | ATTEMPT_RELEASED
                | CANCELLATION_LIFECYCLE
                | RECOVERY
                | CANCELLATION_REQUESTED
                | SAME_GENERATION_VERIFIED)
            != 0
    }

    pub(super) const fn opens_retry_pass(self) -> bool {
        self.bits & (CANCELLATION_LIFECYCLE | RECOVERY) != 0
    }

    pub(super) const fn shutdown(self) -> bool {
        self.shutdown || self.bits & SHUTDOWN != 0
    }

    pub(super) const fn opens_next_pass(self) -> bool {
        self.bits
            & (ACCEPTED_NEXT_READY
                | EXECUTION_READY
                | CANCELLATION_LIFECYCLE
                | RECOVERY
                | CANCELLATION_REQUESTED
                | SAME_GENERATION_VERIFIED)
            != 0
    }

    pub(super) const fn restarts_recovered_pending_pass(self) -> bool {
        self.bits & (RECOVERY | EXECUTION_READY | SAME_GENERATION_VERIFIED) != 0
    }

    pub(super) const fn continues_recovered_pending_pass(self) -> bool {
        self.bits & RECOVERED_PENDING_CONTINUE != 0
    }

    pub(super) const fn projection_flight_released(self) -> bool {
        self.bits & PROJECTION_FLIGHT_RELEASED != 0
    }

    pub(super) const fn execution_ready(self) -> bool {
        self.bits & EXECUTION_READY != 0
    }

    pub(super) const fn next_worker_capacity_released(self) -> bool {
        self.bits & NEXT_WORKER_CAPACITY_RELEASED != 0
    }

    pub(super) const fn worker_completed(self) -> bool {
        self.bits & WORKER_COMPLETED != 0
    }

    pub(super) const fn same_generation_verified(self) -> bool {
        self.bits & SAME_GENERATION_VERIFIED != 0
    }
}

impl AcceptedInputSchedulerSignal {
    pub(in crate::cas_projection) fn new() -> Self {
        Self {
            inner: Arc::new(SignalInner {
                state: Mutex::new(SignalState {
                    pending: 0,
                    shutdown: false,
                    diagnostics: AcceptedInputSchedulerDiagnostics {
                        wake_count: 0,
                        coalesced_wake_count: 0,
                        startup_recovery_page_reads: 0,
                        startup_recovery_cases: 0,
                        startup_active_convergences: 0,
                        startup_terminal_convergences: 0,
                        startup_pending_turns: 0,
                        startup_deferred_compactions: 0,
                        recovery_handed_off: false,
                        verification_pauses: 0,
                        steering_pass_count: 0,
                        recovered_pending_pass_count: 0,
                        recovered_projection_pass_count: 0,
                        next_pass_count: 0,
                        steering_source_page_reads: 0,
                        recovered_pending_source_page_reads: 0,
                        next_source_page_reads: 0,
                        steering_candidate_page_reads: 0,
                        next_candidate_page_reads: 0,
                        point_reads: 0,
                        target_misses: 0,
                        steering_stale_scans: 0,
                        recovered_pending_stale_scans: 0,
                        next_stale_scans: 0,
                        steering_capacity_waits: 0,
                        recovered_pending_capacity_waits: 0,
                        next_capacity_waits: 0,
                        attempt_waits: 0,
                        recovered_pending_flight_waits: 0,
                        recovered_projection_flight_waits: 0,
                        next_flight_waits: 0,
                        recovered_pending_execution_unavailable: 0,
                        recovered_projection_execution_unavailable: 0,
                        recovered_projection_staged: 0,
                        recovered_projection_retained: 0,
                        recovered_projection_high_water: 0,
                        recovered_projection_requeues: 0,
                        next_execution_unavailable: 0,
                        workers_active: 0,
                        workers_high_water: 0,
                        workers_started: 0,
                        workers_joined: 0,
                        steering_retained_source_cursor: false,
                        recovered_pending_retained_source_cursor: false,
                        steering_retained_candidate_cursor: false,
                        next_retained_source_cursor: false,
                        next_retained_candidate_cursor: false,
                        retry_state: ActiveSteeringRetryState::Ineligible,
                        stopped: false,
                        fatal: false,
                    },
                }),
                changed: Condvar::new(),
            }),
        }
    }

    pub(in crate::cas_projection) fn wake(&self, reason: AcceptedInputWakeReason) {
        self.wake_bits(reason.bit());
    }

    pub(in crate::cas_projection) fn wake_worker_release(
        &self,
        steering: bool,
        scheduled_ordinary: bool,
    ) {
        let mut bits = 0;
        if steering {
            bits |= WORKER_RELEASED;
        }
        if scheduled_ordinary {
            bits |= NEXT_WORKER_CAPACITY_RELEASED;
        }
        if bits != 0 {
            self.wake_bits(bits);
        }
    }

    fn wake_bits(&self, bits: u16) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let new = bits & !state.pending;
        let coalesced = bits & state.pending;
        state.pending |= bits;
        state.diagnostics.wake_count = state
            .diagnostics
            .wake_count
            .saturating_add(u64::from(new.count_ones()));
        state.diagnostics.coalesced_wake_count = state
            .diagnostics
            .coalesced_wake_count
            .saturating_add(u64::from(coalesced.count_ones()));
        drop(state);
        self.inner.changed.notify_one();
    }

    pub(in crate::cas_projection) fn hand_off_recovery(
        &self,
        recovery: StartupRecoveryDiagnostics,
    ) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        debug_assert!(!state.diagnostics.recovery_handed_off);
        state.diagnostics.startup_recovery_page_reads = recovery.page_reads;
        state.diagnostics.startup_recovery_cases = recovery.cases;
        state.diagnostics.startup_active_convergences = recovery.active_convergences;
        state.diagnostics.startup_terminal_convergences = recovery.terminal_convergences;
        state.diagnostics.startup_pending_turns = recovery.pending_turns;
        state.diagnostics.startup_deferred_compactions = recovery.deferred_compactions;
        state.diagnostics.recovery_handed_off = true;
        let bit = AcceptedInputWakeReason::Recovery.bit();
        if state.pending & bit == 0 {
            state.pending |= bit;
            state.diagnostics.wake_count = state.diagnostics.wake_count.saturating_add(1);
        } else {
            state.diagnostics.coalesced_wake_count =
                state.diagnostics.coalesced_wake_count.saturating_add(1);
        }
        drop(state);
        self.inner.changed.notify_one();
    }

    pub(in crate::cas_projection) fn record_recovered_projection_stage(&self, count: usize) {
        self.update_diagnostics(|diagnostics| {
            diagnostics.recovered_projection_staged = u64::try_from(count).unwrap_or(u64::MAX);
            diagnostics.recovered_projection_retained = count;
            diagnostics.recovered_projection_high_water = count;
        });
    }

    pub(super) fn record_recovered_projection_dequeued(&self, count: usize) {
        self.update_diagnostics(|diagnostics| {
            diagnostics.recovered_projection_retained = diagnostics
                .recovered_projection_retained
                .saturating_sub(count);
        });
    }

    pub(super) fn record_recovered_projection_requeued(&self) {
        self.update_diagnostics(|diagnostics| {
            diagnostics.recovered_projection_retained =
                diagnostics.recovered_projection_retained.saturating_add(1);
            diagnostics.recovered_projection_high_water = diagnostics
                .recovered_projection_high_water
                .max(diagnostics.recovered_projection_retained);
            diagnostics.recovered_projection_requeues =
                diagnostics.recovered_projection_requeues.saturating_add(1);
        });
    }

    pub(in crate::cas_projection) fn request_shutdown(&self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.shutdown = true;
        state.pending |= SHUTDOWN;
        drop(state);
        self.inner.changed.notify_all();
    }

    pub(super) fn wait(&self) -> WakeBatch {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while state.pending == 0 && !state.shutdown {
            state = self
                .inner
                .changed
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
        let bits = std::mem::take(&mut state.pending);
        WakeBatch {
            bits,
            shutdown: state.shutdown,
        }
    }

    pub(super) fn is_shutdown(&self) -> bool {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .shutdown
    }

    pub(in crate::cas_projection) fn diagnostics(&self) -> AcceptedInputSchedulerDiagnostics {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .diagnostics
    }

    pub(super) fn update_diagnostics(
        &self,
        update: impl FnOnce(&mut AcceptedInputSchedulerDiagnostics),
    ) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        update(&mut state.diagnostics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_wakes_coalesce_without_promoting_ordinary_reasons() {
        let signal = AcceptedInputSchedulerSignal::new();
        signal.wake(AcceptedInputWakeReason::AcceptedReady);
        signal.wake(AcceptedInputWakeReason::AcceptedReady);
        signal.wake(AcceptedInputWakeReason::TargetReady);
        signal.wake(AcceptedInputWakeReason::TargetReady);
        signal.wake(AcceptedInputWakeReason::AcceptedNextReady);
        signal.wake(AcceptedInputWakeReason::AcceptedNextReady);
        signal.wake(AcceptedInputWakeReason::ProjectionFlightReleased);
        signal.wake(AcceptedInputWakeReason::ProjectionFlightReleased);
        signal.wake(AcceptedInputWakeReason::ExecutionReady);
        signal.wake(AcceptedInputWakeReason::ExecutionReady);
        signal.wake(AcceptedInputWakeReason::WorkerCompleted);
        signal.wake(AcceptedInputWakeReason::WorkerCompleted);
        signal.wake(AcceptedInputWakeReason::NextWorkerCapacityReleased);
        signal.wake(AcceptedInputWakeReason::NextWorkerCapacityReleased);
        signal.wake(AcceptedInputWakeReason::RecoveredPendingContinue);
        signal.wake(AcceptedInputWakeReason::RecoveredPendingContinue);
        signal.wake(AcceptedInputWakeReason::SameGenerationVerified);
        signal.wake(AcceptedInputWakeReason::SameGenerationVerified);

        let diagnostics = signal.diagnostics();
        assert_eq!(diagnostics.wake_count(), 9);
        assert_eq!(diagnostics.coalesced_wake_count(), 9);
        let wake = signal.wait();
        assert!(!wake.opens_retry_pass());
        assert!(wake.opens_next_pass());
        assert!(wake.opens_steering_pass());
        assert!(wake.restarts_recovered_pending_pass());
        assert!(wake.same_generation_verified());
        assert!(wake.next_worker_capacity_released());
        assert!(wake.continues_recovered_pending_pass());

        signal.wake(AcceptedInputWakeReason::CancellationLifecycle);
        signal.wake(AcceptedInputWakeReason::CancellationLifecycle);
        let diagnostics = signal.diagnostics();
        assert_eq!(diagnostics.wake_count(), 10);
        assert_eq!(diagnostics.coalesced_wake_count(), 10);
        assert!(signal.wait().opens_retry_pass());
    }

    #[test]
    fn same_generation_verified_resumes_every_lane_without_retry_eligibility() {
        let signal = AcceptedInputSchedulerSignal::new();
        signal.wake(AcceptedInputWakeReason::SameGenerationVerified);

        let wake = signal.wait();
        assert!(wake.opens_steering_pass());
        assert!(wake.opens_next_pass());
        assert!(wake.restarts_recovered_pending_pass());
        assert!(wake.same_generation_verified());
        assert!(!wake.opens_retry_pass());
        assert!(!wake.execution_ready());
    }

    #[test]
    fn recovery_handoff_publishes_diagnostics_and_one_typed_wake() {
        let signal = AcceptedInputSchedulerSignal::new();
        signal.hand_off_recovery(StartupRecoveryDiagnostics {
            page_reads: 3,
            cases: 5,
            active_convergences: 1,
            terminal_convergences: 2,
            pending_turns: 1,
            deferred_compactions: 1,
        });

        let diagnostics = signal.diagnostics();
        assert!(diagnostics.recovery_handed_off());
        assert_eq!(diagnostics.startup_recovery_page_reads(), 3);
        assert_eq!(diagnostics.startup_recovery_cases(), 5);
        assert_eq!(diagnostics.startup_active_convergences(), 1);
        assert_eq!(diagnostics.startup_terminal_convergences(), 2);
        assert_eq!(diagnostics.startup_pending_turns(), 1);
        assert_eq!(diagnostics.startup_deferred_compactions(), 1);

        let wake = signal.wait();
        assert!(wake.opens_retry_pass());
        assert!(wake.restarts_recovered_pending_pass());
        assert!(wake.opens_next_pass());
    }

    #[test]
    fn worker_completion_alone_does_not_open_another_next_turn_pass() {
        let signal = AcceptedInputSchedulerSignal::new();
        signal.wake(AcceptedInputWakeReason::WorkerCompleted);

        let wake = signal.wait();
        assert!(!wake.opens_steering_pass());
        assert!(!wake.opens_retry_pass());
        assert!(!wake.opens_next_pass());
        assert!(!wake.next_worker_capacity_released());
    }

    #[test]
    fn combined_worker_release_preserves_both_lane_facts_atomically() {
        let signal = AcceptedInputSchedulerSignal::new();
        signal.wake_worker_release(true, true);

        let wake = signal.wait();
        assert!(wake.opens_steering_pass());
        assert!(wake.next_worker_capacity_released());
        assert_eq!(signal.diagnostics().wake_count(), 2);
        assert_eq!(signal.diagnostics().coalesced_wake_count(), 0);
    }

    #[test]
    fn shutdown_wakes_a_blocked_scheduler_wait() {
        let signal = AcceptedInputSchedulerSignal::new();
        let waiter = signal.clone();
        let blocked = std::thread::spawn(move || waiter.wait());

        signal.request_shutdown();
        let wake = blocked.join().unwrap();
        assert!(wake.shutdown());
        assert!(signal.is_shutdown());
    }
}
