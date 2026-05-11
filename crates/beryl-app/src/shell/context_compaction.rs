use beryl_backend::{ThreadItem, ThreadStatus, TurnInfo, TurnStreamEvent};

const CONTEXT_COMPACTION_ITEM_TYPE: &str = "contextCompaction";

#[derive(Clone, Debug, Default)]
pub(crate) struct ContextCompactionStreamState {
    observed_compaction_activity: bool,
    active_turn_id: Option<String>,
}

impl ContextCompactionStreamState {
    pub(crate) fn observe(&mut self, target_thread_id: &str, event: &TurnStreamEvent) -> bool {
        if let Some(turn_id) = context_compaction_turn_id(target_thread_id, event) {
            self.observed_compaction_activity = true;
            if self.active_turn_id.as_deref() != Some(turn_id) {
                self.active_turn_id = Some(turn_id.to_string());
            }
        } else if event_is_context_compaction_activity(target_thread_id, event) {
            self.observed_compaction_activity = true;
        }

        self.observed_compaction_activity
            && matches!(
                event,
                TurnStreamEvent::ThreadStatusChanged { thread_id, status }
                    if thread_id == target_thread_id && matches!(status, ThreadStatus::Idle)
            )
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn active_turn_id(&self) -> Option<&str> {
        self.active_turn_id.as_deref()
    }
}

pub(crate) fn context_compaction_turn_id<'a>(
    target_thread_id: &str,
    event: &'a TurnStreamEvent,
) -> Option<&'a str> {
    match event {
        TurnStreamEvent::TurnStarted { thread_id, turn }
        | TurnStreamEvent::TurnCompleted { thread_id, turn }
            if thread_id == target_thread_id && turn_contains_context_compaction(turn) =>
        {
            Some(turn.id.as_str())
        }
        TurnStreamEvent::ItemStarted {
            thread_id,
            turn_id,
            item,
            ..
        }
        | TurnStreamEvent::ItemCompleted {
            thread_id,
            turn_id,
            item,
            ..
        } if thread_id == target_thread_id && item_is_context_compaction(item) => {
            Some(turn_id.as_str())
        }
        _ => None,
    }
}

fn event_is_context_compaction_activity(target_thread_id: &str, event: &TurnStreamEvent) -> bool {
    match event {
        TurnStreamEvent::ThreadStatusChanged {
            thread_id,
            status: ThreadStatus::Active { .. },
        } if thread_id == target_thread_id => true,
        _ => context_compaction_turn_id(target_thread_id, event).is_some(),
    }
}

fn turn_contains_context_compaction(turn: &TurnInfo) -> bool {
    turn.items.iter().any(item_is_context_compaction)
}

fn item_is_context_compaction(item: &ThreadItem) -> bool {
    item.item_type() == CONTEXT_COMPACTION_ITEM_TYPE
}
