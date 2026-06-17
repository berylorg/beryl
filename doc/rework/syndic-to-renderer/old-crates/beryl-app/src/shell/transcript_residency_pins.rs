use std::collections::BTreeSet;
use std::ops::Range;

use beryl_backend::{TurnInfo, TurnItemsView};

use super::transcript_history::{
    TranscriptResidencyBudgetReason, TranscriptResidencyTargetPlan,
    estimate_turn_payload_resident_bytes, is_oversized_turn_fallback_marker,
};
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyAdmissionSummary {
    pub(crate) request_kind: &'static str,
    pub(crate) source_range: Range<usize>,
    pub(crate) transport_turns: usize,
    pub(crate) transport_payload_bytes: usize,
    pub(crate) admitted_turns: usize,
    pub(crate) admitted_payload_bytes: usize,
    pub(crate) oversized_fallback_turns: usize,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TranscriptResidencyDiagnostics {
    pub(super) last_requested_turns: usize,
    pub(super) last_transport_turns: usize,
    pub(super) last_transport_payload_bytes: usize,
    pub(super) staged_admission_turns: usize,
    pub(super) last_admitted_turns: usize,
    pub(super) last_admitted_payload_bytes: usize,
    pub(super) last_released_turns: usize,
    pub(super) last_desired_turns: usize,
    pub(super) last_desired_bytes: usize,
    pub(super) last_release_intents: usize,
    pub(super) last_missing_transport_ranges: usize,
    pub(super) last_oversized_fallback_turns: usize,
    pub(super) last_target_margin_satisfied: bool,
    pub(super) last_target_window_shrunk_by_budget: bool,
    pub(super) last_target_budget_reason: TranscriptResidencyBudgetReason,
}

impl TranscriptResidencyAdmissionSummary {
    pub(crate) fn from_transport_page(
        request_kind: &'static str,
        source_range: Range<usize>,
        turns: &[TurnInfo],
    ) -> Self {
        let transport_payload_bytes = turns
            .iter()
            .map(estimate_turn_payload_resident_bytes)
            .sum::<usize>();
        Self {
            request_kind,
            source_range,
            transport_turns: turns.len(),
            transport_payload_bytes,
            ..Self::default()
        }
    }

    pub(crate) fn from_admitted_turns(
        request_kind: &'static str,
        source_range: Range<usize>,
        turns: &[TurnInfo],
    ) -> Self {
        let mut summary = Self::from_transport_page(request_kind, source_range, turns);
        for turn in turns {
            if turn.items_view == TurnItemsView::Full {
                summary.admitted_turns = summary.admitted_turns.saturating_add(1);
                summary.admitted_payload_bytes = summary
                    .admitted_payload_bytes
                    .saturating_add(estimate_turn_payload_resident_bytes(turn));
            }
            if is_oversized_turn_fallback_marker(turn) {
                summary.oversized_fallback_turns =
                    summary.oversized_fallback_turns.saturating_add(1);
            }
        }
        summary
    }

    pub(crate) fn with_transport_observation(mut self, transport: &Self) -> Self {
        self.transport_turns = transport.transport_turns;
        self.transport_payload_bytes = transport.transport_payload_bytes;
        self
    }
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
        self.invalidate_transcript_residency_controller();
    }

    pub(super) fn note_transcript_residency_request_started(&mut self, requested_turns: usize) {
        self.transcript_residency_diagnostics.last_requested_turns = requested_turns;
    }

    pub(super) fn note_transcript_residency_transport_page(
        &mut self,
        summary: &TranscriptResidencyAdmissionSummary,
    ) {
        self.transcript_residency_diagnostics.last_transport_turns = summary.transport_turns;
        self.transcript_residency_diagnostics
            .last_transport_payload_bytes = summary.transport_payload_bytes;
    }

    pub(super) fn note_transcript_residency_staged_admission(
        &mut self,
        summary: &TranscriptResidencyAdmissionSummary,
    ) {
        self.note_transcript_residency_transport_page(summary);
        self.transcript_residency_diagnostics.staged_admission_turns = summary.admitted_turns;
        self.transcript_residency_diagnostics
            .last_oversized_fallback_turns = summary.oversized_fallback_turns;
    }

    pub(super) fn clear_transcript_residency_staged_admission(&mut self) {
        self.transcript_residency_diagnostics.staged_admission_turns = 0;
    }

    pub(super) fn note_transcript_residency_admission(
        &mut self,
        summary: &TranscriptResidencyAdmissionSummary,
    ) {
        self.note_transcript_residency_transport_page(summary);
        self.transcript_residency_diagnostics.staged_admission_turns = 0;
        self.transcript_residency_diagnostics.last_admitted_turns = summary.admitted_turns;
        self.transcript_residency_diagnostics
            .last_admitted_payload_bytes = summary.admitted_payload_bytes;
        self.transcript_residency_diagnostics
            .last_oversized_fallback_turns = summary.oversized_fallback_turns;
    }

    pub(super) fn note_transcript_residency_release(&mut self, released_turns: usize) {
        self.transcript_residency_diagnostics.last_released_turns = released_turns;
    }

    pub(super) fn note_transcript_residency_controller_plan(
        &mut self,
        plan: &TranscriptResidencyTargetPlan,
    ) {
        self.transcript_residency_diagnostics.last_desired_turns = plan.desired_full_turn_ids.len();
        self.transcript_residency_diagnostics.last_desired_bytes =
            plan.diagnostics.desired_resident_bytes;
        self.transcript_residency_diagnostics.last_release_intents = plan.release_turn_ids.len();
        self.transcript_residency_diagnostics
            .last_missing_transport_ranges = plan.missing_transport_ranges.len();
        self.transcript_residency_diagnostics
            .last_target_margin_satisfied = plan.diagnostics.viewport_margin_satisfied;
        self.transcript_residency_diagnostics
            .last_target_window_shrunk_by_budget = !plan.diagnostics.viewport_margin_satisfied
            && (plan.diagnostics.resident_turn_limit
                || plan.diagnostics.resident_byte_limit
                || plan.diagnostics.oversized_turn_fallback);
        self.transcript_residency_diagnostics
            .last_target_budget_reason = plan.diagnostics.limiting_reason;
        self.transcript_residency_diagnostics
            .last_oversized_fallback_turns = plan.oversized_turn_fallback_ids.len();
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
}

fn unique_turn_ids(turn_ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    turn_ids
        .into_iter()
        .filter(|turn_id| seen.insert(turn_id.clone()))
        .collect()
}
