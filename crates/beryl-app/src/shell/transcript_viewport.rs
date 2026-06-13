#![allow(dead_code)]

use std::ops::Range;

use gpui::{Pixels, px};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptViewportState {
    mode: TranscriptViewportMode,
    live_autoscroll: TranscriptViewportLiveAutoscroll,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum TranscriptViewportMode {
    #[default]
    Empty,
    Ordinary(TranscriptOrdinaryViewportAnchor),
    Streamed(TranscriptStreamedViewportAnchor),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptViewportTurnAnchor {
    pub(crate) turn_index: usize,
    pub(crate) row_identity: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) turn_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptViewportChunkAnchor {
    pub(crate) chunk_index: usize,
    pub(crate) chunk_identity: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptOrdinaryViewportAnchor {
    pub(crate) turn: TranscriptViewportTurnAnchor,
    pub(crate) placement: TranscriptViewportPlacement,
    pub(crate) local_offset: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptStreamedViewportAnchor {
    pub(crate) turn: TranscriptViewportTurnAnchor,
    pub(crate) anchor_chunk: TranscriptViewportChunkAnchor,
    pub(crate) rendered_chunk_range: Range<usize>,
    pub(crate) chunk_count: usize,
    pub(crate) placement: TranscriptViewportPlacement,
    pub(crate) local_anchor_offset: Option<Pixels>,
    pub(crate) last_navigation_direction: Option<TranscriptViewportNavigationDirection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptViewportPlacement {
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptViewportNavigationDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TranscriptViewportLiveAutoscroll {
    #[default]
    Detached,
    FollowingTail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptViewportBoundary {
    Start,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptViewportInvalidation {
    Width,
    Theme,
    Font,
    Media,
    CodePanel,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptViewportScrollInput {
    pub(crate) kind: TranscriptViewportScrollKind,
    pub(crate) direction: TranscriptViewportNavigationDirection,
    pub(crate) distance: Pixels,
    pub(crate) streamed_frame: Option<TranscriptStreamedNavigationFrame>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptViewportScrollKind {
    Wheel,
    Touchpad,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptStreamedNavigationFrame {
    pub(crate) chunk_count: usize,
    pub(crate) rendered_chunk_range: Range<usize>,
    pub(crate) first_rendered_chunk: Option<TranscriptViewportChunkAnchor>,
    pub(crate) last_rendered_chunk: Option<TranscriptViewportChunkAnchor>,
    pub(crate) previous_chunk: Option<TranscriptViewportChunkAnchor>,
    pub(crate) next_chunk: Option<TranscriptViewportChunkAnchor>,
    pub(crate) local_scroll_offset: Pixels,
    pub(crate) local_scroll_max: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptViewportTurnTarget {
    pub(crate) turn: TranscriptViewportTurnAnchor,
    pub(crate) kind: TranscriptViewportTurnTargetKind,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TranscriptViewportTurnTargetKind {
    Ordinary,
    Streamed {
        anchor_chunk: TranscriptViewportChunkAnchor,
        chunk_count: usize,
        placement: TranscriptViewportPlacement,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptViewportReduceOutcome {
    pub(crate) changed: bool,
    pub(crate) live_autoscroll_detached: bool,
    pub(crate) local_scroll_offset: Option<Pixels>,
    pub(crate) semantic_refill: bool,
    pub(crate) ordinary_pixel_scroll: bool,
    pub(crate) boundary: Option<TranscriptViewportBoundary>,
}

impl TranscriptViewportTurnAnchor {
    pub(crate) fn new(
        turn_index: usize,
        row_identity: Option<String>,
        thread_id: Option<String>,
        turn_id: Option<String>,
    ) -> Self {
        Self {
            turn_index,
            row_identity,
            thread_id,
            turn_id,
        }
    }

    fn shift_index_for_mutation(&mut self, mutation: TranscriptViewportRowMutation) {
        match mutation {
            TranscriptViewportRowMutation::Inserted { index, count } => {
                if index <= self.turn_index {
                    self.turn_index = self.turn_index.saturating_add(count);
                }
            }
            TranscriptViewportRowMutation::Removed { index, count } => {
                let end = index.saturating_add(count);
                if end <= self.turn_index {
                    self.turn_index = self.turn_index.saturating_sub(count);
                } else if index <= self.turn_index {
                    self.turn_index = index;
                }
            }
        }
    }
}

impl TranscriptViewportChunkAnchor {
    pub(crate) fn new(chunk_index: usize, chunk_identity: impl Into<String>) -> Self {
        Self {
            chunk_index,
            chunk_identity: chunk_identity.into(),
        }
    }
}

impl TranscriptViewportScrollInput {
    pub(crate) fn wheel(
        direction: TranscriptViewportNavigationDirection,
        distance: Pixels,
        streamed_frame: Option<TranscriptStreamedNavigationFrame>,
    ) -> Self {
        Self {
            kind: TranscriptViewportScrollKind::Wheel,
            direction,
            distance,
            streamed_frame,
        }
    }

    pub(crate) fn touchpad(
        direction: TranscriptViewportNavigationDirection,
        distance: Pixels,
        streamed_frame: Option<TranscriptStreamedNavigationFrame>,
    ) -> Self {
        Self {
            kind: TranscriptViewportScrollKind::Touchpad,
            direction,
            distance,
            streamed_frame,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptViewportRowMutation {
    Inserted { index: usize, count: usize },
    Removed { index: usize, count: usize },
}

impl TranscriptViewportState {
    pub(crate) fn mode(&self) -> &TranscriptViewportMode {
        &self.mode
    }

    pub(crate) fn live_autoscroll(&self) -> TranscriptViewportLiveAutoscroll {
        self.live_autoscroll
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn follow_live_tail(&mut self) {
        self.live_autoscroll = TranscriptViewportLiveAutoscroll::FollowingTail;
    }

    pub(crate) fn reset_to_tail(&mut self, turn_count: usize) {
        self.live_autoscroll = TranscriptViewportLiveAutoscroll::FollowingTail;
        self.mode = if turn_count == 0 {
            TranscriptViewportMode::Empty
        } else {
            TranscriptViewportMode::Ordinary(TranscriptOrdinaryViewportAnchor {
                turn: TranscriptViewportTurnAnchor::new(turn_count - 1, None, None, None),
                placement: TranscriptViewportPlacement::Bottom,
                local_offset: px(0.0),
            })
        };
    }

    pub(crate) fn anchor_ordinary(
        &mut self,
        turn: TranscriptViewportTurnAnchor,
        placement: TranscriptViewportPlacement,
        local_offset: Pixels,
    ) {
        self.mode = TranscriptViewportMode::Ordinary(TranscriptOrdinaryViewportAnchor {
            turn,
            placement,
            local_offset: local_offset.max(px(0.0)),
        });
    }

    pub(crate) fn anchor_streamed(
        &mut self,
        turn: TranscriptViewportTurnAnchor,
        anchor_chunk: TranscriptViewportChunkAnchor,
        chunk_count: usize,
        placement: TranscriptViewportPlacement,
    ) {
        self.mode = TranscriptViewportMode::Streamed(TranscriptStreamedViewportAnchor::new(
            turn,
            anchor_chunk,
            chunk_count,
            placement,
        ));
    }

    pub(crate) fn apply_scroll(
        &mut self,
        input: TranscriptViewportScrollInput,
    ) -> TranscriptViewportReduceOutcome {
        let mut outcome = self.detach_live_autoscroll_for_manual_navigation();
        if input.distance <= px(0.0) {
            return outcome;
        }

        match &mut self.mode {
            TranscriptViewportMode::Empty => outcome,
            TranscriptViewportMode::Ordinary(_) => {
                outcome.changed = true;
                outcome.ordinary_pixel_scroll = true;
                outcome
            }
            TranscriptViewportMode::Streamed(anchor) => {
                let Some(frame) = input.streamed_frame else {
                    outcome.boundary = Some(boundary_for_direction(input.direction));
                    return outcome;
                };
                outcome.changed |= anchor.reconcile_frame(&frame);
                if apply_local_streamed_scroll(anchor, &frame, input.direction, input.distance)
                    .map(|local_offset| {
                        outcome.changed = true;
                        outcome.local_scroll_offset = Some(local_offset);
                    })
                    .is_some()
                {
                    return outcome;
                }

                match input.direction {
                    TranscriptViewportNavigationDirection::Up => {
                        let Some(previous) = frame.previous_chunk else {
                            outcome.boundary = Some(TranscriptViewportBoundary::Start);
                            return outcome;
                        };
                        anchor.set_anchor_chunk(
                            previous,
                            TranscriptViewportPlacement::Top,
                            TranscriptViewportNavigationDirection::Up,
                        );
                    }
                    TranscriptViewportNavigationDirection::Down => {
                        let Some(next) = frame.next_chunk else {
                            outcome.boundary = Some(TranscriptViewportBoundary::End);
                            return outcome;
                        };
                        anchor.set_anchor_chunk(
                            next,
                            TranscriptViewportPlacement::Bottom,
                            TranscriptViewportNavigationDirection::Down,
                        );
                    }
                }
                outcome.changed = true;
                outcome.semantic_refill = true;
                outcome
            }
        }
    }

    pub(crate) fn apply_page(
        &mut self,
        direction: TranscriptViewportNavigationDirection,
        streamed_frame: Option<TranscriptStreamedNavigationFrame>,
    ) -> TranscriptViewportReduceOutcome {
        let mut outcome = self.detach_live_autoscroll_for_manual_navigation();
        match &mut self.mode {
            TranscriptViewportMode::Empty => outcome,
            TranscriptViewportMode::Ordinary(anchor) => {
                anchor.placement = match direction {
                    TranscriptViewportNavigationDirection::Up => TranscriptViewportPlacement::Top,
                    TranscriptViewportNavigationDirection::Down => {
                        TranscriptViewportPlacement::Bottom
                    }
                };
                anchor.local_offset = px(0.0);
                outcome.changed = true;
                outcome.ordinary_pixel_scroll = true;
                outcome
            }
            TranscriptViewportMode::Streamed(anchor) => {
                let Some(frame) = streamed_frame else {
                    outcome.boundary = Some(boundary_for_direction(direction));
                    return outcome;
                };
                outcome.changed |= anchor.reconcile_frame(&frame);
                match direction {
                    TranscriptViewportNavigationDirection::Up => {
                        let Some(first) = frame.first_rendered_chunk else {
                            outcome.boundary = Some(TranscriptViewportBoundary::Start);
                            return outcome;
                        };
                        if first.chunk_index == 0
                            && anchor.anchor_chunk.chunk_index == 0
                            && anchor.placement == TranscriptViewportPlacement::Top
                        {
                            outcome.boundary = Some(TranscriptViewportBoundary::Start);
                            return outcome;
                        }
                        anchor.set_anchor_chunk(
                            first,
                            TranscriptViewportPlacement::Bottom,
                            TranscriptViewportNavigationDirection::Up,
                        );
                    }
                    TranscriptViewportNavigationDirection::Down => {
                        let Some(last) = frame.last_rendered_chunk else {
                            outcome.boundary = Some(TranscriptViewportBoundary::End);
                            return outcome;
                        };
                        if last.chunk_index.saturating_add(1) >= frame.chunk_count
                            && anchor.anchor_chunk.chunk_index == last.chunk_index
                            && anchor.placement == TranscriptViewportPlacement::Bottom
                        {
                            outcome.boundary = Some(TranscriptViewportBoundary::End);
                            return outcome;
                        }
                        anchor.set_anchor_chunk(
                            last,
                            TranscriptViewportPlacement::Top,
                            TranscriptViewportNavigationDirection::Down,
                        );
                    }
                }
                outcome.changed = true;
                outcome.semantic_refill = true;
                outcome
            }
        }
    }

    pub(crate) fn apply_turn_jump(
        &mut self,
        target: Option<TranscriptViewportTurnTarget>,
    ) -> TranscriptViewportReduceOutcome {
        let mut outcome = self.detach_live_autoscroll_for_manual_navigation();
        let Some(target) = target else {
            return outcome;
        };
        self.anchor_turn_target(target);
        outcome.changed = true;
        outcome.semantic_refill = true;
        outcome
    }

    pub(crate) fn invalidate_layout(
        &mut self,
        _reason: TranscriptViewportInvalidation,
    ) -> TranscriptViewportReduceOutcome {
        match &mut self.mode {
            TranscriptViewportMode::Empty => TranscriptViewportReduceOutcome::default(),
            TranscriptViewportMode::Ordinary(anchor) => {
                let changed = anchor.local_offset != px(0.0)
                    || anchor.placement != TranscriptViewportPlacement::Top;
                anchor.local_offset = px(0.0);
                anchor.placement = TranscriptViewportPlacement::Top;
                TranscriptViewportReduceOutcome {
                    changed,
                    semantic_refill: changed,
                    ..TranscriptViewportReduceOutcome::default()
                }
            }
            TranscriptViewportMode::Streamed(anchor) => {
                let next_range =
                    anchor.anchor_chunk.chunk_index..anchor.anchor_chunk.chunk_index + 1;
                let changed = anchor.rendered_chunk_range != next_range
                    || anchor.local_anchor_offset.is_some();
                anchor.rendered_chunk_range = next_range;
                anchor.local_anchor_offset = None;
                TranscriptViewportReduceOutcome {
                    changed,
                    semantic_refill: changed,
                    ..TranscriptViewportReduceOutcome::default()
                }
            }
        }
    }

    pub(crate) fn reconcile_row_mutation(&mut self, mutation: TranscriptViewportRowMutation) {
        match &mut self.mode {
            TranscriptViewportMode::Empty => {}
            TranscriptViewportMode::Ordinary(anchor) => {
                anchor.turn.shift_index_for_mutation(mutation);
                anchor.local_offset = px(0.0);
            }
            TranscriptViewportMode::Streamed(anchor) => {
                anchor.turn.shift_index_for_mutation(mutation);
                anchor.local_anchor_offset = None;
            }
        }
    }

    fn anchor_turn_target(&mut self, target: TranscriptViewportTurnTarget) {
        match target.kind {
            TranscriptViewportTurnTargetKind::Ordinary => {
                self.anchor_ordinary(target.turn, TranscriptViewportPlacement::Top, px(0.0));
            }
            TranscriptViewportTurnTargetKind::Streamed {
                anchor_chunk,
                chunk_count,
                placement,
            } => {
                self.anchor_streamed(target.turn, anchor_chunk, chunk_count, placement);
            }
        }
    }

    fn detach_live_autoscroll_for_manual_navigation(&mut self) -> TranscriptViewportReduceOutcome {
        let detached = self.live_autoscroll == TranscriptViewportLiveAutoscroll::FollowingTail;
        if detached {
            self.live_autoscroll = TranscriptViewportLiveAutoscroll::Detached;
        }
        TranscriptViewportReduceOutcome {
            changed: detached,
            live_autoscroll_detached: detached,
            ..TranscriptViewportReduceOutcome::default()
        }
    }
}

impl TranscriptStreamedNavigationFrame {
    pub(crate) fn new(
        chunk_count: usize,
        rendered_chunk_range: Range<usize>,
        first_rendered_chunk: Option<TranscriptViewportChunkAnchor>,
        last_rendered_chunk: Option<TranscriptViewportChunkAnchor>,
        previous_chunk: Option<TranscriptViewportChunkAnchor>,
        next_chunk: Option<TranscriptViewportChunkAnchor>,
        local_scroll_offset: Pixels,
        local_scroll_max: Pixels,
    ) -> Self {
        Self {
            chunk_count,
            rendered_chunk_range,
            first_rendered_chunk,
            last_rendered_chunk,
            previous_chunk,
            next_chunk,
            local_scroll_offset: local_scroll_offset.max(px(0.0)),
            local_scroll_max: local_scroll_max.max(px(0.0)),
        }
    }
}

impl TranscriptViewportTurnTarget {
    pub(crate) fn ordinary(turn: TranscriptViewportTurnAnchor) -> Self {
        Self {
            turn,
            kind: TranscriptViewportTurnTargetKind::Ordinary,
        }
    }

    pub(crate) fn streamed(
        turn: TranscriptViewportTurnAnchor,
        anchor_chunk: TranscriptViewportChunkAnchor,
        chunk_count: usize,
        placement: TranscriptViewportPlacement,
    ) -> Self {
        Self {
            turn,
            kind: TranscriptViewportTurnTargetKind::Streamed {
                anchor_chunk,
                chunk_count,
                placement,
            },
        }
    }
}

impl TranscriptStreamedViewportAnchor {
    fn new(
        turn: TranscriptViewportTurnAnchor,
        anchor_chunk: TranscriptViewportChunkAnchor,
        chunk_count: usize,
        placement: TranscriptViewportPlacement,
    ) -> Self {
        let chunk_count = chunk_count.max(1);
        let chunk_index = anchor_chunk.chunk_index.min(chunk_count - 1);
        let anchor_chunk = TranscriptViewportChunkAnchor {
            chunk_index,
            chunk_identity: anchor_chunk.chunk_identity,
        };
        Self {
            turn,
            rendered_chunk_range: chunk_index..chunk_index + 1,
            anchor_chunk,
            chunk_count,
            placement,
            local_anchor_offset: None,
            last_navigation_direction: None,
        }
    }

    fn reconcile_frame(&mut self, frame: &TranscriptStreamedNavigationFrame) -> bool {
        let previous_chunk_count = self.chunk_count;
        let previous_range = self.rendered_chunk_range.clone();
        let previous_anchor_index = self.anchor_chunk.chunk_index;
        self.chunk_count = frame.chunk_count.max(1);
        self.rendered_chunk_range =
            clamp_rendered_range(frame.rendered_chunk_range.clone(), self.chunk_count);
        self.anchor_chunk.chunk_index = self.anchor_chunk.chunk_index.min(self.chunk_count - 1);
        previous_chunk_count != self.chunk_count
            || previous_range != self.rendered_chunk_range
            || previous_anchor_index != self.anchor_chunk.chunk_index
    }

    fn set_anchor_chunk(
        &mut self,
        chunk: TranscriptViewportChunkAnchor,
        placement: TranscriptViewportPlacement,
        direction: TranscriptViewportNavigationDirection,
    ) {
        let chunk_index = chunk.chunk_index.min(self.chunk_count.saturating_sub(1));
        self.anchor_chunk = TranscriptViewportChunkAnchor {
            chunk_index,
            chunk_identity: chunk.chunk_identity,
        };
        self.rendered_chunk_range = chunk_index..chunk_index.saturating_add(1);
        self.placement = placement;
        self.local_anchor_offset = None;
        self.last_navigation_direction = Some(direction);
    }
}

fn apply_local_streamed_scroll(
    anchor: &mut TranscriptStreamedViewportAnchor,
    frame: &TranscriptStreamedNavigationFrame,
    direction: TranscriptViewportNavigationDirection,
    distance: Pixels,
) -> Option<Pixels> {
    let offset = frame.local_scroll_offset.max(px(0.0));
    let max = frame.local_scroll_max.max(px(0.0));
    match direction {
        TranscriptViewportNavigationDirection::Up if offset > px(0.0) => {
            let next = (offset - distance).max(px(0.0));
            anchor.local_anchor_offset = Some(next);
            Some(next)
        }
        TranscriptViewportNavigationDirection::Down if offset < max => {
            let next = (offset + distance).min(max);
            anchor.local_anchor_offset = Some(next);
            Some(next)
        }
        _ => None,
    }
}

fn boundary_for_direction(
    direction: TranscriptViewportNavigationDirection,
) -> TranscriptViewportBoundary {
    match direction {
        TranscriptViewportNavigationDirection::Up => TranscriptViewportBoundary::Start,
        TranscriptViewportNavigationDirection::Down => TranscriptViewportBoundary::End,
    }
}

fn clamp_rendered_range(range: Range<usize>, chunk_count: usize) -> Range<usize> {
    let start = range.start.min(chunk_count);
    let end = range.end.min(chunk_count).max(start);
    if start == end && chunk_count > 0 {
        let start = start.min(chunk_count - 1);
        start..start + 1
    } else {
        start..end
    }
}
