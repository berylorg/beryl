use super::*;
use syndic_storage::FirstAcceptanceKind;

#[test]
fn direct_submission_opens_pending_execution_without_steering_retry_or_idle_maintenance() {
    let signal = AcceptedInputSchedulerSignal::new();
    let kind = FirstAcceptanceKind::Idle {
        user_item_id: beryl_model::SyndicItemId::from_bytes([1; 16]),
    };
    signal.wake_submission(kind);
    signal.wake_submission(kind);
    assert_eq!(signal.diagnostics().wake_count(), 1);
    assert_eq!(signal.diagnostics().coalesced_wake_count(), 1);
    let batch = signal.wait();
    assert!(batch.restarts_recovered_pending_pass());
    assert!(batch.execution_ready());
    assert!(batch.opens_next_pass());
    assert!(!batch.opens_steering_pass());
    assert!(!batch.opens_retry_pass());
    assert!(!batch.rechecks_idle_sessions());
}

#[test]
fn accepted_submission_coalesces_both_eligible_lanes_without_recovery_or_retry() {
    let signal = AcceptedInputSchedulerSignal::new();
    signal.wake_submission(FirstAcceptanceKind::Accepted);
    signal.wake_submission(FirstAcceptanceKind::Accepted);
    assert_eq!(signal.diagnostics().wake_count(), 2);
    assert_eq!(signal.diagnostics().coalesced_wake_count(), 2);
    let batch = signal.wait();
    assert!(batch.opens_steering_pass());
    assert!(batch.opens_next_pass());
    assert!(!batch.restarts_recovered_pending_pass());
    assert!(!batch.execution_ready());
    assert!(!batch.opens_retry_pass());
    assert!(!batch.rechecks_idle_sessions());
}
