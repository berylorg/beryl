#![allow(dead_code)]

pub use beryl_app::LifecycleYieldOutcome;

#[path = "../src/shell/lifecycle_yield.rs"]
mod lifecycle_yield;
#[path = "../src/shell/notifications.rs"]
mod notifications;

use lifecycle_yield::LifecycleYieldState;
use notifications::{LifecycleNotificationCandidate, LifecycleNotificationKind};

#[test]
fn lifecycle_yield_waits_for_matching_terminal_turn() {
    let mut state = LifecycleYieldState::default();

    assert!(state.record("thread_1", "turn_1", LifecycleYieldOutcome::PhaseContinue));
    assert!(
        state
            .apply_terminal_turn("thread_1", "turn_other")
            .is_none()
    );
    assert!(
        state
            .apply_terminal_turn("thread_other", "turn_1")
            .is_none()
    );

    let applied = state
        .apply_terminal_turn("thread_1", "turn_1")
        .expect("matching terminal turn should consume pending yield");
    assert!(applied.suppresses_ordinary_end_turn_sound());
    assert_eq!(applied.lifecycle_notification_candidate(), None);
    assert!(state.apply_terminal_turn("thread_1", "turn_1").is_none());
}

#[test]
fn lifecycle_yield_keeps_first_recorded_outcome_for_turn() {
    let mut state = LifecycleYieldState::default();

    assert!(state.record(
        "thread_1",
        "turn_1",
        LifecycleYieldOutcome::PhaseNeedsReview
    ));
    assert!(!state.record("thread_1", "turn_1", LifecycleYieldOutcome::PlanComplete));

    let applied = state
        .apply_terminal_turn("thread_1", "turn_1")
        .expect("matching terminal turn should consume pending yield");
    assert!(!applied.suppresses_ordinary_end_turn_sound());
    assert_eq!(applied.lifecycle_notification_candidate(), None);
}

#[test]
fn blocked_yield_emits_operator_attention_notification() {
    let mut state = LifecycleYieldState::default();

    assert!(state.record(
        "thread_1",
        "turn_1",
        LifecycleYieldOutcome::BlockedNeedsOperator
    ));

    let applied = state
        .apply_terminal_turn("thread_1", "turn_1")
        .expect("matching terminal turn should consume pending yield");
    assert!(applied.suppresses_ordinary_end_turn_sound());
    assert_eq!(
        applied.lifecycle_notification_candidate(),
        Some(LifecycleNotificationCandidate::new(
            Some("thread_1".into()),
            Some("turn_1".into()),
            LifecycleNotificationKind::OperatorAttention,
        ))
    );
}

#[test]
fn plan_complete_yield_emits_completion_notification() {
    let mut state = LifecycleYieldState::default();

    assert!(state.record("thread_1", "turn_1", LifecycleYieldOutcome::PlanComplete));

    let applied = state
        .apply_terminal_turn("thread_1", "turn_1")
        .expect("matching terminal turn should consume pending yield");
    assert!(applied.suppresses_ordinary_end_turn_sound());
    assert_eq!(
        applied.lifecycle_notification_candidate(),
        Some(LifecycleNotificationCandidate::new(
            Some("thread_1".into()),
            Some("turn_1".into()),
            LifecycleNotificationKind::PlanComplete,
        ))
    );
}

#[test]
fn lifecycle_yield_can_clear_stale_failed_turn() {
    let mut state = LifecycleYieldState::default();

    assert!(state.record(
        "thread_1",
        "turn_1",
        LifecycleYieldOutcome::BlockedNeedsOperator
    ));
    assert!(state.clear_turn("thread_1", "turn_1"));
    assert!(state.apply_terminal_turn("thread_1", "turn_1").is_none());
}
