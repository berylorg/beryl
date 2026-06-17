use std::sync::Arc;

use super::{
    ConversationSurfaceState, execution_detail::TurnExecutionRecord,
    transcript_presentation::TranscriptPresentationMutation,
    transcript_viewport::TranscriptViewportRowMutation,
};

impl ConversationSurfaceState {
    pub(super) fn prepend_transcript_presentation_rows(
        &mut self,
        turns: &[Arc<TurnExecutionRecord>],
    ) -> usize {
        let inserted = self.transcript_presentation.prepend_from_turns(turns);
        if inserted > 0 {
            self.shift_transcript_anchor(inserted);
            self.reconcile_transcript_presentation_mutation(
                TranscriptPresentationMutation::Inserted {
                    index: 0,
                    count: inserted,
                },
            );
        }
        inserted
    }

    pub(super) fn append_transcript_presentation_turn(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
    ) -> Option<usize> {
        let index = self
            .transcript_presentation
            .append_turn(source_turn_index, turn)?;
        self.reconcile_transcript_presentation_mutation(TranscriptPresentationMutation::Inserted {
            index,
            count: 1,
        });
        Some(index)
    }

    pub(super) fn replace_transcript_presentation_turn(
        &mut self,
        source_turn_index: usize,
        turn: Arc<TurnExecutionRecord>,
    ) -> TranscriptPresentationMutation {
        let mutation = self
            .transcript_presentation
            .replace_turn(source_turn_index, turn);
        self.reconcile_transcript_presentation_mutation(mutation);
        mutation
    }

    pub(super) fn reconcile_transcript_presentation_mutation(
        &mut self,
        mutation: TranscriptPresentationMutation,
    ) {
        if !matches!(mutation, TranscriptPresentationMutation::Unchanged) {
            self.transcript_event_time_scroll.clear();
        }
        match mutation {
            TranscriptPresentationMutation::Unchanged => {}
            TranscriptPresentationMutation::Replaced { index } => {
                self.transcript_list_state
                    .invalidate_item_measurement(index);
            }
            TranscriptPresentationMutation::Inserted { index, count } => {
                if count == 0 {
                    return;
                }
                self.transcript_viewport.reconcile_row_mutation(
                    TranscriptViewportRowMutation::Inserted { index, count },
                );
                let start = index.min(self.transcript_list_state.item_count());
                self.transcript_list_state.splice(start..start, count);
            }
            TranscriptPresentationMutation::Removed { index, count } => {
                if count == 0 {
                    return;
                }
                self.transcript_viewport.reconcile_row_mutation(
                    TranscriptViewportRowMutation::Removed { index, count },
                );
                let item_count = self.transcript_list_state.item_count();
                let start = index.min(item_count);
                let end = index.saturating_add(count).min(item_count).max(start);
                if start < end {
                    self.transcript_list_state.splice(start..end, 0);
                }
                self.reconcile_transcript_branch_menu_target();
                self.reconcile_transcript_edit_mode();
            }
        }
    }
}
