use std::{collections::HashMap, ops::Range};

use super::transcript_history::{
    TranscriptResidencyMeasuredTurnHeight, TranscriptResidencyStreamedTurnFill,
    TranscriptResidencyTargetPlan,
};
use super::transcript_residency_logging::log_transcript_residency_target_decision;
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TranscriptResidencyControllerFacts {
    pub(super) presentation_visible_range: Range<usize>,
    pub(super) presentation_planning_range: Range<usize>,
    pub(super) source_visible_range: Range<usize>,
    pub(super) source_planning_range: Range<usize>,
    pub(super) viewport_height: usize,
    pub(super) measured_turn_heights: Vec<TranscriptResidencyMeasuredTurnHeight>,
    pub(super) streamed_turn_fills: Vec<TranscriptResidencyStreamedTurnFill>,
    pub(super) active_turn_id: Option<String>,
    signature: TranscriptResidencyControllerSignature,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TranscriptResidencyControllerSignature {
    selected_thread_id: Option<String>,
    presentation_visible_range: Range<usize>,
    presentation_planning_range: Range<usize>,
    source_visible_range: Range<usize>,
    source_planning_range: Range<usize>,
    viewport_height: usize,
    measured_turn_heights: Vec<TranscriptResidencyMeasuredTurnHeight>,
    streamed_turn_fills: Vec<TranscriptResidencyStreamedTurnFill>,
    active_turn_id: Option<String>,
    residency_revision: u64,
    resident_turn_count: usize,
    pinned_turn_count: usize,
    indexed_turn_count: usize,
    staged_admission_pending: bool,
    request_allowed: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TranscriptResidencyControllerUpdate {
    pub(super) request: Option<PendingTranscriptResidencyPageRequest>,
    pub(super) released_content: bool,
}

impl ConversationSurfaceState {
    pub(super) fn invalidate_transcript_residency_controller(&mut self) {
        self.last_transcript_residency_controller_signature = None;
    }

    pub(super) fn replace_transcript_streamed_residency_fill_facts<I>(&mut self, facts: I) -> bool
    where
        I: IntoIterator<Item = (String, TranscriptResidencyStreamedTurnFill)>,
    {
        let next = facts.into_iter().collect::<HashMap<_, _>>();
        if self.transcript_streamed_residency_fill_facts == next {
            return false;
        }
        self.transcript_streamed_residency_fill_facts = next;
        self.invalidate_transcript_residency_controller();
        true
    }

    fn transcript_streamed_residency_fill_facts_for_source_range(
        &self,
        source_range: &Range<usize>,
    ) -> Vec<TranscriptResidencyStreamedTurnFill> {
        self.transcript_streamed_residency_fill_facts
            .values()
            .copied()
            .filter(|fact| source_range.contains(&fact.source_position))
            .collect()
    }

    pub(super) fn latest_transcript_residency_controller_facts(
        &self,
    ) -> Option<&TranscriptResidencyControllerFacts> {
        self.transcript_residency_controller_facts.as_ref()
    }

    pub(super) fn begin_transcript_residency_controller_update(
        &mut self,
        presentation_visible_range: &Range<usize>,
        allow_request: bool,
        force: bool,
    ) -> TranscriptResidencyControllerUpdate {
        if self.transcript_residency_controller_update_deferred(presentation_visible_range) {
            return TranscriptResidencyControllerUpdate::default();
        }

        let presentation_planning_range =
            self.transcript_residency_planning_presentation_range(presentation_visible_range);
        let derived_estimates_changed =
            self.sync_transcript_residency_derived_byte_estimates(&presentation_planning_range);
        let facts = self.transcript_residency_controller_facts(
            presentation_visible_range,
            presentation_planning_range,
            allow_request,
        );
        if !force
            && !derived_estimates_changed
            && self
                .last_transcript_residency_controller_signature
                .as_ref()
                .is_some_and(|signature| signature == &facts.signature)
        {
            return TranscriptResidencyControllerUpdate::default();
        }

        self.last_transcript_residency_controller_signature = Some(facts.signature.clone());
        self.transcript_residency_controller_facts = Some(facts.clone());

        let plan = self
            .transcript_history_window
            .residency_target_plan_for_source_window_with_streamed_fill(
                facts.source_visible_range.clone(),
                facts.source_planning_range.clone(),
                facts.viewport_height,
                facts.measured_turn_heights.clone(),
                facts.streamed_turn_fills.clone(),
                facts.active_turn_id.as_deref(),
            );
        self.note_transcript_residency_controller_plan(&plan);
        let residency_counts = self.transcript_history_window.residency_retained_counts();
        log_transcript_residency_target_decision(
            self.selected_thread_id(),
            &plan,
            &residency_counts,
        );

        let released_content = self.release_resident_turn_payloads_for_plan(&plan);
        let request = allow_request
            .then(|| self.begin_loading_thread_history_page_for_residency_plan(&facts, &plan))
            .flatten();

        TranscriptResidencyControllerUpdate {
            request,
            released_content,
        }
    }

    fn sync_transcript_residency_derived_byte_estimates(
        &mut self,
        presentation_planning_range: &Range<usize>,
    ) -> bool {
        let estimates = self
            .transcript_presentation
            .derived_byte_estimates_by_turn_id_for_range(presentation_planning_range);
        self.transcript_history_window
            .update_residency_derived_byte_estimates(estimates)
    }

    fn transcript_residency_controller_update_deferred(
        &self,
        presentation_visible_range: &Range<usize>,
    ) -> bool {
        self.staged_selected_thread_activation.is_some()
            || self.transcript_residency_controller_viewport_unready(presentation_visible_range)
    }

    fn transcript_residency_controller_viewport_unready(
        &self,
        presentation_visible_range: &Range<usize>,
    ) -> bool {
        self.transcript_presentation.len() > 0
            && (presentation_visible_range.is_empty()
                || f32::from(self.transcript_list_state.viewport_bounds().size.height) <= 0.0)
    }

    fn transcript_residency_controller_facts(
        &self,
        presentation_visible_range: &Range<usize>,
        presentation_planning_range: Range<usize>,
        request_allowed: bool,
    ) -> TranscriptResidencyControllerFacts {
        let source_visible_range = self
            .transcript_presentation
            .source_range_for_presentation_range(presentation_visible_range);
        let viewport_height =
            pixels_to_residency_units(self.transcript_list_state.viewport_bounds().size.height);
        let source_planning_range = self
            .transcript_history_window
            .source_planning_range_for_visible_range(source_visible_range.clone(), viewport_height);
        let measured_turn_heights = self
            .transcript_presentation
            .presentation_range_for_source_range(&source_planning_range)
            .filter_map(|presentation_index| {
                let source_position = self
                    .transcript_presentation
                    .source_turn_index_at(presentation_index)?;
                let measured_height = self
                    .transcript_list_state
                    .measured_item_size(presentation_index)?;
                Some(TranscriptResidencyMeasuredTurnHeight {
                    source_position,
                    measured_height: pixels_to_residency_units(measured_height.height),
                })
            })
            .collect::<Vec<_>>();
        let streamed_turn_fills =
            self.transcript_streamed_residency_fill_facts_for_source_range(&source_planning_range);
        let active_turn_id = self.active_transcript_residency_turn_id();
        let residency_counts = self.transcript_history_window.residency_retained_counts();
        let signature = TranscriptResidencyControllerSignature {
            selected_thread_id: self.selected_thread_id().map(str::to_string),
            presentation_visible_range: presentation_visible_range.clone(),
            presentation_planning_range: presentation_planning_range.clone(),
            source_visible_range: source_visible_range.clone(),
            source_planning_range: source_planning_range.clone(),
            viewport_height,
            measured_turn_heights: measured_turn_heights.clone(),
            streamed_turn_fills: streamed_turn_fills.clone(),
            active_turn_id: active_turn_id.clone(),
            residency_revision: self.transcript_history_window.residency_revision(),
            resident_turn_count: residency_counts.resident_turns,
            pinned_turn_count: residency_counts.pinned_turns,
            indexed_turn_count: self.transcript_history_window.indexed_turn_count(),
            staged_admission_pending: self.staged_transcript_residency_page.is_some(),
            request_allowed,
        };
        TranscriptResidencyControllerFacts {
            presentation_visible_range: presentation_visible_range.clone(),
            presentation_planning_range,
            source_visible_range,
            source_planning_range,
            viewport_height,
            measured_turn_heights,
            streamed_turn_fills,
            active_turn_id,
            signature,
        }
    }

    fn transcript_residency_planning_presentation_range(
        &self,
        presentation_visible_range: &Range<usize>,
    ) -> Range<usize> {
        let turn_count = self.transcript_presentation.len();
        if turn_count == 0 {
            return 0..0;
        }

        let visible_start = presentation_visible_range.start.min(turn_count);
        let visible_end = presentation_visible_range
            .end
            .min(turn_count)
            .max(visible_start);
        let residency = self.transcript_history_window.residency_retained_counts();
        let margin_viewports = residency
            .leading_viewport_margins
            .max(residency.trailing_viewport_margins);
        let viewport_height = self.transcript_list_state.viewport_bounds().size.height;
        let planning_range = self
            .transcript_list_state
            .range_with_vertical_margin(viewport_height * margin_viewports as f32);
        let start = planning_range.start.min(visible_start).min(turn_count);
        let end = planning_range
            .end
            .max(visible_end)
            .min(turn_count)
            .max(start);
        start..end
    }

    fn active_transcript_residency_turn_id(&self) -> Option<String> {
        let selected_thread_id = self.selected_thread_id()?;
        let active = self.execution_details.active_turn_identity()?;
        if active.thread_id.as_deref() != Some(selected_thread_id) {
            return None;
        }
        active.turn_id
    }

    fn begin_loading_thread_history_page_for_residency_plan(
        &mut self,
        facts: &TranscriptResidencyControllerFacts,
        plan: &TranscriptResidencyTargetPlan,
    ) -> Option<PendingTranscriptResidencyPageRequest> {
        if self.staged_transcript_residency_page.is_some() {
            return None;
        }
        let thread_id = self.selected_thread_id()?.to_string();
        let request = self
            .transcript_history_window
            .begin_loading_page_for_residency_target_plan(plan, &facts.source_visible_range)?;
        self.transcript_residency_page_cancellation_generation = self
            .transcript_residency_page_cancellation_generation
            .saturating_add(1);
        self.note_transcript_residency_request_started(
            requested_turn_count_for_plan(plan).max(THREAD_HISTORY_PAGE_LIMIT as usize),
        );
        Some(PendingTranscriptResidencyPageRequest::new(
            thread_id,
            request,
            facts.presentation_visible_range.clone(),
            facts.source_visible_range.clone(),
            self.transcript_residency_page_cancellation_generation,
        ))
    }
}

fn pixels_to_residency_units(pixels: Pixels) -> usize {
    f32::from(pixels).ceil().max(1.0) as usize
}

fn requested_turn_count_for_plan(plan: &TranscriptResidencyTargetPlan) -> usize {
    plan.missing_transport_ranges
        .iter()
        .map(|range| range.len())
        .sum()
}
