use std::collections::BTreeSet;

use super::*;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TranscriptResidencyDiagnostics {
    pub(super) last_requested_turns: usize,
    pub(super) last_released_turns: usize,
}

impl ConversationSurfaceState {
    pub(super) fn sync_transcript_residency_ui_pins(&mut self) {
        let context_menu_turn_ids = self.active_context_menu_residency_pin_turn_ids();
        let edit_target_turn_ids = self.active_edit_target_residency_pin_turn_ids();
        let media_action_turn_ids = self.active_media_action_residency_pin_turn_ids();
        let active_turn_ids = self.active_turn_residency_pin_turn_ids();

        self.transcript_history_window.replace_residency_pins(
            TranscriptResidencyPinKind::ActiveContextMenu,
            context_menu_turn_ids,
        );
        self.transcript_history_window
            .replace_residency_pins(TranscriptResidencyPinKind::EditTarget, edit_target_turn_ids);
        self.transcript_history_window.replace_residency_pins(
            TranscriptResidencyPinKind::MediaActionTarget,
            media_action_turn_ids,
        );
        self.transcript_history_window
            .replace_residency_pins(TranscriptResidencyPinKind::ActiveTurn, active_turn_ids);
        self.release_unpinned_resident_turns_for_current_viewport();
    }

    pub(super) fn note_transcript_residency_request_started(&mut self) {
        self.transcript_residency_diagnostics.last_requested_turns =
            THREAD_HISTORY_PAGE_LIMIT as usize;
    }

    pub(super) fn note_transcript_residency_release(&mut self, released_turns: usize) {
        self.transcript_residency_diagnostics.last_released_turns = released_turns;
    }

    fn turn_id_for_transcript_row_identity(&self, row_identity: &str) -> Option<String> {
        let row_index = self
            .transcript_presentation
            .row_index_for_identity(row_identity)?;
        self.transcript_presentation
            .turn_at(row_index)?
            .turn
            .turn_id
            .clone()
    }

    fn active_context_menu_residency_pin_turn_ids(&self) -> Vec<String> {
        let Some(open) = self.transcript_branch_menu.active() else {
            return Vec::new();
        };
        let mut turn_ids = Vec::new();
        if let Some(target) = open.branch_target() {
            turn_ids.push(target.source_turn_id().to_string());
        }
        if let Some(identity) = open.edit_entry().and_then(|entry| entry.target_identity()) {
            turn_ids.push(identity.source_turn_id().to_string());
        }
        if let Some(identity) = open
            .title_update_entry()
            .and_then(|entry| entry.target_identity())
        {
            turn_ids.push(identity.source_turn_id().to_string());
        }
        if let Some(turn_id) = open
            .image_target()
            .and_then(|target| self.turn_id_for_transcript_row_identity(target.row_identity()))
        {
            turn_ids.push(turn_id);
        }
        unique_turn_ids(turn_ids)
    }

    fn active_edit_target_residency_pin_turn_ids(&self) -> Vec<String> {
        self.transcript_edit_mode
            .as_ref()
            .map(|edit_mode| edit_mode.target().source_turn_id().to_string())
            .into_iter()
            .collect()
    }

    fn active_media_action_residency_pin_turn_ids(&self) -> Vec<String> {
        self.transcript_branch_menu
            .active()
            .and_then(|open| open.image_target())
            .and_then(|target| self.turn_id_for_transcript_row_identity(target.row_identity()))
            .into_iter()
            .collect()
    }

    fn active_turn_residency_pin_turn_ids(&self) -> Vec<String> {
        let Some(selected_thread_id) = self.selected_thread_id() else {
            return Vec::new();
        };
        let Some(active) = self.execution_details.active_turn_identity() else {
            return Vec::new();
        };
        if active.thread_id.as_deref() != Some(selected_thread_id) {
            return Vec::new();
        }
        active.turn_id.into_iter().collect()
    }

    fn release_unpinned_resident_turns_for_current_viewport(&mut self) {
        let turn_count = self.transcript_presentation.len();
        let retained_range = self
            .transcript_history_window
            .retention_range_for_visible_range(
                self.transcript_list_state.visible_range(),
                turn_count,
            );
        let retained_turn_ids = self.transcript_turn_ids_for_range(retained_range);
        let retention = TranscriptResidencyRetention::from_turn_ids(retained_turn_ids);
        let released = self
            .transcript_history_window
            .release_unretained_resident_turns(&retention);
        self.note_transcript_residency_release(released.released_turn_ids.len());
    }

    fn transcript_turn_ids_for_range(&self, range: std::ops::Range<usize>) -> Vec<String> {
        self.transcript_presentation
            .window_for_range(range)
            .rows()
            .iter()
            .filter_map(|row| row.turn.turn_id.as_deref())
            .map(str::to_string)
            .collect()
    }
}

fn unique_turn_ids(turn_ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    turn_ids
        .into_iter()
        .filter(|turn_id| seen.insert(turn_id.clone()))
        .collect()
}
