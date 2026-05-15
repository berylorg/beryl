use std::time::Instant;

use beryl_model::semantic_graph::ThreadRefId;
use gpui::{
    AnyElement, AnyView, App, Context, DispatchPhase, InteractiveElement, KeyDownEvent, KeyUpEvent,
    MouseDownEvent, Render, StatefulInteractiveElement, Window, anchored, canvas, div, prelude::*,
    px,
};

use crate::{
    member_thread_inventory::{
        MemberThreadInventoryGroup, MemberThreadInventorySnapshot, MemberThreadInventoryThread,
    },
    shell::{
        ConversationSurfaceState, LoadedWorkspaceState, ScrollbarRegion, ShellView,
        graph_link_menu::GraphThreadLinkMenuView,
        graph_node_action_policy::{
            GRAPH_NODE_ACTION_BUSY_REASON, GRAPH_NODE_ACTION_STALE_REASON,
            GraphNodeLeafDeleteAvailability, graph_node_delete_blocked_by_graph_work,
            graph_node_leaf_delete_availability, graph_node_recursive_delete_disabled_reason,
        },
    },
};

use super::common::{disabled_secondary_button, secondary_button};
use super::graph_link_menu_rows::{
    action_row, actions_back_row, back_row, delete_leaf_row, delete_recursive_hold_row,
    disabled_action_row, disabled_menu_row, menu_header, status_row,
};
use super::scrollbars::{ScrollbarAxis, render_div_scrollbar};

#[derive(Clone)]
enum ThreadLinkMenuMode {
    Link,
    Rebind(ThreadRefId),
}

impl ThreadLinkMenuMode {
    fn header(&self) -> &'static str {
        match self {
            Self::Link => "Link thread",
            Self::Rebind(_) => "Rebind thread link",
        }
    }
}

struct LinkMenuTooltip {
    message: String,
    theme: LinkMenuTooltipTheme,
}

#[derive(Clone, Copy)]
struct LinkMenuTooltipTheme {
    background: gpui::Rgba,
    border: gpui::Rgba,
    foreground: gpui::Rgba,
}

impl LinkMenuTooltipTheme {
    fn from_shell(shell: &ShellView) -> Self {
        Self {
            background: shell.popup_surface_background(),
            border: shell.surface_border(),
            foreground: shell.general_ui_foreground(),
        }
    }
}

pub(super) fn render_graph_thread_link_menu_listeners(
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
                        view.handle_graph_thread_link_menu_mouse_down(event, window, cx);
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
                        view.handle_graph_thread_link_menu_key_down(event, window, cx)
                    });
                    if handled {
                        cx.stop_propagation();
                    }
                }
            });
            window.on_key_event({
                let entity = entity.clone();
                move |event: &KeyUpEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }

                    let handled = entity.update(cx, |view, cx| {
                        view.handle_graph_thread_link_menu_key_up(event, window, cx)
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

pub(super) fn render_graph_thread_link_menu(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    let menu = surface.graph_thread_link_menu().active()?;
    let entity = cx.entity();
    let content = render_menu_content(shell, loaded, surface, menu.view(), cx);
    let scroll_handle = surface.graph_thread_link_menu_scroll_handle();
    let scrollbar_visibility =
        shell.scrollbar_visibility_policy(&ScrollbarRegion::GraphThreadLinkMenu, cx);
    let mut panel = div()
        .id("graph-thread-link-menu-panel")
        .relative()
        .w(px(292.0))
        .max_h(px(360.0))
        .overflow_hidden()
        .occlude()
        .rounded_lg()
        .border_1()
        .border_color(shell.surface_border())
        .bg(shell.popup_surface_background())
        .shadow_lg()
        .on_mouse_move(cx.listener(ShellView::note_graph_thread_link_menu_scrollbar_motion))
        .on_scroll_wheel(cx.listener(ShellView::note_graph_thread_link_menu_scrollbar_scroll))
        .child(
            div()
                .id("graph-thread-link-menu-scroll")
                .w_full()
                .max_h(px(360.0))
                .min_h(px(0.0))
                .track_scroll(&scroll_handle)
                .overflow_y_scroll()
                .p_2()
                .child(content),
        );
    if let Some(scrollbar) = render_div_scrollbar(
        "graph-thread-link-menu-scrollbar",
        &scroll_handle,
        ScrollbarAxis::Vertical,
        scrollbar_visibility,
    ) {
        panel = panel.child(scrollbar);
    }

    Some(
        anchored()
            .position(menu.position())
            .snap_to_window_with_margin(px(8.0))
            .child(
                div()
                    .on_children_prepainted(move |children, _, cx| {
                        let bounds = children.first().copied();
                        entity.update(cx, |view, cx| {
                            view.record_graph_thread_link_menu_bounds(bounds, cx)
                        });
                    })
                    .child(panel),
            )
            .into_any_element(),
    )
}

fn render_menu_content(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
    view: &GraphThreadLinkMenuView,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    let snapshot = surface.member_thread_inventory().snapshot();
    match view {
        GraphThreadLinkMenuView::Root => {
            render_node_action_menu(shell, loaded, surface, cx).into_any_element()
        }
        GraphThreadLinkMenuView::LinkThreads if loaded.selected_runtime().is_none() => {
            render_missing_runtime_menu(shell, ThreadLinkMenuMode::Link, cx).into_any_element()
        }
        GraphThreadLinkMenuView::LinkThreads => {
            render_link_thread_menu(shell, surface, ThreadLinkMenuMode::Link, cx)
        }
        GraphThreadLinkMenuView::MemberThreads(member_key) => snapshot
            .group(member_key)
            .map(|group| {
                render_thread_list(
                    shell,
                    group,
                    surface,
                    ThreadLinkMenuMode::Link,
                    cx,
                    true,
                    false,
                )
                .into_any_element()
            })
            .unwrap_or_else(|| {
                render_stale_member_menu(shell, ThreadLinkMenuMode::Link, cx).into_any_element()
            }),
        GraphThreadLinkMenuView::RebindThreads(thread_ref_id)
            if loaded.selected_runtime().is_none() =>
        {
            render_missing_runtime_menu(
                shell,
                ThreadLinkMenuMode::Rebind(thread_ref_id.clone()),
                cx,
            )
            .into_any_element()
        }
        GraphThreadLinkMenuView::RebindThreads(thread_ref_id) => render_link_thread_menu(
            shell,
            surface,
            ThreadLinkMenuMode::Rebind(thread_ref_id.clone()),
            cx,
        ),
        GraphThreadLinkMenuView::RebindMemberThreads {
            thread_ref_id,
            member_key,
        } => snapshot
            .group(member_key)
            .map(|group| {
                render_thread_list(
                    shell,
                    group,
                    surface,
                    ThreadLinkMenuMode::Rebind(thread_ref_id.clone()),
                    cx,
                    true,
                    false,
                )
                .into_any_element()
            })
            .unwrap_or_else(|| {
                render_stale_member_menu(
                    shell,
                    ThreadLinkMenuMode::Rebind(thread_ref_id.clone()),
                    cx,
                )
                .into_any_element()
            }),
    }
}

fn render_node_action_menu(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let active_node_id = surface
        .graph_thread_link_menu()
        .active()
        .map(|open| open.node_id().clone());
    let graph = surface.graph_overlay().graph();
    let target_exists = active_node_id
        .as_ref()
        .is_some_and(|node_id| graph.node(node_id).is_some());
    let has_hard_children = active_node_id.as_ref().is_some_and(|node_id| {
        graph
            .child_ids_of(node_id)
            .is_some_and(|children| !children.is_empty())
    });
    let graph_mutation_in_flight = shell.graph_receiver.is_some();
    let graph_thread_start_in_flight = shell.graph_thread_start_receiver.is_some();
    let graph_work_blocked = graph_node_delete_blocked_by_graph_work(
        graph_mutation_in_flight,
        graph_thread_start_in_flight,
    );
    let delete_progress = active_node_id.as_ref().and_then(|node_id| {
        surface
            .graph_thread_link_menu()
            .delete_hold_progress_for_target(node_id, Instant::now())
    });
    let leaf_delete_in_flight = active_node_id.as_ref().is_some_and(|node_id| {
        surface
            .graph_thread_link_menu()
            .leaf_delete_in_flight_for_target(node_id)
    });
    let subtree_delete_in_flight = active_node_id.as_ref().is_some_and(|node_id| {
        surface
            .graph_thread_link_menu()
            .subtree_delete_in_flight_for_target(node_id)
    });
    let recursive_delete_disabled_reason = graph_node_recursive_delete_disabled_reason(
        target_exists,
        graph_mutation_in_flight,
        graph_thread_start_in_flight,
        subtree_delete_in_flight,
    );
    let leaf_delete_availability = graph_node_leaf_delete_availability(
        target_exists,
        has_hard_children,
        graph_mutation_in_flight,
        graph_thread_start_in_flight,
    );
    let mut menu = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(shell, "Node actions"));

    if leaf_delete_in_flight {
        menu = menu.child(disabled_action_row(
            shell,
            "graph-node-delete-row",
            "Deleting...",
        ));
    } else {
        match leaf_delete_availability {
            GraphNodeLeafDeleteAvailability::Enabled => {
                menu = menu.child(delete_leaf_row(
                    shell,
                    cx.listener(ShellView::delete_graph_node_leaf_from_action_menu),
                    cx.listener(ShellView::delete_graph_node_leaf_keyboard_from_action_menu),
                ));
            }
            GraphNodeLeafDeleteAvailability::Disabled(reason) => {
                menu = menu.child(disabled_delete_leaf_row(shell, reason));
            }
        }
    }

    if active_node_id.is_some() && recursive_delete_disabled_reason.is_none() {
        menu = menu.child(delete_recursive_hold_row(
            shell,
            delete_progress,
            subtree_delete_in_flight,
            cx.listener(ShellView::begin_graph_node_delete_hold_from_action_menu),
            cx.listener(ShellView::cancel_graph_node_delete_hold_from_action_menu),
            cx.listener(ShellView::cancel_graph_node_delete_hold_from_action_menu),
            cx.listener(ShellView::cancel_graph_node_delete_hold_on_hover_change),
            cx.listener(ShellView::begin_graph_node_delete_keyboard_hold_from_action_menu),
            cx.listener(ShellView::cancel_graph_node_delete_keyboard_hold_from_action_menu),
        ));
    } else {
        let reason = recursive_delete_disabled_reason.unwrap_or(GRAPH_NODE_ACTION_STALE_REASON);
        menu = menu.child(disabled_graph_action_row(
            shell,
            "graph-node-delete-recursively-row",
            "Delete Recursively",
            reason,
        ));
    }

    if loaded.selected_runtime().is_none() {
        menu.child(disabled_link_thread_row(shell))
    } else if graph_work_blocked {
        menu.child(disabled_graph_work_row(shell, "Link thread"))
    } else {
        menu.child(action_row(
            shell,
            "graph-node-action-link-thread-row",
            "Link thread",
            cx.listener(ShellView::show_graph_thread_link_menu),
        ))
    }
}

fn render_link_thread_menu(
    shell: &ShellView,
    surface: &ConversationSurfaceState,
    mode: ThreadLinkMenuMode,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    let snapshot = surface.member_thread_inventory().snapshot();
    if snapshot.groups().len() == 1 {
        return render_thread_list(
            shell,
            snapshot.groups().first().unwrap(),
            surface,
            mode,
            cx,
            false,
            true,
        )
        .into_any_element();
    }

    render_member_list(shell, snapshot, surface, mode, cx).into_any_element()
}

fn render_missing_runtime_menu(
    shell: &ShellView,
    mode: ThreadLinkMenuMode,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(shell, mode.header()))
        .child(disabled_link_thread_row(shell))
        .child(action_row(
            shell,
            "graph-node-action-back-row",
            "Back to actions",
            cx.listener(ShellView::show_graph_node_action_menu),
        ))
}

fn disabled_link_thread_row(shell: &ShellView) -> impl IntoElement {
    let reason = "Select a workspace runtime environment before linking threads.".to_string();
    let tooltip_theme = LinkMenuTooltipTheme::from_shell(shell);
    disabled_secondary_button(
        shell,
        "graph-thread-link-disabled-link-thread",
        "Link thread",
    )
    .tooltip(move |_, cx| build_link_menu_tooltip(reason.clone(), tooltip_theme, cx))
}

fn disabled_delete_leaf_row(shell: &ShellView, reason: &'static str) -> impl IntoElement {
    let tooltip_theme = LinkMenuTooltipTheme::from_shell(shell);
    disabled_secondary_button(shell, "graph-node-delete-row", "Delete")
        .tooltip(move |_, cx| build_link_menu_tooltip(reason.to_string(), tooltip_theme, cx))
}

fn disabled_graph_work_row(shell: &ShellView, label: &'static str) -> impl IntoElement {
    disabled_graph_action_row(
        shell,
        "graph-node-action-disabled-graph-work-row",
        label,
        GRAPH_NODE_ACTION_BUSY_REASON,
    )
}

fn disabled_graph_action_row(
    shell: &ShellView,
    id: &'static str,
    label: &'static str,
    reason: &'static str,
) -> impl IntoElement {
    let tooltip_theme = LinkMenuTooltipTheme::from_shell(shell);
    disabled_secondary_button(shell, id, label)
        .tooltip(move |_, cx| build_link_menu_tooltip(reason.to_string(), tooltip_theme, cx))
}

fn render_member_list(
    shell: &ShellView,
    snapshot: &MemberThreadInventorySnapshot,
    surface: &ConversationSurfaceState,
    mode: ThreadLinkMenuMode,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let mut list = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(shell, mode.header()))
        .child(actions_back_row(shell, cx));
    if surface.member_thread_inventory().refreshing() {
        list = list.child(status_row(shell, "Refreshing thread list..."));
    } else if let Some(error) = surface.member_thread_inventory().last_error() {
        list = list.child(status_row(shell, error));
    }

    for (index, group) in snapshot.groups().iter().enumerate() {
        list = list.child(render_member_row(shell, index, group, mode.clone(), cx));
    }
    list
}

fn render_thread_list(
    shell: &ShellView,
    group: &MemberThreadInventoryGroup,
    surface: &ConversationSurfaceState,
    mode: ThreadLinkMenuMode,
    cx: &mut Context<ShellView>,
    show_member_back: bool,
    show_actions_back: bool,
) -> impl IntoElement {
    let mut list = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(shell, mode.header()));
    if show_actions_back {
        list = list.child(actions_back_row(shell, cx));
    }
    if show_member_back {
        list = list.child(render_member_back_row(shell, mode.clone(), cx));
    }
    list = list.child(
        div()
            .px_2()
            .pb_1()
            .text_xs()
            .text_color(shell.surface_muted_foreground())
            .whitespace_nowrap()
            .truncate()
            .child(group.label().to_string()),
    );

    if surface.member_thread_inventory().refreshing() {
        list = list.child(status_row(shell, "Refreshing thread list..."));
    } else if let Some(error) = surface.member_thread_inventory().last_error() {
        list = list.child(status_row(shell, error));
    }

    if group.threads().is_empty() {
        return list.child(disabled_menu_row(shell, "No threads"));
    }

    for (index, thread) in group.threads().iter().enumerate() {
        list = list.child(render_thread_row(shell, index, thread, mode.clone(), cx));
    }
    list
}

fn render_stale_member_menu(
    shell: &ShellView,
    mode: ThreadLinkMenuMode,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(shell, mode.header()))
        .child(disabled_menu_row(shell, "Member unavailable"))
        .child(render_member_back_row(shell, mode, cx))
}

fn render_member_row(
    shell: &ShellView,
    index: usize,
    group: &MemberThreadInventoryGroup,
    mode: ThreadLinkMenuMode,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let member_key = group.key().clone();
    let secondary = shell.secondary_button_theme();
    div()
        .id(("graph-thread-link-member-row", index))
        .rounded_md()
        .px_2()
        .py_2()
        .cursor_pointer()
        .hover(move |style| style.bg(secondary.hover.background))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .text_sm()
                        .text_color(shell.general_ui_foreground())
                        .whitespace_nowrap()
                        .truncate()
                        .child(group.label().to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(shell.surface_muted_foreground())
                        .child(group.threads().len().to_string()),
                ),
        )
        .on_click(cx.listener(move |view, event, window, cx| match &mode {
            ThreadLinkMenuMode::Link => {
                view.open_graph_thread_link_member(member_key.clone(), event, window, cx);
            }
            ThreadLinkMenuMode::Rebind(thread_ref_id) => {
                view.open_graph_thread_rebind_member(
                    thread_ref_id.clone(),
                    member_key.clone(),
                    event,
                    window,
                    cx,
                );
            }
        }))
}

fn render_thread_row(
    shell: &ShellView,
    index: usize,
    thread: &MemberThreadInventoryThread,
    mode: ThreadLinkMenuMode,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let thread = thread.clone();
    let label = thread.title().to_string();
    secondary_button(
        shell,
        ("graph-thread-link-thread-row", index),
        label,
        cx.listener(move |view, event, window, cx| match &mode {
            ThreadLinkMenuMode::Link => {
                view.link_graph_thread_to_node(thread.clone(), event, window, cx);
            }
            ThreadLinkMenuMode::Rebind(thread_ref_id) => {
                view.rebind_graph_thread_ref(
                    thread_ref_id.clone(),
                    thread.clone(),
                    event,
                    window,
                    cx,
                );
            }
        }),
    )
    .w_full()
    .justify_start()
    .truncate()
}

fn render_member_back_row(
    shell: &ShellView,
    mode: ThreadLinkMenuMode,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    match mode {
        ThreadLinkMenuMode::Link => back_row(shell, cx).into_any_element(),
        ThreadLinkMenuMode::Rebind(thread_ref_id) => action_row(
            shell,
            "graph-thread-rebind-back-row",
            "Back to members",
            cx.listener(move |view, event, window, cx| {
                view.show_graph_thread_ref_rebind_members(thread_ref_id.clone(), event, window, cx);
            }),
        )
        .into_any_element(),
    }
}

fn build_link_menu_tooltip(message: String, theme: LinkMenuTooltipTheme, cx: &mut App) -> AnyView {
    cx.new(|_| LinkMenuTooltip { message, theme }).into()
}

impl Render for LinkMenuTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(260.0))
            .rounded_md()
            .bg(self.theme.background)
            .border_1()
            .border_color(self.theme.border)
            .px_3()
            .py_2()
            .text_xs()
            .text_color(self.theme.foreground)
            .child(self.message.clone())
    }
}
