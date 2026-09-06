mod element;
pub(super) use element::NoticeDetailElement;

use std::{cell::RefCell, ops::Range, rc::Rc};
use unicode_segmentation::UnicodeSegmentation;

use gpui::{
    App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior, Hsla,
    IntoElement, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    Pixels, Point, SharedString, Style, TextRun, Window, WrappedLine, fill, point, px, size,
};

#[derive(Clone, Default)]
pub(super) struct DetailState(Rc<RefCell<DetailStateInner>>);

#[derive(Default)]
struct DetailStateInner {
    selection: Option<DetailSelection>,
    layout: Option<DetailLayout>,
    pending_scroll_anchor: Option<DetailScrollAnchor>,
    reset_scroll: bool,
    dragging: bool,
    preferred_x: Option<Pixels>,
    caret_row: Option<usize>,
    generation: u64,
    enabled: bool,
}

#[derive(Clone)]
struct DetailScrollAnchor {
    start: usize,
    end: usize,
    source: SharedString,
    visual_top: Pixels,
    line_height: Pixels,
    font: gpui::Font,
    font_size: Pixels,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DetailSelection {
    pub(super) anchor: usize,
    pub(super) head: usize,
}

impl DetailSelection {
    pub(super) fn range(self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

impl DetailState {
    pub(super) fn clear(&self) {
        let mut state = self.0.borrow_mut();
        state.selection = None;
        state.dragging = false;
        state.preferred_x = None;
        state.caret_row = None;
    }

    pub(super) fn invalidate_interaction(&self, enabled: bool) {
        let mut state = self.0.borrow_mut();
        state.generation = state.generation.wrapping_add(1);
        state.enabled = enabled;
        state.dragging = false;
    }

    pub(super) fn retire_layout(&self) {
        let mut state = self.0.borrow_mut();
        state.layout = None;
        state.pending_scroll_anchor = None;
        state.reset_scroll = false;
    }

    fn accepts(&self, generation: u64) -> bool {
        let state = self.0.borrow();
        state.enabled && state.generation == generation
    }

    pub(super) fn reset_navigation(&self) {
        let mut state = self.0.borrow_mut();
        state.preferred_x = None;
        state.caret_row = None;
    }

    pub(super) fn selection(&self) -> Option<DetailSelection> {
        self.0.borrow().selection
    }

    pub(super) fn set_selection(&self, selection: Option<DetailSelection>) {
        self.0.borrow_mut().selection = selection;
    }

    pub(super) fn begin_drag(&self, index: usize, extend: bool, click_count: usize) {
        let mut state = self.0.borrow_mut();
        let selected = state
            .layout
            .as_ref()
            .map(|layout| {
                if click_count >= 3 {
                    layout
                        .lines
                        .iter()
                        .find(|line| index <= line.range.end)
                        .map(|line| line.range.start..(line.range.end + 1).min(layout.text_len))
                        .unwrap_or(index..index)
                } else if click_count == 2 {
                    layout
                        .text
                        .split_word_bound_indices()
                        .find(|(start, text)| *start <= index && index < start + text.len())
                        .map(|(start, text)| start..start + text.len())
                        .unwrap_or(index..index)
                } else {
                    index..index
                }
            })
            .unwrap_or(index..index);
        let anchor = if extend {
            state
                .selection
                .map_or(selected.start, |selection| selection.anchor)
        } else {
            selected.start
        };
        state.selection = Some(DetailSelection {
            anchor,
            head: selected.end,
        });
        state.dragging = true;
        state.preferred_x = None;
        state.caret_row = None;
    }

    pub(super) fn extend_drag(&self, index: usize) -> bool {
        let mut state = self.0.borrow_mut();
        let Some(mut selection) = state.selection else {
            return false;
        };
        if !state.dragging {
            return false;
        }
        selection.head = index;
        state.selection = Some(selection);
        true
    }

    pub(super) fn end_drag(&self) {
        self.0.borrow_mut().dragging = false;
    }

    pub(super) fn capture_scroll_anchor(&self, offset: Pixels) -> bool {
        let mut state = self.0.borrow_mut();
        state.pending_scroll_anchor = state
            .layout
            .as_ref()
            .and_then(|layout| layout.scroll_anchor(offset));
        state.pending_scroll_anchor.is_some()
    }

    pub(super) fn take_scroll_reset(&self) -> bool {
        std::mem::take(&mut self.0.borrow_mut().reset_scroll)
    }

    pub(super) fn index_for_position(&self, position: Point<Pixels>) -> usize {
        self.0
            .borrow()
            .layout
            .as_ref()
            .map_or(0, |layout| layout.index_for_position(position))
    }

    pub(super) fn vertical_target(&self, offset: usize, direction: i32) -> usize {
        let mut state = self.0.borrow_mut();
        let Some(layout) = state.layout.as_ref() else {
            return offset;
        };
        let (target, x, row) =
            layout.vertical_target(offset, direction, state.preferred_x, state.caret_row);
        state.preferred_x = Some(x);
        state.caret_row = Some(row);
        target
    }

    pub(super) fn line_edge(&self, offset: usize, end: bool) -> usize {
        let mut state = self.0.borrow_mut();
        let Some(layout) = state.layout.as_ref() else {
            return offset;
        };
        let (target, row) = layout.line_edge(offset, end, state.caret_row);
        state.caret_row = Some(row);
        state.preferred_x = None;
        target
    }

    pub(super) fn caret_vertical_bounds(&self, offset: usize) -> Option<Range<Pixels>> {
        let state = self.0.borrow();
        let layout = state.layout.as_ref()?;
        let rows = layout.visual_rows();
        let index = state
            .caret_row
            .filter(|index| {
                rows.get(*index)
                    .is_some_and(|row| row.range.contains(&offset) || row.range.end == offset)
            })
            .or_else(|| layout.visual_row_index(&rows, offset))?;
        Some(rows[index].top..rows[index].top + layout.line_height)
    }
}

struct DetailLayout {
    text: SharedString,
    bounds: Bounds<Pixels>,
    lines: Vec<DetailLine>,
    line_height: Pixels,
    text_len: usize,
    font: gpui::Font,
    font_size: Pixels,
}

struct DetailLine {
    line: WrappedLine,
    range: Range<usize>,
    origin: Point<Pixels>,
}

struct DetailVisualRow<'a> {
    line: &'a DetailLine,
    range: Range<usize>,
    top: Pixels,
}

impl DetailLayout {
    fn index_for_position(&self, position: Point<Pixels>) -> usize {
        let position = position - self.bounds.origin;
        let rows = self.visual_rows();
        let row = rows
            .iter()
            .find(|row| position.y >= row.top && position.y <= row.top + self.line_height)
            .or_else(|| {
                if position.y < px(0.) {
                    rows.first()
                } else {
                    rows.last()
                }
            });
        let Some(row) = row else {
            return 0;
        };
        let local = row
            .line
            .line
            .closest_index_for_position(
                point(
                    position.x - row.line.origin.x,
                    row.top - row.line.origin.y + self.line_height / 2.,
                ),
                self.line_height,
            )
            .unwrap_or_else(|index| index)
            .clamp(
                row.range.start - row.line.range.start,
                row.range.end - row.line.range.start,
            );
        self.grapheme_boundary(row.line.range.start + local)
    }

    fn vertical_target(
        &self,
        offset: usize,
        direction: i32,
        preferred_x: Option<Pixels>,
        caret_row: Option<usize>,
    ) -> (usize, Pixels, usize) {
        let rows = self.visual_rows();
        let Some(current_index) = caret_row
            .filter(|index| {
                rows.get(*index)
                    .is_some_and(|row| row.range.contains(&offset) || row.range.end == offset)
            })
            .or_else(|| self.visual_row_index(&rows, offset))
        else {
            return (offset.min(self.text_len), px(0.), 0);
        };
        let current = &rows[current_index];
        let target_index = (if direction < 0 {
            current_index.checked_sub(1)
        } else {
            (current_index + 1 < rows.len()).then_some(current_index + 1)
        })
        .unwrap_or(current_index);
        let target = &rows[target_index];
        let x = preferred_x.unwrap_or_else(|| current.x_for_index(offset));
        let target_local = target
            .line
            .line
            .closest_index_for_position(
                point(x, target.top - target.line.origin.y + self.line_height / 2.),
                self.line_height,
            )
            .unwrap_or_else(|index| index)
            .clamp(
                target.range.start.saturating_sub(target.line.range.start),
                target.range.end.saturating_sub(target.line.range.start),
            );
        (
            self.grapheme_boundary(target.line.range.start + target_local),
            x,
            target_index,
        )
    }

    fn line_edge(&self, offset: usize, end: bool, caret_row: Option<usize>) -> (usize, usize) {
        let rows = self.visual_rows();
        let Some(index) = caret_row
            .filter(|index| {
                rows.get(*index)
                    .is_some_and(|row| row.range.contains(&offset) || row.range.end == offset)
            })
            .or_else(|| self.visual_row_index(&rows, offset))
        else {
            return (offset.min(self.text_len), 0);
        };
        if end {
            (rows[index].range.end, index)
        } else {
            (rows[index].range.start, index)
        }
    }

    fn scroll_anchor(&self, offset: Pixels) -> Option<DetailScrollAnchor> {
        self.visual_rows().into_iter().find_map(|row| {
            (offset >= row.top && offset < row.top + self.line_height).then(|| DetailScrollAnchor {
                start: row.range.start,
                end: row.range.end,
                source: self.text.clone(),
                visual_top: row.top,
                line_height: self.line_height,
                font: self.font.clone(),
                font_size: self.font_size,
            })
        })
    }

    fn visual_rows(&self) -> Vec<DetailVisualRow<'_>> {
        let mut rows = Vec::new();
        for line in &self.lines {
            let mut start = line.range.start;
            for (index, boundary) in line.line.wrap_boundaries().iter().enumerate() {
                let end = line.range.start
                    + line.line.runs()[boundary.run_ix].glyphs[boundary.glyph_ix].index;
                rows.push(DetailVisualRow {
                    line,
                    range: start..end,
                    top: line.origin.y + self.line_height * index as f32,
                });
                start = end;
            }
            let index = line.line.wrap_boundaries().len();
            rows.push(DetailVisualRow {
                line,
                range: start..line.range.end,
                top: line.origin.y + self.line_height * index as f32,
            });
        }
        rows
    }

    fn visual_row_index(&self, rows: &[DetailVisualRow<'_>], offset: usize) -> Option<usize> {
        rows.iter()
            .position(|row| {
                offset < row.range.end
                    || (offset == row.range.end && row.range.end == row.line.range.end)
            })
            .or_else(|| rows.len().checked_sub(1))
    }

    fn grapheme_boundary(&self, offset: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain([self.text.len()])
            .min_by_key(|index| index.abs_diff(offset))
            .unwrap_or(0)
    }
}

fn selection_quads(layout: &DetailLayout, selected: Range<usize>, color: Hsla) -> Vec<PaintQuad> {
    let mut quads = Vec::new();
    for row in layout.visual_rows() {
        let start = selected.start.max(row.range.start);
        let end = selected.end.min(row.range.end);
        if start >= end {
            continue;
        }
        let start_x = row.x_for_index(start);
        let end_x = row.x_for_index(end);
        append_selection_quad(
            &mut quads,
            row.line,
            layout.bounds.origin,
            start_x,
            end_x,
            row.top,
            layout.line_height,
            color,
        );
    }
    quads
}

fn append_selection_quad(
    quads: &mut Vec<PaintQuad>,
    line: &DetailLine,
    bounds_origin: Point<Pixels>,
    start_x: Pixels,
    end_x: Pixels,
    y: Pixels,
    line_height: Pixels,
    color: Hsla,
) {
    if end_x <= start_x {
        return;
    }
    quads.push(fill(
        Bounds::from_corners(
            bounds_origin + point(line.origin.x + start_x, y),
            bounds_origin + point(line.origin.x + end_x, y + line_height),
        ),
        color,
    ));
}

impl DetailVisualRow<'_> {
    fn x_for_index(&self, index: usize) -> Pixels {
        self.line
            .line
            .unwrapped_layout
            .x_for_index(index.clamp(self.range.start, self.range.end) - self.line.range.start)
            - self
                .line
                .line
                .unwrapped_layout
                .x_for_index(self.range.start - self.line.range.start)
    }
}
