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
    pub(crate) top_spacer_height: Pixels,
    pub(crate) bottom_spacer_height: Pixels,
    pub(crate) rendered_unknown_chunks: usize,
    pub(crate) skipped_unknown_chunks: usize,
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
    row_scroll_offset: Pixels,
    viewport_height: Pixels,
) -> TranscriptRowChunkRenderWindow {
    if chunk_count == 0 {
        return TranscriptRowChunkRenderWindow {
            range: 0..0,
            top_spacer_height: px(0.0),
            bottom_spacer_height: px(0.0),
            rendered_unknown_chunks: 0,
            skipped_unknown_chunks: 0,
        };
    }

    let viewport_height = viewport_height.max(px(0.0));
    let row_scroll_offset = row_scroll_offset.max(px(0.0));
    let overscan_height = viewport_height * TRANSCRIPT_ROW_CHUNK_RENDER_OVERSCAN_VIEWPORTS;
    let target_start = (row_scroll_offset - overscan_height).max(px(0.0));
    let target_end = (row_scroll_offset + viewport_height + overscan_height).max(target_start);

    let mut start = 0usize;
    let mut measured_offset = px(0.0);
    while start < chunk_count {
        let Some(height) = measured_height_at(measured_chunk_heights, start) else {
            break;
        };
        let next_offset = measured_offset + height;
        if next_offset > target_start {
            break;
        }
        measured_offset = next_offset;
        start = start.saturating_add(1);
    }

    let mut end = start;
    let mut rendered_unknown_chunks = 0usize;
    let mut measured_end_offset = measured_offset;
    while end < chunk_count {
        match measured_height_at(measured_chunk_heights, end) {
            Some(height) => {
                measured_end_offset += height;
            }
            None => {
                rendered_unknown_chunks = rendered_unknown_chunks.saturating_add(1);
            }
        }
        end = end.saturating_add(1);

        if rendered_unknown_chunks == 0
            && measured_end_offset >= target_end
            && !suffix_has_unknown(measured_chunk_heights, end, chunk_count)
        {
            break;
        }

        if rendered_unknown_chunks >= TRANSCRIPT_ROW_CHUNK_UNKNOWN_RENDER_AHEAD
            && measured_end_offset >= row_scroll_offset
        {
            break;
        }
    }

    if end == start {
        end = start.saturating_add(1).min(chunk_count);
    }

    let (bottom_spacer_height, skipped_unknown_chunks) =
        measured_suffix_height(measured_chunk_heights, end, chunk_count);

    TranscriptRowChunkRenderWindow {
        range: start..end,
        top_spacer_height: measured_offset,
        bottom_spacer_height,
        rendered_unknown_chunks,
        skipped_unknown_chunks,
    }
}

fn measured_height_at(measured_chunk_heights: &[Option<Pixels>], index: usize) -> Option<Pixels> {
    measured_chunk_heights
        .get(index)
        .and_then(|height| *height)
        .map(|height| height.max(px(0.0)))
}

fn measured_suffix_height(
    measured_chunk_heights: &[Option<Pixels>],
    start: usize,
    chunk_count: usize,
) -> (Pixels, usize) {
    let mut height = px(0.0);
    let mut skipped_unknown_chunks = 0usize;
    for index in start..chunk_count {
        match measured_height_at(measured_chunk_heights, index) {
            Some(chunk_height) => height += chunk_height,
            None => skipped_unknown_chunks = skipped_unknown_chunks.saturating_add(1),
        }
    }
    if skipped_unknown_chunks > 0 {
        (px(0.0), skipped_unknown_chunks)
    } else {
        (height, 0)
    }
}

fn suffix_has_unknown(
    measured_chunk_heights: &[Option<Pixels>],
    start: usize,
    chunk_count: usize,
) -> bool {
    (start..chunk_count).any(|index| measured_height_at(measured_chunk_heights, index).is_none())
}
