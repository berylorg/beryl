use std::ops::Range;

use gpui::{Bounds, Pixels, Point, WrappedLineLayout, point, px, size};

pub(crate) fn wrapped_line_selection_highlight_bounds(
    line: &WrappedLineLayout,
    origin: Point<Pixels>,
    line_height: Pixels,
    fallback_width: Pixels,
    selected_range: Range<usize>,
) -> Vec<Bounds<Pixels>> {
    let visual_ranges = visual_line_ranges_for_wrapped_line(line);
    selection_highlight_bounds_for_visual_ranges(
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

pub(crate) fn selection_highlight_bounds_for_visual_ranges(
    origin: Point<Pixels>,
    line_height: Pixels,
    fallback_width: Pixels,
    visual_ranges: &[Range<usize>],
    selected_range: Range<usize>,
    mut position_for_index: impl FnMut(usize, usize) -> Option<Point<Pixels>>,
) -> Vec<Bounds<Pixels>> {
    let height = line_height.max(px(2.0));
    let mut bounds = Vec::new();

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

        bounds.push(Bounds::new(
            point(origin.x + start_position.x, origin.y + start_position.y),
            size(end_position.x - start_position.x, height),
        ));
    }

    bounds
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

fn visual_line_ranges_for_wrapped_line(line: &WrappedLineLayout) -> Vec<Range<usize>> {
    let wrap_indices = line.wrap_boundaries().iter().filter_map(|boundary| {
        let run = line.runs().get(boundary.run_ix)?;
        let glyph = run.glyphs.get(boundary.glyph_ix)?;
        Some(glyph.index)
    });

    visual_line_ranges_for_wrap_indices(line.len(), wrap_indices)
}
