use gpui::{AnyElement, ElementId, Pixels, ScrollHandle};
use gpui_scrollbar::{ScrollbarStyle, render_scroll_handle_scrollbar};

use crate::shell::ShellRenderStyleSnapshot;

#[allow(unused_imports)]
pub(super) use gpui_scrollbar::{
    Axis as ScrollbarAxis, ScrollDirection, ScrollbarInteraction, ScrollbarOwnerUpdateCallback,
    ScrollbarScrollState, ScrollbarVisibilityPolicy, ScrollbarVisibilityState,
    ScrollbarVisibilityUpdateCallback,
};

fn beryl_scrollbar_style() -> ScrollbarStyle {
    ScrollbarStyle::default()
}

fn themed_beryl_scrollbar_style(style: &ShellRenderStyleSnapshot) -> ScrollbarStyle {
    ScrollbarStyle {
        thumb_color: style.scrollbar_thumb_color(),
        ..ScrollbarStyle::default()
    }
}

pub(super) fn render_themed_div_scrollbar(
    style: &ShellRenderStyleSnapshot,
    id: impl Into<ElementId>,
    scroll_handle: &ScrollHandle,
    axis: ScrollbarAxis,
    visibility: ScrollbarVisibilityPolicy,
) -> Option<AnyElement> {
    render_scroll_handle_scrollbar(
        id,
        scroll_handle,
        axis,
        themed_beryl_scrollbar_style(style),
        visibility,
    )
}

pub(super) fn render_div_scrollbar_with_owner_update(
    id: impl Into<ElementId>,
    scroll_handle: &ScrollHandle,
    axis: ScrollbarAxis,
    visibility: ScrollbarVisibilityPolicy,
    on_owner_update: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
) -> Option<AnyElement> {
    let interaction = ScrollbarInteraction::for_scroll_handle_with_owner_update(
        scroll_handle.clone(),
        axis,
        on_owner_update,
    );
    gpui_scrollbar::render_scrollbar(id, axis, beryl_scrollbar_style(), visibility, interaction)
}

pub(super) fn render_interactive_vertical_scrollbar(
    id: impl Into<ElementId>,
    _viewport_length: Pixels,
    _overflow_length: Pixels,
    _scroll_offset: Pixels,
    visibility: ScrollbarVisibilityPolicy,
    interaction: ScrollbarInteraction,
) -> Option<AnyElement> {
    gpui_scrollbar::render_scrollbar(
        id,
        ScrollbarAxis::Vertical,
        beryl_scrollbar_style(),
        visibility,
        interaction,
    )
}
