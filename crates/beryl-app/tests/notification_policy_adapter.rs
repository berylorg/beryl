#![allow(dead_code)]

#[path = "../src/shell/notification_policy.rs"]
mod notification_policy;
#[path = "../src/shell/notification_policy_adapter.rs"]
mod notification_policy_adapter;
#[path = "../src/shell/notifications.rs"]
mod notifications;

use std::path::PathBuf;

use notification_policy::{
    AttentionTriggerState, BerylWindowFocusState, NotificationCandidateKind,
    NotificationPlaybackRequest, NotificationPolicyDecision, NotificationSuppressionReason,
    PlatformAttentionState,
};
use notifications::LifecycleNotificationKind;

#[test]
fn adapter_allows_focused_end_turn_when_platform_attention_is_active() {
    let mut platform_attention = PlatformAttentionState::inactive();
    platform_attention.local_input_idle = AttentionTriggerState::Active;

    assert_eq!(
        notification_policy_adapter::terminal_parent_turn_notification_decision(
            NotificationCandidateKind::OrdinaryEndTurn,
            Some(sound_path()),
            BerylWindowFocusState::Focused,
            platform_attention,
        ),
        NotificationPolicyDecision::Play(NotificationPlaybackRequest::EndTurn {
            path: sound_path()
        })
    );
}

#[test]
fn adapter_keeps_focused_unknown_platform_state_suppressed() {
    assert_eq!(
        notification_policy_adapter::terminal_parent_turn_notification_decision(
            NotificationCandidateKind::OrdinaryEndTurn,
            Some(sound_path()),
            BerylWindowFocusState::Focused,
            PlatformAttentionState::unknown(),
        ),
        NotificationPolicyDecision::Suppress(
            NotificationSuppressionReason::NoAttentionTriggerActive
        )
    );
}

#[test]
fn adapter_maps_lifecycle_kind_to_lifecycle_playback_request() {
    assert_eq!(
        notification_policy_adapter::terminal_parent_turn_notification_decision(
            NotificationCandidateKind::Lifecycle(LifecycleNotificationKind::OperatorAttention),
            Some(sound_path()),
            BerylWindowFocusState::Unfocused,
            PlatformAttentionState::unknown(),
        ),
        NotificationPolicyDecision::Play(NotificationPlaybackRequest::Lifecycle {
            kind: LifecycleNotificationKind::OperatorAttention,
            path: sound_path(),
        })
    );
}

fn sound_path() -> PathBuf {
    PathBuf::from("C:\\sounds\\done.wav")
}
