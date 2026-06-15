use gpui::{Pixels, px};

use super::{
    ListOffset, ListScrollPosition, ListState,
    transcript_viewport::{
        TranscriptStreamedNavigationFrame, TranscriptViewportBoundary,
        TranscriptViewportChunkAnchor, TranscriptViewportFrame, TranscriptViewportMode,
        TranscriptViewportNavigationDirection, TranscriptViewportPlacement,
        TranscriptViewportReduceOutcome, TranscriptViewportScrollCursor,
        TranscriptViewportScrollInput, TranscriptViewportState,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptStreamedNavigationSnapshot {
    pub(crate) row_index: usize,
    pub(crate) row_identity: String,
    pub(crate) frame: TranscriptStreamedNavigationFrame,
    pub(crate) rendered_frame: TranscriptViewportFrame,
}

impl TranscriptStreamedNavigationSnapshot {
    pub(crate) fn new(
        row_index: usize,
        row_identity: String,
        frame: TranscriptStreamedNavigationFrame,
        rendered_frame: TranscriptViewportFrame,
    ) -> Self {
        Self {
            row_index,
            row_identity,
            frame,
            rendered_frame,
        }
    }

    pub(crate) fn matches_viewport(&self, viewport: &TranscriptViewportState) -> bool {
        transcript_viewport_is_streamed_for(viewport, self.row_index, self.row_identity.as_str())
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_rendered_transcript_streamed_navigation_snapshot(
    row_index: usize,
    row_identity: impl Into<String>,
    chunk_count: usize,
    rendered_chunk_range: std::ops::Range<usize>,
    first_rendered_chunk: Option<TranscriptViewportChunkAnchor>,
    last_rendered_chunk: Option<TranscriptViewportChunkAnchor>,
    previous_chunk: Option<TranscriptViewportChunkAnchor>,
    next_chunk: Option<TranscriptViewportChunkAnchor>,
    row_local_scroll_offset: Pixels,
    row_local_scroll_max: Pixels,
    rendered_frame: TranscriptViewportFrame,
) -> TranscriptStreamedNavigationSnapshot {
    let frame = TranscriptStreamedNavigationFrame::new(
        chunk_count,
        rendered_chunk_range,
        first_rendered_chunk,
        last_rendered_chunk,
        previous_chunk,
        next_chunk,
        row_local_scroll_offset,
        row_local_scroll_max,
    );
    TranscriptStreamedNavigationSnapshot::new(row_index, row_identity.into(), frame, rendered_frame)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptViewportScrollKindForShell {
    Wheel,
    Touchpad,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptViewportNavigationApplication {
    pub(crate) changed: bool,
    pub(crate) consumed: bool,
    pub(crate) event_time_rendered_frame: Option<TranscriptViewportFrame>,
    pub(crate) blocked_direction: Option<TranscriptViewportNavigationDirection>,
    pub(crate) scroll_cursor: Option<TranscriptViewportScrollCursor>,
    pub(crate) residual_delta: Option<Pixels>,
    pub(crate) boundary: Option<TranscriptViewportBoundary>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptEventTimeScrollState {
    rendered_frame: Option<TranscriptViewportFrame>,
    blocked_direction: Option<TranscriptViewportNavigationDirection>,
}

impl TranscriptEventTimeScrollState {
    pub(crate) fn clear(&mut self) -> bool {
        let changed = self.rendered_frame.is_some() || self.blocked_direction.is_some();
        self.rendered_frame = None;
        self.blocked_direction = None;
        changed
    }

    pub(crate) fn is_blocked(&self, direction: TranscriptViewportNavigationDirection) -> bool {
        self.blocked_direction == Some(direction)
    }

    pub(crate) fn effective_rendered_frame(
        &self,
        rendered_frame: Option<TranscriptViewportFrame>,
    ) -> Option<TranscriptViewportFrame> {
        self.rendered_frame.clone().or(rendered_frame)
    }

    pub(crate) fn effective_streamed_snapshot(
        &self,
        streamed: Option<TranscriptStreamedNavigationSnapshot>,
    ) -> Option<TranscriptStreamedNavigationSnapshot> {
        streamed.map(|mut snapshot| {
            if let Some(rendered_frame) = self.rendered_frame.clone() {
                snapshot.rendered_frame = rendered_frame;
            }
            snapshot
        })
    }

    pub(crate) fn apply_navigation_application(
        &mut self,
        direction: TranscriptViewportNavigationDirection,
        application: &TranscriptViewportNavigationApplication,
    ) -> bool {
        if let Some(rendered_frame) = application.event_time_rendered_frame.clone() {
            let changed = self.rendered_frame.as_ref() != Some(&rendered_frame)
                || self.blocked_direction.is_some();
            self.rendered_frame = Some(rendered_frame);
            self.blocked_direction = None;
            return changed;
        }

        if application.blocked_direction == Some(direction) {
            let changed = self.blocked_direction != Some(direction);
            self.blocked_direction = Some(direction);
            return changed;
        }

        false
    }
}

pub(crate) fn apply_transcript_viewport_scroll(
    viewport: &mut TranscriptViewportState,
    list_state: &ListState,
    direction: TranscriptViewportNavigationDirection,
    distance: Pixels,
    kind: TranscriptViewportScrollKindForShell,
    streamed: Option<TranscriptStreamedNavigationSnapshot>,
    rendered_frame: Option<TranscriptViewportFrame>,
) -> TranscriptViewportNavigationApplication {
    let streamed = streamed.filter(|snapshot| snapshot.matches_viewport(viewport));
    let rendered_frame = streamed
        .as_ref()
        .map(|snapshot| snapshot.rendered_frame.clone())
        .or(rendered_frame);
    let Some(rendered_frame) = rendered_frame else {
        return TranscriptViewportNavigationApplication::default();
    };

    let input = match kind {
        TranscriptViewportScrollKindForShell::Wheel => TranscriptViewportScrollInput::wheel(
            direction,
            distance,
            streamed.as_ref().map(|snapshot| snapshot.frame.clone()),
        ),
        TranscriptViewportScrollKindForShell::Touchpad => TranscriptViewportScrollInput::touchpad(
            direction,
            distance,
            streamed.as_ref().map(|snapshot| snapshot.frame.clone()),
        ),
    }
    .with_rendered_frame(rendered_frame.clone());
    let outcome = viewport.apply_scroll(input);
    let scroll_cursor = outcome.scroll_cursor.clone();
    let residual_delta = outcome.residual_delta;
    let boundary = outcome.boundary;
    let event_time_rendered_frame = outcome
        .scroll_cursor
        .as_ref()
        .and_then(|cursor| rendered_frame.with_event_time_scroll_cursor(cursor));
    let blocked_direction = (outcome.scroll_cursor.is_none()
        && outcome.residual_delta.is_some()
        && outcome.boundary.is_none())
    .then_some(direction);
    let mut application = apply_transcript_viewport_reduce_outcome(viewport, list_state, outcome);
    application.event_time_rendered_frame = event_time_rendered_frame;
    application.blocked_direction = blocked_direction;
    application.scroll_cursor = scroll_cursor;
    application.residual_delta = residual_delta;
    application.boundary = boundary;
    application.consumed |= blocked_direction.is_some();
    application
}

pub(crate) fn apply_transcript_viewport_reduce_outcome(
    viewport: &TranscriptViewportState,
    list_state: &ListState,
    outcome: TranscriptViewportReduceOutcome,
) -> TranscriptViewportNavigationApplication {
    let cursor_row_index = outcome
        .scroll_cursor
        .as_ref()
        .map(|cursor| cursor.segment.key.turn.turn_index);
    let row_index = cursor_row_index.or_else(|| transcript_viewport_anchor_row_index(viewport));
    if let Some(cursor) = outcome.scroll_cursor.as_ref() {
        list_state.scroll_to_position(ListScrollPosition::Content(ListOffset {
            item_ix: cursor.segment.key.turn.turn_index,
            offset_in_item: cursor.local_offset,
        }));
    }
    if outcome.semantic_refill
        && let Some(row_index) = row_index
    {
        list_state.invalidate_item_measurement(row_index);
    }
    TranscriptViewportNavigationApplication {
        changed: outcome.changed,
        consumed: outcome.scroll_cursor.is_some()
            || outcome.semantic_refill
            || outcome.boundary.is_some()
            || outcome.live_autoscroll_detached,
        ..TranscriptViewportNavigationApplication::default()
    }
}

pub(crate) fn sync_transcript_list_state_to_viewport_anchor(
    viewport: &TranscriptViewportState,
    list_state: &ListState,
) {
    match viewport.mode() {
        TranscriptViewportMode::Empty => {}
        TranscriptViewportMode::Ordinary(anchor) => {
            scroll_transcript_turn_to_placement(
                list_state,
                anchor.turn.turn_index,
                anchor.placement,
                anchor.local_offset,
            );
        }
        TranscriptViewportMode::Streamed(anchor) => {
            list_state.invalidate_item_measurement(anchor.turn.turn_index);
            scroll_transcript_turn_to_placement(
                list_state,
                anchor.turn.turn_index,
                anchor.placement,
                anchor.local_anchor_offset.unwrap_or(px(0.0)),
            );
        }
    }
}

pub(crate) fn scroll_transcript_turn_to_placement(
    list_state: &ListState,
    row_index: usize,
    placement: TranscriptViewportPlacement,
    local_offset: Pixels,
) {
    match placement {
        TranscriptViewportPlacement::Top => {
            list_state.scroll_to_position(ListScrollPosition::Content(ListOffset {
                item_ix: row_index,
                offset_in_item: local_offset.max(px(0.0)),
            }));
        }
        TranscriptViewportPlacement::Bottom => {
            list_state.scroll_to_reveal_item_end(row_index);
        }
    }
}

fn transcript_viewport_is_streamed_for(
    viewport: &TranscriptViewportState,
    row_index: usize,
    row_identity: &str,
) -> bool {
    let TranscriptViewportMode::Streamed(anchor) = viewport.mode() else {
        return false;
    };
    if let Some(anchor_identity) = anchor.turn.row_identity.as_deref() {
        return anchor_identity == row_identity;
    }
    anchor.turn.turn_index == row_index
}

fn transcript_viewport_anchor_row_index(viewport: &TranscriptViewportState) -> Option<usize> {
    match viewport.mode() {
        TranscriptViewportMode::Empty => None,
        TranscriptViewportMode::Ordinary(anchor) => Some(anchor.turn.turn_index),
        TranscriptViewportMode::Streamed(anchor) => Some(anchor.turn.turn_index),
    }
}
