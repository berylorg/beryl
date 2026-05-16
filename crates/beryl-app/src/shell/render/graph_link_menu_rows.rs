use gpui::{
    Context, InteractiveElement, KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent,
    MouseUpEvent, StatefulInteractiveElement, Window, div, prelude::*, px, relative, rgba,
};

use crate::shell::{ShellView, layout};

use super::common::{disabled_secondary_button, secondary_button};

pub(super) fn action_row(
    shell: &ShellView,
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    secondary_button(shell, id, label, on_click)
}

pub(super) fn delete_leaf_row(
    shell: &ShellView,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    secondary_button(shell, "graph-node-delete-row", "Delete", on_click)
        .tab_index(0)
        .on_key_down(on_key_down)
}

pub(super) fn delete_recursive_hold_row(
    shell: &ShellView,
    progress: Option<f32>,
    in_flight: bool,
    on_mouse_down: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    on_mouse_up: impl Fn(&MouseUpEvent, &mut Window, &mut gpui::App) + 'static,
    on_mouse_up_out: impl Fn(&MouseUpEvent, &mut Window, &mut gpui::App) + 'static,
    on_hover: impl Fn(&bool, &mut Window, &mut gpui::App) + 'static,
    on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut gpui::App) + 'static,
    on_key_up: impl Fn(&KeyUpEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let secondary = shell.secondary_button_theme();
    let progress = if in_flight {
        1.0
    } else {
        progress.unwrap_or(0.0).clamp(0.0, 1.0)
    };
    let label = if in_flight {
        "Deleting..."
    } else {
        "Delete Recursively"
    };
    div()
        .id("graph-node-delete-recursively-row")
        .relative()
        .overflow_hidden()
        .h(px(layout::BUTTON_OUTER_HEIGHT))
        .px(px(layout::BUTTON_HORIZONTAL_PADDING))
        .py(px(layout::BUTTON_VERTICAL_PADDING))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(secondary.normal.background)
        .border_1()
        .border_color(secondary.normal.border)
        .flex()
        .items_center()
        .cursor_pointer()
        .tab_index(0)
        .hover(move |style| style.bg(secondary.hover.background))
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(relative(progress))
                .bg(rgba(0xdc26264d)),
        )
        .child(
            div()
                .relative()
                .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
                .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
                .font_weight(secondary.font_weight)
                .text_color(secondary.normal.foreground)
                .child(label),
        )
        .on_mouse_down(MouseButton::Left, on_mouse_down)
        .on_mouse_up(MouseButton::Left, on_mouse_up)
        .on_mouse_up_out(MouseButton::Left, on_mouse_up_out)
        .on_hover(on_hover)
        .on_key_down(on_key_down)
        .on_key_up(on_key_up)
}

pub(super) fn actions_back_row(shell: &ShellView, cx: &mut Context<ShellView>) -> impl IntoElement {
    action_row(
        shell,
        "graph-node-action-back-row",
        "Back to actions",
        cx.listener(ShellView::show_graph_node_action_menu),
    )
}

pub(super) fn back_row(shell: &ShellView, cx: &mut Context<ShellView>) -> impl IntoElement {
    secondary_button(
        shell,
        "graph-thread-link-back-row",
        "Back to members",
        cx.listener(ShellView::show_graph_thread_link_members),
    )
}

pub(super) fn menu_header(shell: &ShellView, label: &str) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(shell.general_ui_foreground())
        .child(label.to_string())
}

pub(super) fn status_row(shell: &ShellView, message: &str) -> impl IntoElement {
    div()
        .rounded_md()
        .px_2()
        .py_2()
        .text_xs()
        .text_color(shell.surface_muted_foreground())
        .child(message.to_string())
}

pub(super) fn disabled_menu_row(shell: &ShellView, label: &str) -> impl IntoElement {
    div()
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .px_2()
        .py_2()
        .text_sm()
        .text_color(shell.surface_muted_foreground())
        .child(label.to_string())
}

pub(super) fn disabled_action_row(
    shell: &ShellView,
    id: &'static str,
    label: &'static str,
) -> impl IntoElement {
    disabled_secondary_button(shell, id, label)
}
