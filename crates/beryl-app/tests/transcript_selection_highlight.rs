#[path = "../src/shell/render/transcript/selection_highlight.rs"]
#[allow(dead_code)]
mod selection_highlight;

use gpui::{Bounds, point, px, size};
use selection_highlight::{
    selection_highlight_bounds_for_visual_ranges, selection_paint_masks_for_visual_ranges,
    visual_line_ranges_for_wrap_indices, wrapped_glyph_paint_origin,
};

#[test]
fn visual_line_ranges_follow_wrap_indices() {
    assert_eq!(
        visual_line_ranges_for_wrap_indices(30, [10, 20]),
        vec![0..10, 10..20, 20..30]
    );
}

#[test]
fn visual_line_ranges_clamp_invalid_wrap_indices() {
    assert_eq!(
        visual_line_ranges_for_wrap_indices(12, [5, 4, 20]),
        vec![0..5, 5..5, 5..12, 12..12]
    );
}

#[test]
fn selection_highlight_splits_across_wrapped_visual_lines() {
    let visual_ranges = vec![0..10, 10..20, 20..30];
    let bounds = selection_highlight_bounds_for_visual_ranges(
        point(px(100.0), px(50.0)),
        px(18.0),
        px(60.0),
        visual_ranges.as_slice(),
        4..24,
        |index, visual_index| {
            let visual_start = visual_ranges[visual_index].start;
            Some(point(
                px((index - visual_start) as f32 * 5.0),
                px(visual_index as f32 * 18.0),
            ))
        },
    );

    assert_eq!(
        bounds,
        vec![
            Bounds::new(point(px(120.0), px(50.0)), size(px(30.0), px(18.0))),
            Bounds::new(point(px(100.0), px(68.0)), size(px(50.0), px(18.0))),
            Bounds::new(point(px(100.0), px(86.0)), size(px(20.0), px(18.0))),
        ]
    );
}

#[test]
fn selection_highlight_keeps_single_visual_line_partial() {
    let visual_ranges = vec![0..10, 10..20];
    let bounds = selection_highlight_bounds_for_visual_ranges(
        point(px(20.0), px(30.0)),
        px(16.0),
        px(80.0),
        visual_ranges.as_slice(),
        2..7,
        |index, visual_index| {
            let visual_start = visual_ranges[visual_index].start;
            Some(point(
                px((index - visual_start) as f32 * 8.0),
                px(visual_index as f32 * 16.0),
            ))
        },
    );

    assert_eq!(
        bounds,
        vec![Bounds::new(
            point(px(36.0), px(30.0)),
            size(px(40.0), px(16.0)),
        )]
    );
}

#[test]
fn selection_highlight_bounds_follow_current_line_origin_after_reflow() {
    let visual_ranges = vec![0..12];
    let selection = 1..5;
    let position_for_index = |index, _visual_index| Some(point(px(index as f32 * 6.0), px(0.0)));
    let old_origin = point(px(40.0), px(120.0));
    let current_origin = point(px(40.0), px(88.0));

    let old_bounds = selection_highlight_bounds_for_visual_ranges(
        old_origin,
        px(18.0),
        px(120.0),
        visual_ranges.as_slice(),
        selection.clone(),
        position_for_index,
    );
    let current_bounds = selection_highlight_bounds_for_visual_ranges(
        current_origin,
        px(18.0),
        px(120.0),
        visual_ranges.as_slice(),
        selection,
        position_for_index,
    );

    assert_eq!(
        old_bounds,
        vec![Bounds::new(
            point(px(46.0), px(120.0)),
            size(px(24.0), px(18.0)),
        )]
    );
    assert_eq!(
        current_bounds,
        vec![Bounds::new(
            point(px(46.0), px(88.0)),
            size(px(24.0), px(18.0)),
        )]
    );
}

#[test]
fn selection_paint_masks_retain_soft_wrapped_visual_line_indices() {
    let visual_ranges = vec![0..10, 10..20, 20..30];
    let masks = selection_paint_masks_for_visual_ranges(
        point(px(100.0), px(50.0)),
        px(18.0),
        px(60.0),
        visual_ranges.as_slice(),
        4..24,
        |index, visual_index| {
            Some(point(
                px((index - visual_ranges[visual_index].start) as f32 * 5.0),
                px(visual_index as f32 * 18.0),
            ))
        },
    );

    assert_eq!(
        masks
            .iter()
            .map(|mask| mask.visual_line_index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(
        masks.iter().map(|mask| mask.bounds).collect::<Vec<_>>(),
        vec![
            Bounds::new(point(px(120.0), px(50.0)), size(px(30.0), px(18.0))),
            Bounds::new(point(px(100.0), px(68.0)), size(px(50.0), px(18.0))),
            Bounds::new(point(px(100.0), px(86.0)), size(px(20.0), px(18.0))),
        ]
    );
}

#[test]
fn selection_masks_keep_utf8_byte_endpoints_without_text_slicing() {
    let requested_indices = std::cell::RefCell::new(Vec::new());
    let masks = selection_paint_masks_for_visual_ranges(
        point(px(12.0), px(20.0)),
        px(16.0),
        px(100.0),
        &[0..8],
        1..5,
        |index, _| {
            requested_indices.borrow_mut().push(index);
            Some(point(px(index as f32 * 3.0), px(0.0)))
        },
    );

    assert_eq!(*requested_indices.borrow(), vec![1, 5]);
    assert_eq!(
        masks[0].bounds,
        Bounds::new(point(px(15.0), px(20.0)), size(px(12.0), px(16.0)))
    );
}

#[test]
fn wrapped_glyph_origin_reproduces_left_aligned_wrap_baseline() {
    let first_run_origin = wrapped_glyph_paint_origin(
        point(px(40.0), px(30.0)),
        px(20.0),
        px(12.0),
        px(4.0),
        px(8.0),
        px(0.0),
        0,
    );
    let wrapped_cross_run_origin = wrapped_glyph_paint_origin(
        point(px(40.0), px(30.0)),
        px(20.0),
        px(12.0),
        px(4.0),
        px(74.0),
        px(60.0),
        1,
    );

    assert_eq!(first_run_origin, point(px(48.0), px(44.0)));
    assert_eq!(wrapped_cross_run_origin, point(px(54.0), px(64.0)));
}
