use std::sync::Arc;

use super::{
    AcceptedInputSchedulerDiagnostics, AcceptedInputWakeReason, StartupRecoveryDiagnostics,
    state::{SignalInner, SignalState},
    wake::{NEXT_WORKER_CAPACITY_RELEASED, SHUTDOWN, WORKER_RELEASED, WakeBatch},
};

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

impl AcceptedInputSchedulerSignal {
    pub(in crate::cas_projection) fn new() -> Self {
        Self {
            inner: Arc::new(SignalInner {
                state: std::sync::Mutex::new(SignalState::new()),
                changed: std::sync::Condvar::new(),
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

    pub(in super::super) fn record_recovered_projection_dequeued(&self, count: usize) {
        self.update_diagnostics(|diagnostics| {
            diagnostics.recovered_projection_retained = diagnostics
                .recovered_projection_retained
                .saturating_sub(count);
        });
    }

    pub(in super::super) fn record_recovered_projection_requeued(&self) {
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

    pub(in super::super) fn wait(&self) -> WakeBatch {
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

    pub(in super::super) fn is_shutdown(&self) -> bool {
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

    pub(in super::super) fn update_diagnostics(
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
