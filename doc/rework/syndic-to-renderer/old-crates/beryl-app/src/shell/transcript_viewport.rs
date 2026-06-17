#![allow(dead_code)]

use std::{collections::HashMap, ops::Range};

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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptViewportTurnAnchor {
    pub(crate) turn_index: usize,
    pub(crate) row_identity: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) turn_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptViewportChunkAnchor {
    pub(crate) chunk_index: usize,
    pub(crate) chunk_identity: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptOrdinaryViewportAnchor {
    pub(crate) turn: TranscriptViewportTurnAnchor,
    pub(crate) placement: TranscriptViewportPlacement,
    pub(crate) local_offset: Pixels,
    pub(crate) local_offset_basis: TranscriptViewportLocalOffsetBasis,
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum TranscriptViewportLocalOffsetBasis {
    #[default]
    Top,
    Trailing {
        distance_from_end: Pixels,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptFrameSegmentKey {
    pub(crate) turn: TranscriptViewportTurnAnchor,
    pub(crate) kind: TranscriptFrameSegmentKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TranscriptFrameSegmentKind {
    OrdinaryRow,
    StreamedChunk {
        chunk: TranscriptViewportChunkAnchor,
    },
    RenderBudgetFallbackChunk {
        chunk: TranscriptViewportChunkAnchor,
        reason: String,
    },
    ResidentBudgetFallbackRow {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptFrameSegment {
    pub(crate) key: TranscriptFrameSegmentKey,
    pub(crate) measured_height: Option<Pixels>,
    pub(crate) streamed_chunk_count: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptViewportFrame {
    segments: Vec<TranscriptFrameSegment>,
    visible_segment_range: Range<usize>,
    local_scroll_offset: Pixels,
    local_scroll_max: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptViewportFrameReduction {
    pub(crate) cursor: Option<TranscriptViewportScrollCursor>,
    pub(crate) residual_delta: Pixels,
    pub(crate) boundary: Option<TranscriptViewportBoundary>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptViewportScrollCursor {
    pub(crate) segment: TranscriptFrameSegment,
    pub(crate) local_offset: Pixels,
    pub(crate) placement: TranscriptViewportPlacement,
    pub(crate) local_offset_basis: TranscriptViewportLocalOffsetBasis,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptSegmentMeasurementRevision(u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptSegmentMeasurementKey {
    pub(crate) segment: TranscriptFrameSegmentKey,
    pub(crate) revision: TranscriptSegmentMeasurementRevision,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptSegmentMeasurementQueue {
    staged: HashMap<TranscriptSegmentMeasurementKey, Pixels>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptSegmentMeasurementCache {
    heights: HashMap<TranscriptSegmentMeasurementKey, Pixels>,
    active_revisions: HashMap<TranscriptFrameSegmentKey, TranscriptSegmentMeasurementRevision>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptSegmentMeasurementCommit {
    pub(crate) changed: Vec<TranscriptSegmentMeasurementChange>,
    pub(crate) unchanged: usize,
    pub(crate) anchor_offset_correction: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptSegmentMeasurementChange {
    pub(crate) key: TranscriptSegmentMeasurementKey,
    pub(crate) previous_height: Option<Pixels>,
    pub(crate) measured_height: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptSegmentMeasurementAnchor {
    pub(crate) key: TranscriptFrameSegmentKey,
    pub(crate) local_offset: Pixels,
    pub(crate) local_offset_basis: TranscriptViewportLocalOffsetBasis,
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
    pub(crate) rendered_frame: Option<TranscriptViewportFrame>,
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
    pub(crate) scroll_cursor: Option<TranscriptViewportScrollCursor>,
    pub(crate) residual_delta: Option<Pixels>,
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

impl TranscriptFrameSegmentKey {
    pub(crate) fn ordinary_row(turn: TranscriptViewportTurnAnchor) -> Self {
        Self {
            turn,
            kind: TranscriptFrameSegmentKind::OrdinaryRow,
        }
    }

    pub(crate) fn streamed_chunk(
        turn: TranscriptViewportTurnAnchor,
        chunk: TranscriptViewportChunkAnchor,
    ) -> Self {
        Self {
            turn,
            kind: TranscriptFrameSegmentKind::StreamedChunk { chunk },
        }
    }

    pub(crate) fn render_budget_fallback_chunk(
        turn: TranscriptViewportTurnAnchor,
        chunk: TranscriptViewportChunkAnchor,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            turn,
            kind: TranscriptFrameSegmentKind::RenderBudgetFallbackChunk {
                chunk,
                reason: reason.into(),
            },
        }
    }

    pub(crate) fn resident_budget_fallback_row(
        turn: TranscriptViewportTurnAnchor,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            turn,
            kind: TranscriptFrameSegmentKind::ResidentBudgetFallbackRow {
                reason: reason.into(),
            },
        }
    }

    pub(crate) fn streamed_chunk_anchor(&self) -> Option<&TranscriptViewportChunkAnchor> {
        match &self.kind {
            TranscriptFrameSegmentKind::StreamedChunk { chunk }
            | TranscriptFrameSegmentKind::RenderBudgetFallbackChunk { chunk, .. } => Some(chunk),
            TranscriptFrameSegmentKind::OrdinaryRow
            | TranscriptFrameSegmentKind::ResidentBudgetFallbackRow { .. } => None,
        }
    }
}

impl TranscriptFrameSegment {
    pub(crate) fn new(key: TranscriptFrameSegmentKey, measured_height: Option<Pixels>) -> Self {
        Self {
            key,
            measured_height: measured_height.map(|height| height.max(px(0.0))),
            streamed_chunk_count: None,
        }
    }

    pub(crate) fn with_streamed_chunk_count(mut self, chunk_count: usize) -> Self {
        self.streamed_chunk_count = Some(chunk_count.max(1));
        self
    }
}

impl TranscriptViewportLocalOffsetBasis {
    fn trailing_for_height_and_offset(height: Pixels, local_offset: Pixels) -> Self {
        Self::Trailing {
            distance_from_end: (height.max(px(0.0)) - local_offset.max(px(0.0))).max(px(0.0)),
        }
    }

    fn effective_local_offset(
        self,
        stored_local_offset: Pixels,
        measured_height: Option<Pixels>,
    ) -> Pixels {
        match self {
            Self::Top => stored_local_offset.max(px(0.0)),
            Self::Trailing { distance_from_end } => measured_height
                .map(|height| (height.max(px(0.0)) - distance_from_end).max(px(0.0)))
                .unwrap_or_else(|| stored_local_offset.max(px(0.0))),
        }
    }

    fn measurement_baseline_height(self, stored_local_offset: Pixels) -> Option<Pixels> {
        match self {
            Self::Top => None,
            Self::Trailing { distance_from_end } => {
                Some(stored_local_offset.max(px(0.0)) + distance_from_end)
            }
        }
    }
}

impl TranscriptViewportScrollCursor {
    fn new(
        segment: TranscriptFrameSegment,
        local_offset: Pixels,
        placement: TranscriptViewportPlacement,
    ) -> Self {
        Self {
            segment,
            local_offset: local_offset.max(px(0.0)),
            placement,
            local_offset_basis: TranscriptViewportLocalOffsetBasis::Top,
        }
    }

    fn with_local_offset_basis(mut self, basis: TranscriptViewportLocalOffsetBasis) -> Self {
        self.local_offset_basis = basis;
        self
    }

    pub(crate) fn effective_local_offset(&self, measured_height: Option<Pixels>) -> Pixels {
        self.local_offset_basis
            .effective_local_offset(self.local_offset, measured_height)
    }
}

impl TranscriptOrdinaryViewportAnchor {
    pub(crate) fn effective_local_offset(&self, measured_height: Option<Pixels>) -> Pixels {
        self.local_offset_basis
            .effective_local_offset(self.local_offset, measured_height)
    }

    fn set_local_offset(
        &mut self,
        local_offset: Pixels,
        basis: TranscriptViewportLocalOffsetBasis,
    ) {
        self.local_offset = local_offset.max(px(0.0));
        self.local_offset_basis = basis;
    }
}

impl TranscriptSegmentMeasurementRevision {
    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }
}

impl TranscriptSegmentMeasurementKey {
    pub(crate) fn new(
        segment: TranscriptFrameSegmentKey,
        revision: TranscriptSegmentMeasurementRevision,
    ) -> Self {
        Self { segment, revision }
    }
}

impl TranscriptSegmentMeasurementQueue {
    pub(crate) fn is_empty(&self) -> bool {
        self.staged.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.staged.clear();
    }

    pub(crate) fn retain_keys(
        &mut self,
        mut keep: impl FnMut(&TranscriptSegmentMeasurementKey) -> bool,
    ) {
        self.staged.retain(|key, _| keep(key));
    }

    pub(crate) fn stage(&mut self, key: TranscriptSegmentMeasurementKey, height: Pixels) {
        self.staged.insert(key, height.max(px(0.0)));
    }

    pub(crate) fn commit_into(
        &mut self,
        cache: &mut TranscriptSegmentMeasurementCache,
        anchor: Option<&TranscriptSegmentMeasurementAnchor>,
    ) -> TranscriptSegmentMeasurementCommit {
        let mut commit = TranscriptSegmentMeasurementCommit::default();
        for (key, measured_height) in self.staged.drain() {
            cache.retain_only_revision_for_segment(&key.segment, key.revision);
            let previous_height = cache.heights.get(&key).copied();
            if previous_height == Some(measured_height) {
                commit.unchanged = commit.unchanged.saturating_add(1);
                continue;
            }

            if let Some(anchor) = anchor
                && anchor.key == key.segment
                && anchor.local_offset > px(0.0)
                && let Some(previous_height) = previous_height.or_else(|| {
                    anchor
                        .local_offset_basis
                        .measurement_baseline_height(anchor.local_offset)
                })
            {
                commit.anchor_offset_correction += measured_height - previous_height;
            }

            cache.heights.insert(key.clone(), measured_height);
            commit.changed.push(TranscriptSegmentMeasurementChange {
                key,
                previous_height,
                measured_height,
            });
        }
        commit
    }
}

impl TranscriptSegmentMeasurementCache {
    pub(crate) fn clear(&mut self) {
        self.heights.clear();
        self.active_revisions.clear();
    }

    pub(crate) fn height(&self, key: &TranscriptSegmentMeasurementKey) -> Option<Pixels> {
        self.heights.get(key).copied()
    }

    fn retain_only_revision_for_segment(
        &mut self,
        segment: &TranscriptFrameSegmentKey,
        revision: TranscriptSegmentMeasurementRevision,
    ) {
        let previous_revision = self.active_revisions.insert(segment.clone(), revision);
        match previous_revision {
            None => return,
            Some(previous) if previous == revision => return,
            Some(_) => {}
        }
        self.heights
            .retain(|key, _| key.segment != *segment || key.revision == revision);
    }
}

impl TranscriptViewportFrame {
    pub(crate) fn new(
        segments: Vec<TranscriptFrameSegment>,
        visible_segment_range: Range<usize>,
        local_scroll_offset: Pixels,
        local_scroll_max: Pixels,
    ) -> Self {
        let visible_segment_range = clamp_segment_range(visible_segment_range, segments.len());
        Self {
            segments,
            visible_segment_range,
            local_scroll_offset: local_scroll_offset.max(px(0.0)),
            local_scroll_max: local_scroll_max.max(px(0.0)),
        }
    }

    pub(crate) fn segments(&self) -> &[TranscriptFrameSegment] {
        &self.segments
    }

    pub(crate) fn visible_segment_range(&self) -> Range<usize> {
        self.visible_segment_range.clone()
    }

    pub(crate) fn local_scroll_offset(&self) -> Pixels {
        self.local_scroll_offset
    }

    pub(crate) fn local_scroll_max(&self) -> Pixels {
        self.local_scroll_max
    }

    pub(crate) fn with_local_scroll_range(
        mut self,
        local_scroll_offset: Pixels,
        local_scroll_max: Pixels,
    ) -> Self {
        self.local_scroll_offset = local_scroll_offset.max(px(0.0));
        self.local_scroll_max = local_scroll_max.max(px(0.0));
        self
    }

    pub(crate) fn with_event_time_scroll_cursor(
        &self,
        cursor: &TranscriptViewportScrollCursor,
    ) -> Option<Self> {
        let cursor_index = self
            .segments
            .iter()
            .position(|segment| segment.key == cursor.segment.key)?;
        let visible_range = if self.visible_segment_range.contains(&cursor_index) {
            self.visible_segment_range.clone()
        } else {
            cursor_index..cursor_index.saturating_add(1)
        };
        let visible_top = self.segment_top(visible_range.start);
        let cursor_top = self.segment_top(cursor_index);
        let cursor_segment_height = self
            .segments
            .get(cursor_index)
            .and_then(|segment| segment.measured_height)
            .map(|height| height.max(px(0.0)));
        let cursor_local_offset = cursor.effective_local_offset(cursor_segment_height);
        let local_offset = (cursor_top - visible_top + cursor_local_offset).max(px(0.0));
        let local_max = if self.visible_segment_range.contains(&cursor_index) {
            self.local_scroll_max.max(local_offset)
        } else {
            let previous_visible_top = self.segment_top(self.visible_segment_range.start);
            let previous_absolute_max = previous_visible_top + self.local_scroll_max;
            let previous_rebased_max = (previous_absolute_max - visible_top).max(px(0.0));
            let cursor_segment_height = cursor_segment_height.unwrap_or(cursor_local_offset);
            let cursor_segment_bottom = cursor_top + cursor_segment_height;
            let cursor_rebased_bottom = (cursor_segment_bottom - visible_top).max(px(0.0));
            previous_rebased_max
                .max(cursor_rebased_bottom)
                .max(local_offset)
        };

        Some(Self::new(
            self.segments.clone(),
            visible_range,
            local_offset,
            local_max,
        ))
    }

    pub(crate) fn with_viewport_anchor_rebased(&self, viewport: &TranscriptViewportState) -> Self {
        let Some(local_offset) = self.local_scroll_offset_for_viewport_anchor(viewport) else {
            return self.clone();
        };
        self.clone()
            .with_local_scroll_range(local_offset, self.local_scroll_max.max(local_offset))
    }

    fn local_scroll_offset_for_viewport_anchor(
        &self,
        viewport: &TranscriptViewportState,
    ) -> Option<Pixels> {
        let TranscriptViewportMode::Ordinary(anchor) = viewport.mode() else {
            return None;
        };
        if anchor.placement != TranscriptViewportPlacement::Top {
            return None;
        }
        let index = self.segments.iter().position(|segment| {
            transcript_frame_turn_matches(&segment.key.turn, &anchor.turn)
                && matches!(
                    segment.key.kind,
                    TranscriptFrameSegmentKind::OrdinaryRow
                        | TranscriptFrameSegmentKind::ResidentBudgetFallbackRow { .. }
                )
        })?;
        if !self.visible_segment_range.contains(&index) {
            return None;
        }
        let visible_top = self.segment_top(self.visible_segment_range.start);
        let segment_top = self.segment_top(index);
        let segment_height = self
            .segments
            .get(index)
            .and_then(|segment| segment.measured_height);
        Some(
            (segment_top - visible_top + anchor.effective_local_offset(segment_height))
                .max(px(0.0)),
        )
    }

    pub(crate) fn first_visible_segment(&self) -> Option<&TranscriptFrameSegment> {
        if self.visible_segment_range.is_empty() {
            return None;
        }
        self.segments.get(self.visible_segment_range.start)
    }

    pub(crate) fn last_visible_segment(&self) -> Option<&TranscriptFrameSegment> {
        if self.visible_segment_range.is_empty() {
            return None;
        }
        self.visible_segment_range
            .end
            .checked_sub(1)
            .and_then(|index| self.segments.get(index))
    }

    pub(crate) fn segment_before_visible(&self) -> Option<&TranscriptFrameSegment> {
        if self.visible_segment_range.is_empty() {
            return None;
        }
        self.visible_segment_range
            .start
            .checked_sub(1)
            .and_then(|index| self.segments.get(index))
    }

    pub(crate) fn segment_after_visible(&self) -> Option<&TranscriptFrameSegment> {
        if self.visible_segment_range.is_empty() {
            return None;
        }
        self.segments.get(self.visible_segment_range.end)
    }

    pub(crate) fn adjacent_segment(
        &self,
        key: &TranscriptFrameSegmentKey,
        direction: TranscriptViewportNavigationDirection,
    ) -> Option<&TranscriptFrameSegment> {
        let index = self
            .segments
            .iter()
            .position(|segment| &segment.key == key)?;
        match direction {
            TranscriptViewportNavigationDirection::Up => index
                .checked_sub(1)
                .and_then(|index| self.segments.get(index)),
            TranscriptViewportNavigationDirection::Down => self.segments.get(index + 1),
        }
    }

    pub(crate) fn adjacent_streamed_chunk_at_visible_edge(
        &self,
        direction: TranscriptViewportNavigationDirection,
        current_turn: &TranscriptViewportTurnAnchor,
    ) -> Option<TranscriptViewportChunkAnchor> {
        let segment = match direction {
            TranscriptViewportNavigationDirection::Up => self.segment_before_visible(),
            TranscriptViewportNavigationDirection::Down => self.segment_after_visible(),
        }?;
        if !transcript_frame_turn_matches(&segment.key.turn, current_turn) {
            return None;
        }
        segment.key.streamed_chunk_anchor().cloned()
    }

    pub(crate) fn current_scroll_cursor(&self) -> Option<TranscriptViewportScrollCursor> {
        self.scroll_cursor_for_visible_local_offset(self.local_scroll_offset)
    }

    pub(crate) fn absolute_offset_for_cursor(
        &self,
        cursor: &TranscriptViewportScrollCursor,
    ) -> Option<Pixels> {
        let index = self
            .segments
            .iter()
            .position(|segment| segment.key == cursor.segment.key)?;
        let measured_height = self
            .segments
            .get(index)
            .and_then(|segment| segment.measured_height);
        Some(
            (self.segment_top(index) + cursor.effective_local_offset(measured_height)).max(px(0.0)),
        )
    }

    fn segment_top(&self, index: usize) -> Pixels {
        self.segments
            .iter()
            .take(index.min(self.segments.len()))
            .fold(px(0.0), |top, segment| {
                top + segment.measured_height.unwrap_or(px(0.0)).max(px(0.0))
            })
    }

    pub(crate) fn reduce_scroll_delta(
        &self,
        direction: TranscriptViewportNavigationDirection,
        distance: Pixels,
    ) -> TranscriptViewportFrameReduction {
        let distance = distance.max(px(0.0));
        let local_max = self.local_scroll_max.max(px(0.0));
        let local_offset = self.local_scroll_offset.clamp(px(0.0), local_max);
        let (edge_offset, local_available) = match direction {
            TranscriptViewportNavigationDirection::Up => (px(0.0), local_offset),
            TranscriptViewportNavigationDirection::Down => {
                (local_max, (local_max - local_offset).max(px(0.0)))
            }
        };
        let local_consumed = distance.min(local_available);
        let mut next_offset = match direction {
            TranscriptViewportNavigationDirection::Up => local_offset - local_consumed,
            TranscriptViewportNavigationDirection::Down => local_offset + local_consumed,
        };
        let residual_after_local = distance - local_consumed;
        if residual_after_local <= px(0.0) {
            return TranscriptViewportFrameReduction {
                cursor: (local_consumed > px(0.0))
                    .then(|| self.scroll_cursor_for_visible_local_offset(next_offset))
                    .flatten(),
                residual_delta: px(0.0),
                boundary: None,
            };
        }

        next_offset = edge_offset;
        let mut residual = residual_after_local;
        let mut consumed_adjacent = px(0.0);
        let mut hit_unknown_segment = false;

        for (_, segment) in self.adjacent_segments_from_visible_edge(direction) {
            let Some(height) = segment.measured_height.map(|height| height.max(px(0.0))) else {
                hit_unknown_segment = true;
                break;
            };
            if residual <= height {
                consumed_adjacent += residual;
                residual = px(0.0);
                break;
            }
            consumed_adjacent += height;
            residual -= height;
        }

        next_offset = match direction {
            TranscriptViewportNavigationDirection::Up => next_offset - consumed_adjacent,
            TranscriptViewportNavigationDirection::Down => next_offset + consumed_adjacent,
        };
        let boundary = (residual > px(0.0) && !hit_unknown_segment)
            .then_some(boundary_for_direction(direction));
        let consumed = (distance - residual.max(px(0.0))).max(px(0.0));

        TranscriptViewportFrameReduction {
            cursor: (consumed > px(0.0))
                .then(|| self.scroll_cursor_for_visible_local_offset(next_offset))
                .flatten(),
            residual_delta: residual.max(px(0.0)),
            boundary,
        }
    }

    fn scroll_cursor_for_visible_local_offset(
        &self,
        offset: Pixels,
    ) -> Option<TranscriptViewportScrollCursor> {
        if self.visible_segment_range.is_empty() {
            return None;
        }
        let visible_start = self.visible_segment_range.start.min(self.segments.len());
        if offset < px(0.0) {
            return self.scroll_cursor_before_visible_start(visible_start, px(0.0) - offset);
        }
        self.scroll_cursor_after_visible_start(visible_start, offset)
    }

    fn scroll_cursor_after_visible_start(
        &self,
        visible_start: usize,
        mut offset: Pixels,
    ) -> Option<TranscriptViewportScrollCursor> {
        let mut index = visible_start;
        while let Some(segment) = self.segments.get(index) {
            let height = segment.measured_height.map(|height| height.max(px(0.0)));
            let Some(height) = height else {
                return index
                    .checked_sub(1)
                    .and_then(|previous| self.segments.get(previous))
                    .and_then(|previous| {
                        previous.measured_height.map(|height| {
                            TranscriptViewportScrollCursor::new(
                                previous.clone(),
                                height,
                                TranscriptViewportPlacement::Top,
                            )
                        })
                    });
            };
            if height <= px(0.0) {
                if offset <= px(0.0) {
                    return Some(TranscriptViewportScrollCursor::new(
                        segment.clone(),
                        px(0.0),
                        TranscriptViewportPlacement::Top,
                    ));
                }
                index += 1;
                continue;
            }
            if offset < height || index + 1 >= self.segments.len() {
                return Some(TranscriptViewportScrollCursor::new(
                    segment.clone(),
                    offset.min(height),
                    TranscriptViewportPlacement::Top,
                ));
            }
            offset -= height;
            index += 1;
        }
        None
    }

    fn scroll_cursor_before_visible_start(
        &self,
        visible_start: usize,
        mut distance: Pixels,
    ) -> Option<TranscriptViewportScrollCursor> {
        let mut index = visible_start.checked_sub(1)?;
        loop {
            let segment = self.segments.get(index)?;
            let height = segment.measured_height.map(|height| height.max(px(0.0)))?;
            if distance <= height {
                return Some(
                    TranscriptViewportScrollCursor::new(
                        segment.clone(),
                        height - distance,
                        TranscriptViewportPlacement::Top,
                    )
                    .with_local_offset_basis(
                        TranscriptViewportLocalOffsetBasis::trailing_for_height_and_offset(
                            height,
                            height - distance,
                        ),
                    ),
                );
            }
            distance -= height;
            index = index.checked_sub(1)?;
        }
    }

    fn adjacent_segments_from_visible_edge(
        &self,
        direction: TranscriptViewportNavigationDirection,
    ) -> Vec<(usize, &TranscriptFrameSegment)> {
        if self.visible_segment_range.is_empty() {
            return Vec::new();
        }
        match direction {
            TranscriptViewportNavigationDirection::Up => self.segments
                [..self.visible_segment_range.start.min(self.segments.len())]
                .iter()
                .enumerate()
                .rev()
                .collect(),
            TranscriptViewportNavigationDirection::Down => self.segments
                [self.visible_segment_range.end.min(self.segments.len())..]
                .iter()
                .enumerate()
                .map(|(offset, segment)| {
                    (
                        self.visible_segment_range
                            .end
                            .min(self.segments.len())
                            .saturating_add(offset),
                        segment,
                    )
                })
                .collect(),
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
            rendered_frame: None,
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
            rendered_frame: None,
        }
    }

    pub(crate) fn with_rendered_frame(mut self, frame: TranscriptViewportFrame) -> Self {
        self.rendered_frame = Some(frame);
        self
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
                local_offset_basis: TranscriptViewportLocalOffsetBasis::Top,
            })
        };
    }

    pub(crate) fn reset_to_tail_target(
        &mut self,
        target: Option<TranscriptViewportTurnTarget>,
        turn_count: usize,
    ) {
        self.live_autoscroll = TranscriptViewportLiveAutoscroll::FollowingTail;
        if let Some(target) = target {
            self.anchor_turn_target(target);
        } else {
            self.reset_to_tail(turn_count);
        }
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
            local_offset_basis: TranscriptViewportLocalOffsetBasis::Top,
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

        if matches!(self.mode, TranscriptViewportMode::Empty) {
            return outcome;
        }

        let Some(rendered_frame) = input.rendered_frame.as_ref() else {
            outcome.boundary = Some(boundary_for_direction(input.direction));
            return outcome;
        };
        if let TranscriptViewportMode::Streamed(anchor) = &mut self.mode
            && let Some(frame) = input.streamed_frame.as_ref()
        {
            outcome.changed |= anchor.reconcile_frame(frame);
        }

        let rendered_frame = rendered_frame.with_viewport_anchor_rebased(self);
        let reduction = rendered_frame.reduce_scroll_delta(input.direction, input.distance);
        if let Some(cursor) = reduction.cursor.as_ref() {
            let cursor =
                continuous_scroll_cursor_with_basis(cursor.clone(), &self.mode, input.direction);
            let same_segment = viewport_mode_matches_segment(&self.mode, &cursor.segment);
            if same_segment {
                outcome.changed |= apply_cursor_to_matching_viewport_mode(&mut self.mode, &cursor);
                outcome.ordinary_pixel_scroll |=
                    matches!(self.mode, TranscriptViewportMode::Ordinary(_));
            } else {
                let next_mode = continuous_scroll_mode_for_frame_segment(
                    &cursor.segment,
                    cursor.placement,
                    cursor.local_offset,
                    cursor.local_offset_basis,
                    input.direction,
                );
                outcome.changed |= self.mode != next_mode;
                self.mode = next_mode;
                outcome.semantic_refill = true;
            }
            outcome.scroll_cursor = Some(cursor.clone());
        }
        if reduction.residual_delta > px(0.0) {
            outcome.residual_delta = Some(reduction.residual_delta);
        }
        if let Some(boundary) = reduction.boundary {
            outcome.boundary = Some(boundary);
        }
        outcome
    }

    pub(crate) fn apply_page_to_frame(
        &mut self,
        direction: TranscriptViewportNavigationDirection,
        frame: &TranscriptViewportFrame,
    ) -> TranscriptViewportReduceOutcome {
        let mut outcome = self.detach_live_autoscroll_for_manual_navigation();
        let Some(selection) = explicit_page_frame_selection(&self.mode, direction, frame) else {
            outcome.boundary = Some(boundary_for_direction(direction));
            return outcome;
        };

        if selection.boundary {
            outcome.boundary = Some(boundary_for_direction(direction));
            return outcome;
        }

        let next_mode = explicit_navigation_mode_for_frame_segment(
            selection.segment,
            selection.placement,
            direction,
        );
        outcome.changed = self.mode != next_mode;
        self.mode = next_mode;
        outcome.semantic_refill = true;
        outcome
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
                    || anchor.placement != TranscriptViewportPlacement::Top
                    || anchor.local_offset_basis != TranscriptViewportLocalOffsetBasis::Top;
                anchor.set_local_offset(px(0.0), TranscriptViewportLocalOffsetBasis::Top);
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

    pub(crate) fn segment_measurement_anchor(&self) -> Option<TranscriptSegmentMeasurementAnchor> {
        match &self.mode {
            TranscriptViewportMode::Empty => None,
            TranscriptViewportMode::Ordinary(anchor) => Some(TranscriptSegmentMeasurementAnchor {
                key: TranscriptFrameSegmentKey::ordinary_row(anchor.turn.clone()),
                local_offset: anchor.local_offset,
                local_offset_basis: anchor.local_offset_basis,
            }),
            TranscriptViewportMode::Streamed(anchor) => Some(TranscriptSegmentMeasurementAnchor {
                key: TranscriptFrameSegmentKey::streamed_chunk(
                    anchor.turn.clone(),
                    anchor.anchor_chunk.clone(),
                ),
                local_offset: anchor.local_anchor_offset.unwrap_or(px(0.0)),
                local_offset_basis: TranscriptViewportLocalOffsetBasis::Top,
            }),
        }
    }

    pub(crate) fn apply_segment_measurement_anchor_correction(
        &mut self,
        correction: Pixels,
    ) -> bool {
        if correction == px(0.0) {
            return false;
        }
        match &mut self.mode {
            TranscriptViewportMode::Empty => false,
            TranscriptViewportMode::Ordinary(anchor) => {
                if anchor.local_offset <= px(0.0) {
                    return false;
                }
                let next = (anchor.local_offset + correction).max(px(0.0));
                if next == anchor.local_offset {
                    return false;
                }
                anchor.local_offset = next;
                true
            }
            TranscriptViewportMode::Streamed(anchor) => {
                let Some(offset) = anchor.local_anchor_offset.as_mut() else {
                    return false;
                };
                if *offset <= px(0.0) {
                    return false;
                }
                let next = (*offset + correction).max(px(0.0));
                if next == *offset {
                    return false;
                }
                *offset = next;
                true
            }
        }
    }

    pub(crate) fn reconcile_row_mutation(&mut self, mutation: TranscriptViewportRowMutation) {
        match &mut self.mode {
            TranscriptViewportMode::Empty => {}
            TranscriptViewportMode::Ordinary(anchor) => {
                anchor.turn.shift_index_for_mutation(mutation);
                anchor.set_local_offset(px(0.0), TranscriptViewportLocalOffsetBasis::Top);
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
}

fn continuous_scroll_mode_for_frame_segment(
    target: &TranscriptFrameSegment,
    placement: TranscriptViewportPlacement,
    local_offset: Pixels,
    local_offset_basis: TranscriptViewportLocalOffsetBasis,
    direction: TranscriptViewportNavigationDirection,
) -> TranscriptViewportMode {
    match &target.key.kind {
        TranscriptFrameSegmentKind::StreamedChunk { chunk }
        | TranscriptFrameSegmentKind::RenderBudgetFallbackChunk { chunk, .. } => {
            let chunk_count = target
                .streamed_chunk_count
                .unwrap_or_else(|| chunk.chunk_index.saturating_add(1).max(1));
            let mut anchor = TranscriptStreamedViewportAnchor::new(
                target.key.turn.clone(),
                chunk.clone(),
                chunk_count,
                placement,
            );
            anchor.local_anchor_offset = Some(local_offset);
            anchor.last_navigation_direction = Some(direction);
            TranscriptViewportMode::Streamed(anchor)
        }
        TranscriptFrameSegmentKind::OrdinaryRow
        | TranscriptFrameSegmentKind::ResidentBudgetFallbackRow { .. } => {
            TranscriptViewportMode::Ordinary(TranscriptOrdinaryViewportAnchor {
                turn: target.key.turn.clone(),
                placement,
                local_offset,
                local_offset_basis,
            })
        }
    }
}

fn continuous_scroll_cursor_with_basis(
    cursor: TranscriptViewportScrollCursor,
    current_mode: &TranscriptViewportMode,
    _direction: TranscriptViewportNavigationDirection,
) -> TranscriptViewportScrollCursor {
    let TranscriptViewportMode::Ordinary(anchor) = current_mode else {
        return cursor;
    };
    if !transcript_frame_turn_matches(&cursor.segment.key.turn, &anchor.turn) {
        return cursor;
    }
    if !matches!(
        anchor.local_offset_basis,
        TranscriptViewportLocalOffsetBasis::Trailing { .. }
    ) {
        return cursor;
    }
    let Some(height) = cursor
        .segment
        .measured_height
        .map(|height| height.max(px(0.0)))
    else {
        return cursor;
    };
    let local_offset = cursor.local_offset;
    cursor.with_local_offset_basis(
        TranscriptViewportLocalOffsetBasis::trailing_for_height_and_offset(height, local_offset),
    )
}

fn apply_cursor_to_matching_viewport_mode(
    mode: &mut TranscriptViewportMode,
    cursor: &TranscriptViewportScrollCursor,
) -> bool {
    match mode {
        TranscriptViewportMode::Ordinary(anchor) => {
            let changed = anchor.local_offset != cursor.local_offset
                || anchor.placement != cursor.placement
                || anchor.local_offset_basis != cursor.local_offset_basis;
            anchor.set_local_offset(cursor.local_offset, cursor.local_offset_basis);
            anchor.placement = cursor.placement;
            changed
        }
        TranscriptViewportMode::Streamed(anchor) => {
            let changed = anchor.local_anchor_offset != Some(cursor.local_offset)
                || anchor.placement != cursor.placement;
            anchor.local_anchor_offset = Some(cursor.local_offset);
            anchor.placement = cursor.placement;
            changed
        }
        TranscriptViewportMode::Empty => false,
    }
}

struct ExplicitPageFrameSelection<'a> {
    segment: &'a TranscriptFrameSegment,
    placement: TranscriptViewportPlacement,
    boundary: bool,
}

fn explicit_page_frame_selection<'a>(
    current_mode: &TranscriptViewportMode,
    direction: TranscriptViewportNavigationDirection,
    frame: &'a TranscriptViewportFrame,
) -> Option<ExplicitPageFrameSelection<'a>> {
    let visible_range = frame.visible_segment_range();
    if visible_range.is_empty() {
        return None;
    }

    match direction {
        TranscriptViewportNavigationDirection::Up => {
            let index = visible_range.start;
            let segment = frame.segments().get(index)?;
            if index == 0 && viewport_mode_matches_segment(current_mode, segment) {
                let current_placement = viewport_mode_placement(current_mode);
                return Some(ExplicitPageFrameSelection {
                    segment,
                    placement: TranscriptViewportPlacement::Top,
                    boundary: current_placement == Some(TranscriptViewportPlacement::Top),
                });
            }
            Some(ExplicitPageFrameSelection {
                segment,
                placement: TranscriptViewportPlacement::Bottom,
                boundary: false,
            })
        }
        TranscriptViewportNavigationDirection::Down => {
            let index = visible_range.end.checked_sub(1)?;
            let segment = frame.segments().get(index)?;
            if index.saturating_add(1) >= frame.segments().len()
                && viewport_mode_matches_segment(current_mode, segment)
            {
                let current_placement = viewport_mode_placement(current_mode);
                return Some(ExplicitPageFrameSelection {
                    segment,
                    placement: TranscriptViewportPlacement::Bottom,
                    boundary: current_placement == Some(TranscriptViewportPlacement::Bottom),
                });
            }
            Some(ExplicitPageFrameSelection {
                segment,
                placement: TranscriptViewportPlacement::Top,
                boundary: false,
            })
        }
    }
}

fn explicit_navigation_mode_for_frame_segment(
    target: &TranscriptFrameSegment,
    placement: TranscriptViewportPlacement,
    direction: TranscriptViewportNavigationDirection,
) -> TranscriptViewportMode {
    match &target.key.kind {
        TranscriptFrameSegmentKind::StreamedChunk { chunk }
        | TranscriptFrameSegmentKind::RenderBudgetFallbackChunk { chunk, .. } => {
            let chunk_count = target
                .streamed_chunk_count
                .unwrap_or_else(|| chunk.chunk_index.saturating_add(1))
                .max(1);
            let mut anchor = TranscriptStreamedViewportAnchor::new(
                target.key.turn.clone(),
                chunk.clone(),
                chunk_count,
                placement,
            );
            anchor.last_navigation_direction = Some(direction);
            TranscriptViewportMode::Streamed(anchor)
        }
        TranscriptFrameSegmentKind::OrdinaryRow
        | TranscriptFrameSegmentKind::ResidentBudgetFallbackRow { .. } => {
            TranscriptViewportMode::Ordinary(TranscriptOrdinaryViewportAnchor {
                turn: target.key.turn.clone(),
                placement,
                local_offset: px(0.0),
                local_offset_basis: TranscriptViewportLocalOffsetBasis::Top,
            })
        }
    }
}

fn viewport_mode_matches_segment(
    mode: &TranscriptViewportMode,
    segment: &TranscriptFrameSegment,
) -> bool {
    match (mode, &segment.key.kind) {
        (
            TranscriptViewportMode::Ordinary(anchor),
            TranscriptFrameSegmentKind::OrdinaryRow
            | TranscriptFrameSegmentKind::ResidentBudgetFallbackRow { .. },
        ) => transcript_frame_turn_matches(&segment.key.turn, &anchor.turn),
        (
            TranscriptViewportMode::Streamed(anchor),
            TranscriptFrameSegmentKind::StreamedChunk { chunk }
            | TranscriptFrameSegmentKind::RenderBudgetFallbackChunk { chunk, .. },
        ) => {
            transcript_frame_turn_matches(&segment.key.turn, &anchor.turn)
                && (chunk.chunk_identity == anchor.anchor_chunk.chunk_identity
                    || chunk.chunk_index == anchor.anchor_chunk.chunk_index)
        }
        _ => false,
    }
}

fn viewport_mode_placement(mode: &TranscriptViewportMode) -> Option<TranscriptViewportPlacement> {
    match mode {
        TranscriptViewportMode::Ordinary(anchor) => Some(anchor.placement),
        TranscriptViewportMode::Streamed(anchor) => Some(anchor.placement),
        TranscriptViewportMode::Empty => None,
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

fn clamp_segment_range(range: Range<usize>, segment_count: usize) -> Range<usize> {
    let start = range.start.min(segment_count);
    let end = range.end.min(segment_count).max(start);
    start..end
}

fn transcript_frame_turn_matches(
    candidate: &TranscriptViewportTurnAnchor,
    current: &TranscriptViewportTurnAnchor,
) -> bool {
    if candidate.row_identity.is_some() && current.row_identity.is_some() {
        return candidate.row_identity == current.row_identity;
    }
    if candidate.thread_id.is_some()
        && candidate.turn_id.is_some()
        && current.thread_id.is_some()
        && current.turn_id.is_some()
    {
        return candidate.thread_id == current.thread_id && candidate.turn_id == current.turn_id;
    }
    candidate.turn_index == current.turn_index
}
