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

#[test]
fn home_observer_wakes_handoff_without_turning_maintenance_into_execution() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Wake, Waker},
    };
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let signal = AcceptedInputSchedulerSignal::new();
    let handoff = Arc::new(Count(AtomicUsize::new(0)));
    signal.set_handoff_waker(Some(Waker::from(Arc::clone(&handoff))));
    signal.idle_recheck_waker().wake();
    assert_eq!(handoff.0.load(Ordering::SeqCst), 1);
    assert!(!signal.wait().execution_ready());
    signal.wake(AcceptedInputWakeReason::ExecutionReady);
    assert_eq!(handoff.0.load(Ordering::SeqCst), 1);
}
