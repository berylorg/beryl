mod scrollbar;

use beryl_model::semantic_graph::ChecklistItemStatus;
use gpui::{
    AnyElement, Context, DispatchPhase, Entity, KeyDownEvent, MouseButton, MouseDownEvent, Render,
    ScrollHandle, Window, anchored, canvas, div, point, prelude::*, px, rgb,
};

use crate::shell::{
    ConversationSurfaceState, LoadedWorkspaceState, ShellView,
    checklist_sidebar_panel_state::{ChecklistSidebarPanelState, ChecklistSidebarProjectionSync},
    checklist_sidebar_projection::{ChecklistSidebarProjection, ChecklistSidebarRow},
    layout,
};

use self::scrollbar::SidebarScrollbarActivity;
use super::{
    common::{disabled_secondary_button, secondary_button},
    scrollbars::{ScrollbarAxis, render_div_scrollbar},
};

pub(crate) struct ChecklistSidebarPanel {
    shell: Entity<ShellView>,
    scroll_handle: ScrollHandle,
    projection_state: ChecklistSidebarPanelState,
    scrollbar_activity: SidebarScrollbarActivity,
}

#[derive(Clone)]
struct ChecklistSidebarSnapshot {
    projection: Option<ChecklistSidebarProjection>,
    viewport_height_hint: gpui::Pixels,
    chrome: ChecklistSidebarChrome,
}

#[derive(Clone, Copy)]
struct ChecklistSidebarChrome {
    background: gpui::Rgba,
    border: gpui::Rgba,
    foreground: gpui::Rgba,
}

impl ChecklistSidebarPanel {
    pub(crate) fn new(shell: Entity<ShellView>, _: &mut Context<Self>) -> Self {
        Self {
            shell,
            scroll_handle: ScrollHandle::new(),
            projection_state: ChecklistSidebarPanelState::default(),
            scrollbar_activity: SidebarScrollbarActivity::default(),
        }
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
        let chrome = ChecklistSidebarChrome {
            background: shell.transcript_shell_background(),
            border: shell.separator_color(),
            foreground: shell.transcript_shell_foreground(),
        };

        ChecklistSidebarSnapshot {
            projection,
            viewport_height_hint,
            chrome,
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.snapshot(cx);
        self.sync_projection_scroll(snapshot.projection.as_ref());
        if self.scrollbar_animating() {
            window.request_animation_frame();
        }

        render_checklist_sidebar(
            snapshot,
            self.shell.clone(),
            self.scroll_handle.clone(),
            self.scrollbar_opacity(),
            cx,
        )
        .into_any_element()
    }
}

fn render_checklist_sidebar(
    snapshot: ChecklistSidebarSnapshot,
    shell_entity: Entity<ShellView>,
    scroll_handle: ScrollHandle,
    scrollbar_opacity: f32,
    cx: &mut Context<ChecklistSidebarPanel>,
) -> impl IntoElement {
    let projection = snapshot.projection.as_ref();
    let mut panel = div().size_full().min_h(px(0.0)).flex().flex_col().child(
        div()
            .w_full()
            .px_4()
            .pt_4()
            .pb_2()
            .child(render_sidebar_title("Checklist")),
    );

    if let Some(projection) = projection {
        panel = panel.child(render_checklist_title(projection.title()));
    }

    let body = match projection {
        Some(projection) => render_checklist_items(
            projection,
            shell_entity,
            scroll_handle.clone(),
            snapshot.viewport_height_hint,
            cx,
        )
        .into_any_element(),
        None => empty_state().into_any_element(),
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
    if let Some(scrollbar) =
        render_div_scrollbar(&scroll_handle, ScrollbarAxis::Vertical, scrollbar_opacity)
    {
        scroll_region = scroll_region.child(scrollbar);
    }

    checklist_panel_shell(snapshot.chrome, panel.child(scroll_region))
}

fn checklist_panel_shell(
    chrome: ChecklistSidebarChrome,
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
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    let menu = surface.checklist_thread_start_menu().active()?;
    let entity = cx.entity();
    let content = if loaded.selected_runtime().is_some() {
        render_start_thread_menu_content(shell, cx).into_any_element()
    } else {
        disabled_menu_content(
            shell,
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
                            .border_color(rgb(0x334155))
                            .bg(rgb(0x07111f))
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
    viewport_height_hint: gpui::Pixels,
    cx: &mut Context<ChecklistSidebarPanel>,
) -> impl IntoElement {
    let row_count = projection.row_count();
    if row_count == 0 {
        return div()
            .w_full()
            .min_w(px(0.0))
            .child(empty_message("No checklist items."));
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
        list = list.child(render_checklist_item_row(row, shell.clone()));
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
) -> impl IntoElement {
    let item_node_id = row.node_id.clone();
    div()
        .id(gpui::ElementId::Name(row.element_key().into()))
        .h(px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT))
        .min_h(px(layout::CHECKLIST_SIDEBAR_ROW_HEIGHT))
        .w_full()
        .min_w(px(0.0))
        .rounded_md()
        .border_1()
        .border_color(rgb(0x1f2937))
        .bg(rgb(0x111827))
        .px_3()
        .py_2()
        .cursor_pointer()
        .hover(|style| style.bg(rgb(0x1e293b)))
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
                        .text_color(rgb(0x94a3b8))
                        .child(format!("{}.", row.number)),
                )
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(status_color(row.status))
                        .child(row.status_label),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .text_sm()
                        .overflow_hidden()
                        .whitespace_normal()
                        .text_color(rgb(0xe2e8f0))
                        .child(row.title),
                ),
        )
}

fn render_start_thread_menu_content(
    shell: &ShellView,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header("Checklist item"))
        .child(secondary_button(
            shell,
            "checklist-start-thread-row",
            "Start New Codex Thread",
            cx.listener(ShellView::start_checklist_item_thread_from_menu),
        ))
}

fn disabled_menu_content(shell: &ShellView, message: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header("Checklist item"))
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
                .text_color(rgb(0x94a3b8))
                .child(message),
        )
}

fn render_sidebar_title(title: &str) -> impl IntoElement {
    div().flex().items_center().child(
        div()
            .text_sm()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(0x7dd3fc))
            .child(title.to_string()),
    )
}

fn render_checklist_title(title: &str) -> impl IntoElement {
    div().w_full().px_4().pb_3().min_w(px(0.0)).child(
        div()
            .min_w(px(0.0))
            .text_sm()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .whitespace_normal()
            .text_color(rgb(0xe2e8f0))
            .child(title.to_string()),
    )
}

fn menu_header(label: &str) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(0x7dd3fc))
        .child(label.to_string())
}

fn empty_state() -> impl IntoElement {
    div()
        .rounded_md()
        .bg(rgb(0x111827))
        .border_1()
        .border_color(rgb(0x1f2937))
        .p_3()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xe2e8f0))
                .child("No checklist is selected."),
        )
}

fn empty_message(message: &'static str) -> impl IntoElement {
    div()
        .rounded_md()
        .border_1()
        .border_color(rgb(0x1f2937))
        .bg(rgb(0x0b1220))
        .p_3()
        .text_sm()
        .text_color(rgb(0x94a3b8))
        .child(message)
}

fn status_color(status: Option<ChecklistItemStatus>) -> gpui::Rgba {
    match status.unwrap_or(ChecklistItemStatus::Todo) {
        ChecklistItemStatus::Todo => rgb(0x94a3b8),
        ChecklistItemStatus::InProgress => rgb(0xfde68a),
        ChecklistItemStatus::Done => rgb(0x86efac),
    }
}
