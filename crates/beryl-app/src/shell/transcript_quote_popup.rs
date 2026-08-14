use gpui::{Bounds, Pixels, Point, point, px, size};

const POPUP_WIDTH: f32 = 74.0;
const POPUP_HEIGHT: f32 = 34.0;
const POPUP_GAP: f32 = 8.0;
const POPUP_MARGIN: f32 = 8.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptQuotePopupState {
    selection_generation: u64,
    open: Option<TranscriptQuotePopupOpen>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TranscriptQuotePopupOpen {
    selection_generation: u64,
    position: Point<Pixels>,
    popup_bounds: Option<Bounds<Pixels>>,
}

impl TranscriptQuotePopupState {
    pub(crate) fn position(&self) -> Option<Point<Pixels>> {
        self.open.map(|open| open.position)
    }

    pub(crate) fn set_popup_bounds(&mut self, bounds: Option<Bounds<Pixels>>) -> bool {
        let Some(open) = self.open.as_mut() else {
            return false;
        };
        if open.popup_bounds == bounds {
            return false;
        }

        open.popup_bounds = bounds;
        true
    }

    pub(crate) fn note_selection_mutated(&mut self) -> bool {
        self.selection_generation = self.selection_generation.saturating_add(1);
        self.open.take().is_some()
    }

    pub(crate) fn clear_selection(&mut self) -> bool {
        self.selection_generation = self.selection_generation.saturating_add(1);
        self.open.take().is_some()
    }

    pub(crate) fn open_for_selection(
        &mut self,
        selection_bounds: Bounds<Pixels>,
        viewport_bounds: Bounds<Pixels>,
    ) -> bool {
        let position = quote_popup_position(selection_bounds, viewport_bounds);
        let popup_bounds = self
            .open
            .filter(|open| {
                open.selection_generation == self.selection_generation && open.position == position
            })
            .and_then(|open| open.popup_bounds);
        let next = TranscriptQuotePopupOpen {
            selection_generation: self.selection_generation,
            position,
            popup_bounds,
        };
        if self.open == Some(next) {
            return false;
        }

        self.open = Some(next);
        true
    }
}

pub(crate) fn popup_width() -> Pixels {
    px(POPUP_WIDTH)
}

pub(crate) fn popup_height() -> Pixels {
    px(POPUP_HEIGHT)
}

pub(crate) fn selection_bounds_union(
    bounds: impl IntoIterator<Item = Bounds<Pixels>>,
) -> Option<Bounds<Pixels>> {
    let mut bounds = bounds.into_iter();
    let first = bounds.next()?;
    let mut left = first.left();
    let mut top = first.top();
    let mut right = first.right();
    let mut bottom = first.bottom();

    for bounds in bounds {
        left = left.min(bounds.left());
        top = top.min(bounds.top());
        right = right.max(bounds.right());
        bottom = bottom.max(bounds.bottom());
    }

    Some(Bounds::new(
        point(left, top),
        size(right - left, bottom - top),
    ))
}

pub(crate) fn selection_geometry_matches_viewport(
    visible_geometry_viewport_bounds: Option<Bounds<Pixels>>,
    viewport_bounds: Bounds<Pixels>,
) -> bool {
    visible_geometry_viewport_bounds == Some(viewport_bounds)
}

pub(crate) fn quote_popup_position(
    selection_bounds: Bounds<Pixels>,
    viewport_bounds: Bounds<Pixels>,
) -> Point<Pixels> {
    let width = popup_width();
    let height = popup_height();
    let gap = px(POPUP_GAP);
    let margin = px(POPUP_MARGIN);

    let min_x = viewport_bounds.left() + margin;
    let max_x = (viewport_bounds.right() - margin - width).max(min_x);
    let selection_center_x = selection_bounds.left() + (selection_bounds.size.width / 2.0);
    let x = (selection_center_x - (width / 2.0)).clamp(min_x, max_x);

    let min_y = viewport_bounds.top() + margin;
    let max_y = (viewport_bounds.bottom() - margin - height).max(min_y);
    let above_y = selection_bounds.top() - height - gap;
    let below_y = selection_bounds.bottom() + gap;
    let y = if above_y >= min_y { above_y } else { below_y }.clamp(min_y, max_y);

    point(x, y)
}
