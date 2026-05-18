use std::rc::Rc;

use beryl_model::semantic_graph::ChecklistItemStatus;
use gpui::{
    AnyElement, App, Context, DispatchPhase, Entity, KeyDownEvent, MouseButton, MouseDownEvent,
    Render, ScrollHandle, Window, anchored, canvas, div, point, prelude::*, px,
};

use crate::{
    BerylThemeRole,
    shell::{
        ConversationSurfaceState, LoadedWorkspaceState, ShellRenderFrame, ShellRenderStyleSnapshot,
        ShellView,
        checklist_sidebar_panel_state::{
            ChecklistSidebarPanelState, ChecklistSidebarProjectionSync,
        },
        checklist_sidebar_projection::{ChecklistSidebarProjection, ChecklistSidebarRow},
        layout,
    },
};

use super::{
    common::{disabled_secondary_button, secondary_button},
    scrollbars::{
        ScrollbarAxis, ScrollbarVisibilityPolicy, ScrollbarVisibilityState,
        ScrollbarVisibilityUpdateCallback, render_themed_div_scrollbar,
    },
};

pub(crate) struct ChecklistSidebarPanel {
    shell: Entity<ShellView>,
    scroll_handle: ScrollHandle,
    projection_state: ChecklistSidebarPanelState,
    scrollbar_visibility: ScrollbarVisibilityState,
}

#[derive(Clone)]
struct ChecklistSidebarSnapshot {
    projection: Option<ChecklistSidebarProjection>,
    viewport_height_hint: gpui::Pixels,
    style: ShellRenderStyleSnapshot,
    theme: ChecklistSidebarTheme,
}

#[derive(Clone, Copy)]
struct ChecklistRoleStyle {
    background: gpui::Rgba,
    border: gpui::Rgba,
    foreground: gpui::Rgba,
    font_weight: gpui::FontWeight,
}

#[derive(Clone, Copy)]
struct ChecklistSidebarTheme {
    sidebar: ChecklistRoleStyle,
    header: ChecklistRoleStyle,
    row: ChecklistRoleStyle,
    row_hover: ChecklistRoleStyle,
    row_number_text: ChecklistRoleStyle,
    row_text: ChecklistRoleStyle,
    row_disabled: ChecklistRoleStyle,
    status_todo_text: ChecklistRoleStyle,
    status_in_progress_text: ChecklistRoleStyle,
    status_done_text: ChecklistRoleStyle,
    popup: ChecklistRoleStyle,
}

impl ChecklistSidebarTheme {
    fn from_style(style: &ShellRenderStyleSnapshot) -> Self {
        Self {
            sidebar: checklist_role_style(style, BerylThemeRole::ChecklistSidebar),
            header: checklist_role_style(style, BerylThemeRole::ChecklistHeader),
            row: checklist_role_style(style, BerylThemeRole::ChecklistRow),
            row_hover: checklist_role_style(style, BerylThemeRole::SurfaceRowHover),
            row_number_text: checklist_role_style(style, BerylThemeRole::ChecklistRowNumberText),
            row_text: checklist_role_style(style, BerylThemeRole::ChecklistRowText),
            row_disabled: checklist_role_style(style, BerylThemeRole::SurfaceRowDisabled),
            status_todo_text: checklist_role_style(style, BerylThemeRole::ChecklistStatusTodoText),
            status_in_progress_text: checklist_role_style(
                style,
                BerylThemeRole::ChecklistStatusInProgressText,
            ),
            status_done_text: checklist_role_style(style, BerylThemeRole::ChecklistStatusDoneText),
            popup: checklist_role_style(style, BerylThemeRole::PopupSurface),
        }
    }

    fn status(&self, status: Option<ChecklistItemStatus>) -> ChecklistRoleStyle {
        match status.unwrap_or(ChecklistItemStatus::Todo) {
            ChecklistItemStatus::Todo => self.status_todo_text,
            ChecklistItemStatus::InProgress => self.status_in_progress_text,
            ChecklistItemStatus::Done => self.status_done_text,
        }
    }
}

fn checklist_role_style(
    style: &ShellRenderStyleSnapshot,
    role: BerylThemeRole,
) -> ChecklistRoleStyle {
    ChecklistRoleStyle {
        background: style.role_background(role, style.panel_surface_background()),
        border: style.role_border(role, style.surface_border()),
        foreground: style.role_foreground(role, style.surface_foreground()),
        font_weight: style.role_font_weight(role, gpui::FontWeight::SEMIBOLD),
    }
}

impl ChecklistSidebarPanel {
    pub(crate) fn new(shell: Entity<ShellView>, _: &mut Context<Self>) -> Self {
        Self {
            shell,
            scroll_handle: ScrollHandle::new(),
            projection_state: ChecklistSidebarPanelState::default(),
            scrollbar_visibility: ScrollbarVisibilityState::default(),
        }
    }

    fn scrollbar_update_callback(entity: Entity<Self>) -> ScrollbarVisibilityUpdateCallback {
        Rc::new(move |_: &mut Window, cx: &mut App| {
            entity.update(cx, |_, cx| {
                cx.notify();
            });
        })
    }

    fn note_scrollbar_activity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let on_update = Self::scrollbar_update_callback(cx.entity());
        self.scrollbar_visibility
            .record_viewport_activity(window, cx, on_update);
        cx.notify();
    }

    fn note_scrollbar_motion(
        &mut self,
        _: &gpui::MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(window, cx);
    }

    fn note_scrollbar_scroll(
        &mut self,
        _: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_scrollbar_activity(window, cx);
    }

    fn snapshot(&self, cx: &mut Context<Self>) -> ChecklistSidebarSnapshot {
        let shell = self.shell.read(cx);
        let projection = shell
            .conversation_surface()
            .and_then(ConversationSurfaceState::checklist_sidebar_projection)
            .cloned();
        let viewport_height_hint = shell.conversation_surface().map_or_else(
            || px(layout::WINDOW_MIN_HEIGHT - 96.0),
            |surface| surface.checklist_sidebar_viewport_height_hint(),
        );
        let style = shell.render_style_snapshot();
        let theme = ChecklistSidebarTheme::from_style(&style);

        ChecklistSidebarSnapshot {
            projection,
            viewport_height_hint,
            style,
            theme,
        }
    }

    fn sync_projection_scroll(&mut self, projection: Option<&ChecklistSidebarProjection>) {
        let row_count = projection.map_or(0, ChecklistSidebarProjection::row_count);
        let checklist_id = projection.map(ChecklistSidebarProjection::checklist_id);
        match self
            .projection_state
            .sync_projection(checklist_id, row_count)
        {
            ChecklistSidebarProjectionSync::Unchanged => {}
            ChecklistSidebarProjectionSync::ResetScroll => {
                self.scroll_handle.set_offset(point(px(0.0), px(0.0)));
            }
            ChecklistSidebarProjectionSync::ClampScroll => {
                let max_offset = self.scroll_handle.max_offset().height;
                let current_offset = self.scroll_handle.offset();
                if -current_offset.y > max_offset {
                    self.scroll_handle
                        .set_offset(point(current_offset.x, -max_offset));
                }
            }
        }
    }
}

impl Render for ChecklistSidebarPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.snapshot(cx);
        self.sync_projection_scroll(snapshot.projection.as_ref());

        render_checklist_sidebar(
            snapshot,
            self.shell.clone(),
            self.scroll_handle.clone(),
            self.scrollbar_visibility
                .managed(Self::scrollbar_update_callback(cx.entity())),
            cx,
        )
        .into_any_element()
    }
}

fn render_checklist_sidebar(
    snapshot: ChecklistSidebarSnapshot,
    shell_entity: Entity<ShellView>,
    scroll_handle: ScrollHandle,
    scrollbar_visibility: ScrollbarVisibilityPolicy,
    cx: &mut Context<ChecklistSidebarPanel>,
) -> impl IntoElement {
    let projection = snapshot.projection.as_ref();
    let style = snapshot.style.clone();
    let theme = snapshot.theme;
    let mut panel = div().size_full().min_h(px(0.0)).flex().flex_col().child(
        div()
            .w_full()
            .px_4()
            .pt_4()
            .pb_2()
            .child(render_sidebar_title(theme, "Checklist")),
    );

    if let Some(projection) = projection {
        panel = panel.child(render_checklist_title(theme, projection.title()));
    }

    let body = match projection {
        Some(projection) => render_checklist_items(
            projection,
            shell_entity.clone(),
            scroll_handle.clone(),
            theme,
            snapshot.viewport_height_hint,
            cx,
        )
        .into_any_element(),
        None => empty_state(theme).into_any_element(),
    };
    let mut scroll_region = div()
        .relative()
        .flex_1()
        .min_h(px(0.0))
        .on_mouse_move(cx.listener(ChecklistSidebarPanel::note_scrollbar_motion))
        .on_scroll_wheel(cx.listener(ChecklistSidebarPanel::note_scrollbar_scroll))
        .child(
            div()
                .id("checklist-sidebar-scroll")
                .track_scroll(&scroll_handle)
                .overflow_x_hidden()
                .overflow_y_scroll()
                .size_full()
                .px_4()
                .pb_4()
                .child(body),
        );
    if let Some(scrollbar) = render_themed_div_scrollbar(
        &style,
        "checklist-sidebar-scrollbar",
        &scroll_handle,
        ScrollbarAxis::Vertical,
        scrollbar_visibility,
    ) {
        scroll_region = scroll_region.child(scrollbar);
    }

    checklist_panel_shell(theme.sidebar, panel.child(scroll_region))
}

fn checklist_panel_shell(
    chrome: ChecklistRoleStyle,
    content: impl IntoElement,
) -> impl IntoElement {
    div()
        .size_full()
        .min_h(px(0.0))
        .bg(chrome.background)
        .border_1()
        .border_color(chrome.border)
        .text_color(chrome.foreground)
        .overflow_hidden()
        .child(content)
}

pub(super) fn render_checklist_thread_start_menu_listeners(
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let entity = cx.entity();

    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            window.on_mouse_event({
                let entity = entity.clone();
                move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }

                    entity.update(cx, |view, cx| {
                        view.handle_checklist_thread_start_menu_mouse_down(event, window, cx);
                    });
                }
            });
            window.on_key_event({
                let entity = entity.clone();
                move |event: &KeyDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }

                    let handled = entity.update(cx, |view, cx| {
                        view.handle_checklist_thread_start_menu_key_down(event, window, cx)
                    });
                    if handled {
                        cx.stop_propagation();
                    }
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

pub(super) fn render_checklist_thread_start_menu(
    shell: &ShellRenderFrame<'_>,
    loaded: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
    new_thread_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    let menu = surface.checklist_thread_start_menu().active()?;
    let entity = cx.entity();
    let theme = ChecklistSidebarTheme::from_style(shell.style());
    let content = if let Some(message) = new_thread_controls_disabled {
        disabled_menu_content(shell, theme, message).into_any_element()
    } else if loaded.selected_runtime().is_some() {
        render_start_thread_menu_content(shell, theme, cx).into_any_element()
    } else {
        disabled_menu_content(
            shell,
            theme,
            "Select a workspace runtime environment before starting a thread.",
        )
        .into_any_element()
    };

    Some(
        anchored()
            .position(menu.position())
            .snap_to_window_with_margin(px(8.0))
            .child(
                div()
                    .on_children_prepainted(move |children, _, cx| {
                        let bounds = children.first().copied();
                        entity.update(cx, |view, cx| {
                            view.record_checklist_thread_start_menu_bounds(bounds, cx);
                        });
                    })
                    .child(
                        div()
                            .id("checklist-thread-start-menu-panel")
                            .w(px(260.0))
                            .occlude()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.popup.border)
                            .bg(theme.popup.background)
                            .shadow_lg()
                            .p_2()
                            .child(content),
                    ),
            )
            .into_any_element(),
    )
}

fn render_checklist_items(
    projection: &ChecklistSidebarProjection,
    shell: Entity<ShellView>,
    scroll_handle: ScrollHandle,
    theme: ChecklistSidebarTheme,
    viewport_height_hint: gpui::Pixels,
    cx: &mut Context<ChecklistSidebarPanel>,
) -> impl IntoElement {
    let row_count = projection.row_count();
    if row_count == 0 {
        return div()
            .w_full()
            .min_w(px(0.0))
            .child(empty_message(theme, "No checklist items."));
    }

    let viewport_height = scroll_handle.bounds().size.height;
    let viewport_height = if viewport_height > px(0.0) {
        viewport_height
    } else {
        viewport_height_hint
    };
    let row_window = layout::checklist_sidebar_row_window(
        row_count,
        viewport_height,
        -scroll_handle.offset().y,
        layout::CHECKLIST_SIDEBAR_OVERSCAN_ROWS,
    );
    let mut list = div()
        .w_full()
        .h(row_window.content_height)
        .min_h(row_window.content_height)
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .h(row_window.top_spacer_height)
                .min_h(row_window.top_spacer_height),
        );

    let visible_rows = shell
        .read(cx)
        .conversation_surface()
        .map(|surface| {
            row_window
                .range
                .clone()
                .filter_map(|index| surface.checklist_sidebar_row(index))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for row in visible_rows {
        list = list.child(render_checklist_item_row(row, shell.clone(), theme));
    }

    list.child(
        div()
            .w_full()
            .h(row_window.bottom_spacer_height)
            .min_h(row_window.bottom_spacer_height),
    )
}

fn render_checklist_item_row(
    row: ChecklistSidebarRow,
    entity: gpui::Entity<ShellView>,
    theme: ChecklistSidebarTheme,
) -> impl IntoElement {
    let item_node_id = row.node_id.clone();
    let status = theme.status(row.status);
    div()
        .id(gpui::ElementId::Name(row.element_key().into()))
        .h(px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT))
        .min_h(px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT))
        .w_full()
        .min_w(px(0.0))
        .rounded_md()
        .border_1()
        .border_color(theme.row.border)
        .bg(theme.row.background)
        .px_3()
        .py_2()
        .cursor_pointer()
        .hover(move |style| style.bg(theme.row_hover.background))
        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
            entity.update(cx, |view, cx| {
                view.open_checklist_item_thread_start_menu(item_node_id.clone(), event, window, cx);
            });
        })
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .text_sm()
                        .font_weight(theme.row_number_text.font_weight)
                        .text_color(theme.row_number_text.foreground)
                        .child(format!("{}.", row.number)),
                )
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .font_weight(status.font_weight)
                        .text_color(status.foreground)
                        .child(row.status_label),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .text_sm()
                        .overflow_hidden()
                        .whitespace_normal()
                        .font_weight(theme.row_text.font_weight)
                        .text_color(theme.row_text.foreground)
                        .child(row.title),
                ),
        )
}

fn render_start_thread_menu_content(
    shell: &ShellRenderFrame<'_>,
    theme: ChecklistSidebarTheme,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(theme, "Checklist item"))
        .child(secondary_button(
            shell,
            "checklist-start-thread-row",
            "Start New Codex Thread",
            cx.listener(ShellView::start_checklist_item_thread_from_menu),
        ))
}

fn disabled_menu_content(
    shell: &ShellRenderFrame<'_>,
    theme: ChecklistSidebarTheme,
    message: impl Into<String>,
) -> impl IntoElement {
    let message = message.into();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(theme, "Checklist item"))
        .child(disabled_secondary_button(
            shell,
            "checklist-start-thread-row-disabled",
            "Start New Codex Thread",
        ))
        .child(
            div()
                .px_2()
                .pb_1()
                .text_xs()
                .text_color(theme.row_disabled.foreground)
                .child(message),
        )
}

fn render_sidebar_title(theme: ChecklistSidebarTheme, title: &str) -> impl IntoElement {
    div().flex().items_center().child(
        div()
            .text_sm()
            .font_weight(theme.header.font_weight)
            .text_color(theme.header.foreground)
            .child(title.to_string()),
    )
}

fn render_checklist_title(theme: ChecklistSidebarTheme, title: &str) -> impl IntoElement {
    div().w_full().px_4().pb_3().min_w(px(0.0)).child(
        div()
            .min_w(px(0.0))
            .text_sm()
            .font_weight(theme.header.font_weight)
            .whitespace_normal()
            .text_color(theme.header.foreground)
            .child(title.to_string()),
    )
}

fn menu_header(theme: ChecklistSidebarTheme, label: &str) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .text_xs()
        .font_weight(theme.header.font_weight)
        .text_color(theme.header.foreground)
        .child(label.to_string())
}

fn empty_state(theme: ChecklistSidebarTheme) -> impl IntoElement {
    div()
        .rounded_md()
        .bg(theme.row.background)
        .border_1()
        .border_color(theme.row.border)
        .p_3()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_sm()
                .text_color(theme.row.foreground)
                .child("No checklist is selected."),
        )
}

fn empty_message(theme: ChecklistSidebarTheme, message: &'static str) -> impl IntoElement {
    div()
        .rounded_md()
        .border_1()
        .border_color(theme.row_disabled.border)
        .bg(theme.row_disabled.background)
        .p_3()
        .text_sm()
        .text_color(theme.row_disabled.foreground)
        .child(message)
}
