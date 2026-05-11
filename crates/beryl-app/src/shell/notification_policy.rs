use std::path::PathBuf;

use super::notifications::LifecycleNotificationKind;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NotificationPolicyInput {
    pub(super) candidate_kind: NotificationCandidateKind,
    pub(super) configured_sound: ConfiguredNotificationSound,
    pub(super) parent_turn: ParentTurnNotificationState,
    pub(super) lifecycle_suppression: LifecycleSuppressionState,
    pub(super) window_focus: BerylWindowFocusState,
    pub(super) platform_attention: PlatformAttentionState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NotificationCandidateKind {
    OrdinaryEndTurn,
    Lifecycle(LifecycleNotificationKind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ConfiguredNotificationSound {
    Disabled,
    Selected(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ParentTurnNotificationState {
    pub(super) user_visible: bool,
    pub(super) terminal: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum LifecycleSuppressionState {
    None,
    SuppressOrdinaryEndTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BerylWindowFocusState {
    Focused,
    Unfocused,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PlatformAttentionState {
    pub(super) local_input_idle: AttentionTriggerState,
    pub(super) session_locked: AttentionTriggerState,
    pub(super) lid_closed: AttentionTriggerState,
    pub(super) display_inactive: AttentionTriggerState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AttentionTriggerState {
    Active,
    Inactive,
    Unknown,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NotificationPolicyDecision {
    Play(NotificationPlaybackRequest),
    Suppress(NotificationSuppressionReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NotificationPlaybackRequest {
    EndTurn {
        path: PathBuf,
    },
    Lifecycle {
        kind: LifecycleNotificationKind,
        path: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NotificationSuppressionReason {
    SoundDisabled,
    NotVisibleParentTerminalTurn,
    OrdinaryEndTurnSuppressedByLifecycle,
    NoAttentionTriggerActive,
}

impl ConfiguredNotificationSound {
    pub(super) fn from_optional_path(path: Option<PathBuf>) -> Self {
        path.map(Self::Selected).unwrap_or(Self::Disabled)
    }
}

impl ParentTurnNotificationState {
    pub(super) fn visible_terminal() -> Self {
        Self {
            user_visible: true,
            terminal: true,
        }
    }
}

impl PlatformAttentionState {
    #[cfg(test)]
    pub(super) fn inactive() -> Self {
        Self {
            local_input_idle: AttentionTriggerState::Inactive,
            session_locked: AttentionTriggerState::Inactive,
            lid_closed: AttentionTriggerState::Inactive,
            display_inactive: AttentionTriggerState::Inactive,
        }
    }

    #[cfg(test)]
    pub(super) fn unknown() -> Self {
        Self {
            local_input_idle: AttentionTriggerState::Unknown,
            session_locked: AttentionTriggerState::Unknown,
            lid_closed: AttentionTriggerState::Unknown,
            display_inactive: AttentionTriggerState::Unknown,
        }
    }

    fn has_active_trigger(self) -> bool {
        [
            self.local_input_idle,
            self.session_locked,
            self.lid_closed,
            self.display_inactive,
        ]
        .into_iter()
        .any(|state| state == AttentionTriggerState::Active)
    }
}

pub(super) fn notification_policy_decision(
    input: NotificationPolicyInput,
) -> NotificationPolicyDecision {
    let path = match input.configured_sound {
        ConfiguredNotificationSound::Selected(path) => path,
        ConfiguredNotificationSound::Disabled => {
            return NotificationPolicyDecision::Suppress(
                NotificationSuppressionReason::SoundDisabled,
            );
        }
    };

    if !input.parent_turn.user_visible || !input.parent_turn.terminal {
        return NotificationPolicyDecision::Suppress(
            NotificationSuppressionReason::NotVisibleParentTerminalTurn,
        );
    }

    if matches!(
        (input.candidate_kind, input.lifecycle_suppression,),
        (
            NotificationCandidateKind::OrdinaryEndTurn,
            LifecycleSuppressionState::SuppressOrdinaryEndTurn,
        )
    ) {
        return NotificationPolicyDecision::Suppress(
            NotificationSuppressionReason::OrdinaryEndTurnSuppressedByLifecycle,
        );
    }

    if input.window_focus != BerylWindowFocusState::Unfocused
        && !input.platform_attention.has_active_trigger()
    {
        return NotificationPolicyDecision::Suppress(
            NotificationSuppressionReason::NoAttentionTriggerActive,
        );
    }

    match input.candidate_kind {
        NotificationCandidateKind::OrdinaryEndTurn => {
            NotificationPolicyDecision::Play(NotificationPlaybackRequest::EndTurn { path })
        }
        NotificationCandidateKind::Lifecycle(kind) => {
            NotificationPolicyDecision::Play(NotificationPlaybackRequest::Lifecycle { kind, path })
        }
    }
}
