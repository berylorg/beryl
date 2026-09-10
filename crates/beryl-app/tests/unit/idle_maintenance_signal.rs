use super::*;

#[test]
fn maintenance_wakes_coalesce_without_opening_dispatch_or_retry_lanes() {
    let signal = AcceptedInputSchedulerSignal::new();
    signal.wake(AcceptedInputWakeReason::IdleRecheck);
    signal.idle_recheck_waker().wake();
    let wake = signal.wait();
    assert!(wake.rechecks_idle_sessions());
    assert!(!wake.opens_steering_pass());
    assert!(!wake.opens_retry_pass());
    assert!(!wake.opens_next_pass());
    assert!(!wake.restarts_recovered_pending_pass());
    assert!(!wake.continues_recovered_pending_pass());
    assert!(!wake.projection_flight_released());
    assert!(!wake.execution_ready());
    assert!(!wake.next_worker_capacity_released());
    assert!(!wake.native_lineage_ready());
    assert!(!wake.native_lineage_route_capacity_released());
    assert!(!wake.shutdown());
    assert_eq!(signal.diagnostics().wake_count(), 1);
    assert_eq!(signal.diagnostics().coalesced_wake_count(), 1);
}

#[test]
fn retained_response_waker_cannot_notify_a_replacement_scheduler() {
    let old = AcceptedInputSchedulerSignal::new();
    let response = old.idle_recheck_waker();
    old.request_shutdown();
    let replacement = AcceptedInputSchedulerSignal::new();
    response.wake();
    let wake = old.wait();
    assert!(wake.shutdown());
    assert!(wake.rechecks_idle_sessions());
    assert_eq!(replacement.diagnostics().wake_count(), 0);
}
