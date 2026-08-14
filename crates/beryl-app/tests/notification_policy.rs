#![allow(dead_code)]

#[path = "../src/shell/notification_policy.rs"]
mod notification_policy;
#[path = "../src/shell/notifications.rs"]
mod notifications;

use std::path::PathBuf;

use notification_policy::{
    AttentionTriggerState, BerylWindowFocusState, ConfiguredNotificationSound,
    LifecycleSuppressionState, NotificationCandidateKind, NotificationPlaybackRequest,
    NotificationPolicyDecision, NotificationPolicyInput, NotificationSuppressionReason,
    ParentTurnNotificationState, PlatformAttentionState, notification_policy_decision,
};
use notifications::LifecycleNotificationKind;

#[test]
fn missing_sound_path_disables_notification_even_with_attention_trigger() {
    let mut input = ordinary_end_turn_input();
    input.configured_sound = ConfiguredNotificationSound::Disabled;
    input.window_focus = BerylWindowFocusState::Unfocused;
    input.platform_attention.local_input_idle = AttentionTriggerState::Active;

    assert_eq!(
        notification_policy_decision(input),
        NotificationPolicyDecision::Suppress(NotificationSuppressionReason::SoundDisabled)
    );
}

#[test]
fn visible_parent_terminal_turn_is_eligible_when_attention_trigger_is_active() {
    let mut input = ordinary_end_turn_input();
    input.window_focus = BerylWindowFocusState::Unfocused;

    assert_eq!(
        notification_policy_decision(input.clone()),
        NotificationPolicyDecision::Play(NotificationPlaybackRequest::EndTurn {
            path: sound_path()
        })
    );

    input.parent_turn.user_visible = false;
    assert_eq!(
        notification_policy_decision(input.clone()),
        NotificationPolicyDecision::Suppress(
            NotificationSuppressionReason::NotVisibleParentTerminalTurn
        )
    );

    input.parent_turn.user_visible = true;
    input.parent_turn.terminal = false;
    assert_eq!(
        notification_policy_decision(input),
        NotificationPolicyDecision::Suppress(
            NotificationSuppressionReason::NotVisibleParentTerminalTurn
        )
    );
}

#[test]
fn lifecycle_suppression_blocks_only_ordinary_end_turn_sound() {
    let mut input = ordinary_end_turn_input();
    input.lifecycle_suppression = LifecycleSuppressionState::SuppressOrdinaryEndTurn;
    input.window_focus = BerylWindowFocusState::Unfocused;

    assert_eq!(
        notification_policy_decision(input),
        NotificationPolicyDecision::Suppress(
            NotificationSuppressionReason::OrdinaryEndTurnSuppressedByLifecycle
        )
    );

    let mut lifecycle_input = lifecycle_input(LifecycleNotificationKind::PlanComplete);
    lifecycle_input.lifecycle_suppression = LifecycleSuppressionState::SuppressOrdinaryEndTurn;
    lifecycle_input.window_focus = BerylWindowFocusState::Unfocused;

    assert_eq!(
        notification_policy_decision(lifecycle_input),
        NotificationPolicyDecision::Play(NotificationPlaybackRequest::Lifecycle {
            kind: LifecycleNotificationKind::PlanComplete,
            path: sound_path(),
        })
    );
}

#[test]
fn unknown_focus_and_platform_states_do_not_trigger_or_block_known_attention() {
    let mut input = ordinary_end_turn_input();
    input.window_focus = BerylWindowFocusState::Unknown;
    input.platform_attention = PlatformAttentionState::unknown();

    assert_eq!(
        notification_policy_decision(input.clone()),
        NotificationPolicyDecision::Suppress(
            NotificationSuppressionReason::NoAttentionTriggerActive
        )
    );

    input.platform_attention.local_input_idle = AttentionTriggerState::Active;
    input.platform_attention.session_locked = AttentionTriggerState::Unknown;
    input.platform_attention.lid_closed = AttentionTriggerState::Unsupported;
    input.platform_attention.display_inactive = AttentionTriggerState::Unknown;

    assert_eq!(
        notification_policy_decision(input),
        NotificationPolicyDecision::Play(NotificationPlaybackRequest::EndTurn {
            path: sound_path()
        })
    );
}

#[test]
fn multiple_active_attention_triggers_produce_one_play_decision() {
    let mut input = ordinary_end_turn_input();
    input.window_focus = BerylWindowFocusState::Unfocused;
    input.platform_attention.local_input_idle = AttentionTriggerState::Active;
    input.platform_attention.session_locked = AttentionTriggerState::Active;
    input.platform_attention.display_inactive = AttentionTriggerState::Active;

    assert_eq!(
        notification_policy_decision(input),
        NotificationPolicyDecision::Play(NotificationPlaybackRequest::EndTurn {
            path: sound_path()
        })
    );
}

fn ordinary_end_turn_input() -> NotificationPolicyInput {
    policy_input(NotificationCandidateKind::OrdinaryEndTurn)
}

fn lifecycle_input(kind: LifecycleNotificationKind) -> NotificationPolicyInput {
    policy_input(NotificationCandidateKind::Lifecycle(kind))
}

fn policy_input(candidate_kind: NotificationCandidateKind) -> NotificationPolicyInput {
    NotificationPolicyInput {
        candidate_kind,
        configured_sound: ConfiguredNotificationSound::Selected(sound_path()),
        parent_turn: ParentTurnNotificationState::visible_terminal(),
        lifecycle_suppression: LifecycleSuppressionState::None,
        window_focus: BerylWindowFocusState::Focused,
        platform_attention: PlatformAttentionState::inactive(),
    }
}

fn sound_path() -> PathBuf {
    PathBuf::from("C:\\sounds\\done.wav")
}
