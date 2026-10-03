#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TurnCompletionSoundCandidate {
    pub(super) thread_id: Option<String>,
    pub(super) turn_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LifecycleNotificationKind {
    OperatorAttention,
    PlanComplete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LifecycleNotificationCandidate {
    pub(super) thread_id: Option<String>,
    pub(super) turn_id: Option<String>,
    pub(super) kind: LifecycleNotificationKind,
}

impl TurnCompletionSoundCandidate {
    pub(super) fn new(thread_id: Option<String>, turn_id: Option<String>) -> Self {
        Self { thread_id, turn_id }
    }
}

impl LifecycleNotificationCandidate {
    pub(super) fn new(
        thread_id: Option<String>,
        turn_id: Option<String>,
        kind: LifecycleNotificationKind,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            kind,
        }
    }
}
