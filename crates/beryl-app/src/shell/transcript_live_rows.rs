use super::{
    ConversationSurfaceState,
    transcript_scroll::{LiveTranscriptRows, sync_live_transcript_rows},
};

impl ConversationSurfaceState {
    pub(super) fn sync_live_transcript_rows(&mut self, previous_turn_count: usize) {
        sync_live_transcript_rows(
            &self.transcript_list_state,
            LiveTranscriptRows {
                previous_turn_count,
                current_turn_count: self.transcript_presentation.len(),
                preserve_user_scroll: self.transcript_user_scrolled,
            },
        );
        self.sync_transcript_turn_detail_ui_pins();
    }
}
