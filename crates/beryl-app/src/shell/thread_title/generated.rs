use std::collections::HashMap;

use beryl_backend::{AgentMessageItem, ProtocolPhase, ThreadItem};

use super::accepted_title;

#[derive(Debug, Default)]
pub(super) struct GeneratedTitleText {
    item_deltas: HashMap<String, String>,
    latest_agent_message: Option<String>,
    final_answer: Option<String>,
}

impl GeneratedTitleText {
    pub(super) fn observe_turn_items(&mut self, items: Vec<ThreadItem>) {
        for item in items {
            self.observe_thread_item(item);
        }
    }

    pub(super) fn observe_thread_item(&mut self, item: ThreadItem) {
        if let ThreadItem::AgentMessage(message) = item {
            self.observe_agent_message(message);
        }
    }

    pub(super) fn observe_agent_message_delta(
        &mut self,
        item_id: impl Into<String>,
        delta: impl AsRef<str>,
    ) {
        self.item_deltas
            .entry(item_id.into())
            .or_default()
            .push_str(delta.as_ref());
    }

    pub(super) fn generated_title(&self) -> Option<String> {
        self.final_answer
            .as_deref()
            .or(self.latest_agent_message.as_deref())
            .or_else(|| {
                self.item_deltas
                    .values()
                    .find(|text| !text.trim().is_empty())
                    .map(String::as_str)
            })
            .and_then(accepted_title)
    }

    fn observe_agent_message(&mut self, message: AgentMessageItem) {
        if !message.text.trim().is_empty() {
            self.latest_agent_message = Some(message.text.clone());
        }
        if message.phase == Some(ProtocolPhase::FinalAnswer) && !message.text.trim().is_empty() {
            self.final_answer = Some(message.text);
        }
    }
}
