#[allow(dead_code)]
#[path = "../src/shell/layout.rs"]
mod layout;

use gpui::px;

#[test]
fn split_layout_clamps_checklist_sidebar_to_minimum_width() {
    let split = layout::split_layout(px(640.0), 0.05, true);

    assert!(split.left_width >= px(layout::PANEL_MIN_WIDTH));
    assert!(split.right_width >= px(layout::PANEL_MIN_WIDTH));
    assert_eq!(
        split.left_width + split.right_width + px(layout::PANEL_DIVIDER_WIDTH),
        px(640.0)
    );
}

#[test]
fn split_layout_uses_full_width_when_checklist_sidebar_is_hidden() {
    let split = layout::split_layout(px(640.0), 0.34, false);

    assert_eq!(split.left_width, px(640.0));
    assert_eq!(split.right_width, px(0.0));
}

#[test]
fn composer_height_stops_at_remaining_available_height() {
    let composer_height = layout::clamp_composer_height(px(240.0), px(760.0), px(220.0));

    assert_eq!(composer_height, px(120.0));
}

#[test]
fn composer_height_keeps_the_minimum_when_space_is_tight() {
    let composer_height = layout::clamp_composer_height(px(150.0), px(760.0), px(20.0));

    assert_eq!(composer_height, px(layout::COMPOSER_MIN_HEIGHT));
}

#[test]
fn composer_height_stops_at_half_the_os_window_height() {
    let composer_height = layout::clamp_composer_height(px(700.0), px(500.0), px(420.0));

    assert_eq!(composer_height, px(250.0));
}

#[test]
fn composer_height_grows_from_wrapped_visual_lines() {
    let composer_height =
        layout::composer_height_for_visual_lines(px(700.0), px(760.0), px(20.0), 4);

    assert_eq!(
        composer_height,
        px(layout::COMPOSER_OUTER_VERTICAL_PADDING
            + layout::COMPOSER_INPUT_VERTICAL_CHROME
            + layout::COMPOSER_INPUT_PAINT_SLACK
            + 80.0)
    );
}

#[test]
fn composer_input_field_height_wraps_text_height_with_fixed_frame_chrome() {
    assert_eq!(layout::composer_input_content_height(px(20.0), 2), px(40.0));
    assert_eq!(
        layout::composer_input_field_height(px(20.0), 2),
        px(layout::COMPOSER_INPUT_VERTICAL_CHROME + layout::COMPOSER_INPUT_PAINT_SLACK + 40.0)
    );
}

#[test]
fn composer_input_scroll_content_height_expands_to_visible_text_plane_for_centering() {
    assert_eq!(
        layout::composer_input_scroll_content_height(px(20.0), 1, px(36.0)),
        px(36.0)
    );
    assert_eq!(
        layout::composer_input_scroll_content_height(px(20.0), 5, px(36.0)),
        px(100.0)
    );
}

#[test]
fn composer_input_centered_text_top_padding_centers_short_content_only() {
    assert_eq!(
        layout::composer_input_centered_text_top_padding(px(20.0), 1, px(36.0)),
        px(8.0)
    );
    assert_eq!(
        layout::composer_input_centered_text_top_padding(px(20.0), 2, px(50.0)),
        px(5.0)
    );
    assert_eq!(
        layout::composer_input_centered_text_top_padding(px(20.0), 5, px(36.0)),
        px(0.0)
    );
}

#[test]
fn composer_text_width_reclaims_former_action_space() {
    let text_width = layout::composer_text_width(px(420.0));

    assert_eq!(
        text_width,
        px(420.0
            - layout::COMPOSER_OUTER_HORIZONTAL_PADDING
            - layout::COMPOSER_INPUT_HORIZONTAL_CHROME)
    );
}

#[test]
fn status_line_height_is_fixed_chrome_below_composer() {
    assert_eq!(layout::STATUS_LINE_HEIGHT, 28.0);
}

#[test]
fn thread_strip_height_is_fixed_top_chrome() {
    assert_eq!(layout::THREAD_STRIP_HEIGHT, 40.0);
    assert_eq!(
        layout::TOOLBAR_STRIP_HEIGHT + layout::THREAD_STRIP_HEIGHT,
        92.0
    );
    assert!(layout::BUTTON_OUTER_HEIGHT <= layout::THREAD_STRIP_HEIGHT);
}

#[test]
fn button_geometry_fits_inside_standard_ui_line_height() {
    assert_eq!(
        layout::BUTTON_OUTER_HEIGHT,
        layout::STANDARD_UI_TEXT_LINE_HEIGHT
    );
    assert_eq!(layout::BUTTON_ICON_OUTER_WIDTH, layout::BUTTON_OUTER_HEIGHT);
    assert!(layout::BUTTON_OUTER_HEIGHT <= layout::STANDARD_UI_TEXT_LINE_HEIGHT);
    assert!(layout::button_required_outer_height() <= layout::BUTTON_OUTER_HEIGHT);
}

#[test]
fn button_padding_is_derived_from_label_cap_height_estimate() {
    assert_eq!(
        layout::BUTTON_VERTICAL_PADDING,
        layout::button_padding_from_label_cap_height(layout::BUTTON_LABEL_CAP_HEIGHT_ESTIMATE)
    );
    assert_eq!(
        layout::BUTTON_HORIZONTAL_PADDING,
        layout::BUTTON_VERTICAL_PADDING
    );
}

#[test]
fn tool_activity_panel_height_uses_persisted_size_without_requiring_rows() {
    assert_eq!(
        layout::tool_activity_panel_height(px(420.0), px(96.0), px(112.0)),
        px(112.0)
    );
    assert_eq!(layout::tool_activity_content_height(0), px(0.0));
}

#[test]
fn tool_activity_panel_height_preserves_transcript_space() {
    assert_eq!(
        layout::tool_activity_panel_height(px(300.0), px(74.0), px(220.0)),
        px(106.0)
    );
    assert_eq!(
        layout::tool_activity_panel_height(px(190.0), px(74.0), px(112.0)),
        px(0.0)
    );
}

#[test]
fn tool_activity_panel_height_keeps_minimum_when_space_allows() {
    assert_eq!(
        layout::tool_activity_panel_height(px(300.0), px(74.0), px(1.0)),
        px(layout::TOOL_ACTIVITY_MIN_PANEL_HEIGHT)
    );
    assert_eq!(
        layout::tool_activity_content_height(7),
        px(layout::TOOL_ACTIVITY_ROW_HEIGHT * 7.0)
    );
}

#[test]
fn tool_activity_row_window_renders_short_lists_without_spacers() {
    let window = layout::tool_activity_row_window(2, px(112.0), px(0.0), 3);

    assert_eq!(window.range, 0..2);
    assert_eq!(window.top_spacer_height, px(0.0));
    assert_eq!(window.bottom_spacer_height, px(0.0));
    assert_eq!(
        window.content_height,
        px(layout::TOOL_ACTIVITY_ROW_HEIGHT * 2.0)
    );
}

#[test]
fn tool_activity_row_window_adds_overscan_and_preserves_content_height() {
    let window = layout::tool_activity_row_window(
        100,
        px(layout::TOOL_ACTIVITY_ROW_HEIGHT * 3.0),
        px(layout::TOOL_ACTIVITY_ROW_HEIGHT * 20.0),
        3,
    );

    assert_eq!(window.range, 17..26);
    assert_eq!(
        window.top_spacer_height,
        px(layout::TOOL_ACTIVITY_ROW_HEIGHT * 17.0)
    );
    assert_eq!(
        window.bottom_spacer_height,
        px(layout::TOOL_ACTIVITY_ROW_HEIGHT * 74.0)
    );
    assert_eq!(
        window.content_height,
        window.top_spacer_height
            + px(layout::TOOL_ACTIVITY_ROW_HEIGHT * window.range.len() as f32)
            + window.bottom_spacer_height
    );
}

#[test]
fn tool_activity_row_window_clamps_scroll_offsets_to_valid_content() {
    let tail_window = layout::tool_activity_row_window(10, px(84.0), px(10_000.0), 1);
    assert_eq!(tail_window.range, 6..10);
    assert_eq!(
        tail_window.top_spacer_height,
        px(layout::TOOL_ACTIVITY_ROW_HEIGHT * 6.0)
    );
    assert_eq!(tail_window.bottom_spacer_height, px(0.0));

    let top_window = layout::tool_activity_row_window(10, px(84.0), px(-10.0), 1);
    assert_eq!(top_window.range, 0..4);
    assert_eq!(top_window.top_spacer_height, px(0.0));
}

#[test]
fn tool_activity_row_window_includes_partially_visible_rows() {
    let window = layout::tool_activity_row_window(
        10,
        px(layout::TOOL_ACTIVITY_ROW_HEIGHT),
        px(layout::TOOL_ACTIVITY_ROW_HEIGHT / 2.0),
        0,
    );

    assert_eq!(window.range, 0..2);
}

#[test]
fn tool_activity_row_window_keeps_scroll_geometry_for_empty_viewports() {
    let window = layout::tool_activity_row_window(10, px(0.0), px(0.0), 3);

    assert_eq!(window.range, 0..0);
    assert_eq!(window.top_spacer_height, px(0.0));
    assert_eq!(
        window.bottom_spacer_height,
        px(layout::TOOL_ACTIVITY_ROW_HEIGHT * 10.0)
    );
    assert_eq!(
        window.content_height,
        px(layout::TOOL_ACTIVITY_ROW_HEIGHT * 10.0)
    );
}

#[test]
fn checklist_sidebar_row_window_adds_overscan_and_preserves_content_height() {
    let window = layout::checklist_sidebar_row_window(
        80,
        px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT * 4.0),
        px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT * 12.0),
        2,
    );

    assert_eq!(window.range, 10..18);
    assert_eq!(
        window.top_spacer_height,
        px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT * 10.0)
    );
    assert_eq!(
        window.bottom_spacer_height,
        px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT * 62.0)
    );
    assert_eq!(
        window.content_height,
        window.top_spacer_height
            + px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT * window.range.len() as f32)
            + window.bottom_spacer_height
    );
}

#[test]
fn checklist_sidebar_row_window_clamps_scroll_offsets_to_valid_content() {
    let tail_window = layout::checklist_sidebar_row_window(8, px(112.0), px(10_000.0), 1);
    assert_eq!(tail_window.range, 5..8);
    assert_eq!(
        tail_window.top_spacer_height,
        px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT * 5.0)
    );
    assert_eq!(tail_window.bottom_spacer_height, px(0.0));

    let top_window = layout::checklist_sidebar_row_window(8, px(112.0), px(-10.0), 1);
    assert_eq!(top_window.range, 0..3);
    assert_eq!(top_window.top_spacer_height, px(0.0));
}

#[test]
fn checklist_sidebar_row_window_keeps_scroll_geometry_for_empty_viewports() {
    let window = layout::checklist_sidebar_row_window(8, px(0.0), px(0.0), 3);

    assert_eq!(window.range, 0..0);
    assert_eq!(window.top_spacer_height, px(0.0));
    assert_eq!(
        window.bottom_spacer_height,
        px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT * 8.0)
    );
    assert_eq!(
        window.content_height,
        px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT * 8.0)
    );
}

#[test]
fn thread_selector_row_window_adds_overscan_and_preserves_content_height_with_gaps() {
    let window = layout::thread_selector_row_window(
        100,
        px((layout::THREAD_SELECTOR_ROW_HEIGHT + layout::THREAD_SELECTOR_ROW_GAP) * 3.0),
        px((layout::THREAD_SELECTOR_ROW_HEIGHT + layout::THREAD_SELECTOR_ROW_GAP) * 20.0),
        3,
    );

    assert_eq!(window.range, 17..26);
    assert_eq!(
        window.top_spacer_height,
        px((layout::THREAD_SELECTOR_ROW_HEIGHT + layout::THREAD_SELECTOR_ROW_GAP) * 17.0)
    );
    assert_eq!(
        window.bottom_spacer_height,
        px(layout::THREAD_SELECTOR_ROW_HEIGHT * 74.0 + layout::THREAD_SELECTOR_ROW_GAP * 73.0)
    );
    assert_eq!(
        window.content_height,
        layout::thread_selector_content_height(100)
    );
}

#[test]
fn thread_selector_row_window_clamps_scroll_offsets_to_valid_content() {
    let tail_window = layout::thread_selector_row_window(10, px(100.0), px(10_000.0), 1);
    assert_eq!(tail_window.range, 6..10);
    assert_eq!(
        tail_window.top_spacer_height,
        px((layout::THREAD_SELECTOR_ROW_HEIGHT + layout::THREAD_SELECTOR_ROW_GAP) * 6.0)
    );
    assert_eq!(tail_window.bottom_spacer_height, px(0.0));

    let top_window = layout::thread_selector_row_window(10, px(100.0), px(-10.0), 1);
    assert_eq!(top_window.range, 0..3);
    assert_eq!(top_window.top_spacer_height, px(0.0));
}

#[test]
fn thread_selector_row_window_keeps_scroll_geometry_for_empty_viewports() {
    let window = layout::thread_selector_row_window(8, px(0.0), px(0.0), 3);

    assert_eq!(window.range, 0..0);
    assert_eq!(window.top_spacer_height, px(0.0));
    assert_eq!(
        window.bottom_spacer_height,
        layout::thread_selector_content_height(8)
    );
    assert_eq!(window.content_height, window.bottom_spacer_height);
}

#[test]
fn thread_selector_row_window_reaches_first_middle_and_final_rows() {
    let row_pitch = layout::THREAD_SELECTOR_ROW_HEIGHT + layout::THREAD_SELECTOR_ROW_GAP;
    let first = layout::thread_selector_row_window(300, px(row_pitch * 5.0), px(0.0), 4);
    assert!(first.range.contains(&0));

    let middle =
        layout::thread_selector_row_window(300, px(row_pitch * 5.0), px(row_pitch * 150.0), 4);
    assert!(middle.range.contains(&150));

    let final_window =
        layout::thread_selector_row_window(300, px(row_pitch * 5.0), px(100_000.0), 4);
    assert!(final_window.range.contains(&299));
    assert_eq!(final_window.bottom_spacer_height, px(0.0));
}

#[test]
fn graph_overlay_height_defaults_to_half_of_available_height() {
    let overlay_height = layout::default_graph_overlay_height(px(520.0));

    assert_eq!(overlay_height, px(260.0));
}

#[test]
fn graph_overlay_height_stays_within_available_bounds() {
    let overlay_height = layout::clamp_graph_overlay_height(px(220.0), px(480.0));

    assert_eq!(overlay_height, px(220.0));
}
