use super::super::code_viewport::{CodeScroll, LINE_HEIGHT, OVERSCAN, VIEWPORT_HEIGHT};

#[test]
fn code_realization_stays_bounded_at_late_offsets_and_preserves_fractional_anchor() {
    let mut scroll = CodeScroll {
        lines: 10_000_000,
        ..Default::default()
    };
    scroll.set_offset(9_000_000.0 * LINE_HEIGHT);
    scroll.scroll_by(8.0);
    let range = scroll.range();
    assert_eq!(range.start, scroll.top_line - OVERSCAN);
    assert_eq!(range.end - range.start, 16);
    assert_eq!(scroll.remainder, 8.0);
    assert!(range.end <= scroll.lines);
}

#[test]
fn code_scroll_clamps_edges_and_rejects_nonfinite_gestures() {
    let mut scroll = CodeScroll {
        lines: 2000,
        ..Default::default()
    };
    scroll.set_offset(f32::MAX);
    assert_eq!(scroll.offset(), 2000.0 * LINE_HEIGHT - VIEWPORT_HEIGHT);
    assert_eq!(scroll.range().end, 2000);
    let before = scroll.offset();
    scroll.set_offset(f32::NAN);
    scroll.set_offset(f32::INFINITY);
    assert_eq!(scroll.offset(), before);
    scroll.set_offset(-50.0);
    assert_eq!(scroll.top_line, 0);
    assert_eq!(scroll.remainder, 0.0);
    scroll.lines = 3;
    scroll.set_offset(100.0);
    assert_eq!(scroll.offset(), 0.0);
    assert_eq!(scroll.range().end, 3);
}

#[test]
fn code_horizontal_scroll_rejects_nonfinite_deltas_and_arithmetic_overflow() {
    let mut scroll = CodeScroll::default();
    scroll.scroll_horizontal_by(45.5);
    for delta in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        scroll.scroll_horizontal_by(delta);
        assert_eq!(scroll.horizontal, 45.5);
    }
    scroll.scroll_horizontal_by(-100.0);
    assert_eq!(scroll.horizontal, 0.0);
    scroll.scroll_horizontal_by(f32::MAX);
    scroll.scroll_horizontal_by(f32::MAX);
    assert_eq!(scroll.horizontal, f32::MAX);
    assert_eq!(scroll.top_line, 0);
    assert_eq!(scroll.remainder, 0.0);
}

#[test]
fn code_font_metric_changes_preserve_visible_line_offset_and_clamp_new_extent() {
    let mut scroll = CodeScroll {
        lines: 2000,
        ..Default::default()
    };
    scroll.set_offset(100.0 * LINE_HEIGHT + 7.5);
    scroll.horizontal = 45.0;
    scroll.set_line_height(50.0);
    assert_eq!(scroll.top_line, 100);
    assert_eq!(scroll.remainder, 7.5);
    assert_eq!(scroll.horizontal, 45.0);
    assert_eq!(scroll.range().end - scroll.range().start, 16);
    assert_eq!(scroll.overflow(), 2000.0 * 50.0 - VIEWPORT_HEIGHT);
    scroll.set_line_height(LINE_HEIGHT);
    assert_eq!(scroll.top_line, 100);
    assert_eq!(scroll.remainder, 7.5);
    scroll.set_offset(f32::MAX);
    scroll.set_line_height(50.0);
    let end = scroll.overflow();
    scroll.set_offset(end);
    scroll.set_line_height(LINE_HEIGHT);
    assert_eq!(scroll.offset(), scroll.overflow());
    scroll.lines = 1;
    scroll.set_line_height(40.0);
    assert_eq!(scroll.offset(), 0.0);
}

#[test]
fn code_source_extent_changes_preserve_late_line_and_fractional_offset_when_they_fit() {
    let mut scroll = CodeScroll {
        lines: 10_000_000,
        ..Default::default()
    };
    scroll.set_offset(9_000_000.0 * LINE_HEIGHT);
    scroll.scroll_by(7.5);
    scroll.horizontal = 45.0;
    scroll.set_lines(20_000_000);
    assert_eq!(scroll.top_line, 9_000_000);
    assert_eq!(scroll.remainder, 7.5);
    scroll.set_lines(9_000_100);
    assert_eq!(scroll.top_line, 9_000_000);
    assert_eq!(scroll.remainder, 7.5);
    assert_eq!(scroll.horizontal, 45.0);
    assert_eq!(scroll.range().end - scroll.range().start, 16);
}

#[test]
fn code_source_shrink_clamps_to_the_new_end_before_requesting_lines() {
    let mut scroll = CodeScroll {
        lines: 2000,
        ..Default::default()
    };
    scroll.set_offset(f32::MAX);
    scroll.set_lines(25);
    assert_eq!(scroll.offset(), 25.0 * LINE_HEIGHT - VIEWPORT_HEIGHT);
    assert_eq!(scroll.top_line, 13);
    assert_eq!(scroll.remainder, 0.0);
    assert_eq!(scroll.range().start, 11);
    assert_eq!(scroll.range().end, 25);
    scroll.set_lines(3);
    assert_eq!(scroll.top_line, 0);
    assert_eq!(scroll.remainder, 0.0);
    assert_eq!(scroll.overflow(), 0.0);
    assert_eq!(scroll.range().start, 0);
    assert_eq!(scroll.range().end, 3);
}

#[test]
fn code_empty_source_resets_both_axes_and_growth_stays_at_the_origin() {
    let mut scroll = CodeScroll {
        lines: 2000,
        ..Default::default()
    };
    scroll.set_offset(1007.5);
    scroll.horizontal = 45.0;
    scroll.set_lines(0);
    assert_eq!(scroll.top_line, 0);
    assert_eq!(scroll.remainder, 0.0);
    assert_eq!(scroll.horizontal, 0.0);
    assert_eq!(scroll.overflow(), 0.0);
    scroll.set_lines(2000);
    assert_eq!(scroll.offset(), 0.0);
    assert_eq!(scroll.horizontal, 0.0);
}
