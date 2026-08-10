use std::ops::Range;

use gpui::{Bounds, ContentMask, Hsla, Pixels, Point, Window, WrappedLineLayout, point, px, size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WrappedLineSelectionPaintMask {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) visual_line_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SelectionOpacityRegion {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionGlyphPaintMode {
    Monochrome,
    Emoji,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SelectionGlyphPaintInput<Metadata> {
    pub(crate) run_index: usize,
    pub(crate) glyph_index: usize,
    pub(crate) text_index: usize,
    pub(crate) x: Pixels,
    pub(crate) mode: SelectionGlyphPaintMode,
    pub(crate) metadata: Metadata,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlannedSelectionGlyph<Metadata> {
    pub(crate) run_index: usize,
    pub(crate) glyph_index: usize,
    pub(crate) text_index: usize,
    pub(crate) source_x: Pixels,
    pub(crate) origin: Point<Pixels>,
    pub(crate) mask: WrappedLineSelectionPaintMask,
    pub(crate) mode: SelectionGlyphPaintMode,
    pub(crate) metadata: Metadata,
}

pub(crate) fn wrapped_line_selection_highlight_bounds(
    line: &WrappedLineLayout,
    origin: Point<Pixels>,
    line_height: Pixels,
    fallback_width: Pixels,
    selected_range: Range<usize>,
) -> Vec<Bounds<Pixels>> {
    let visual_ranges = visual_line_ranges_for_wrapped_line(line);
    selection_paint_masks_for_visual_ranges(
        origin,
        line_height,
        fallback_width,
        visual_ranges.as_slice(),
        selected_range,
        |index, visual_index| {
            if visual_index > 0
                && visual_ranges
                    .get(visual_index)
                    .is_some_and(|range| range.start == index)
            {
                return Some(point(px(0.0), line_height * visual_index as f32));
            }

            line.position_for_index(index, line_height)
        },
    )
    .into_iter()
    .map(|mask| mask.bounds)
    .collect()
}

pub(crate) fn wrapped_line_selection_paint_masks(
    line: &WrappedLineLayout,
    origin: Point<Pixels>,
    line_height: Pixels,
    fallback_width: Pixels,
    selected_range: Range<usize>,
) -> Vec<WrappedLineSelectionPaintMask> {
    let visual_ranges = visual_line_ranges_for_wrapped_line(line);
    selection_paint_masks_for_visual_ranges(
        origin,
        line_height,
        fallback_width,
        visual_ranges.as_slice(),
        selected_range,
        |index, visual_index| {
            if visual_index > 0
                && visual_ranges
                    .get(visual_index)
                    .is_some_and(|range| range.start == index)
            {
                return Some(point(px(0.0), line_height * visual_index as f32));
            }

            line.position_for_index(index, line_height)
        },
    )
}

#[allow(dead_code)]
pub(crate) fn selection_highlight_bounds_for_visual_ranges(
    origin: Point<Pixels>,
    line_height: Pixels,
    fallback_width: Pixels,
    visual_ranges: &[Range<usize>],
    selected_range: Range<usize>,
    position_for_index: impl FnMut(usize, usize) -> Option<Point<Pixels>>,
) -> Vec<Bounds<Pixels>> {
    selection_paint_masks_for_visual_ranges(
        origin,
        line_height,
        fallback_width,
        visual_ranges,
        selected_range,
        position_for_index,
    )
    .into_iter()
    .map(|mask| mask.bounds)
    .collect()
}

pub(crate) fn selection_paint_masks_for_visual_ranges(
    origin: Point<Pixels>,
    line_height: Pixels,
    fallback_width: Pixels,
    visual_ranges: &[Range<usize>],
    selected_range: Range<usize>,
    mut position_for_index: impl FnMut(usize, usize) -> Option<Point<Pixels>>,
) -> Vec<WrappedLineSelectionPaintMask> {
    let height = line_height.max(px(2.0));
    let mut masks = Vec::new();

    for (visual_index, visual_range) in visual_ranges.iter().enumerate() {
        let visual_start = selected_range.start.max(visual_range.start);
        let visual_end = selected_range.end.min(visual_range.end);
        if visual_start >= visual_end {
            continue;
        }

        let start_position = position_for_index(visual_start, visual_index)
            .unwrap_or_else(|| point(px(0.0), line_height * visual_index as f32));
        let end_position = position_for_index(visual_end, visual_index)
            .unwrap_or_else(|| point(fallback_width, line_height * visual_index as f32));
        if end_position.x <= start_position.x {
            continue;
        }

        masks.push(WrappedLineSelectionPaintMask {
            bounds: Bounds::new(
                point(origin.x + start_position.x, origin.y + start_position.y),
                size(end_position.x - start_position.x, height),
            ),
            visual_line_index: visual_index,
        });
    }

    masks
}

pub(crate) fn visual_line_ranges_for_wrap_indices(
    len: usize,
    wrap_indices: impl IntoIterator<Item = usize>,
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0usize;
    for index in wrap_indices {
        let end = index.min(len).max(start);
        ranges.push(start..end);
        start = end;
    }
    ranges.push(start..len);
    ranges
}

pub(crate) fn wrapped_glyph_paint_origin(
    line_origin: Point<Pixels>,
    line_height: Pixels,
    ascent: Pixels,
    descent: Pixels,
    glyph_x: Pixels,
    visual_line_start_x: Pixels,
    visual_line_index: usize,
) -> Point<Pixels> {
    let padding_top = (line_height - ascent - descent) / 2.0;
    point(
        line_origin.x + glyph_x - visual_line_start_x,
        line_origin.y + line_height * visual_line_index as f32 + padding_top + ascent,
    )
}

pub(crate) fn selection_opacity_for_bounds(
    bounds: Bounds<Pixels>,
    regions: &[SelectionOpacityRegion],
) -> f32 {
    let center = point(
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    );
    regions
        .iter()
        .find(|region| region.bounds.contains(&center))
        .map_or(1.0, |region| region.opacity.clamp(0.0, 1.0))
}

pub(crate) fn selection_glyph_paint_plan<Metadata>(
    line_origin: Point<Pixels>,
    line_height: Pixels,
    ascent: Pixels,
    descent: Pixels,
    glyphs: impl IntoIterator<Item = SelectionGlyphPaintInput<Metadata>>,
    wrap_boundaries: impl IntoIterator<Item = (usize, usize)>,
    masks: &[WrappedLineSelectionPaintMask],
) -> Vec<PlannedSelectionGlyph<Metadata>> {
    let mut plan = Vec::new();
    let mut mask_index = 0usize;
    let mut visual_line_index = 0usize;
    let mut visual_line_start_x = px(0.0);
    let mut wraps = wrap_boundaries.into_iter().peekable();

    for glyph in glyphs {
        if wraps
            .peek()
            .is_some_and(|boundary| *boundary == (glyph.run_index, glyph.glyph_index))
        {
            wraps.next();
            visual_line_index += 1;
            visual_line_start_x = glyph.x;
        }

        while masks
            .get(mask_index)
            .is_some_and(|mask| mask.visual_line_index < visual_line_index)
        {
            mask_index += 1;
        }
        let Some(mask) = masks
            .get(mask_index)
            .filter(|mask| mask.visual_line_index == visual_line_index)
            .copied()
        else {
            continue;
        };

        plan.push(PlannedSelectionGlyph {
            run_index: glyph.run_index,
            glyph_index: glyph.glyph_index,
            text_index: glyph.text_index,
            source_x: glyph.x,
            origin: wrapped_glyph_paint_origin(
                line_origin,
                line_height,
                ascent,
                descent,
                glyph.x,
                visual_line_start_x,
                visual_line_index,
            ),
            mask,
            mode: glyph.mode,
            metadata: glyph.metadata,
        });
    }

    plan
}

pub(crate) fn retain_first_paint_error<Error>(
    first_error: &mut Option<Error>,
    result: Result<(), Error>,
) {
    if first_error.is_none()
        && let Err(error) = result
    {
        *first_error = Some(error);
    }
}

pub(crate) fn paint_wrapped_line_selection_foreground(
    line: &WrappedLineLayout,
    origin: Point<Pixels>,
    line_height: Pixels,
    masks: &[WrappedLineSelectionPaintMask],
    foreground: Hsla,
    window: &mut Window,
) {
    let plan = selection_glyph_paint_plan(
        origin,
        line_height,
        line.ascent(),
        line.descent(),
        line.runs().iter().enumerate().flat_map(|(run_index, run)| {
            run.glyphs
                .iter()
                .enumerate()
                .map(move |(glyph_index, glyph)| SelectionGlyphPaintInput {
                    run_index,
                    glyph_index,
                    text_index: glyph.index,
                    x: glyph.position.x,
                    mode: if glyph.is_emoji {
                        SelectionGlyphPaintMode::Emoji
                    } else {
                        SelectionGlyphPaintMode::Monochrome
                    },
                    metadata: (run.font_id, glyph.id),
                })
        }),
        line.wrap_boundaries()
            .iter()
            .map(|boundary| (boundary.run_ix, boundary.glyph_ix)),
        masks,
    );
    let mut first_error = None;

    for glyph in plan {
        let (font_id, glyph_id) = glyph.metadata;
        window.with_content_mask(
            Some(ContentMask {
                bounds: glyph.mask.bounds,
            }),
            |window| {
                let result = if glyph.mode == SelectionGlyphPaintMode::Emoji {
                    window.paint_emoji(glyph.origin, font_id, glyph_id, line.font_size())
                } else {
                    window.paint_glyph(
                        glyph.origin,
                        font_id,
                        glyph_id,
                        line.font_size(),
                        foreground,
                    )
                };
                retain_first_paint_error(&mut first_error, result);
            },
        );
    }

    if let Some(error) = first_error {
        tracing::error!(error = %error, "transcript selection glyph repaint failed");
    }
}

fn visual_line_ranges_for_wrapped_line(line: &WrappedLineLayout) -> Vec<Range<usize>> {
    let wrap_indices = line.wrap_boundaries().iter().filter_map(|boundary| {
        let run = line.runs().get(boundary.run_ix)?;
        let glyph = run.glyphs.get(boundary.glyph_ix)?;
        Some(glyph.index)
    });

    visual_line_ranges_for_wrap_indices(line.len(), wrap_indices)
}
