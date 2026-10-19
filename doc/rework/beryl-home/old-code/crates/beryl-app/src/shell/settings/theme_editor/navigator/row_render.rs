use gpui::{
    AnyElement, FontWeight, InteractiveElement, MouseButton, ParentElement, SharedString,
    StatefulInteractiveElement, div, prelude::*, px,
};
use gpui_settings_window::SettingsWindowTheme;

use super::super::{ThemeRoleNavigatorProjection, ThemeRoleNavigatorRow};
use super::{
    SelectRole,
    chrome::{element_id_suffix, theme_color},
    layout::NAVIGATOR_ROW_HEIGHT,
};

pub(super) fn render_row(
    projection: &ThemeRoleNavigatorProjection,
    row: &ThemeRoleNavigatorRow,
    column_index: usize,
    row_index: usize,
    theme: &SettingsWindowTheme,
    select_role: SelectRole,
) -> AnyElement {
    let selected = row.role_id() == projection.selected_role_id();
    let button_theme = &theme.navigation_button;
    let state = if selected {
        &button_theme.active
    } else {
        &button_theme.normal
    };
    let hover = button_theme.hover.clone();
    let pressed = button_theme.active.clone();
    let preview = row.preview_style();
    let foreground = preview
        .and_then(|style| style.foreground())
        .unwrap_or(state.foreground);
    let background = preview
        .and_then(|style| style.background())
        .unwrap_or(state.background);
    let border = if selected {
        state.border
    } else {
        preview
            .and_then(|style| style.border())
            .unwrap_or(state.border)
    };
    let font_size = preview
        .and_then(|style| style.font_size())
        .map(f32::from)
        .unwrap_or(13.0);
    let font_weight = preview
        .and_then(|style| style.font_weight())
        .unwrap_or(button_theme.font_weight);
    let font_family = preview
        .and_then(|style| style.font_family())
        .map(|font_family| SharedString::from(font_family.to_owned()));
    let child_count = row.child_role_ids().len();
    let property_count = row.property_row_count();
    let role_id = row.role_id().clone();

    div()
        .id(SharedString::from(format!(
            "theme-role-navigator-row-{}-{}-{}",
            column_index,
            row_index,
            element_id_suffix(role_id.as_str())
        )))
        .flex_none()
        .w_full()
        .h(px(NAVIGATOR_ROW_HEIGHT))
        .min_h(px(NAVIGATOR_ROW_HEIGHT))
        .overflow_hidden()
        .px_2()
        .border_1()
        .border_color(theme_color(border))
        .bg(theme_color(background))
        .text_color(theme_color(foreground))
        .text_size(px(font_size))
        .font_weight(FontWeight(font_weight as f32))
        .hover(move |style| {
            style
                .border_color(theme_color(hover.border))
                .bg(theme_color(hover.background))
                .text_color(theme_color(hover.foreground))
        })
        .active(move |style| {
            style
                .border_color(theme_color(pressed.border))
                .bg(theme_color(pressed.background))
                .text_color(theme_color(pressed.foreground))
        })
        .cursor_pointer()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .when_some(font_family, |element, family| element.font_family(family))
                .child(row.label().to_owned()),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(11.0))
                .text_color(theme_color(theme.row.muted_foreground))
                .child(format!("{property_count}p")),
        )
        .children((child_count > 0).then(|| {
            div()
                .flex_none()
                .text_size(px(12.0))
                .text_color(theme_color(theme.row.muted_foreground))
                .child(">")
        }))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            select_role(role_id.clone(), cx);
        })
        .into_any_element()
}
