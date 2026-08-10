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
