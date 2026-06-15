use gpui::{Context, Pixels, ScrollDelta, ScrollWheelEvent, TouchPhase, Window, point, px};

use crate::diagnostic_dynamic_tools::{
    PresentationRangeDiagnostic, TranscriptFrameAnchorDiagnostic,
    TranscriptScrollInputAnchorDiagnostic, TranscriptScrollInputDiagnostic,
};
use crate::gui_control_dynamic_tools::ScrollTranscriptArguments;

pub(crate) use super::transcript_viewport_scroll_coordinator::TranscriptStreamedNavigationSnapshot;

use super::{
    ConversationSurfaceState, ListOffset, ListScrollEvent, ListScrollPosition, ShellView,
    TranscriptHistoryBoundaryState,
    transcript_scroll::TranscriptTurnJumpDirection,
    transcript_viewport::{
        TranscriptFrameSegment, TranscriptFrameSegmentKind, TranscriptViewportBoundary,
        TranscriptViewportChunkAnchor, TranscriptViewportFrame, TranscriptViewportMode,
        TranscriptViewportNavigationDirection, TranscriptViewportPlacement,
        TranscriptViewportScrollCursor, TranscriptViewportTurnAnchor, TranscriptViewportTurnTarget,
        TranscriptViewportTurnTargetKind,
    },
    transcript_viewport_scroll_coordinator::{
        TranscriptViewportNavigationApplication, TranscriptViewportScrollKindForShell,
        apply_transcript_viewport_scroll as apply_coordinated_transcript_viewport_scroll,
        scroll_transcript_turn_to_placement, sync_transcript_list_state_to_viewport_anchor,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TranscriptContentScrollSignature {
    pub(super) visible_range: std::ops::Range<usize>,
    pub(super) item_count: usize,
    pub(super) scroll_position: ListScrollPosition,
    pub(super) user_scrolled: bool,
    pub(super) anchor_preserves_offset: bool,
    pub(super) resident_boundary: TranscriptHistoryBoundaryState,
}

pub(super) fn diagnostic_transcript_scroll_wheel_event(
    delta_y_pixels: f32,
    precise: bool,
    window: &Window,
) -> ScrollWheelEvent {
    let delta = if precise {
        ScrollDelta::Pixels(point(px(0.0), px(delta_y_pixels)))
    } else {
        let line_height = f32::from(window.line_height()).max(1.0);
        ScrollDelta::Lines(point(0.0, delta_y_pixels / line_height))
    };
    ScrollWheelEvent {
        delta,
        touch_phase: TouchPhase::Moved,
        ..ScrollWheelEvent::default()
    }
}

impl ConversationSurfaceState {
    pub(super) fn transcript_content_scroll_signature(
        &self,
        event: &ListScrollEvent,
    ) -> TranscriptContentScrollSignature {
        let source_visible_range = self
            .transcript_presentation
            .source_range_for_presentation_range(&event.visible_range);
        TranscriptContentScrollSignature {
            visible_range: event.visible_range.clone(),
            item_count: event.count,
            scroll_position: self.transcript_list_state.scroll_position(),
            user_scrolled: self.transcript_user_scrolled,
            anchor_preserves_offset: self.transcript_live_scroll_preserves_anchor_offset(),
            resident_boundary: self
                .transcript_history_window
                .boundary_state_for_visible_range(&source_visible_range),
        }
    }

    pub(super) fn note_transcript_content_scroll_signature(
        &mut self,
        event: &ListScrollEvent,
    ) -> bool {
        let signature = self.transcript_content_scroll_signature(event);
        if self.last_transcript_content_scroll_signature.as_ref() == Some(&signature) {
            return false;
        }
        self.last_transcript_content_scroll_signature = Some(signature);
        true
    }

    pub(super) fn replace_transcript_streamed_navigation_snapshot(
        &mut self,
        snapshot: Option<TranscriptStreamedNavigationSnapshot>,
    ) -> bool {
        if self.transcript_streamed_navigation_snapshot == snapshot {
            return false;
        }
        self.transcript_streamed_navigation_snapshot = snapshot;
        true
    }

    pub(super) fn replace_transcript_navigation_frame_snapshot(
        &mut self,
        snapshot: Option<TranscriptViewportFrame>,
    ) -> bool {
        let event_time_changed = self.transcript_event_time_scroll.clear();
        if self.transcript_navigation_frame_snapshot == snapshot {
            return event_time_changed;
        }
        self.transcript_navigation_frame_snapshot = snapshot;
        true
    }

    pub(super) fn current_transcript_scroll_navigation_snapshots(
        &self,
    ) -> (
        Option<TranscriptStreamedNavigationSnapshot>,
        Option<TranscriptViewportFrame>,
    ) {
        (
            self.transcript_event_time_scroll
                .effective_streamed_snapshot(self.transcript_streamed_navigation_snapshot.clone()),
            self.transcript_event_time_scroll
                .effective_rendered_frame(self.transcript_navigation_frame_snapshot.clone()),
        )
    }

    pub(super) fn current_transcript_navigation_frame_snapshot(
        &self,
    ) -> Option<&TranscriptViewportFrame> {
        self.transcript_navigation_frame_snapshot.as_ref()
    }

    fn blocked_transcript_event_time_scroll_application(
        &self,
        direction: TranscriptViewportNavigationDirection,
        distance: Pixels,
    ) -> Option<TranscriptViewportNavigationApplication> {
        self.transcript_event_time_scroll
            .is_blocked(direction)
            .then_some(TranscriptViewportNavigationApplication {
                consumed: true,
                blocked_direction: Some(direction),
                residual_delta: Some(distance),
                ..TranscriptViewportNavigationApplication::default()
            })
    }

    fn note_transcript_event_time_scroll_application(
        &mut self,
        direction: TranscriptViewportNavigationDirection,
        application: &TranscriptViewportNavigationApplication,
    ) {
        self.transcript_event_time_scroll
            .apply_navigation_application(direction, application);
    }

    pub(super) fn clear_transcript_event_time_scroll(&mut self) -> bool {
        self.transcript_event_time_scroll.clear()
    }

    pub(super) fn transcript_viewport_turn_target_for_row(
        &self,
        row_index: usize,
        streamed_placement: TranscriptViewportPlacement,
    ) -> Option<TranscriptViewportTurnTarget> {
        let row = self.transcript_presentation.turn_at(row_index)?;
        let turn = TranscriptViewportTurnAnchor::new(
            row.index,
            Some(row.identity.as_str().to_string()),
            row.turn.thread_id.clone(),
            row.turn.turn_id.clone(),
        );
        let chunks = row.model.chunk_presentation().chunks();
        if row.model.chunk_presentation().requires_chunking() && !chunks.is_empty() {
            let chunk_index = match streamed_placement {
                TranscriptViewportPlacement::Top => 0,
                TranscriptViewportPlacement::Bottom => chunks.len().saturating_sub(1),
            };
            let chunk = chunks.get(chunk_index)?;
            return Some(TranscriptViewportTurnTarget::streamed(
                turn,
                TranscriptViewportChunkAnchor::new(chunk_index, chunk.identity.clone()),
                chunks.len(),
                streamed_placement,
            ));
        }
        Some(TranscriptViewportTurnTarget::ordinary(turn))
    }

    pub(super) fn transcript_viewport_tail_target(&self) -> Option<TranscriptViewportTurnTarget> {
        let last = self.transcript_presentation.len().checked_sub(1)?;
        self.transcript_viewport_turn_target_for_row(last, TranscriptViewportPlacement::Bottom)
    }

    pub(super) fn reset_transcript_viewport_to_tail_anchor(&mut self) -> bool {
        self.clear_transcript_event_time_scroll();
        let item_count = self.transcript_list_item_count();
        let target = self.transcript_viewport_tail_target();
        self.transcript_viewport
            .reset_to_tail_target(target, item_count);
        true
    }

    pub(super) fn transcript_viewport_turn_jump_target_for_frame(
        &self,
        direction: TranscriptTurnJumpDirection,
    ) -> Option<TranscriptViewportTurnTarget> {
        let frame = self.current_transcript_navigation_frame_snapshot()?;
        let first_visible = frame.first_visible_segment()?;
        let current_row = first_visible
            .key
            .turn
            .turn_index
            .min(self.transcript_presentation.len().saturating_sub(1));

        match direction {
            TranscriptTurnJumpDirection::Up => {
                if frame.local_scroll_offset() > px(0.0)
                    || frame_segment_is_streamed_after_turn_start(first_visible)
                    || viewport_mode_matches_frame_segment_with_non_top_placement(
                        self.transcript_viewport.mode(),
                        first_visible,
                    )
                {
                    return self.transcript_viewport_turn_target_for_row(
                        current_row,
                        TranscriptViewportPlacement::Top,
                    );
                }
                current_row.checked_sub(1).and_then(|target_row| {
                    self.transcript_viewport_turn_target_for_row(
                        target_row,
                        TranscriptViewportPlacement::Top,
                    )
                })
            }
            TranscriptTurnJumpDirection::Down => {
                let next_row = current_row.saturating_add(1);
                if next_row < self.transcript_presentation.len() {
                    return self.transcript_viewport_turn_target_for_row(
                        next_row,
                        TranscriptViewportPlacement::Top,
                    );
                }
                if matches!(
                    self.transcript_list_state.scroll_position(),
                    ListScrollPosition::Bottom
                ) || viewport_mode_matches_frame_segment_with_bottom_placement(
                    self.transcript_viewport.mode(),
                    first_visible,
                ) {
                    return None;
                }
                self.transcript_viewport_turn_target_for_row(
                    current_row,
                    TranscriptViewportPlacement::Bottom,
                )
            }
        }
    }

    fn apply_transcript_viewport_scroll(
        &mut self,
        direction: TranscriptViewportNavigationDirection,
        distance: Pixels,
        kind: TranscriptViewportScrollKindForShell,
        streamed: Option<TranscriptStreamedNavigationSnapshot>,
        rendered_frame: Option<TranscriptViewportFrame>,
    ) -> TranscriptViewportNavigationApplication {
        if let Some(application) =
            self.blocked_transcript_event_time_scroll_application(direction, distance)
        {
            return application;
        }
        let application = apply_coordinated_transcript_viewport_scroll(
            &mut self.transcript_viewport,
            &self.transcript_list_state,
            direction,
            distance,
            kind,
            streamed,
            rendered_frame,
        );
        self.note_transcript_event_time_scroll_application(direction, &application);
        application
    }

    pub(super) fn apply_transcript_viewport_turn_jump(
        &mut self,
        target: TranscriptViewportTurnTarget,
    ) -> TranscriptViewportNavigationApplication {
        self.clear_transcript_event_time_scroll();
        let row_index = target.turn.turn_index;
        let outcome = self
            .transcript_viewport
            .apply_turn_jump(Some(target.clone()));
        match target.kind {
            TranscriptViewportTurnTargetKind::Ordinary => {
                sync_transcript_list_state_to_viewport_anchor(
                    &self.transcript_viewport,
                    &self.transcript_list_state,
                );
            }
            TranscriptViewportTurnTargetKind::Streamed { placement, .. } => {
                self.transcript_list_state
                    .invalidate_item_measurement(row_index);
                scroll_transcript_turn_to_placement(
                    &self.transcript_list_state,
                    row_index,
                    placement,
                    px(0.0),
                );
            }
        }
        TranscriptViewportNavigationApplication {
            changed: outcome.changed,
            consumed: true,
            ..TranscriptViewportNavigationApplication::default()
        }
    }

    pub(super) fn apply_transcript_segment_measurement_anchor_correction(
        &mut self,
        correction: Pixels,
    ) -> bool {
        if correction == px(0.0) {
            return false;
        }
        let event_time_changed = self.clear_transcript_event_time_scroll();
        if !self
            .transcript_viewport
            .apply_segment_measurement_anchor_correction(correction)
        {
            return event_time_changed;
        }
        match self.transcript_viewport.mode() {
            TranscriptViewportMode::Empty => {}
            TranscriptViewportMode::Ordinary(anchor) => {
                self.transcript_list_state
                    .scroll_to_position(ListScrollPosition::Content(ListOffset {
                        item_ix: anchor.turn.turn_index,
                        offset_in_item: anchor.local_offset,
                    }));
            }
            TranscriptViewportMode::Streamed(anchor) => {
                if let Some(offset) = anchor.local_anchor_offset {
                    self.transcript_list_state
                        .scroll_to_position(ListScrollPosition::Content(ListOffset {
                            item_ix: anchor.turn.turn_index,
                            offset_in_item: offset,
                        }));
                }
            }
        }
        true
    }
}

impl ShellView {
    pub(super) fn apply_transcript_wheel_command(
        &mut self,
        arguments: ScrollTranscriptArguments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(delta_y) = arguments.delta_y else {
            return Err("Wheel transcript scrolling requires deltaY.".to_string());
        };
        if self.conversation_surface().is_none() {
            return Err("Beryl has no active conversation surface.".to_string());
        }
        let mut consumed_any = false;
        for _ in 0..arguments.repeat {
            let (streamed, rendered_frame) = self
                .conversation_surface()
                .map(|surface| surface.current_transcript_scroll_navigation_snapshots())
                .unwrap_or_default();
            let event =
                diagnostic_transcript_scroll_wheel_event(delta_y, arguments.precise, window);
            let consumed =
                self.apply_transcript_scroll_wheel(&event, streamed, rendered_frame, window, cx);
            if consumed {
                consumed_any = true;
            } else {
                self.release_transcript_submit_anchor(cx);
            }
        }
        consumed_any.then_some(()).ok_or_else(|| {
            "The selected transcript did not consume the diagnostic wheel input.".to_string()
        })
    }

    pub(super) fn apply_transcript_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        streamed: Option<TranscriptStreamedNavigationSnapshot>,
        rendered_frame: Option<TranscriptViewportFrame>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let delta = event.delta.pixel_delta(window.line_height());
        if delta.y == px(0.0) || delta.x.abs() > delta.y.abs() {
            return false;
        }
        let before_rendered_frame = rendered_frame.clone();
        let direction = if delta.y > px(0.0) {
            TranscriptViewportNavigationDirection::Up
        } else {
            TranscriptViewportNavigationDirection::Down
        };
        let kind = if event.delta.precise() {
            TranscriptViewportScrollKindForShell::Touchpad
        } else {
            TranscriptViewportScrollKindForShell::Wheel
        };
        let application = self
            .conversation_surface_mut()
            .map(|surface| {
                let application = surface.apply_transcript_viewport_scroll(
                    direction,
                    delta.y.abs(),
                    kind,
                    streamed,
                    rendered_frame,
                );
                if application.consumed {
                    surface.release_transcript_submit_anchor();
                }
                application
            })
            .unwrap_or_default();
        if self.conversation_surface().is_some() {
            let diagnostic = transcript_scroll_input_diagnostic(
                kind,
                direction,
                delta.y.abs(),
                &application,
                before_rendered_frame.as_ref(),
            );
            self.transcript_panel.update(cx, |panel, _| {
                panel.record_scroll_input_diagnostic(diagnostic);
            });
        }
        if !application.consumed {
            return false;
        }

        let event = self.conversation_surface().map(|surface| {
            let list_state = surface.transcript_list_state();
            ListScrollEvent {
                visible_range: list_state.visible_range(),
                count: list_state.item_count(),
                is_scrolled: !matches!(list_state.scroll_position(), ListScrollPosition::Bottom),
            }
        });
        if let Some(event) = event {
            self.note_transcript_scroll_event(&event, window, cx);
        }
        if application.changed {
            self.notify_transcript_panel(cx);
            cx.notify();
        }
        true
    }
}

fn transcript_scroll_input_diagnostic(
    kind: TranscriptViewportScrollKindForShell,
    direction: TranscriptViewportNavigationDirection,
    requested_delta: Pixels,
    application: &TranscriptViewportNavigationApplication,
    before_rendered_frame: Option<&TranscriptViewportFrame>,
) -> TranscriptScrollInputDiagnostic {
    let after_rendered_frame = application
        .event_time_rendered_frame
        .as_ref()
        .or(before_rendered_frame);
    let before_cursor =
        before_rendered_frame.and_then(TranscriptViewportFrame::current_scroll_cursor);
    let after_cursor = application
        .scroll_cursor
        .clone()
        .or_else(|| after_rendered_frame.and_then(TranscriptViewportFrame::current_scroll_cursor));
    let residual_delta = application.residual_delta.unwrap_or(px(0.0));
    let consumed_delta = (requested_delta - residual_delta).max(px(0.0));
    TranscriptScrollInputDiagnostic {
        sequence: 0,
        input_kind: transcript_scroll_kind_label(kind).to_string(),
        direction: transcript_viewport_navigation_direction_label(direction).to_string(),
        consumed: application.consumed,
        changed: application.changed,
        requested_delta: pixels_diagnostic(requested_delta),
        consumed_delta: pixels_diagnostic(consumed_delta),
        residual_delta: pixels_diagnostic(residual_delta),
        before_anchor: before_rendered_frame
            .zip(before_cursor.as_ref())
            .and_then(|(frame, cursor)| transcript_scroll_input_anchor_diagnostic(frame, cursor)),
        after_anchor: after_rendered_frame
            .zip(after_cursor.as_ref())
            .and_then(|(frame, cursor)| transcript_scroll_input_anchor_diagnostic(frame, cursor)),
        before_visible_segment_range: before_rendered_frame.map(visible_segment_range_diagnostic),
        after_visible_segment_range: after_rendered_frame.map(visible_segment_range_diagnostic),
        before_rendered_frame_range: before_rendered_frame.map(rendered_frame_range_diagnostic),
        after_rendered_frame_range: after_rendered_frame.map(rendered_frame_range_diagnostic),
        clamp_or_expansion_reason: scroll_input_reason(
            application,
            before_rendered_frame,
            after_rendered_frame,
            before_cursor.as_ref(),
            after_cursor.as_ref(),
        ),
    }
}

fn transcript_scroll_input_anchor_diagnostic(
    frame: &TranscriptViewportFrame,
    cursor: &TranscriptViewportScrollCursor,
) -> Option<TranscriptScrollInputAnchorDiagnostic> {
    Some(TranscriptScrollInputAnchorDiagnostic {
        segment: transcript_frame_segment_diagnostic(&cursor.segment, Some(cursor.placement)),
        segment_local_offset: pixels_diagnostic(cursor.local_offset),
        absolute_rendered_offset: pixels_diagnostic(frame.absolute_offset_for_cursor(cursor)?),
    })
}

fn scroll_input_reason(
    application: &TranscriptViewportNavigationApplication,
    before_rendered_frame: Option<&TranscriptViewportFrame>,
    after_rendered_frame: Option<&TranscriptViewportFrame>,
    before_cursor: Option<&TranscriptViewportScrollCursor>,
    after_cursor: Option<&TranscriptViewportScrollCursor>,
) -> Option<String> {
    if application.blocked_direction.is_some() {
        return Some("missing_adjacent_geometry".to_string());
    }
    if let Some(boundary) = application.boundary {
        return Some(
            match boundary {
                TranscriptViewportBoundary::Start => "resident_start_boundary",
                TranscriptViewportBoundary::End => "resident_end_boundary",
            }
            .to_string(),
        );
    }
    if application
        .residual_delta
        .is_some_and(|residual| residual > px(0.0))
    {
        return Some("partial_consumption".to_string());
    }
    let frame_expanded =
        before_rendered_frame
            .zip(after_rendered_frame)
            .is_some_and(|(before, after)| {
                before.visible_segment_range() != after.visible_segment_range()
                    || before.segments().len() != after.segments().len()
            });
    let anchor_changed = before_cursor
        .zip(after_cursor)
        .is_some_and(|(before, after)| before.segment.key != after.segment.key);
    (frame_expanded || anchor_changed).then(|| "frame_expansion".to_string())
}

fn visible_segment_range_diagnostic(
    frame: &TranscriptViewportFrame,
) -> PresentationRangeDiagnostic {
    range_diagnostic(frame.visible_segment_range())
}

fn rendered_frame_range_diagnostic(frame: &TranscriptViewportFrame) -> PresentationRangeDiagnostic {
    range_diagnostic(0..frame.segments().len())
}

fn range_diagnostic(range: std::ops::Range<usize>) -> PresentationRangeDiagnostic {
    PresentationRangeDiagnostic {
        start: range.start,
        end: range.end,
    }
}

fn transcript_frame_segment_diagnostic(
    segment: &TranscriptFrameSegment,
    placement: Option<TranscriptViewportPlacement>,
) -> TranscriptFrameAnchorDiagnostic {
    let (chunk_index, chunk_identity, fallback_reason) = match &segment.key.kind {
        TranscriptFrameSegmentKind::OrdinaryRow => (None, None, None),
        TranscriptFrameSegmentKind::StreamedChunk { chunk } => (
            Some(chunk.chunk_index),
            Some(chunk.chunk_identity.clone()),
            None,
        ),
        TranscriptFrameSegmentKind::RenderBudgetFallbackChunk { chunk, reason } => (
            Some(chunk.chunk_index),
            Some(chunk.chunk_identity.clone()),
            Some(reason.clone()),
        ),
        TranscriptFrameSegmentKind::ResidentBudgetFallbackRow { reason } => {
            (None, None, Some(reason.clone()))
        }
    };
    TranscriptFrameAnchorDiagnostic {
        segment_kind: transcript_frame_segment_kind_label(&segment.key.kind).to_string(),
        row_index: segment.key.turn.turn_index,
        row_identity: segment.key.turn.row_identity.clone(),
        chunk_index,
        chunk_identity,
        fallback_reason,
        placement: placement
            .map(transcript_viewport_placement_label)
            .map(str::to_string),
    }
}

fn pixels_diagnostic(pixels: Pixels) -> f64 {
    f64::from(f32::from(pixels))
}

fn transcript_scroll_kind_label(kind: TranscriptViewportScrollKindForShell) -> &'static str {
    match kind {
        TranscriptViewportScrollKindForShell::Wheel => "wheel",
        TranscriptViewportScrollKindForShell::Touchpad => "touchpad",
    }
}

fn transcript_viewport_placement_label(placement: TranscriptViewportPlacement) -> &'static str {
    match placement {
        TranscriptViewportPlacement::Top => "top",
        TranscriptViewportPlacement::Bottom => "bottom",
    }
}

fn transcript_viewport_navigation_direction_label(
    direction: TranscriptViewportNavigationDirection,
) -> &'static str {
    match direction {
        TranscriptViewportNavigationDirection::Up => "up",
        TranscriptViewportNavigationDirection::Down => "down",
    }
}

fn transcript_frame_segment_kind_label(kind: &TranscriptFrameSegmentKind) -> &'static str {
    match kind {
        TranscriptFrameSegmentKind::OrdinaryRow => "ordinary_row",
        TranscriptFrameSegmentKind::StreamedChunk { .. } => "streamed_chunk",
        TranscriptFrameSegmentKind::RenderBudgetFallbackChunk { .. } => {
            "render_budget_fallback_chunk"
        }
        TranscriptFrameSegmentKind::ResidentBudgetFallbackRow { .. } => {
            "resident_budget_fallback_row"
        }
    }
}

fn frame_segment_is_streamed_after_turn_start(
    segment: &super::transcript_viewport::TranscriptFrameSegment,
) -> bool {
    match &segment.key.kind {
        TranscriptFrameSegmentKind::StreamedChunk { chunk }
        | TranscriptFrameSegmentKind::RenderBudgetFallbackChunk { chunk, .. } => {
            chunk.chunk_index > 0
        }
        TranscriptFrameSegmentKind::OrdinaryRow
        | TranscriptFrameSegmentKind::ResidentBudgetFallbackRow { .. } => false,
    }
}

fn viewport_mode_matches_frame_segment_with_non_top_placement(
    mode: &TranscriptViewportMode,
    segment: &super::transcript_viewport::TranscriptFrameSegment,
) -> bool {
    match mode {
        TranscriptViewportMode::Ordinary(anchor) => {
            anchor.turn == segment.key.turn && anchor.placement != TranscriptViewportPlacement::Top
        }
        TranscriptViewportMode::Streamed(anchor) => {
            let Some(chunk) = segment.key.streamed_chunk_anchor() else {
                return false;
            };
            anchor.turn == segment.key.turn
                && (anchor.anchor_chunk.chunk_identity == chunk.chunk_identity
                    || anchor.anchor_chunk.chunk_index == chunk.chunk_index)
                && anchor.placement != TranscriptViewportPlacement::Top
        }
        TranscriptViewportMode::Empty => false,
    }
}

fn viewport_mode_matches_frame_segment_with_bottom_placement(
    mode: &TranscriptViewportMode,
    segment: &super::transcript_viewport::TranscriptFrameSegment,
) -> bool {
    match mode {
        TranscriptViewportMode::Ordinary(anchor) => {
            anchor.turn == segment.key.turn
                && anchor.placement == TranscriptViewportPlacement::Bottom
        }
        TranscriptViewportMode::Streamed(anchor) => {
            let Some(chunk) = segment.key.streamed_chunk_anchor() else {
                return false;
            };
            anchor.turn == segment.key.turn
                && (anchor.anchor_chunk.chunk_identity == chunk.chunk_identity
                    || anchor.anchor_chunk.chunk_index == chunk.chunk_index)
                && anchor.placement == TranscriptViewportPlacement::Bottom
        }
        TranscriptViewportMode::Empty => false,
    }
}
