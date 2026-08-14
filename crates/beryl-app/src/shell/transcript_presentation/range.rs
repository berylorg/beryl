use std::ops::Range;

use gpui::Pixels;

use super::super::virtual_list::{ListScrollPosition, ListState};

pub(crate) const TRANSCRIPT_INITIAL_PRESENTATION_ROWS: usize = 96;
pub(crate) const TRANSCRIPT_MAX_PRESENTATION_ROWS: usize = 256;

pub(crate) fn transcript_frame_presentation_range(
    list_state: &ListState,
    turn_count: usize,
) -> Range<usize> {
    let range = clamp_transcript_range(list_state.presentation_range(), turn_count);
    if range.len() <= TRANSCRIPT_MAX_PRESENTATION_ROWS && (!range.is_empty() || turn_count == 0) {
        return range;
    }

    fallback_transcript_presentation_range(list_state, turn_count)
}

pub(crate) fn transcript_frame_preload_range(
    list_state: &ListState,
    turn_count: usize,
    vertical_margin: Pixels,
) -> Range<usize> {
    let range = clamp_transcript_range(
        list_state.range_with_vertical_margin(vertical_margin),
        turn_count,
    );
    if range.len() <= TRANSCRIPT_MAX_PRESENTATION_ROWS {
        return range;
    }

    let visible = clamp_transcript_range(list_state.visible_range(), turn_count);
    if visible.is_empty() {
        let end = range
            .start
            .saturating_add(TRANSCRIPT_MAX_PRESENTATION_ROWS)
            .min(range.end);
        return range.start..end;
    }

    let extra = TRANSCRIPT_MAX_PRESENTATION_ROWS.saturating_sub(visible.len());
    let before = extra / 2;
    let mut start = visible.start.saturating_sub(before).max(range.start);
    let mut end = start
        .saturating_add(TRANSCRIPT_MAX_PRESENTATION_ROWS)
        .min(range.end);
    start = end
        .saturating_sub(TRANSCRIPT_MAX_PRESENTATION_ROWS)
        .max(range.start);
    end = end.max(start);
    start..end
}

fn fallback_transcript_presentation_range(
    list_state: &ListState,
    turn_count: usize,
) -> Range<usize> {
    match list_state.scroll_position() {
        ListScrollPosition::Content(offset) => {
            let start = offset.item_ix.min(turn_count);
            let end = start
                .saturating_add(TRANSCRIPT_INITIAL_PRESENTATION_ROWS)
                .min(turn_count);
            start..end
        }
        ListScrollPosition::Bottom | ListScrollPosition::VirtualTail { .. } => {
            turn_count.saturating_sub(TRANSCRIPT_INITIAL_PRESENTATION_ROWS)..turn_count
        }
    }
}

fn clamp_transcript_range(range: Range<usize>, turn_count: usize) -> Range<usize> {
    let start = range.start.min(turn_count);
    let end = range.end.min(turn_count).max(start);
    start..end
}
