#![allow(dead_code)]

#[path = "../src/shell/notifications.rs"]
mod notifications;

use notifications::{
    LifecycleNotificationCandidate, LifecycleNotificationKind, TurnCompletionSoundCandidate,
};

#[test]
fn turn_completion_sound_candidate_carries_turn_identity() {
    assert_eq!(
        TurnCompletionSoundCandidate::new(Some("thread_1".into()), Some("turn_1".into())),
        TurnCompletionSoundCandidate {
            thread_id: Some("thread_1".into()),
            turn_id: Some("turn_1".into())
        }
    );
}

#[test]
fn lifecycle_notification_candidate_carries_distinct_event_kind() {
    assert_eq!(
        LifecycleNotificationCandidate::new(
            Some("thread_1".into()),
            Some("turn_1".into()),
            LifecycleNotificationKind::PlanComplete,
        ),
        LifecycleNotificationCandidate {
            thread_id: Some("thread_1".into()),
            turn_id: Some("turn_1".into()),
            kind: LifecycleNotificationKind::PlanComplete,
        }
    );
}
