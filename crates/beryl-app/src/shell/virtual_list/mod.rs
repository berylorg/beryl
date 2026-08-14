//! A list element that can be used to render a large number of differently sized elements
//! efficiently. Clients of this API need to ensure that elements outside of the scrolled
//! area do not change their height for this element to function correctly. If your elements
//! do change height, notify the list element via [`ListState::splice`], [`ListState::reset`],
//! or [`ListState::invalidate_item_measurement`].
//! In order to minimize re-renders, this element's state is stored intrusively
//! on your own views, so that your code can coordinate directly with the list element's cached state.
//!
//! If all of your elements are the same height, see [`gpui::UniformList`] for a simpler API

use gpui::Refineable as _;
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, DispatchPhase, Edges, Element, EntityId,
    FocusHandle, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement,
    Overflow, Pixels, Point, ScrollDelta, ScrollWheelEvent, Size, Style, StyleRefinement, Styled,
    Window, point, px, size,
};
use gpui_sum_tree::{Bias, Dimensions, SumTree};
use std::collections::VecDeque;
use std::{cell::RefCell, ops::Range, rc::Rc};

mod element;
mod item;
mod layout_state;
mod scroll_state;
mod state;

type RenderItemFn = dyn FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static;

/// Construct a new list element
pub fn list(
    state: ListState,
    render_item: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
) -> List {
    List {
        state,
        render_item: Box::new(render_item),
        style: StyleRefinement::default(),
        sizing_behavior: ListSizingBehavior::default(),
    }
}

/// A list element
pub struct List {
    state: ListState,
    render_item: Box<RenderItemFn>,
    style: StyleRefinement,
    sizing_behavior: ListSizingBehavior,
}

impl List {
    /// Set the sizing behavior for the list.
    pub fn with_sizing_behavior(mut self, behavior: ListSizingBehavior) -> Self {
        self.sizing_behavior = behavior;
        self
    }
}

/// The list state that views must hold on behalf of the list element.
#[derive(Clone)]
pub struct ListState(Rc<RefCell<StateInner>>);

impl std::fmt::Debug for ListState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ListState")
    }
}

struct StateInner {
    last_layout_bounds: Option<Bounds<Pixels>>,
    last_padding: Option<Edges<Pixels>>,
    items: SumTree<ListItem>,
    scroll_position: ListScrollPosition,
    virtual_trailing_scroll_allowance: Pixels,
    alignment: ListAlignment,
    overdraw: Pixels,
    reset: bool,
    #[allow(clippy::type_complexity)]
    scroll_handler: Option<Box<dyn FnMut(&ListScrollEvent, &mut Window, &mut App)>>,
    scrollbar_drag_start_height: Option<Pixels>,
    measuring_behavior: ListMeasuringBehavior,
}

/// Whether the list is scrolling from top to bottom or bottom to top.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListAlignment {
    /// The list is scrolling from top to bottom, like most lists.
    Top,
    /// The list is scrolling from bottom to top, like a chat log.
    Bottom,
}

/// The list's current scroll intent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListScrollPosition {
    /// Follow the real content bottom in a bottom-aligned list.
    Bottom,
    /// Preserve a durable offset into real list content.
    Content(ListOffset),
    /// Preserve a position inside virtual trailing scroll allowance.
    VirtualTail {
        /// Pixels past the real-content scroll end.
        offset_from_content_end: Pixels,
    },
}

/// A scroll event that has been converted to be in terms of the list's items.
pub struct ListScrollEvent {
    /// The range of items currently visible in the list, after applying the scroll event.
    pub visible_range: Range<usize>,

    /// The number of items that are currently visible in the list, after applying the scroll event.
    pub count: usize,

    /// Whether the list has been scrolled.
    pub is_scrolled: bool,
}

/// The sizing behavior to apply during layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ListSizingBehavior {
    /// The list should calculate its size based on the size of its items.
    Infer,
    /// The list should not calculate a fixed size.
    #[default]
    Auto,
}

/// The measuring behavior to apply during layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ListMeasuringBehavior {
    /// Measure all items in the list.
    /// Note: This can be expensive for the first frame in a large list.
    Measure(bool),
    /// Only measure visible items
    #[default]
    Visible,
}

impl ListMeasuringBehavior {
    fn reset(&mut self) {
        match self {
            ListMeasuringBehavior::Measure(has_measured) => *has_measured = false,
            ListMeasuringBehavior::Visible => {}
        }
    }
}

/// The horizontal sizing behavior to apply during layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ListHorizontalSizingBehavior {
    /// List items' width can never exceed the width of the list.
    #[default]
    FitList,
    /// List items' width may go over the width of the list, if any item is wider.
    Unconstrained,
}

struct LayoutItemsResponse {
    max_item_width: Pixels,
    scroll_top: ListOffset,
    item_layouts: VecDeque<ItemLayout>,
}

struct ItemLayout {
    index: usize,
    element: AnyElement,
    size: Size<Pixels>,
}

/// Frame state used by the [List] element after layout.
pub struct ListPrepaintState {
    hitbox: Hitbox,
    layout: LayoutItemsResponse,
}

#[derive(Clone)]
enum ListItem {
    Unmeasured {
        focus_handle: Option<FocusHandle>,
    },
    Measured {
        size: Size<Pixels>,
        focus_handle: Option<FocusHandle>,
    },
    DirtyMeasured {
        size: Size<Pixels>,
        focus_handle: Option<FocusHandle>,
    },
}

impl ListItem {
    fn size(&self) -> Option<Size<Pixels>> {
        if let ListItem::Measured { size, .. } | ListItem::DirtyMeasured { size, .. } = self {
            Some(*size)
        } else {
            None
        }
    }

    fn needs_measurement(&self) -> bool {
        matches!(
            self,
            ListItem::Unmeasured { .. } | ListItem::DirtyMeasured { .. }
        )
    }

    fn focus_handle(&self) -> Option<FocusHandle> {
        match self {
            ListItem::Unmeasured { focus_handle }
            | ListItem::Measured { focus_handle, .. }
            | ListItem::DirtyMeasured { focus_handle, .. } => focus_handle.clone(),
        }
    }

    fn contains_focused(&self, window: &Window, cx: &App) -> bool {
        match self {
            ListItem::Unmeasured { focus_handle }
            | ListItem::Measured { focus_handle, .. }
            | ListItem::DirtyMeasured { focus_handle, .. } => focus_handle
                .as_ref()
                .is_some_and(|handle| handle.contains_focused(window, cx)),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ListItemSummary {
    count: usize,
    rendered_count: usize,
    unrendered_count: usize,
    height: Pixels,
    has_focus_handles: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Count(usize);

#[derive(Clone, Debug, Default)]
struct Height(Pixels);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ListOffset {
    /// The index of an item in the list
    pub item_ix: usize,
    /// The number of pixels to offset from the item index.
    pub offset_in_item: Pixels,
}

impl ListScrollPosition {
    fn normalized(self) -> Self {
        match self {
            ListScrollPosition::VirtualTail {
                offset_from_content_end,
            } => ListScrollPosition::VirtualTail {
                offset_from_content_end: offset_from_content_end.max(px(0.)),
            },
            position => position,
        }
    }

    fn limit_virtual_tail(self, allowance: Pixels) -> Self {
        match self {
            ListScrollPosition::VirtualTail {
                offset_from_content_end,
            } => ListScrollPosition::VirtualTail {
                offset_from_content_end: offset_from_content_end.min(allowance.max(px(0.))),
            },
            position => position,
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn set_measured_item_heights(state: &ListState, heights: &[Pixels]) {
        state.0.borrow_mut().items = SumTree::from_iter(
            heights.iter().map(|height| ListItem::Measured {
                size: size(px(1.0), *height),
                focus_handle: None,
            }),
            (),
        );
    }

    pub(crate) fn set_viewport_height(state: &ListState, height: Pixels) {
        let mut inner = state.0.borrow_mut();
        inner.last_layout_bounds = Some(Bounds::new(
            point(px(0.0), px(0.0)),
            size(px(100.0), height),
        ));
        inner.last_padding = Some(Edges::default());
    }

    pub(crate) fn visible_range(state: &ListState) -> Range<usize> {
        state.visible_range()
    }

    pub(crate) fn presentation_range(state: &ListState) -> Range<usize> {
        state.presentation_range()
    }

    pub(crate) fn visible_virtual_trailing_height(state: &ListState) -> Pixels {
        let inner = state.0.borrow();
        let bounds = inner.last_layout_bounds.unwrap_or_default();
        let padding = inner.last_padding.unwrap_or_default();
        let scroll_top = inner.logical_scroll_top();
        inner.visible_virtual_trailing_height(bounds.size.height, &padding, &scroll_top)
    }

    pub(crate) fn invalidate_item_measurement(state: &ListState, ix: usize) {
        state.invalidate_item_measurement(ix);
    }

    pub(crate) fn apply_item_height_change_to_content_anchor(
        state: &ListState,
        item_ix: usize,
        new_height: Pixels,
    ) -> Option<ListOffset> {
        let mut inner = state.0.borrow_mut();
        let old_height = inner
            .items
            .iter()
            .nth(item_ix)
            .and_then(|item| item.size())
            .map(|size| size.height)?;
        let heights = inner
            .items
            .iter()
            .enumerate()
            .map(|(ix, item)| {
                if ix == item_ix {
                    new_height
                } else {
                    item.size().map(|size| size.height).unwrap_or_default()
                }
            })
            .collect::<Vec<_>>();
        inner.items = SumTree::from_iter(
            heights.into_iter().map(|height| ListItem::Measured {
                size: size(px(1.0), height),
                focus_handle: None,
            }),
            (),
        );
        inner.adjust_content_scroll_for_item_height_change(item_ix, old_height, new_height)
    }
}
