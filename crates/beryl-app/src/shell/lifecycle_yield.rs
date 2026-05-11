use std::collections::HashMap;

use crate::LifecycleYieldOutcome;

use super::notifications::{LifecycleNotificationCandidate, LifecycleNotificationKind};

#[derive(Clone, Debug, Default)]
pub(super) struct LifecycleYieldState {
    pending: HashMap<LifecycleYieldTurnKey, LifecycleYieldOutcome>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct LifecycleYieldTurnKey {
    thread_id: String,
    turn_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TerminalLifecycleYield {
    thread_id: String,
    turn_id: String,
    outcome: LifecycleYieldOutcome,
}

impl LifecycleYieldState {
    pub(super) fn record(
        &mut self,
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
        outcome: LifecycleYieldOutcome,
    ) -> bool {
        let key = LifecycleYieldTurnKey::new(thread_id, turn_id);
        match self.pending.entry(key) {
            std::collections::hash_map::Entry::Occupied(_) => false,
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(outcome);
                true
            }
        }
    }

    pub(super) fn apply_terminal_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
    ) -> Option<TerminalLifecycleYield> {
        let key = LifecycleYieldTurnKey::new(thread_id, turn_id);
        let outcome = self.pending.remove(&key)?;
        Some(TerminalLifecycleYield {
            thread_id: key.thread_id,
            turn_id: key.turn_id,
            outcome,
        })
    }

    pub(super) fn clear_turn(&mut self, thread_id: &str, turn_id: &str) -> bool {
        self.pending
            .remove(&LifecycleYieldTurnKey::new(thread_id, turn_id))
            .is_some()
    }

    pub(super) fn clear_all(&mut self) {
        self.pending.clear();
    }
}

impl LifecycleYieldTurnKey {
    fn new(thread_id: impl Into<String>, turn_id: impl Into<String>) -> Self {
        Self {
            thread_id: thread_id.into(),
            turn_id: turn_id.into(),
        }
    }
}

impl TerminalLifecycleYield {
    pub(super) fn thread_id(&self) -> &str {
        self.thread_id.as_str()
    }

    pub(super) fn outcome(&self) -> LifecycleYieldOutcome {
        self.outcome
    }

    pub(super) fn suppresses_ordinary_end_turn_sound(&self) -> bool {
        matches!(
            self.outcome,
            LifecycleYieldOutcome::BlockedNeedsOperator
                | LifecycleYieldOutcome::PhaseContinue
                | LifecycleYieldOutcome::PlanComplete
        )
    }

    pub(super) fn lifecycle_notification_candidate(
        &self,
    ) -> Option<LifecycleNotificationCandidate> {
        let kind = match self.outcome {
            LifecycleYieldOutcome::BlockedNeedsOperator => {
                LifecycleNotificationKind::OperatorAttention
            }
            LifecycleYieldOutcome::PlanComplete => LifecycleNotificationKind::PlanComplete,
            LifecycleYieldOutcome::PhaseNeedsReview | LifecycleYieldOutcome::PhaseContinue => {
                return None;
            }
        };

        Some(LifecycleNotificationCandidate::new(
            Some(self.thread_id.clone()),
            Some(self.turn_id.clone()),
            kind,
        ))
    }
}
