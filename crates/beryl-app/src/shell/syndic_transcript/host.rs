use std::ops::Range;

use crate::diagnostic_dynamic_tools::{
    PresentationRangeDiagnostic, TranscriptFrameAnchorDiagnostic, TranscriptFrameMetric,
    TranscriptFrameMetricsLog, TranscriptFrameMetricsSnapshot,
    TranscriptFrameRenderBudgetDiagnostic, TranscriptRenderedFrameDiagnostic,
    TranscriptResidencyRequestDiagnostic, TranscriptScrollInputAnchorDiagnostic,
    TranscriptScrollInputDiagnostic, TranscriptScrollInputLog,
    TranscriptSegmentMeasurementCommitDiagnostic, TranscriptSemanticViewportDiagnostic,
};

use super::{
    DemandFact, DemandFactSinkSnapshot, ManualTranscriptScrollCommand,
    PreparedTranscriptActivation, RealizedFrameAnchor, RealizedFrameClamp, RealizedFrameRequest,
    RealizedFrameScrollController, RealizedFrameScrollStateSnapshot, RealizedFrameWindow,
    ResidentContextMenuCommand, ResidentContextMenuCommandTarget, ResidentContextMenuOutcome,
    ResidentMediaActionCommand, ResidentMediaActionOutcome, ResidentMediaActionUnavailable,
    ResidentMediaCopyCommandTarget, ResidentMediaPreviewCommandTarget,
    ResidentMediaSaveCommandTarget, ResidentProviderResponseEffect, ResidentQuoteCommand,
    ResidentQuoteOutcome, ResidentSelectionCommand, ResidentSelectionOutcome,
    ResidentSelectionUnavailable, ResidentTranscriptContextMenuTarget,
    ResidentTranscriptCopyPayload, ResidentTranscriptCore, ResidentTranscriptMediaActionTarget,
    ResidentTranscriptMediaPayload, ResidentTranscriptQuotePayload, ResidentTranscriptQuoteTarget,
    ResidentTranscriptSelection, ResidentTranscriptSnapshot, ResidentTranscriptStatusFacts,
    SyndicTranscriptDiagnosticSnapshot, TranscriptActivationOutcome, TranscriptActivationPlacement,
    TranscriptActivationSeed, TranscriptActivationSource, TranscriptCommandResult,
    TranscriptProviderResponse,
};

#[derive(Clone, Debug)]
pub(crate) struct SyndicTranscriptHost {
    core: ResidentTranscriptCore,
    scroll_controller: RealizedFrameScrollController,
    frame_metrics: TranscriptFrameMetricsLog,
    scroll_inputs: TranscriptScrollInputLog,
    last_frame_window: Option<RealizedFrameWindow>,
}

impl Default for SyndicTranscriptHost {
    fn default() -> Self {
        Self::empty()
    }
}

impl SyndicTranscriptHost {
    pub(crate) fn empty() -> Self {
        Self {
            core: ResidentTranscriptCore::empty(),
            scroll_controller: RealizedFrameScrollController::new(),
            frame_metrics: TranscriptFrameMetricsLog::default(),
            scroll_inputs: TranscriptScrollInputLog::default(),
            last_frame_window: None,
        }
    }

    pub(crate) fn snapshot(&self) -> ResidentTranscriptSnapshot {
        self.core.presentation_snapshot()
    }

    pub(crate) fn status_facts(&self) -> ResidentTranscriptStatusFacts {
        ResidentTranscriptStatusFacts::from_core_snapshot(
            &self.core.core_snapshot(),
            self.scroll_controller.state_snapshot(),
        )
    }

    pub(crate) fn demand_fact_snapshot(&self) -> DemandFactSinkSnapshot {
        self.core.demand_fact_snapshot()
    }

    pub(crate) fn push_demand_fact(&mut self, fact: DemandFact) {
        self.core.push_demand_fact(fact);
    }

    pub(crate) fn begin_activation(
        &mut self,
        seed: TranscriptActivationSeed,
    ) -> TranscriptActivationOutcome {
        match seed.placement {
            TranscriptActivationPlacement::Tail => {
                self.scroll_controller.begin_live_tail_following()
            }
            TranscriptActivationPlacement::Start | TranscriptActivationPlacement::Position(_) => {
                self.scroll_controller.detach_live_tail_following();
            }
        }
        self.core.begin_activation(seed)
    }

    pub(crate) fn apply_prepared_activation(
        &mut self,
        prepared: PreparedTranscriptActivation,
        source: TranscriptActivationSource,
    ) -> TranscriptActivationOutcome {
        match prepared.placement {
            TranscriptActivationPlacement::Tail => {
                self.scroll_controller.begin_live_tail_following()
            }
            TranscriptActivationPlacement::Start | TranscriptActivationPlacement::Position(_) => {
                self.scroll_controller.detach_live_tail_following();
            }
        }
        self.core.apply_prepared_activation(prepared, source)
    }

    pub(crate) fn handle_provider_response(
        &mut self,
        response: TranscriptProviderResponse,
    ) -> ResidentProviderResponseEffect {
        self.core.handle_provider_response(response)
    }

    pub(crate) fn drain_demand_facts(&mut self) -> Vec<DemandFact> {
        self.core.drain_demand_facts()
    }

    pub(crate) fn realize_frame(&mut self, request: RealizedFrameRequest) -> RealizedFrameWindow {
        let snapshot = self.core.presentation_snapshot();
        let window = self.scroll_controller.realize(&snapshot, request);
        for fact in &window.demand_facts {
            self.core.push_demand_fact(fact.clone());
        }
        self.record_frame_metric(&snapshot, &window);
        self.last_frame_window = Some(window.clone());
        window
    }

    pub(crate) fn manual_scroll(
        &mut self,
        command: ManualTranscriptScrollCommand,
    ) -> RealizedFrameWindow {
        let before_state = self.scroll_controller.state_snapshot();
        let before_window = self.last_frame_window.clone();
        let window = self.realize_frame(command.frame_request());
        self.record_scroll_input(command, before_state, before_window.as_ref(), &window);
        window
    }

    pub(crate) fn apply_resident_selection(
        &mut self,
        command: ResidentSelectionCommand,
    ) -> ResidentSelectionOutcome {
        self.core.apply_resident_selection(command)
    }

    pub(crate) fn clear_resident_selection(&mut self) -> ResidentSelectionOutcome {
        self.core.clear_resident_selection()
    }

    pub(crate) fn resident_copy_payload(
        &self,
    ) -> Result<ResidentTranscriptCopyPayload, ResidentSelectionUnavailable> {
        self.core.resident_copy_payload()
    }

    pub(crate) fn resident_selection(&self) -> Option<ResidentTranscriptSelection> {
        self.core.resident_selection()
    }

    pub(crate) fn apply_resident_quote_target(
        &mut self,
        command: ResidentQuoteCommand,
    ) -> ResidentQuoteOutcome {
        self.core.apply_resident_quote_target(command)
    }

    pub(crate) fn clear_resident_quote_target(&mut self) -> ResidentQuoteOutcome {
        self.core.clear_resident_quote_target()
    }

    pub(crate) fn resident_quote_payload(
        &self,
    ) -> Result<ResidentTranscriptQuotePayload, ResidentSelectionUnavailable> {
        self.core.resident_quote_payload()
    }

    pub(crate) fn resident_quote_target(&self) -> Option<ResidentTranscriptQuoteTarget> {
        self.core.resident_quote_target()
    }

    pub(crate) fn apply_resident_context_menu_target(
        &mut self,
        command: ResidentContextMenuCommand,
    ) -> ResidentContextMenuOutcome {
        self.core.apply_resident_context_menu_target(command)
    }

    pub(crate) fn clear_resident_context_menu_target(&mut self) -> ResidentContextMenuOutcome {
        self.core.clear_resident_context_menu_target()
    }

    pub(crate) fn resident_context_menu_target(
        &self,
    ) -> Option<ResidentTranscriptContextMenuTarget> {
        self.core.resident_context_menu_target()
    }

    pub(crate) fn resident_context_menu_command_target(&self) -> ResidentContextMenuCommandTarget {
        ResidentContextMenuCommandTarget::from_active_target(
            self.core.resident_context_menu_target(),
        )
    }

    pub(crate) fn apply_resident_media_action_target(
        &mut self,
        command: ResidentMediaActionCommand,
    ) -> ResidentMediaActionOutcome {
        self.core.apply_resident_media_action_target(command)
    }

    pub(crate) fn clear_resident_media_action_target(&mut self) -> ResidentMediaActionOutcome {
        self.core.clear_resident_media_action_target()
    }

    pub(crate) fn resident_media_action_payload(
        &self,
    ) -> Result<ResidentTranscriptMediaPayload, ResidentMediaActionUnavailable> {
        self.core.resident_media_action_payload()
    }

    pub(crate) fn resident_media_action_target(
        &self,
    ) -> Option<ResidentTranscriptMediaActionTarget> {
        self.core.resident_media_action_target()
    }

    pub(crate) fn resident_media_preview_command_target(
        &self,
    ) -> ResidentMediaPreviewCommandTarget {
        ResidentMediaPreviewCommandTarget::from_resident_payload(
            self.core.resident_media_action_payload(),
        )
    }

    pub(crate) fn resident_media_copy_command_target(&self) -> ResidentMediaCopyCommandTarget {
        ResidentMediaCopyCommandTarget::from_resident_payload(
            self.core.resident_media_action_payload(),
        )
    }

    pub(crate) fn resident_media_save_command_target(&self) -> ResidentMediaSaveCommandTarget {
        ResidentMediaSaveCommandTarget::from_resident_payload(
            self.core.resident_media_action_payload(),
        )
    }

    pub(crate) fn diagnostic_snapshot(&self) -> SyndicTranscriptDiagnosticSnapshot {
        let core_snapshot = self.core.core_snapshot();
        let scroll_snapshot = self.scroll_controller.state_snapshot();
        let mut snapshot = SyndicTranscriptDiagnosticSnapshot::from_core_snapshot(&core_snapshot);
        snapshot.frame.scroll_mode = scroll_snapshot.scroll_mode.diagnostic_label();
        snapshot.frame.anchor_record = scroll_snapshot.anchor.map(|anchor| anchor.record_id.0);
        snapshot
    }

    fn record_frame_metric(
        &mut self,
        snapshot: &ResidentTranscriptSnapshot,
        window: &RealizedFrameWindow,
    ) {
        let visible_row_count = window.visible_range.len();
        let retained_bytes = snapshot
            .records
            .iter()
            .map(|record| record.estimated_bytes)
            .sum();
        let metric = TranscriptFrameMetric {
            sequence: 0,
            selected_thread_id: None,
            semantic_viewport: TranscriptSemanticViewportDiagnostic {
                viewport_mode: "resident".to_string(),
                live_autoscroll: self
                    .scroll_controller
                    .state_snapshot()
                    .scroll_mode
                    .diagnostic_label()
                    .to_string(),
                anchor_row_index: window.anchor.as_ref().map(|anchor| anchor.index),
                anchor_row_identity: window
                    .anchor
                    .as_ref()
                    .map(|anchor| anchor.record_id.0.clone()),
                anchor_chunk_index: None,
                anchor_chunk_identity: None,
                anchor_placement: None,
                rendered_chunk_range: Some(diagnostic_range(&window.overscan_range)),
                chunk_count: Some(window.records.len()),
                fill_direction: None,
            },
            rendered_frame: rendered_frame_diagnostic(snapshot, window),
            presentation_range: Some(PresentationRangeDiagnostic {
                start: 0,
                end: snapshot.records.len(),
            }),
            visible_range: Some(diagnostic_range(&window.visible_range)),
            total_loaded_turn_count: snapshot.records.len(),
            total_item_count: Some(snapshot.records.len()),
            total_text_chars: None,
            presentation_range_len: snapshot.records.len(),
            visible_row_count,
            panel_state_inspected_row_count: snapshot.records.len(),
            residency_resident_turn_count: snapshot.records.len(),
            residency_retained_bytes: retained_bytes,
            residency_in_flight_requests: 0,
            residency_budget_reason: None,
            residency_requests: TranscriptResidencyRequestDiagnostic::default(),
            clamp_reasons: window
                .clamp
                .as_ref()
                .map(clamp_reason)
                .into_iter()
                .collect(),
            segment_measurement_commit: TranscriptSegmentMeasurementCommitDiagnostic::default(),
            active_turn_source_pin_active: false,
            active_turn_source_retained_bytes: 0,
            active_turn_source_budget_max_bytes: 0,
            active_turn_source_budget_fallback_active: false,
            resident_budget_fallback_row_count: 0,
            active_source_budget_fallback_row_count: 0,
            transcript_scrollbar_visible: visible_row_count < snapshot.records.len(),
            frame_micros: 0,
            snapshot_micros: 0,
            render_state_pruning_micros: 0,
            style_snapshot_micros: 0,
            composer_measurement_micros: 0,
            chunk_window_computation_micros: 0,
            render_budget: TranscriptFrameRenderBudgetDiagnostic {
                chunk_window_count: window.records.len(),
                admitted_chunk_count: window.records.len(),
                rendered_chunk_count: window.records.len(),
                fallback_chunk_count: 0,
                rendered_cost_units: window.records.len(),
                fallback_cost_units: 0,
                largest_chunk_cost_units: 1,
                max_chunk_cost_units: usize::MAX,
                max_frame_cost_units: usize::MAX,
                fallback_reasons: Vec::new(),
            },
            row_build_total_micros: 0,
            row_prepaint_total_micros: 0,
            inline_text_construction_micros: 0,
            code_panel_render_micros: 0,
            media_run_render_micros: 0,
            media_preload_micros: 0,
            slowest_row_build_micros: 0,
            slowest_row_build_index: None,
            slowest_row_build_identity: None,
            slowest_row_prepaint_micros: 0,
            slowest_row_prepaint_index: None,
            slowest_row_prepaint_identity: None,
            largest_visible_row_text_chars: 0,
            largest_visible_row_text_chars_index: None,
            largest_visible_row_item_count: 0,
            largest_visible_row_item_count_index: None,
            dominant_cost_category: "resident-transcript-frame".to_string(),
        };
        self.frame_metrics.record(metric);
    }

    fn record_scroll_input(
        &mut self,
        command: ManualTranscriptScrollCommand,
        before_state: RealizedFrameScrollStateSnapshot,
        before_window: Option<&RealizedFrameWindow>,
        after_window: &RealizedFrameWindow,
    ) {
        let snapshot = self.core.presentation_snapshot();
        let delta_px = command.delta_px;
        let requested_delta = f64::from(delta_px.abs());
        let before_absolute_offset = before_state
            .anchor
            .as_ref()
            .or_else(|| before_window.and_then(|window| window.anchor.as_ref()))
            .map(|anchor| frame_anchor_absolute_offset(&snapshot, before_window, anchor));
        let after_absolute_offset = after_window
            .anchor
            .as_ref()
            .map(|anchor| frame_anchor_absolute_offset(&snapshot, Some(after_window), anchor));
        let consumed_delta = match (before_absolute_offset, after_absolute_offset) {
            (Some(before), Some(after)) => (after - before).abs(),
            _ => 0.0,
        };
        let changed = consumed_delta > f64::EPSILON;
        let direction = match (before_absolute_offset, after_absolute_offset) {
            (Some(before), Some(after)) if after < before => "up",
            (Some(before), Some(after)) if after > before => "down",
            _ => scroll_direction_label(delta_px),
        };
        let before_anchor = before_state
            .anchor
            .as_ref()
            .or_else(|| before_window.and_then(|window| window.anchor.as_ref()))
            .zip(before_absolute_offset)
            .map(|(anchor, absolute_offset)| {
                scroll_input_anchor_diagnostic(anchor, absolute_offset)
            });
        let after_anchor = after_window.anchor.as_ref().zip(after_absolute_offset).map(
            |(anchor, absolute_offset)| scroll_input_anchor_diagnostic(anchor, absolute_offset),
        );
        self.scroll_inputs.record(TranscriptScrollInputDiagnostic {
            sequence: 0,
            input_kind: "wheel".to_string(),
            direction: direction.to_string(),
            consumed: changed,
            changed,
            requested_delta,
            consumed_delta,
            residual_delta: (requested_delta - consumed_delta).max(0.0),
            before_anchor,
            after_anchor,
            before_visible_segment_range: before_window
                .map(|window| diagnostic_range(&window.visible_range)),
            after_visible_segment_range: Some(diagnostic_range(&after_window.visible_range)),
            before_rendered_frame_range: before_window
                .map(|window| diagnostic_range(&window.overscan_range)),
            after_rendered_frame_range: Some(diagnostic_range(&after_window.overscan_range)),
            clamp_or_expansion_reason: after_window.clamp.as_ref().map(clamp_reason),
        });
    }

    pub(crate) fn frame_metrics_snapshot(&self) -> TranscriptFrameMetricsSnapshot {
        let mut snapshot = self.frame_metrics.snapshot();
        snapshot.scroll_inputs = self.scroll_inputs.snapshot();
        snapshot
    }

    pub(crate) fn unavailable_command(&self, command: &'static str) -> TranscriptCommandResult {
        TranscriptCommandResult::unavailable(command)
    }
}

fn diagnostic_range(range: &Range<usize>) -> PresentationRangeDiagnostic {
    PresentationRangeDiagnostic {
        start: range.start,
        end: range.end,
    }
}

fn rendered_frame_diagnostic(
    snapshot: &ResidentTranscriptSnapshot,
    window: &RealizedFrameWindow,
) -> TranscriptRenderedFrameDiagnostic {
    let visible_segment_count = window.visible_range.len();
    let overscan_segment_count = window.overscan_range.len();
    let local_scroll_offset = window
        .anchor
        .as_ref()
        .map(|anchor| frame_anchor_absolute_offset(snapshot, Some(window), anchor))
        .unwrap_or(0.0);
    TranscriptRenderedFrameDiagnostic {
        anchor: window.anchor.as_ref().map(frame_anchor_diagnostic),
        total_segment_count: snapshot.records.len(),
        visible_segment_count,
        visible_segment_range: Some(diagnostic_range(&window.visible_range)),
        overscan_segment_count,
        leading_overscan_segment_count: window
            .visible_range
            .start
            .saturating_sub(window.overscan_range.start),
        trailing_overscan_segment_count: window
            .overscan_range
            .end
            .saturating_sub(window.visible_range.end),
        local_scroll_offset,
        local_scroll_max: diagnostic_content_height(snapshot, Some(window))
            .max(local_scroll_offset),
    }
}

fn frame_anchor_absolute_offset(
    snapshot: &ResidentTranscriptSnapshot,
    window: Option<&RealizedFrameWindow>,
    anchor: &RealizedFrameAnchor,
) -> f64 {
    let preceding_height = (0..anchor.index)
        .map(|index| diagnostic_record_height(snapshot, window, index))
        .sum::<f64>();
    preceding_height - f64::from(anchor.viewport_y_px)
}

fn diagnostic_content_height(
    snapshot: &ResidentTranscriptSnapshot,
    window: Option<&RealizedFrameWindow>,
) -> f64 {
    (0..snapshot.records.len())
        .map(|index| diagnostic_record_height(snapshot, window, index))
        .sum()
}

fn diagnostic_record_height(
    snapshot: &ResidentTranscriptSnapshot,
    window: Option<&RealizedFrameWindow>,
    index: usize,
) -> f64 {
    if let Some(record) = window.and_then(|window| {
        window
            .records
            .iter()
            .find(|record| record.index == index)
            .map(|record| record.height_px)
    }) {
        return f64::from(record);
    }

    window
        .and_then(|window| {
            (!window.records.is_empty()).then(|| {
                window
                    .records
                    .iter()
                    .map(|record| f64::from(record.height_px))
                    .sum::<f64>()
                    / window.records.len() as f64
            })
        })
        .unwrap_or_else(|| snapshot.records.get(index).map(|_| 1.0).unwrap_or_default())
}

fn frame_anchor_diagnostic(anchor: &RealizedFrameAnchor) -> TranscriptFrameAnchorDiagnostic {
    TranscriptFrameAnchorDiagnostic {
        segment_kind: "record".to_string(),
        row_index: anchor.index,
        row_identity: Some(anchor.record_id.0.clone()),
        chunk_index: None,
        chunk_identity: None,
        fallback_reason: None,
        placement: None,
    }
}

fn scroll_input_anchor_diagnostic(
    anchor: &RealizedFrameAnchor,
    absolute_rendered_offset: f64,
) -> TranscriptScrollInputAnchorDiagnostic {
    TranscriptScrollInputAnchorDiagnostic {
        segment: frame_anchor_diagnostic(anchor),
        segment_local_offset: f64::from(anchor.viewport_y_px),
        absolute_rendered_offset,
    }
}

fn scroll_direction_label(delta_px: f32) -> &'static str {
    if delta_px < 0.0 {
        "up"
    } else if delta_px > 0.0 {
        "down"
    } else {
        "none"
    }
}

fn clamp_reason(clamp: &RealizedFrameClamp) -> String {
    format!("clamped-{:?}", clamp.direction).to_ascii_lowercase()
}
