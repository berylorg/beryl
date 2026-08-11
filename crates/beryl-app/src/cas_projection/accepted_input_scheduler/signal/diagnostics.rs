use super::{AcceptedInputSchedulerDiagnostics, ActiveSteeringRetryState};

impl AcceptedInputSchedulerDiagnostics {
    #[must_use]
    pub const fn wake_count(self) -> u64 {
        self.wake_count
    }

    #[must_use]
    pub const fn coalesced_wake_count(self) -> u64 {
        self.coalesced_wake_count
    }

    #[must_use]
    pub const fn pass_count(self) -> u64 {
        self.steering_pass_count
            .saturating_add(self.recovered_pending_pass_count)
            .saturating_add(self.next_pass_count)
    }

    #[must_use]
    pub const fn startup_recovery_page_reads(self) -> u64 {
        self.startup_recovery_page_reads
    }

    #[must_use]
    pub const fn startup_recovery_cases(self) -> u64 {
        self.startup_recovery_cases
    }

    #[must_use]
    pub const fn startup_active_convergences(self) -> u64 {
        self.startup_active_convergences
    }

    #[must_use]
    pub const fn startup_terminal_convergences(self) -> u64 {
        self.startup_terminal_convergences
    }

    #[must_use]
    pub const fn startup_pending_turns(self) -> u64 {
        self.startup_pending_turns
    }

    #[must_use]
    pub const fn startup_deferred_compactions(self) -> u64 {
        self.startup_deferred_compactions
    }

    #[must_use]
    pub const fn recovery_handed_off(self) -> bool {
        self.recovery_handed_off
    }

    #[must_use]
    pub const fn steering_pass_count(self) -> u64 {
        self.steering_pass_count
    }

    #[must_use]
    pub const fn next_pass_count(self) -> u64 {
        self.next_pass_count
    }

    #[must_use]
    pub const fn recovered_pending_pass_count(self) -> u64 {
        self.recovered_pending_pass_count
    }

    #[must_use]
    pub const fn source_page_reads(self) -> u64 {
        self.steering_source_page_reads
            .saturating_add(self.recovered_pending_source_page_reads)
            .saturating_add(self.next_source_page_reads)
    }

    #[must_use]
    pub const fn steering_source_page_reads(self) -> u64 {
        self.steering_source_page_reads
    }

    #[must_use]
    pub const fn next_source_page_reads(self) -> u64 {
        self.next_source_page_reads
    }

    #[must_use]
    pub const fn recovered_pending_source_page_reads(self) -> u64 {
        self.recovered_pending_source_page_reads
    }

    #[must_use]
    pub const fn candidate_page_reads(self) -> u64 {
        self.steering_candidate_page_reads
            .saturating_add(self.next_candidate_page_reads)
    }

    #[must_use]
    pub const fn steering_candidate_page_reads(self) -> u64 {
        self.steering_candidate_page_reads
    }

    #[must_use]
    pub const fn next_candidate_page_reads(self) -> u64 {
        self.next_candidate_page_reads
    }

    #[must_use]
    pub const fn point_reads(self) -> u64 {
        self.point_reads
    }

    #[must_use]
    pub const fn target_misses(self) -> u64 {
        self.target_misses
    }

    #[must_use]
    pub const fn stale_scans(self) -> u64 {
        self.steering_stale_scans
            .saturating_add(self.recovered_pending_stale_scans)
            .saturating_add(self.next_stale_scans)
    }

    #[must_use]
    pub const fn steering_stale_scans(self) -> u64 {
        self.steering_stale_scans
    }

    #[must_use]
    pub const fn next_stale_scans(self) -> u64 {
        self.next_stale_scans
    }

    #[must_use]
    pub const fn recovered_pending_stale_scans(self) -> u64 {
        self.recovered_pending_stale_scans
    }

    #[must_use]
    pub const fn capacity_waits(self) -> u64 {
        self.steering_capacity_waits
            .saturating_add(self.recovered_pending_capacity_waits)
            .saturating_add(self.next_capacity_waits)
    }

    #[must_use]
    pub const fn steering_capacity_waits(self) -> u64 {
        self.steering_capacity_waits
    }

    #[must_use]
    pub const fn next_capacity_waits(self) -> u64 {
        self.next_capacity_waits
    }

    #[must_use]
    pub const fn recovered_pending_capacity_waits(self) -> u64 {
        self.recovered_pending_capacity_waits
    }

    #[must_use]
    pub const fn attempt_waits(self) -> u64 {
        self.attempt_waits
    }

    #[must_use]
    pub const fn next_flight_waits(self) -> u64 {
        self.next_flight_waits
    }

    #[must_use]
    pub const fn recovered_pending_flight_waits(self) -> u64 {
        self.recovered_pending_flight_waits
    }

    #[must_use]
    pub const fn next_execution_unavailable(self) -> u64 {
        self.next_execution_unavailable
    }

    #[must_use]
    pub const fn recovered_pending_execution_unavailable(self) -> u64 {
        self.recovered_pending_execution_unavailable
    }

    #[must_use]
    pub const fn workers_active(self) -> usize {
        self.workers_active
    }

    #[must_use]
    pub const fn workers_high_water(self) -> usize {
        self.workers_high_water
    }

    #[must_use]
    pub const fn workers_started(self) -> u64 {
        self.workers_started
    }

    #[must_use]
    pub const fn workers_joined(self) -> u64 {
        self.workers_joined
    }

    #[must_use]
    pub const fn retained_source_cursor(self) -> bool {
        self.steering_retained_source_cursor
            || self.recovered_pending_retained_source_cursor
            || self.next_retained_source_cursor
    }

    #[must_use]
    pub const fn steering_retained_source_cursor(self) -> bool {
        self.steering_retained_source_cursor
    }

    #[must_use]
    pub const fn next_retained_source_cursor(self) -> bool {
        self.next_retained_source_cursor
    }

    #[must_use]
    pub const fn recovered_pending_retained_source_cursor(self) -> bool {
        self.recovered_pending_retained_source_cursor
    }

    #[must_use]
    pub const fn retained_candidate_cursor(self) -> bool {
        self.steering_retained_candidate_cursor || self.next_retained_candidate_cursor
    }

    #[must_use]
    pub const fn steering_retained_candidate_cursor(self) -> bool {
        self.steering_retained_candidate_cursor
    }

    #[must_use]
    pub const fn next_retained_candidate_cursor(self) -> bool {
        self.next_retained_candidate_cursor
    }

    /// The mounted scheduler has no timer-eligible retry cause.
    #[must_use]
    pub const fn armed_retry_timers(self) -> usize {
        0
    }

    #[must_use]
    pub const fn retry_state(self) -> ActiveSteeringRetryState {
        self.retry_state
    }

    #[must_use]
    pub const fn stopped(self) -> bool {
        self.stopped
    }

    #[must_use]
    pub const fn fatal(self) -> bool {
        self.fatal
    }
}
