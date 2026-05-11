use std::ops::Range;

use gpui::{Pixels, px};

pub(crate) const WINDOW_MIN_WIDTH: f32 = 420.0;
pub(crate) const WINDOW_MIN_HEIGHT: f32 = 320.0;

pub(crate) const TOOLBAR_STRIP_HEIGHT: f32 = 52.0;
pub(crate) const THREAD_STRIP_HEIGHT: f32 = 40.0;
pub(crate) const STATUS_LINE_HEIGHT: f32 = 28.0;
pub(crate) const STANDARD_UI_TEXT_FONT_SIZE: f32 = 16.0;
pub(crate) const STANDARD_UI_TEXT_LINE_HEIGHT: f32 = STANDARD_UI_TEXT_FONT_SIZE * 1.625;
pub(crate) const ROUNDED_WIDGET_CORNER_RADIUS: f32 = 6.0;
pub(crate) const BUTTON_OUTER_HEIGHT: f32 = STANDARD_UI_TEXT_LINE_HEIGHT;
pub(crate) const BUTTON_ICON_OUTER_WIDTH: f32 = BUTTON_OUTER_HEIGHT;
pub(crate) const BUTTON_LABEL_FONT_SIZE: f32 = 12.0;
pub(crate) const BUTTON_LABEL_LINE_HEIGHT: f32 = BUTTON_LABEL_FONT_SIZE;
pub(crate) const BUTTON_LABEL_CAP_HEIGHT_ESTIMATE: f32 = BUTTON_LABEL_FONT_SIZE;
pub(crate) const BUTTON_BORDER_WIDTH: f32 = 1.0;
pub(crate) const BUTTON_VERTICAL_PADDING: f32 =
    button_padding_from_label_cap_height(BUTTON_LABEL_CAP_HEIGHT_ESTIMATE);
pub(crate) const BUTTON_HORIZONTAL_PADDING: f32 = BUTTON_VERTICAL_PADDING;
pub(crate) const TOOL_ACTIVITY_ROW_HEIGHT: f32 = 28.0;
pub(crate) const TOOL_ACTIVITY_OVERSCAN_ROWS: usize = 3;
pub(crate) const TOOL_ACTIVITY_MIN_PANEL_HEIGHT: f32 = TOOL_ACTIVITY_ROW_HEIGHT;
pub(crate) const TOOL_ACTIVITY_RESIZE_HANDLE_HEIGHT: f32 = 8.0;
pub(crate) const CHECKLIST_SIDEBAR_ROW_HEIGHT: f32 = 56.0;
pub(crate) const CHECKLIST_SIDEBAR_OVERSCAN_ROWS: usize = 4;
pub(crate) const THREAD_SELECTOR_ROW_HEIGHT: f32 = 42.0;
pub(crate) const THREAD_SELECTOR_ROW_GAP: f32 = 8.0;
pub(crate) const THREAD_SELECTOR_OVERSCAN_ROWS: usize = 4;
pub(crate) const PANEL_DIVIDER_WIDTH: f32 = 10.0;
pub(crate) const PANEL_MIN_WIDTH: f32 = 100.0;
pub(crate) const MAIN_REGION_MIN_HEIGHT: f32 = 120.0;
pub(crate) const COMPOSER_MIN_HEIGHT: f32 = 74.0;
pub(crate) const DEFAULT_COMPOSER_HEIGHT: f32 = COMPOSER_MIN_HEIGHT;
pub(crate) const COMPOSER_MAX_HEIGHT_RATIO: f32 = 0.5;
pub(crate) const COMPOSER_OUTER_VERTICAL_PADDING: f32 = 24.0;
pub(crate) const COMPOSER_INPUT_VERTICAL_CHROME: f32 = 14.0;
pub(crate) const COMPOSER_INPUT_PAINT_SLACK: f32 = 4.0;
pub(crate) const COMPOSER_INPUT_HORIZONTAL_CHROME: f32 = 24.0;
pub(crate) const COMPOSER_OUTER_HORIZONTAL_PADDING: f32 = 32.0;
pub(crate) const GRAPH_OVERLAY_MIN_HEIGHT: f32 = 160.0;
pub(crate) const DEFAULT_GRAPH_OVERLAY_HEIGHT_RATIO: f32 = 0.5;
pub(crate) const WORKSPACE_PICKER_MARGIN: f32 = 12.0;
pub(crate) const WORKSPACE_PICKER_OFFSET_Y: f32 = 8.0;
pub(crate) const WORKSPACE_PICKER_VIEWPORT_WIDTH_RATIO: f32 = 0.94;
pub(crate) const WORKSPACE_PICKER_PREFERRED_WIDTH: f32 = 840.0;
pub(crate) const WORKSPACE_PICKER_MIN_WIDTH: f32 = 620.0;
pub(crate) const WORKSPACE_PICKER_WORKSPACES_COLUMN_PREFERRED_WIDTH: f32 = 420.0;
pub(crate) const WORKSPACE_PICKER_MEMBERS_COLUMN_PREFERRED_WIDTH: f32 = 419.0;
pub(crate) const WORKSPACE_PICKER_COLUMN_DIVIDER_WIDTH: f32 = 1.0;
pub(crate) const WORKSPACE_PICKER_HEADER_HEIGHT: f32 = 64.0;
pub(crate) const WORKSPACE_PICKER_FILTER_HEIGHT: f32 = 62.0;
pub(crate) const WORKSPACE_PICKER_MEMBERS_CONTROL_HEIGHT: f32 = WORKSPACE_PICKER_FILTER_HEIGHT;
pub(crate) const WORKSPACE_PICKER_MEMBERS_CONTROL_PADDING_X: f32 = 16.0;
pub(crate) const WORKSPACE_PICKER_MEMBERS_CONTROL_PADDING_Y: f32 = 12.0;
pub(crate) const WORKSPACE_PICKER_CREATE_ROW_HEIGHT: f32 = 56.0;
pub(crate) const WORKSPACE_PICKER_ROW_HEIGHT: f32 = 104.0;
pub(crate) const WORKSPACE_PICKER_MEMBERS_ATTACH_ROW_HEIGHT: f32 =
    WORKSPACE_PICKER_CREATE_ROW_HEIGHT;
pub(crate) const WORKSPACE_PICKER_MEMBERS_ROW_HEIGHT: f32 = WORKSPACE_PICKER_ROW_HEIGHT;
pub(crate) const WORKSPACE_PICKER_CREATE_ADD_PLUS_SLOT_WIDTH: f32 = 18.0;
pub(crate) const WORKSPACE_PICKER_CREATE_ADD_PLUS_FONT_SIZE: f32 = 18.0;
pub(crate) const WORKSPACE_PICKER_CREATE_ADD_PLUS_GLYPH_Y_OFFSET: f32 = -1.5;
pub(crate) const WORKSPACE_PICKER_RUNTIME_SELECTOR_DETAIL_LINE_HEIGHT: f32 = 14.0;
pub(crate) const WORKSPACE_PICKER_RUNTIME_SELECTOR_ARROW_SLOT_WIDTH: f32 = 20.0;
pub(crate) const WORKSPACE_PICKER_RUNTIME_SELECTOR_ARROW_FONT_SIZE: f32 = 14.0;
pub(crate) const WORKSPACE_PICKER_RUNTIME_SELECTOR_TRIGGER_HEIGHT: f32 = BUTTON_BORDER_WIDTH * 2.0
    + BUTTON_VERTICAL_PADDING * 2.0
    + BUTTON_LABEL_LINE_HEIGHT
    + WORKSPACE_PICKER_RUNTIME_SELECTOR_DETAIL_LINE_HEIGHT;
pub(crate) const WORKSPACE_PICKER_RUNTIME_SELECTOR_DROPDOWN_RELATIVE_TOP: f32 =
    WORKSPACE_PICKER_RUNTIME_SELECTOR_TRIGGER_HEIGHT - BUTTON_BORDER_WIDTH;
pub(crate) const WORKSPACE_PICKER_RUNTIME_SELECTOR_DROPDOWN_COLUMN_TOP: f32 =
    WORKSPACE_PICKER_MEMBERS_CONTROL_PADDING_Y
        + WORKSPACE_PICKER_RUNTIME_SELECTOR_DROPDOWN_RELATIVE_TOP;
pub(crate) const WORKSPACE_PICKER_RUNTIME_DROPDOWN_ROW_HEIGHT: f32 = 44.0;
pub(crate) const WORKSPACE_PICKER_RUNTIME_DROPDOWN_MAX_VISIBLE_ROWS: usize = 6;
pub(crate) const WORKSPACE_PICKER_MAX_HEIGHT_RATIO: f32 = 0.72;

pub(crate) const fn button_padding_from_label_cap_height(label_cap_height: f32) -> f32 {
    label_cap_height / 3.0
}

pub(crate) const fn button_required_outer_height() -> f32 {
    BUTTON_BORDER_WIDTH * 2.0 + BUTTON_VERTICAL_PADDING * 2.0 + BUTTON_LABEL_LINE_HEIGHT
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SplitLayout {
    pub(crate) left_width: Pixels,
    pub(crate) right_width: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ToolActivityRowWindow {
    pub(crate) range: Range<usize>,
    pub(crate) top_spacer_height: Pixels,
    pub(crate) bottom_spacer_height: Pixels,
    pub(crate) content_height: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChecklistSidebarRowWindow {
    pub(crate) range: Range<usize>,
    pub(crate) top_spacer_height: Pixels,
    pub(crate) bottom_spacer_height: Pixels,
    pub(crate) content_height: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThreadSelectorRowWindow {
    pub(crate) range: Range<usize>,
    pub(crate) top_spacer_height: Pixels,
    pub(crate) bottom_spacer_height: Pixels,
    pub(crate) content_height: Pixels,
}

#[derive(Clone, Debug, PartialEq)]
struct FixedRowWindow {
    range: Range<usize>,
    top_spacer_height: Pixels,
    bottom_spacer_height: Pixels,
    content_height: Pixels,
}

pub(crate) fn split_layout(
    total_width: Pixels,
    desired_sidebar_ratio: f32,
    sidebar_visible: bool,
) -> SplitLayout {
    if !sidebar_visible {
        return SplitLayout {
            left_width: total_width.max(Pixels::ZERO),
            right_width: Pixels::ZERO,
        };
    }

    let divider_width = px(PANEL_DIVIDER_WIDTH);
    let available_width = (total_width - divider_width).max(Pixels::ZERO);
    let sidebar_ratio = clamped_checklist_sidebar_ratio(available_width, desired_sidebar_ratio);
    let right_width = available_width * sidebar_ratio;

    SplitLayout {
        left_width: available_width - right_width,
        right_width,
    }
}

pub(crate) fn clamped_checklist_sidebar_ratio(
    available_width: Pixels,
    desired_sidebar_ratio: f32,
) -> f32 {
    let desired_sidebar_ratio = desired_sidebar_ratio.clamp(0.0, 1.0);
    let min_panel_width = px(PANEL_MIN_WIDTH);

    if available_width <= min_panel_width * 2.0 {
        return 0.5;
    }

    let min_secondary_ratio = min_panel_width / available_width;
    desired_sidebar_ratio.clamp(min_secondary_ratio, 1.0 - min_secondary_ratio)
}

pub(crate) fn clamp_composer_height(
    main_region_height: Pixels,
    os_window_height: Pixels,
    desired_composer_height: Pixels,
) -> Pixels {
    let min_height = px(COMPOSER_MIN_HEIGHT);
    let transcript_preserving_max =
        if main_region_height <= px(MAIN_REGION_MIN_HEIGHT + COMPOSER_MIN_HEIGHT) {
            min_height
        } else {
            main_region_height - px(MAIN_REGION_MIN_HEIGHT)
        };
    let window_cap = os_window_height * COMPOSER_MAX_HEIGHT_RATIO;
    let max_height = transcript_preserving_max.min(window_cap).max(min_height);

    desired_composer_height.clamp(min_height, max_height)
}

pub(crate) fn composer_input_text_height(line_height: Pixels, visual_line_count: usize) -> Pixels {
    line_height * visual_line_count.max(1) as f32
}

pub(crate) fn composer_input_field_height(line_height: Pixels, visual_line_count: usize) -> Pixels {
    px(COMPOSER_INPUT_VERTICAL_CHROME + COMPOSER_INPUT_PAINT_SLACK)
        + composer_input_text_height(line_height, visual_line_count)
}

pub(crate) fn composer_input_content_height(
    line_height: Pixels,
    visual_line_count: usize,
) -> Pixels {
    composer_input_text_height(line_height, visual_line_count)
}

pub(crate) fn composer_input_scroll_content_height(
    line_height: Pixels,
    visual_line_count: usize,
    visible_text_height: Pixels,
) -> Pixels {
    composer_input_content_height(line_height, visual_line_count).max(visible_text_height)
}

pub(crate) fn composer_input_centered_text_top_padding(
    line_height: Pixels,
    visual_line_count: usize,
    visible_text_height: Pixels,
) -> Pixels {
    (composer_input_scroll_content_height(line_height, visual_line_count, visible_text_height)
        - composer_input_content_height(line_height, visual_line_count))
        / 2.0
}

pub(crate) fn tool_activity_panel_height_bounds(
    main_region_height: Pixels,
    composer_height: Pixels,
) -> (Pixels, Pixels) {
    let available_height =
        (main_region_height - composer_height - px(MAIN_REGION_MIN_HEIGHT)).max(Pixels::ZERO);
    if available_height <= Pixels::ZERO {
        return (Pixels::ZERO, Pixels::ZERO);
    }

    let min_height = px(TOOL_ACTIVITY_MIN_PANEL_HEIGHT).min(available_height);
    (min_height, available_height)
}

pub(crate) fn tool_activity_panel_height(
    main_region_height: Pixels,
    composer_height: Pixels,
    desired_panel_height: Pixels,
) -> Pixels {
    let (min_height, max_height) =
        tool_activity_panel_height_bounds(main_region_height, composer_height);
    if max_height <= Pixels::ZERO {
        return Pixels::ZERO;
    }

    desired_panel_height.clamp(min_height, max_height)
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn tool_activity_content_height(row_count: usize) -> Pixels {
    fixed_row_content_height(row_count, px(TOOL_ACTIVITY_ROW_HEIGHT), px(0.0))
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn checklist_sidebar_content_height(row_count: usize) -> Pixels {
    fixed_row_content_height(row_count, px(CHECKLIST_SIDEBAR_ROW_HEIGHT), px(0.0))
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn thread_selector_content_height(row_count: usize) -> Pixels {
    fixed_row_content_height(
        row_count,
        px(THREAD_SELECTOR_ROW_HEIGHT),
        px(THREAD_SELECTOR_ROW_GAP),
    )
}

pub(crate) fn tool_activity_row_window(
    row_count: usize,
    viewport_height: Pixels,
    scroll_offset: Pixels,
    overscan_rows: usize,
) -> ToolActivityRowWindow {
    let window = fixed_row_window(
        row_count,
        viewport_height,
        scroll_offset,
        overscan_rows,
        px(TOOL_ACTIVITY_ROW_HEIGHT),
        px(0.0),
    );

    ToolActivityRowWindow {
        range: window.range,
        top_spacer_height: window.top_spacer_height,
        bottom_spacer_height: window.bottom_spacer_height,
        content_height: window.content_height,
    }
}

pub(crate) fn checklist_sidebar_row_window(
    row_count: usize,
    viewport_height: Pixels,
    scroll_offset: Pixels,
    overscan_rows: usize,
) -> ChecklistSidebarRowWindow {
    let window = fixed_row_window(
        row_count,
        viewport_height,
        scroll_offset,
        overscan_rows,
        px(CHECKLIST_SIDEBAR_ROW_HEIGHT),
        px(0.0),
    );

    ChecklistSidebarRowWindow {
        range: window.range,
        top_spacer_height: window.top_spacer_height,
        bottom_spacer_height: window.bottom_spacer_height,
        content_height: window.content_height,
    }
}

pub(crate) fn thread_selector_row_window(
    row_count: usize,
    viewport_height: Pixels,
    scroll_offset: Pixels,
    overscan_rows: usize,
) -> ThreadSelectorRowWindow {
    let window = fixed_row_window(
        row_count,
        viewport_height,
        scroll_offset,
        overscan_rows,
        px(THREAD_SELECTOR_ROW_HEIGHT),
        px(THREAD_SELECTOR_ROW_GAP),
    );

    ThreadSelectorRowWindow {
        range: window.range,
        top_spacer_height: window.top_spacer_height,
        bottom_spacer_height: window.bottom_spacer_height,
        content_height: window.content_height,
    }
}

fn fixed_row_content_height(row_count: usize, row_height: Pixels, row_gap: Pixels) -> Pixels {
    if row_count == 0 {
        return Pixels::ZERO;
    }

    row_height * row_count as f32 + row_gap * row_count.saturating_sub(1) as f32
}

fn fixed_row_window(
    row_count: usize,
    viewport_height: Pixels,
    scroll_offset: Pixels,
    overscan_rows: usize,
    row_height: Pixels,
    row_gap: Pixels,
) -> FixedRowWindow {
    let content_height = fixed_row_content_height(row_count, row_height, row_gap);
    if row_count == 0 || viewport_height <= Pixels::ZERO {
        return FixedRowWindow {
            range: 0..0,
            top_spacer_height: Pixels::ZERO,
            bottom_spacer_height: content_height,
            content_height,
        };
    }

    let row_pitch = row_height + row_gap;
    let max_scroll_offset = (content_height - viewport_height).max(Pixels::ZERO);
    let scroll_offset = scroll_offset.clamp(Pixels::ZERO, max_scroll_offset);
    let first_visible_row = (f32::from(scroll_offset) / f32::from(row_pitch)).floor() as usize;
    let visible_end_row =
        (f32::from(scroll_offset + viewport_height) / f32::from(row_pitch)).ceil() as usize;

    let start = first_visible_row
        .min(row_count)
        .saturating_sub(overscan_rows);
    let end = visible_end_row
        .saturating_add(overscan_rows)
        .min(row_count)
        .max(start);
    let rendered_row_count = end.saturating_sub(start);
    let rendered_gap_count = if rendered_row_count == 0 {
        0
    } else if end < row_count {
        rendered_row_count
    } else {
        rendered_row_count.saturating_sub(1)
    };
    let rendered_height =
        row_height * rendered_row_count as f32 + row_gap * rendered_gap_count as f32;
    let top_spacer_height = row_pitch * start as f32;
    let bottom_spacer_height =
        (content_height - top_spacer_height - rendered_height).max(Pixels::ZERO);

    FixedRowWindow {
        range: start..end,
        top_spacer_height,
        bottom_spacer_height,
        content_height,
    }
}

pub(crate) fn composer_height_for_visual_lines(
    main_region_height: Pixels,
    os_window_height: Pixels,
    line_height: Pixels,
    visual_line_count: usize,
) -> Pixels {
    clamp_composer_height(
        main_region_height,
        os_window_height,
        px(COMPOSER_OUTER_VERTICAL_PADDING)
            + composer_input_field_height(line_height, visual_line_count),
    )
}

pub(crate) fn composer_text_width(conversation_column_width: Pixels) -> Pixels {
    (conversation_column_width
        - px(COMPOSER_OUTER_HORIZONTAL_PADDING + COMPOSER_INPUT_HORIZONTAL_CHROME))
    .max(px(1.0))
}

pub(crate) fn default_graph_overlay_height(available_height: Pixels) -> Pixels {
    clamp_graph_overlay_height(
        available_height,
        available_height * DEFAULT_GRAPH_OVERLAY_HEIGHT_RATIO,
    )
}

pub(crate) fn clamp_graph_overlay_height(
    available_height: Pixels,
    desired_graph_overlay_height: Pixels,
) -> Pixels {
    let max_height = available_height.max(Pixels::ZERO);
    let min_height = px(GRAPH_OVERLAY_MIN_HEIGHT).min(max_height);

    if max_height <= Pixels::ZERO {
        return Pixels::ZERO;
    }

    desired_graph_overlay_height.clamp(min_height, max_height)
}
