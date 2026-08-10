#[path = "../src/shell/render/transcript/selection_highlight.rs"]
#[allow(dead_code)]
mod selection_highlight;

use gpui::{Bounds, point, px, size};
use selection_highlight::{
    SelectionGlyphPaintInput, SelectionGlyphPaintMode, SelectionOpacityRegion,
    WrappedLineSelectionPaintMask, retain_first_paint_error, selection_glyph_paint_plan,
    selection_opacity_for_bounds, selection_paint_masks_for_visual_ranges,
};

const TRANSCRIPT_SOURCE: &str = include_str!("../src/shell/render/transcript.rs");
const SELECTION_PAINT_SOURCE: &str =
    include_str!("../src/shell/render/transcript/selection_highlight.rs");
const INLINE_MARKDOWN_SOURCE: &str =
    include_str!("../src/shell/render/transcript/inline_markdown.rs");

#[derive(Clone, Debug, PartialEq, Eq)]
struct TestGlyphMetadata {
    font_id: u8,
    glyph_id: u8,
    weight: u16,
    decorated: bool,
}

fn glyph(
    run_index: usize,
    glyph_index: usize,
    text_index: usize,
    x: f32,
    mode: SelectionGlyphPaintMode,
    metadata: TestGlyphMetadata,
) -> SelectionGlyphPaintInput<TestGlyphMetadata> {
    SelectionGlyphPaintInput {
        run_index,
        glyph_index,
        text_index,
        x: px(x),
        mode,
        metadata,
    }
}

#[test]
fn paint_plan_preserves_cross_run_metadata_for_partial_utf8_selection() {
    let text = "ffi e\u{301} 😀";
    let combining_cluster_end = "ffi e\u{301}".len();
    let emoji_index = text.find('😀').expect("emoji should be present");
    let requested_indices = std::cell::RefCell::new(Vec::new());
    let masks = selection_paint_masks_for_visual_ranges(
        point(px(100.0), px(40.0)),
        px(20.0),
        px(80.0),
        &[0..text.len()],
        1..combining_cluster_end,
        |index, _| {
            requested_indices.borrow_mut().push(index);
            Some(point(if index == 1 { px(7.0) } else { px(25.0) }, px(0.0)))
        },
    );
    let source = vec![
        glyph(
            0,
            0,
            0,
            0.0,
            SelectionGlyphPaintMode::Monochrome,
            TestGlyphMetadata {
                font_id: 1,
                glyph_id: 10,
                weight: 400,
                decorated: false,
            },
        ),
        glyph(
            0,
            1,
            3,
            10.0,
            SelectionGlyphPaintMode::Monochrome,
            TestGlyphMetadata {
                font_id: 1,
                glyph_id: 11,
                weight: 400,
                decorated: false,
            },
        ),
        glyph(
            1,
            0,
            4,
            20.0,
            SelectionGlyphPaintMode::Monochrome,
            TestGlyphMetadata {
                font_id: 2,
                glyph_id: 20,
                weight: 700,
                decorated: true,
            },
        ),
        glyph(
            1,
            1,
            emoji_index,
            30.0,
            SelectionGlyphPaintMode::Emoji,
            TestGlyphMetadata {
                font_id: 2,
                glyph_id: 21,
                weight: 700,
                decorated: true,
            },
        ),
    ];
    let expected_metadata = source
        .iter()
        .map(|glyph| glyph.metadata.clone())
        .collect::<Vec<_>>();
    let plan = selection_glyph_paint_plan(
        point(px(100.0), px(40.0)),
        px(20.0),
        px(12.0),
        px(4.0),
        source,
        [],
        masks.as_slice(),
    );

    assert_eq!(*requested_indices.borrow(), vec![1, combining_cluster_end]);
    assert_eq!(
        plan.iter()
            .map(|glyph| glyph.text_index)
            .collect::<Vec<_>>(),
        vec![0, 3, 4, emoji_index]
    );
    assert_eq!(
        plan.iter()
            .map(|glyph| (glyph.run_index, glyph.glyph_index))
            .collect::<Vec<_>>(),
        vec![(0, 0), (0, 1), (1, 0), (1, 1)]
    );
    assert_eq!(
        plan.iter().map(|glyph| glyph.source_x).collect::<Vec<_>>(),
        vec![px(0.0), px(10.0), px(20.0), px(30.0)]
    );
    assert_eq!(
        plan.iter()
            .map(|glyph| glyph.metadata.clone())
            .collect::<Vec<_>>(),
        expected_metadata
    );
    assert_eq!(plan[3].mode, SelectionGlyphPaintMode::Emoji);
    assert!(plan.iter().all(|glyph| glyph.mask == masks[0]));
    assert!(plan[0].origin.x < masks[0].bounds.left());
    assert!(plan[3].origin.x > masks[0].bounds.right());
}

#[test]
fn paint_plan_tracks_cross_run_wrap_boundaries_and_multiple_masks() {
    let masks = vec![
        WrappedLineSelectionPaintMask {
            bounds: Bounds::new(point(px(40.0), px(30.0)), size(px(24.0), px(20.0))),
            visual_line_index: 0,
        },
        WrappedLineSelectionPaintMask {
            bounds: Bounds::new(point(px(40.0), px(50.0)), size(px(18.0), px(20.0))),
            visual_line_index: 1,
        },
        WrappedLineSelectionPaintMask {
            bounds: Bounds::new(point(px(40.0), px(70.0)), size(px(18.0), px(20.0))),
            visual_line_index: 2,
        },
    ];
    let plan = selection_glyph_paint_plan(
        point(px(40.0), px(30.0)),
        px(20.0),
        px(12.0),
        px(4.0),
        vec![
            glyph(
                0,
                0,
                0,
                0.0,
                SelectionGlyphPaintMode::Monochrome,
                TestGlyphMetadata {
                    font_id: 1,
                    glyph_id: 1,
                    weight: 400,
                    decorated: false,
                },
            ),
            glyph(
                0,
                1,
                2,
                12.0,
                SelectionGlyphPaintMode::Monochrome,
                TestGlyphMetadata {
                    font_id: 1,
                    glyph_id: 2,
                    weight: 400,
                    decorated: false,
                },
            ),
            glyph(
                1,
                0,
                4,
                24.0,
                SelectionGlyphPaintMode::Monochrome,
                TestGlyphMetadata {
                    font_id: 3,
                    glyph_id: 3,
                    weight: 600,
                    decorated: true,
                },
            ),
            glyph(
                1,
                1,
                6,
                36.0,
                SelectionGlyphPaintMode::Emoji,
                TestGlyphMetadata {
                    font_id: 3,
                    glyph_id: 4,
                    weight: 600,
                    decorated: true,
                },
            ),
            glyph(
                2,
                0,
                8,
                48.0,
                SelectionGlyphPaintMode::Monochrome,
                TestGlyphMetadata {
                    font_id: 4,
                    glyph_id: 5,
                    weight: 300,
                    decorated: false,
                },
            ),
        ],
        [(0, 1), (1, 0), (2, 0)],
        masks.as_slice(),
    );

    assert_eq!(plan.len(), 4);
    assert_eq!(plan[0].origin, point(px(40.0), px(44.0)));
    assert_eq!(plan[1].origin, point(px(40.0), px(64.0)));
    assert_eq!(plan[2].origin, point(px(40.0), px(84.0)));
    assert_eq!(plan[3].origin, point(px(52.0), px(84.0)));
    assert_eq!(plan[1].mask, masks[1]);
    assert_eq!(plan[2].mask, masks[2]);
    assert_eq!(plan[3].mode, SelectionGlyphPaintMode::Emoji);
    assert!(plan.iter().all(|glyph| glyph.metadata.font_id != 4));
}

#[test]
fn edit_mode_opacity_is_resolved_per_visible_block() {
    let regions = [
        SelectionOpacityRegion {
            bounds: Bounds::new(point(px(0.0), px(80.0)), size(px(300.0), px(60.0))),
            opacity: 0.48,
        },
        SelectionOpacityRegion {
            bounds: Bounds::new(point(px(0.0), px(180.0)), size(px(300.0), px(60.0))),
            opacity: 0.48,
        },
    ];
    let normal_line = Bounds::new(point(px(20.0), px(30.0)), size(px(180.0), px(20.0)));
    let dimmed_line = Bounds::new(point(px(20.0), px(100.0)), size(px(180.0), px(20.0)));
    let later_dimmed_line = Bounds::new(point(px(20.0), px(200.0)), size(px(180.0), px(20.0)));

    assert_eq!(selection_opacity_for_bounds(normal_line, &regions), 1.0);
    assert_eq!(selection_opacity_for_bounds(dimmed_line, &regions), 0.48);
    assert_eq!(
        selection_opacity_for_bounds(later_dimmed_line, &regions),
        0.48
    );
    assert!(TRANSCRIPT_SOURCE.contains("TRANSCRIPT_EDIT_DIMMED_OPACITY"));
    assert!(TRANSCRIPT_SOURCE.contains("register_text_opacity_region"));
    assert!(TRANSCRIPT_SOURCE.contains("selection_opacity_for_bounds"));
    let render_turn = rust_function_body(TRANSCRIPT_SOURCE, "fn render_turn");
    assert!(render_turn.contains("profiler.observe_turn_prepaint"));
    assert!(render_turn.contains("view.register_text_opacity_region"));
}

#[test]
fn selection_layer_opacity_is_owned_by_the_div_around_the_canvas() {
    let layer = rust_function_body(TRANSCRIPT_SOURCE, "fn render_selected_text_highlight_layer");
    let wrapper = layer
        .find("div()")
        .expect("selection layer should use an opacity-capable div");
    let opacity = layer
        .find(".opacity(opacity)")
        .expect("selection layer div should own the row opacity");
    let canvas = layer
        .find("canvas(")
        .expect("selection layer div should contain the paint canvas");

    assert!(wrapper < opacity && opacity < canvas);
    assert!(layer[..canvas].contains(".absolute()"));
    assert!(layer[..canvas].contains(".size_full()"));
    assert!(!layer[canvas..].contains(".opacity(opacity)"));
}

#[test]
fn selection_overlay_reads_committed_viewport_and_lines_together_at_paint_time() {
    let layer = rust_function_body(TRANSCRIPT_SOURCE, "fn render_selected_text_highlight_layer");
    let paint_time_read = layer
        .find("entity.update")
        .expect("selection canvas should read its committed frame at paint time");
    let committed_viewport = layer
        .find("view.visible_text_geometry_viewport_bounds?")
        .expect("selection canvas should read the committed current-frame viewport");
    let selected_lines = layer
        .find(".selected_text_paints()")
        .expect("selection canvas should read selected lines in the same entity update");
    let returned_frame = layer
        .rfind("Some((viewport_bounds, selected_lines))")
        .expect("paint-time read should return viewport and selected lines together");
    let outer_mask = layer
        .find("window.with_content_mask")
        .expect("selection painting should install an outer viewport mask");
    let fill = layer
        .find("window.paint_quad")
        .expect("selection overlay should paint its fill inside the viewport mask");
    let foreground = layer
        .find("paint_wrapped_line_selection_foreground")
        .expect("selection overlay should repaint glyphs inside the viewport mask");
    assert!(paint_time_read < committed_viewport);
    assert!(committed_viewport < selected_lines);
    assert!(selected_lines < returned_frame);
    assert!(returned_frame < outer_mask);
    assert!(layer.contains("bounds: viewport_bounds"));
    assert!(outer_mask < fill && outer_mask < foreground);

    let overlay_call = TRANSCRIPT_SOURCE
        .find("self.render_selected_text_highlights")
        .expect("transcript render should mount the selection overlay");
    let call_tail = &TRANSCRIPT_SOURCE[overlay_call..];
    let call_end = call_tail
        .find("theme.selection.foreground()")
        .expect("selection overlay call should consume the themed foreground");
    let call = &call_tail[..call_end];
    assert!(call.contains("entity.clone(),"));
    assert!(!call.contains("bounds,"));
    assert!(!call.contains("transcript_list_state.viewport_bounds()"));

    let overlay = rust_function_body(TRANSCRIPT_SOURCE, "fn render_selected_text_highlights");
    assert!(!overlay.contains("viewport_bounds"));

    let glyph_paint = rust_function_body(
        SELECTION_PAINT_SOURCE,
        "pub(crate) fn paint_wrapped_line_selection_foreground",
    );
    assert!(glyph_paint.contains("bounds: glyph.mask.bounds"));
}

#[test]
fn repaint_errors_retain_only_the_first_failure() {
    let mut first_error = None;
    retain_first_paint_error(&mut first_error, Err("first"));
    retain_first_paint_error(&mut first_error, Err("second"));
    retain_first_paint_error(&mut first_error, Ok::<_, &str>(()));

    assert_eq!(first_error, Some("first"));
    assert!(SELECTION_PAINT_SOURCE.contains("tracing::error!"));
}

#[test]
fn selection_overlay_consumes_both_active_theme_colors() {
    assert!(TRANSCRIPT_SOURCE.contains("theme.selection.text_background()"));
    assert!(TRANSCRIPT_SOURCE.contains("theme.selection.foreground()"));

    let overlay_body =
        rust_function_body(TRANSCRIPT_SOURCE, "fn render_selected_text_highlight_layer");
    let fill = overlay_body
        .find("window.paint_quad")
        .expect("selection overlay should paint the existing fill");
    let foreground = overlay_body
        .find("paint_wrapped_line_selection_foreground")
        .expect("selection overlay should repaint selected glyph foreground");
    assert!(fill < foreground, "highlight fill must be painted first");
}

#[test]
fn selection_overlay_reuses_visible_shaped_layouts_and_ranges() {
    let paints_body = rust_function_body(TRANSCRIPT_SOURCE, "fn selected_text_paints");
    let range_body = rust_function_body(TRANSCRIPT_SOURCE, "fn selected_text_paint_for_range");

    assert!(paints_body.contains("selected_line_ranges(&self.visible_text_frame)"));
    assert!(range_body.contains("self.visible_text_geometry.get(key)"));
    assert!(range_body.contains("geometry.layout.line_layout_for_index(display_start)"));
    assert!(range_body.contains("geometry.layout.line_height()"));
    assert!(range_body.contains("geometry.bounds.origin"));
    assert!(range_body.contains("geometry.bounds.size.width"));
    assert!(!paints_body.contains("text_selection.selected_text()"));
    assert!(!range_body.contains("shape_"));
}

#[test]
fn foreground_repaint_uses_shaped_glyphs_masks_and_polychrome_emoji() {
    let paint_body = rust_function_body(
        SELECTION_PAINT_SOURCE,
        "pub(crate) fn paint_wrapped_line_selection_foreground",
    );

    assert!(paint_body.contains("line.runs().iter()"));
    assert!(paint_body.contains("line.wrap_boundaries()"));
    assert!(paint_body.contains("selection_glyph_paint_plan"));
    assert!(paint_body.contains("window.with_content_mask"));
    assert!(paint_body.contains("bounds: glyph.mask.bounds"));
    assert!(paint_body.contains("window.paint_glyph"));
    assert!(paint_body.contains("window.paint_emoji"));
    assert!(paint_body.contains("line.font_size()"));
    assert!(paint_body.contains("retain_first_paint_error"));
    assert!(!paint_body.contains("shape_"));
    assert!(!paint_body.contains("TextRun"));
}

#[test]
fn selection_foreground_is_not_injected_into_markdown_text_runs() {
    assert!(!INLINE_MARKDOWN_SOURCE.contains("selection.foreground"));
    assert!(!INLINE_MARKDOWN_SOURCE.contains("TranscriptSelectedLinePaint"));
    assert!(!INLINE_MARKDOWN_SOURCE.contains("paint_glyph"));
    assert!(!SELECTION_PAINT_SOURCE.contains("shape_line"));
    assert!(!SELECTION_PAINT_SOURCE.contains("shape_text"));
}

fn rust_function_body<'a>(source: &'a str, function_signature: &str) -> &'a str {
    let signature_index = source
        .find(function_signature)
        .unwrap_or_else(|| panic!("missing function signature {function_signature}"));
    let after_signature = &source[signature_index..];
    let body_start = signature_index
        + after_signature
            .find('{')
            .unwrap_or_else(|| panic!("missing function body for {function_signature}"));
    let mut depth = 0usize;

    for (offset, character) in source[body_start..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[body_start..body_start + offset + character.len_utf8()];
                }
            }
            _ => {}
        }
    }

    panic!("unterminated function body for {function_signature}");
}
