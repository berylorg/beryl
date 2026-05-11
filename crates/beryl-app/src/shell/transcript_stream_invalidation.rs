use std::collections::{HashMap, HashSet};

use beryl_backend::TurnStreamEvent;

#[derive(Clone, Debug, Default)]
pub(crate) struct TranscriptStreamInvalidations {
    turn_ids_by_thread_id: HashMap<String, HashSet<String>>,
}

impl TranscriptStreamInvalidations {
    pub(crate) fn clear(&mut self) {
        self.turn_ids_by_thread_id.clear();
    }

    pub(crate) fn invalidate_turns(
        &mut self,
        thread_id: &str,
        turn_ids: impl IntoIterator<Item = String>,
    ) {
        let invalidated = self
            .turn_ids_by_thread_id
            .entry(thread_id.to_string())
            .or_default();
        invalidated.extend(turn_ids);
    }

    pub(crate) fn event_targets_invalidated_turn(&self, event: &TurnStreamEvent) -> bool {
        let Some((thread_id, turn_id)) = stream_event_thread_turn_id(event) else {
            return false;
        };
        self.turn_ids_by_thread_id
            .get(thread_id)
            .is_some_and(|turns| turns.contains(turn_id))
    }
}

pub(crate) fn stream_event_thread_turn_id(event: &TurnStreamEvent) -> Option<(&str, &str)> {
    match event {
        TurnStreamEvent::TurnStarted { thread_id, turn }
        | TurnStreamEvent::TurnCompleted { thread_id, turn } => {
            Some((thread_id.as_str(), turn.id.as_str()))
        }
        TurnStreamEvent::ItemStarted {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ItemCompleted {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::AgentMessageDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ReasoningSummaryPartAdded {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ReasoningSummaryTextDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::ReasoningTextDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::CommandExecutionOutputDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::FileChangeOutputDelta {
            thread_id, turn_id, ..
        }
        | TurnStreamEvent::TokenUsageUpdated {
            thread_id, turn_id, ..
        } => Some((thread_id.as_str(), turn_id.as_str())),
        TurnStreamEvent::ThreadStarted { .. }
        | TurnStreamEvent::AgentLabelUpdated { .. }
        | TurnStreamEvent::ThreadStatusChanged { .. }
        | TurnStreamEvent::ThreadClosed { .. }
        | TurnStreamEvent::AccountRateLimitsUpdated { .. }
        | TurnStreamEvent::ThreadNameUpdated { .. }
        | TurnStreamEvent::ApprovalRequested(_)
        | TurnStreamEvent::DynamicToolCallRequested(_)
        | TurnStreamEvent::ProtocolError { .. } => None,
    }
}
