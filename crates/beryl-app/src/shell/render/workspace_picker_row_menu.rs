use std::time::Instant;

use gpui::{
    AnyElement, AnyView, App, Context, KeyDownEvent, KeyUpEvent, MouseButton, MouseDownEvent,
    MouseUpEvent, Render, StatefulInteractiveElement, Window, anchored, div, prelude::*, px,
    relative,
};

use crate::shell::{LoadedWorkspaceState, ShellView, layout};

use super::common::{disabled_secondary_button, secondary_button};

#[derive(Clone)]
struct WorkspaceRenameDisabledTooltip {
    message: &'static str,
    background: gpui::Rgba,
    border: gpui::Rgba,
    foreground: gpui::Rgba,
}

pub(super) fn render_workspace_row_action_trigger(
    shell: &ShellView,
    workspace_index: usize,
    workspace_id: beryl_model::workspace::BerylWorkspaceId,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let secondary = shell.secondary_button_theme();
    let open_workspace_id = workspace_id.clone();

    div()
        .id(("workspace-picker-row-action-menu-trigger", workspace_index))
        .h(px(layout::BUTTON_OUTER_HEIGHT))
        .w(px(layout::BUTTON_OUTER_HEIGHT))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(secondary.normal.background)
        .border_1()
        .border_color(secondary.normal.border)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
        .text_color(secondary.normal.foreground)
        .cursor_pointer()
        .hover(move |style| {
            style
                .bg(secondary.hover.background)
                .border_color(secondary.hover.border)
                .text_color(secondary.hover.foreground)
        })
        .active(move |style| {
            style
                .bg(secondary.active.background)
                .border_color(secondary.active.border)
                .text_color(secondary.active.foreground)
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, event, window, cx| {
                view.open_workspace_row_action_menu(open_workspace_id.clone(), event, window, cx);
            }),
        )
        .on_click(|_, _, cx| cx.stop_propagation())
        .child("...")
}

pub(super) fn render_workspace_row_action_menu(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    let menu = loaded.workspace_picker.row_action_menu_active()?;
    let workspace_id = menu.workspace_id().clone();
    let position = menu.position();
    let (workspace_index, _) = loaded
        .known_workspaces
        .iter()
        .enumerate()
        .find(|(_, workspace)| workspace.id() == &workspace_id)?;
    let entity = cx.entity();
    let delete_progress = loaded
        .workspace_picker
        .delete_hold_progress_for_target(&workspace_id, Instant::now());

    let mut content = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(shell, "Workspace"));

    content = content.child(
        if let Some(reason) = shell.workspace_rename_disabled_reason() {
            render_disabled_workspace_rename_action(
                shell,
                ("workspace-picker-row-menu-rename", workspace_index),
                reason,
            )
            .into_any_element()
        } else {
            secondary_button(
                shell,
                ("workspace-picker-row-menu-rename", workspace_index),
                "Rename",
                cx.listener(ShellView::begin_workspace_rename),
            )
            .w_full()
            .justify_start()
            .into_any_element()
        },
    );

    content = content.child(workspace_delete_hold_row(
        shell,
        delete_progress,
        cx.listener(ShellView::begin_workspace_delete_hold_from_action_menu),
        cx.listener(ShellView::cancel_workspace_delete_hold_from_action_menu),
        cx.listener(ShellView::cancel_workspace_delete_hold_from_action_menu),
        cx.listener(ShellView::cancel_workspace_delete_hold_on_hover_change),
        cx.listener(ShellView::begin_workspace_delete_keyboard_hold_from_action_menu),
        cx.listener(ShellView::cancel_workspace_delete_keyboard_hold_from_action_menu),
    ));

    Some(
        anchored()
            .position(position)
            .snap_to_window_with_margin(px(8.0))
            .child(
                div()
                    .on_children_prepainted(move |children, _, cx| {
                        let bounds = children.first().copied();
                        entity.update(cx, |view, cx| {
                            view.record_workspace_row_action_menu_bounds(bounds, cx);
                        });
                    })
                    .child(
                        div()
                            .id("workspace-picker-row-action-menu-panel")
                            .w(px(220.0))
                            .occlude()
                            .rounded_lg()
                            .border_1()
                            .border_color(shell.surface_border())
                            .bg(shell.popup_surface_background())
                            .shadow_lg()
                            .p_2()
                            .child(content),
                    ),
            )
            .into_any_element(),
    )
}

fn workspace_delete_hold_row(
    shell: &ShellView,
    progress: Option<f32>,
    on_mouse_down: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    on_mouse_up: impl Fn(&MouseUpEvent, &mut Window, &mut gpui::App) + 'static,
    on_mouse_up_out: impl Fn(&MouseUpEvent, &mut Window, &mut gpui::App) + 'static,
    on_hover: impl Fn(&bool, &mut Window, &mut gpui::App) + 'static,
    on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut gpui::App) + 'static,
    on_key_up: impl Fn(&KeyUpEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let secondary = shell.secondary_button_theme();
    let progress = progress.unwrap_or(0.0).clamp(0.0, 1.0);

    div()
        .id("workspace-picker-row-menu-delete")
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
                .bg(secondary.active.background),
        )
        .child(
            div()
                .relative()
                .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
                .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
                .text_color(secondary.normal.foreground)
                .child("Delete"),
        )
        .on_mouse_down(MouseButton::Left, on_mouse_down)
        .on_mouse_up(MouseButton::Left, on_mouse_up)
        .on_mouse_up_out(MouseButton::Left, on_mouse_up_out)
        .on_hover(on_hover)
        .on_key_down(on_key_down)
        .on_key_up(on_key_up)
}

fn render_disabled_workspace_rename_action(
    shell: &ShellView,
    id: impl Into<gpui::ElementId>,
    reason: &'static str,
) -> impl IntoElement {
    let tooltip = WorkspaceRenameDisabledTooltip {
        message: reason,
        background: shell.popup_surface_background(),
        border: shell.surface_border(),
        foreground: shell.general_ui_foreground(),
    };
    disabled_secondary_button(shell, id, "Rename")
        .w_full()
        .justify_start()
        .tooltip(move |_, cx| build_workspace_rename_disabled_tooltip(tooltip.clone(), cx))
}

fn build_workspace_rename_disabled_tooltip(
    tooltip: WorkspaceRenameDisabledTooltip,
    cx: &mut App,
) -> AnyView {
    cx.new(|_| tooltip).into()
}

fn menu_header(shell: &ShellView, label: &str) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(shell.general_ui_foreground())
        .child(label.to_string())
}

impl Render for WorkspaceRenameDisabledTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(260.0))
            .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
            .bg(self.background)
            .border_1()
            .border_color(self.border)
            .px_3()
            .py_2()
            .text_xs()
            .text_color(self.foreground)
            .child(self.message)
    }
}
