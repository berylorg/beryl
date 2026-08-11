use std::sync::{Condvar, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveSteeringRetryState {
    Ineligible,
    Eligible,
    Parked,
}

/// Content-free bounded-state diagnostics for automatic active steering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedInputSchedulerDiagnostics {
    pub(in super::super) wake_count: u64,
    pub(in super::super) coalesced_wake_count: u64,
    pub(in super::super) startup_recovery_page_reads: u64,
    pub(in super::super) startup_recovery_cases: u64,
    pub(in super::super) startup_active_convergences: u64,
    pub(in super::super) startup_terminal_convergences: u64,
    pub(in super::super) startup_pending_turns: u64,
    pub(in super::super) startup_deferred_compactions: u64,
    pub(in super::super) recovery_handed_off: bool,
    pub(in super::super) steering_pass_count: u64,
    pub(in super::super) recovered_pending_pass_count: u64,
    pub(in super::super) next_pass_count: u64,
    pub(in super::super) steering_source_page_reads: u64,
    pub(in super::super) recovered_pending_source_page_reads: u64,
    pub(in super::super) next_source_page_reads: u64,
    pub(in super::super) steering_candidate_page_reads: u64,
    pub(in super::super) next_candidate_page_reads: u64,
    pub(in super::super) point_reads: u64,
    pub(in super::super) target_misses: u64,
    pub(in super::super) steering_stale_scans: u64,
    pub(in super::super) recovered_pending_stale_scans: u64,
    pub(in super::super) next_stale_scans: u64,
    pub(in super::super) steering_capacity_waits: u64,
    pub(in super::super) recovered_pending_capacity_waits: u64,
    pub(in super::super) next_capacity_waits: u64,
    pub(in super::super) attempt_waits: u64,
    pub(in super::super) recovered_pending_flight_waits: u64,
    pub(in super::super) next_flight_waits: u64,
    pub(in super::super) recovered_pending_execution_unavailable: u64,
    pub(in super::super) next_execution_unavailable: u64,
    pub(in super::super) workers_active: usize,
    pub(in super::super) workers_high_water: usize,
    pub(in super::super) workers_started: u64,
    pub(in super::super) workers_joined: u64,
    pub(in super::super) steering_retained_source_cursor: bool,
    pub(in super::super) recovered_pending_retained_source_cursor: bool,
    pub(in super::super) steering_retained_candidate_cursor: bool,
    pub(in super::super) next_retained_source_cursor: bool,
    pub(in super::super) next_retained_candidate_cursor: bool,
    pub(in super::super) retry_state: ActiveSteeringRetryState,
    pub(in super::super) stopped: bool,
    pub(in super::super) fatal: bool,
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

pub(super) struct SignalInner {
    pub(super) state: Mutex<SignalState>,
    pub(super) changed: Condvar,
}

pub(super) struct SignalState {
    pub(super) pending: u16,
    pub(super) shutdown: bool,
    pub(super) diagnostics: AcceptedInputSchedulerDiagnostics,
}

impl SignalState {
    pub(super) fn new() -> Self {
        Self {
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
                steering_pass_count: 0,
                recovered_pending_pass_count: 0,
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
                next_flight_waits: 0,
                recovered_pending_execution_unavailable: 0,
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
        }
    }
}
