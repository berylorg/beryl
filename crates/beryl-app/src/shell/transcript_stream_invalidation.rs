use std::collections::{HashMap, HashSet, VecDeque};

use beryl_backend::TurnStreamEvent;

pub(crate) const TRANSCRIPT_STREAM_INVALIDATION_MAX_THREADS: usize = 64;
pub(crate) const TRANSCRIPT_STREAM_INVALIDATION_MAX_TURNS_PER_THREAD: usize = 256;
pub(crate) const TRANSCRIPT_STREAM_INVALIDATION_MAX_TURNS_TOTAL: usize = 1024;

#[derive(Clone, Debug, Default)]
pub(crate) struct TranscriptStreamInvalidations {
    turn_ids_by_thread_id: HashMap<String, HashSet<String>>,
    invalidation_order: VecDeque<(String, String)>,
}

impl TranscriptStreamInvalidations {
    pub(crate) fn clear(&mut self) {
        self.turn_ids_by_thread_id.clear();
        self.invalidation_order.clear();
    }

    pub(crate) fn invalidate_turns(
        &mut self,
        thread_id: &str,
        turn_ids: impl IntoIterator<Item = String>,
    ) {
        let thread_id = thread_id.to_string();
        for turn_id in turn_ids {
            let invalidated = self
                .turn_ids_by_thread_id
                .entry(thread_id.clone())
                .or_default();
            if invalidated.insert(turn_id.clone()) {
                self.invalidation_order
                    .push_back((thread_id.clone(), turn_id));
            }
            self.prune_thread(thread_id.as_str());
            self.prune_global();
        }
    }

    pub(crate) fn event_targets_invalidated_turn(&self, event: &TurnStreamEvent) -> bool {
        let Some((thread_id, turn_id)) = stream_event_thread_turn_id(event) else {
            return false;
        };
        self.turn_ids_by_thread_id
            .get(thread_id)
            .is_some_and(|turns| turns.contains(turn_id))
    }

    fn prune_thread(&mut self, thread_id: &str) {
        while self
            .turn_ids_by_thread_id
            .get(thread_id)
            .is_some_and(|turns| turns.len() > TRANSCRIPT_STREAM_INVALIDATION_MAX_TURNS_PER_THREAD)
        {
            if !self.remove_oldest_for_thread(thread_id) {
                break;
            }
        }
    }

    fn prune_global(&mut self) {
        while self.turn_ids_by_thread_id.len() > TRANSCRIPT_STREAM_INVALIDATION_MAX_THREADS {
            if !self.remove_oldest_thread() {
                break;
            }
        }
        while self.retained_turn_count() > TRANSCRIPT_STREAM_INVALIDATION_MAX_TURNS_TOTAL {
            if !self.remove_oldest_turn() {
                break;
            }
        }
    }

    fn remove_oldest_thread(&mut self) -> bool {
        let Some((thread_id, _)) = self.invalidation_order.front().cloned() else {
            return false;
        };
        self.turn_ids_by_thread_id.remove(&thread_id);
        self.invalidation_order
            .retain(|(candidate_thread_id, _)| candidate_thread_id != &thread_id);
        true
    }

    fn remove_oldest_for_thread(&mut self, thread_id: &str) -> bool {
        let Some(index) =
            self.invalidation_order
                .iter()
                .position(|(candidate_thread_id, candidate_turn_id)| {
                    candidate_thread_id == thread_id
                        && self
                            .turn_ids_by_thread_id
                            .get(candidate_thread_id)
                            .is_some_and(|turns| turns.contains(candidate_turn_id))
                })
        else {
            return false;
        };
        let Some((removed_thread_id, removed_turn_id)) = self.invalidation_order.remove(index)
        else {
            return false;
        };
        self.remove_turn(removed_thread_id.as_str(), removed_turn_id.as_str());
        true
    }

    fn remove_oldest_turn(&mut self) -> bool {
        while let Some((thread_id, turn_id)) = self.invalidation_order.pop_front() {
            if self.remove_turn(thread_id.as_str(), turn_id.as_str()) {
                return true;
            }
        }
        false
    }

    fn remove_turn(&mut self, thread_id: &str, turn_id: &str) -> bool {
        let Some(turns) = self.turn_ids_by_thread_id.get_mut(thread_id) else {
            return false;
        };
        let removed = turns.remove(turn_id);
        if turns.is_empty() {
            self.turn_ids_by_thread_id.remove(thread_id);
        }
        removed
    }

    fn retained_turn_count(&self) -> usize {
        self.turn_ids_by_thread_id.values().map(HashSet::len).sum()
    }

    #[cfg(test)]
    pub(crate) fn retained_thread_count_for_test(&self) -> usize {
        self.turn_ids_by_thread_id.len()
    }

    #[cfg(test)]
    pub(crate) fn retained_turn_count_for_test(&self) -> usize {
        self.retained_turn_count()
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
