use std::ops::Range;

use gpui::{ScrollHandle, div, prelude::*, px};

pub(super) const NAVIGATOR_ROW_HEIGHT: f32 = 32.0;
pub(super) const NAVIGATOR_ROW_GAP: f32 = 4.0;
pub(super) const NAVIGATOR_ROW_OVERSCAN: usize = 3;
pub(super) const NAVIGATOR_COLUMN_WIDTH: f32 = 216.0;
pub(super) const NAVIGATOR_COLUMN_GAP: f32 = 12.0;
pub(super) const NAVIGATOR_COLUMN_HEADER_HEIGHT: f32 = 30.0;
const NAVIGATOR_FALLBACK_BODY_HEIGHT: f32 = 156.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ThemeRoleNavigatorRenderStrategy {
    pub(crate) row_height_px: u16,
    pub(crate) overscan_rows: usize,
    pub(crate) windowed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RowWindow {
    pub(super) range: Range<usize>,
    pub(super) total_height: f32,
    pub(super) top_spacer_height: f32,
    pub(super) bottom_spacer_height: f32,
}

impl ThemeRoleNavigatorRenderStrategy {
    pub(super) fn fixed_height_windowed() -> Self {
        Self {
            row_height_px: NAVIGATOR_ROW_HEIGHT as u16,
            overscan_rows: NAVIGATOR_ROW_OVERSCAN,
            windowed: true,
        }
    }
}

pub(super) fn role_row_window(row_count: usize, scroll_handle: &ScrollHandle) -> RowWindow {
    let viewport_height = role_row_viewport_height(scroll_handle);
    let scroll_offset = -f32::from(scroll_handle.offset().y);
    role_row_window_for_metrics(row_count, scroll_offset, viewport_height)
}

pub(super) fn role_row_visible_window(row_count: usize, scroll_handle: &ScrollHandle) -> RowWindow {
    let viewport_height = role_row_viewport_height(scroll_handle);
    let scroll_offset = -f32::from(scroll_handle.offset().y);
    role_row_window_for_metrics_with_overscan(row_count, scroll_offset, viewport_height, 0)
}

fn role_row_viewport_height(scroll_handle: &ScrollHandle) -> f32 {
    let viewport_height = f32::from(scroll_handle.bounds().size.height);
    if viewport_height > 0.0 {
        viewport_height
    } else {
        NAVIGATOR_FALLBACK_BODY_HEIGHT - NAVIGATOR_COLUMN_HEADER_HEIGHT - 16.0
    }
}

fn role_row_window_for_metrics(
    row_count: usize,
    scroll_offset: f32,
    viewport_height: f32,
) -> RowWindow {
    role_row_window_for_metrics_with_overscan(
        row_count,
        scroll_offset,
        viewport_height,
        NAVIGATOR_ROW_OVERSCAN,
    )
}

fn role_row_window_for_metrics_with_overscan(
    row_count: usize,
    scroll_offset: f32,
    viewport_height: f32,
    overscan_rows: usize,
) -> RowWindow {
    if row_count == 0 {
        return RowWindow {
            range: 0..0,
            total_height: 0.0,
            top_spacer_height: 0.0,
            bottom_spacer_height: 0.0,
        };
    }

    let stride = NAVIGATOR_ROW_HEIGHT + NAVIGATOR_ROW_GAP;
    let visible_start = (scroll_offset.max(0.0) / stride).floor() as usize;
    let visible_len = ((viewport_height.max(0.0) / stride).ceil() as usize).saturating_add(1);
    let start = visible_start.saturating_sub(overscan_rows);
    let end = visible_start
        .saturating_add(visible_len)
        .saturating_add(overscan_rows)
        .min(row_count);
    let range = start..end.max(start);
    let total_height = row_segment_height(row_count);
    let top_spacer_height = row_offset_for_index(range.start);
    let rendered_height = row_segment_height(range.len());
    let bottom_spacer_height = (total_height - top_spacer_height - rendered_height).max(0.0);

    RowWindow {
        range,
        total_height,
        top_spacer_height,
        bottom_spacer_height,
    }
}

fn row_offset_for_index(index: usize) -> f32 {
    if index == 0 {
        0.0
    } else {
        index as f32 * (NAVIGATOR_ROW_HEIGHT + NAVIGATOR_ROW_GAP)
    }
}

pub(super) fn row_segment_height(count: usize) -> f32 {
    if count == 0 {
        0.0
    } else {
        count as f32 * NAVIGATOR_ROW_HEIGHT + count.saturating_sub(1) as f32 * NAVIGATOR_ROW_GAP
    }
}

pub(super) fn column_trail_width(column_count: usize) -> f32 {
    if column_count == 0 {
        0.0
    } else {
        column_count as f32 * NAVIGATOR_COLUMN_WIDTH
            + column_count.saturating_sub(1) as f32 * NAVIGATOR_COLUMN_GAP
            + 16.0
    }
}

pub(super) fn spacer(height: f32) -> gpui::Div {
    div().w_full().h(px(height)).min_h(px(height))
}

#[cfg(test)]
pub(crate) fn theme_role_navigator_render_strategy_for_test() -> ThemeRoleNavigatorRenderStrategy {
    ThemeRoleNavigatorRenderStrategy::fixed_height_windowed()
}

#[cfg(test)]
pub(crate) fn theme_role_navigator_row_window_for_test(
    row_count: usize,
    scroll_offset: f32,
    viewport_height: f32,
) -> Range<usize> {
    role_row_window_for_metrics(row_count, scroll_offset, viewport_height).range
}

#[cfg(test)]
pub(crate) fn theme_role_navigator_row_window_height_sum_for_test(
    row_count: usize,
    scroll_offset: f32,
    viewport_height: f32,
) -> (Range<usize>, f32, f32) {
    let window = role_row_window_for_metrics(row_count, scroll_offset, viewport_height);
    let rendered_height = row_segment_height(window.range.len());
    (
        window.range,
        window.total_height,
        window.top_spacer_height + rendered_height + window.bottom_spacer_height,
    )
}
