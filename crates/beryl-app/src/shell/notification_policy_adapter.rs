use std::path::PathBuf;

use super::notification_policy::{
    BerylWindowFocusState, ConfiguredNotificationSound, LifecycleSuppressionState,
    NotificationCandidateKind, NotificationPolicyDecision, NotificationPolicyInput,
    ParentTurnNotificationState, PlatformAttentionState, notification_policy_decision,
};

pub(super) fn terminal_parent_turn_notification_decision(
    candidate_kind: NotificationCandidateKind,
    configured_sound_path: Option<PathBuf>,
    window_focus: BerylWindowFocusState,
    platform_attention: PlatformAttentionState,
) -> NotificationPolicyDecision {
    notification_policy_decision(NotificationPolicyInput {
        candidate_kind,
        configured_sound: ConfiguredNotificationSound::from_optional_path(configured_sound_path),
        parent_turn: ParentTurnNotificationState::visible_terminal(),
        lifecycle_suppression: LifecycleSuppressionState::None,
        window_focus,
        platform_attention,
    })
}
