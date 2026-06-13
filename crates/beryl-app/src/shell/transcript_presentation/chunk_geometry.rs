use std::{collections::HashMap, ops::Range};

use gpui::{Pixels, px};

use super::row_model::{TranscriptRowMeasurementKey, TranscriptRowRenderChunk};

pub(crate) const TRANSCRIPT_ROW_CHUNK_RENDER_OVERSCAN_VIEWPORTS: f32 = 0.5;
pub(crate) const TRANSCRIPT_ROW_CHUNK_UNKNOWN_RENDER_AHEAD: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TranscriptRowChunkMeasurementKey {
    pub(crate) row_key: TranscriptRowMeasurementKey,
    pub(crate) chunk_identity: String,
    pub(crate) chunk_source_revision: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptRowChunkRenderWindow {
    pub(crate) range: Range<usize>,
    pub(crate) anchor_chunk_index: usize,
    pub(crate) measured_rendered_height: Pixels,
    pub(crate) rendered_unknown_chunks: usize,
    pub(crate) reached_start: bool,
    pub(crate) reached_end: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptRowStreamedAnchorPlacement {
    Top,
    Bottom,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptRowStreamedRenderAnchor {
    pub(crate) chunk_index: usize,
    pub(crate) placement: TranscriptRowStreamedAnchorPlacement,
}

impl TranscriptRowChunkMeasurementKey {
    pub(crate) fn new(
        row_key: TranscriptRowMeasurementKey,
        chunk: &TranscriptRowRenderChunk,
    ) -> Self {
        Self {
            row_key,
            chunk_identity: chunk.identity.clone(),
            chunk_source_revision: chunk.source_revision,
        }
    }

    pub(crate) fn row_identity(&self) -> &str {
        self.row_key.row_identity.as_str()
    }
}

pub(crate) fn measured_chunk_heights_for(
    chunks: &[TranscriptRowRenderChunk],
    row_key: &TranscriptRowMeasurementKey,
    measurements: &HashMap<TranscriptRowChunkMeasurementKey, Pixels>,
) -> Vec<Option<Pixels>> {
    chunks
        .iter()
        .map(|chunk| {
            measurements
                .get(&TranscriptRowChunkMeasurementKey::new(
                    row_key.clone(),
                    chunk,
                ))
                .copied()
        })
        .collect()
}

pub(crate) fn transcript_row_chunk_render_window(
    chunk_count: usize,
    measured_chunk_heights: &[Option<Pixels>],
    anchor: TranscriptRowStreamedRenderAnchor,
    viewport_height: Pixels,
) -> TranscriptRowChunkRenderWindow {
    if chunk_count == 0 {
        return TranscriptRowChunkRenderWindow {
            range: 0..0,
            anchor_chunk_index: 0,
            measured_rendered_height: px(0.0),
            rendered_unknown_chunks: 0,
            reached_start: true,
            reached_end: true,
        };
    }

    let viewport_height = viewport_height.max(px(0.0));
    let target_height =
        viewport_height + viewport_height * TRANSCRIPT_ROW_CHUNK_RENDER_OVERSCAN_VIEWPORTS;
    let anchor_chunk_index = anchor.chunk_index.min(chunk_count - 1);

    let mut window = TranscriptRowChunkRenderAccumulator::new(
        anchor_chunk_index,
        measured_height_at(measured_chunk_heights, anchor_chunk_index),
    );

    match anchor.placement {
        TranscriptRowStreamedAnchorPlacement::Top => {
            fill_down(
                &mut window,
                chunk_count,
                measured_chunk_heights,
                target_height,
            );
            fill_up(&mut window, measured_chunk_heights, target_height);
        }
        TranscriptRowStreamedAnchorPlacement::Bottom => {
            fill_up(&mut window, measured_chunk_heights, target_height);
            fill_down(
                &mut window,
                chunk_count,
                measured_chunk_heights,
                target_height,
            );
        }
    }

    TranscriptRowChunkRenderWindow {
        range: window.start..window.end,
        anchor_chunk_index,
        measured_rendered_height: window.measured_height,
        rendered_unknown_chunks: window.unknown_chunks,
        reached_start: window.start == 0,
        reached_end: window.end >= chunk_count,
    }
}

fn measured_height_at(measured_chunk_heights: &[Option<Pixels>], index: usize) -> Option<Pixels> {
    measured_chunk_heights
        .get(index)
        .and_then(|height| *height)
        .map(|height| height.max(px(0.0)))
}

#[derive(Clone, Debug)]
struct TranscriptRowChunkRenderAccumulator {
    start: usize,
    end: usize,
    measured_height: Pixels,
    unknown_chunks: usize,
}

impl TranscriptRowChunkRenderAccumulator {
    fn new(anchor_index: usize, anchor_height: Option<Pixels>) -> Self {
        let mut this = Self {
            start: anchor_index,
            end: anchor_index.saturating_add(1),
            measured_height: px(0.0),
            unknown_chunks: 0,
        };
        this.add_height(anchor_height);
        this
    }

    fn add_height(&mut self, height: Option<Pixels>) {
        match height {
            Some(height) => self.measured_height += height,
            None => self.unknown_chunks = self.unknown_chunks.saturating_add(1),
        }
    }

    fn is_saturated(&self, target_height: Pixels) -> bool {
        self.measured_height >= target_height
            || self.unknown_chunks >= TRANSCRIPT_ROW_CHUNK_UNKNOWN_RENDER_AHEAD
    }
}

fn fill_down(
    window: &mut TranscriptRowChunkRenderAccumulator,
    chunk_count: usize,
    measured_chunk_heights: &[Option<Pixels>],
    target_height: Pixels,
) {
    while window.end < chunk_count && !window.is_saturated(target_height) {
        let next = window.end;
        window.end = window.end.saturating_add(1);
        window.add_height(measured_height_at(measured_chunk_heights, next));
    }
}

fn fill_up(
    window: &mut TranscriptRowChunkRenderAccumulator,
    measured_chunk_heights: &[Option<Pixels>],
    target_height: Pixels,
) {
    while window.start > 0 && !window.is_saturated(target_height) {
        window.start = window.start.saturating_sub(1);
        window.add_height(measured_height_at(measured_chunk_heights, window.start));
    }
}
