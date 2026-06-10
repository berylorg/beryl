use super::{
    ConversationSurfaceState,
    execution_detail::{
        AgentMessageDetail, ExecutionItem, TurnExecutionRecord, TurnNarrativeEntry,
    },
    transcript_live_scroll::{
        TranscriptFinalAnchor, TranscriptLiveTurnAnchor, TranscriptNarrativeAnchor,
    },
};
use beryl_backend::ProtocolPhase;

impl ConversationSurfaceState {
    pub(super) fn reset_loaded_history_live_scroll(&mut self) {
        self.transcript_live_scroll.clear_for_tail_activation();
        self.set_loaded_history_final_runway();
    }

    pub(super) fn reconcile_loaded_history_final_runway_for_row(
        &mut self,
        presentation_index: Option<usize>,
    ) -> bool {
        let Some(index) = presentation_index else {
            return false;
        };
        if Some(index) != self.transcript_presentation.len().checked_sub(1) {
            return false;
        }
        let Some(anchor) = self.final_runway_anchor_for_presentation_index(index) else {
            return false;
        };
        self.transcript_live_scroll
            .refresh_loaded_history_final_runway(anchor)
    }

    fn set_loaded_history_final_runway(&mut self) -> bool {
        let Some(index) = self.transcript_presentation.len().checked_sub(1) else {
            return false;
        };
        let Some(anchor) = self.final_runway_anchor_for_presentation_index(index) else {
            return false;
        };
        self.transcript_live_scroll.set_passive_final_runway(anchor);
        true
    }

    fn final_runway_anchor_for_presentation_index(
        &self,
        index: usize,
    ) -> Option<TranscriptFinalAnchor> {
        let Some(row) = self.transcript_presentation.turn_at(index) else {
            return None;
        };
        let Some(item_id) = final_answer_item_id(row.turn.as_ref()) else {
            return None;
        };
        let turn_anchor = TranscriptLiveTurnAnchor::new(
            row.index,
            Some(row.identity.as_str().to_string()),
            row.turn.thread_id.clone(),
            row.turn.turn_id.clone(),
        );
        Some(TranscriptFinalAnchor {
            turn: turn_anchor,
            item_id,
        })
    }

    pub(super) fn reconcile_transcript_live_scroll_for_row(
        &mut self,
        presentation_index: Option<usize>,
    ) {
        let Some(row) =
            presentation_index.and_then(|index| self.transcript_presentation.turn_at(index))
        else {
            return;
        };
        let turn_anchor = TranscriptLiveTurnAnchor::new(
            row.index,
            Some(row.identity.as_str().to_string()),
            row.turn.thread_id.clone(),
            row.turn.turn_id.clone(),
        );
        if let Some(item_id) = final_answer_item_id(row.turn.as_ref()) {
            self.transcript_live_scroll
                .transition_to_final_start(TranscriptFinalAnchor {
                    turn: turn_anchor,
                    item_id,
                });
            return;
        }
        if let Some(item_id) = latest_commentary_item_id(row.turn.as_ref()) {
            self.transcript_live_scroll.transition_to_commentary_follow(
                TranscriptNarrativeAnchor {
                    turn: turn_anchor,
                    item_id: Some(item_id),
                },
            );
        }
    }
}

fn final_answer_item_id(turn: &TurnExecutionRecord) -> Option<String> {
    turn.narrative_entries()
        .iter()
        .rev()
        .find_map(|entry| match entry {
            TurnNarrativeEntry::Item { item_id } => {
                let Some(ExecutionItem::AgentMessage(message)) = turn.item_by_id(item_id) else {
                    return None;
                };
                live_scroll_final_message(message).then(|| message.id.clone())
            }
            TurnNarrativeEntry::UserInput { .. } => None,
        })
}

fn live_scroll_final_message(message: &AgentMessageDetail) -> bool {
    matches!(message.phase, Some(ProtocolPhase::FinalAnswer) | None) && !message.text.is_empty()
}

fn latest_commentary_item_id(turn: &TurnExecutionRecord) -> Option<String> {
    turn.narrative_entries()
        .iter()
        .rev()
        .find_map(|entry| match entry {
            TurnNarrativeEntry::Item { item_id } => {
                let Some(ExecutionItem::AgentMessage(message)) = turn.item_by_id(item_id) else {
                    return None;
                };
                (message.phase == Some(ProtocolPhase::Commentary) && !message.text.is_empty())
                    .then(|| message.id.clone())
            }
            TurnNarrativeEntry::UserInput { .. } => None,
        })
}
