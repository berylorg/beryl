use std::rc::Rc;

use gpui::{AnyElement, ElementId, rgb};
use gpui_scrollbar::{
    Axis as ScrollbarAxis, ScrollbarStyle, ScrollbarVisibilityPolicy,
    ScrollbarVisibilityUpdateCallback, render_scroll_handle_scrollbar,
};
use gpui_settings_window::{RgbColor, SettingsWindowTheme};

pub(super) fn theme_color(color: RgbColor) -> gpui::Rgba {
    rgb(packed_rgb(color))
}

pub(super) fn render_navigator_scrollbar(
    id: impl Into<ElementId>,
    scroll_handle: &gpui::ScrollHandle,
    axis: ScrollbarAxis,
    theme: &SettingsWindowTheme,
    visibility: ScrollbarVisibilityPolicy,
) -> Option<AnyElement> {
    let style = ScrollbarStyle {
        thumb_color: packed_rgb(theme.panel.muted_foreground),
        ..ScrollbarStyle::default()
    };
    render_scroll_handle_scrollbar(id, scroll_handle, axis, style, visibility)
}

pub(super) fn navigator_scrollbar_update_callback() -> ScrollbarVisibilityUpdateCallback {
    Rc::new(|window, _| {
        window.refresh();
    })
}

pub(super) fn element_id_suffix(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn packed_rgb(color: RgbColor) -> u32 {
    (u32::from(color.red()) << 16) | (u32::from(color.green()) << 8) | u32::from(color.blue())
}
