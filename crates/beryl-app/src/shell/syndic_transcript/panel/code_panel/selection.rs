use super::super::{code_layout_mount::CodeLayoutMount, code_viewport::CodeViewport};
use gpui::{
    Bounds, Pixels, Point, StreamingLayoutFragment, StreamingLayoutHit, StreamingLayoutMap,
    StreamingLayoutPosition, point, px,
};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in super::super) struct SourceSelection {
    pub anchor: u64,
    pub head: u64,
}

impl SourceSelection {
    pub fn range(self) -> Range<u64> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

pub(in super::super) fn maps(fragment: &StreamingLayoutFragment) -> &[StreamingLayoutMap] {
    match fragment {
        StreamingLayoutFragment::Text(value) => value.maps(),
        StreamingLayoutFragment::OversizeAtom(value) => value.maps(),
        StreamingLayoutFragment::InlineObject(value) => value.maps(),
        StreamingLayoutFragment::Boundary(value) => value.maps(),
    }
}

pub(in super::super) fn source_x(
    fragment: &StreamingLayoutFragment,
    source: u64,
) -> Option<Pixels> {
    let position = StreamingLayoutPosition::at(source);
    if let Some(map) = maps(fragment)
        .iter()
        .find(|map| map.logical_position == position)
    {
        return Some(map.position.x);
    }
    match fragment {
        StreamingLayoutFragment::Text(value) => value
            .position_for_logical_position(position)
            .ok()
            .flatten()
            .map(|point| point.x),
        StreamingLayoutFragment::OversizeAtom(value) => value
            .position_for_logical_position(position)
            .map(|point| point.x),
        StreamingLayoutFragment::InlineObject(value) => value
            .position_for_logical_position(position)
            .map(|point| point.x),
        StreamingLayoutFragment::Boundary(value) => value
            .position_for_logical_position(position)
            .map(|point| point.x),
    }
}

pub(in super::super) fn hit(
    layout: &CodeLayoutMount,
    viewport: &CodeViewport,
    position: Point<Pixels>,
) -> Option<u64> {
    let scroll = viewport.scroll.borrow();
    if !scroll.bounds.contains(&position) || layout.state(viewport) != "ready" {
        return None;
    }
    let local = position - scroll.bounds.origin;
    let row = ((f32::from(local.y) + scroll.remainder) / scroll.line_height).floor();
    if !row.is_finite() || row < 0.0 {
        return None;
    }
    let logical_line = scroll.top_line.checked_add(row as u64)?;
    for prepared in layout.prepared() {
        if !prepared.target.lines.contains(&logical_line) {
            continue;
        }
        if let Some(source) = layout.empty_line_caret(logical_line) {
            return Some(source);
        }
        let point = point(local.x - px(10.0) + prepared.target.inline_start, px(0.0));
        let mut leading = None;
        let mut terminal = None;
        let mut fallback = None;
        for segment in &prepared.fragments {
            if segment.logical_line != logical_line {
                continue;
            }
            for fragment in segment.fragments.iter() {
                if let Some(last) = maps(fragment).last()
                    && prepared.terminal_width == Some(last.position.x)
                {
                    terminal = Some(*last);
                }
                if let Some(first) = maps(fragment).first() {
                    leading = leading.or(Some(*first));
                }
                let result = match fragment {
                    StreamingLayoutFragment::Text(value) => {
                        value.closest_logical_position_for_position(point)
                    }
                    StreamingLayoutFragment::OversizeAtom(value) => {
                        value.closest_logical_position_for_position(point)
                    }
                    StreamingLayoutFragment::InlineObject(value) => {
                        value.closest_logical_position_for_position(point)
                    }
                    StreamingLayoutFragment::Boundary(value) => {
                        terminal = value.maps().first().copied();
                        value.closest_logical_position_for_position(point)
                    }
                };
                let Ok(result) = result else {
                    return None;
                };
                if let Some(StreamingLayoutHit::Gap(position)) = result {
                    return Some(position.byte_offset);
                }
                if let (Some(first), Some(last)) = (maps(fragment).first(), maps(fragment).last()) {
                    if point.x >= first.position.x
                        && point.x <= last.position.x
                        && !matches!(fragment, StreamingLayoutFragment::Boundary(_))
                    {
                        let selected = if matches!(fragment, StreamingLayoutFragment::Text(_))
                            || point.x - first.position.x > last.position.x - point.x
                        {
                            last
                        } else {
                            first
                        };
                        fallback = Some(selected.logical_position.byte_offset);
                    }
                }
            }
        }
        if fallback.is_some() {
            return fallback;
        }
        if let Some(first) = leading.filter(|map| point.x < map.position.x) {
            return Some(first.logical_position.byte_offset);
        }
        if let Some(last) = terminal.filter(|map| point.x >= map.position.x) {
            return Some(last.logical_position.byte_offset);
        }
    }
    None
}

pub(in super::super) fn highlight(
    fragment: &StreamingLayoutFragment,
    selected: &Range<u64>,
    origin: Point<Pixels>,
    height: Pixels,
) -> Option<Bounds<Pixels>> {
    let first = maps(fragment).first()?;
    let last = maps(fragment).last()?;
    let start = first.logical_position.byte_offset.max(selected.start);
    let end = last.logical_position.byte_offset.min(selected.end);
    if start >= end {
        return None;
    }
    let left = source_x(fragment, start).unwrap_or(first.position.x);
    let right = source_x(fragment, end).unwrap_or(last.position.x);
    (right > left).then(|| {
        Bounds::new(
            origin + point(left, first.position.y),
            gpui::size(right - left, height),
        )
    })
}
