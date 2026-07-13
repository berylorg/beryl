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
        ConversationSurfaceState, LoadedWorkspaceState, ScrollbarRegion, ShellRenderFrame,
        ShellView,
        graph_link_menu::GraphThreadLinkMenuView,
        graph_node_action_policy::{
            GRAPH_NODE_ACTION_BUSY_REASON, GRAPH_NODE_ACTION_STALE_REASON,
            GraphNodeLeafDeleteAvailability, graph_node_delete_blocked_by_graph_work,
            graph_node_leaf_delete_availability, graph_node_recursive_delete_disabled_reason,
        },
    },
    threaded_decision_graph_presentation::{
        active_decision_branch_record_for_item, archive_retry_record_for_item,
        checklist_update_retry_record_for_item, decision_branch_start_label,
        latest_handoff_record_for_item,
    },
};

use super::common::{disabled_secondary_button, secondary_button};
use super::graph_link_menu_rows::{
    action_row, actions_back_row, back_row, delete_leaf_row, delete_recursive_hold_row,
    disabled_action_row, disabled_menu_row, menu_header, status_row,
};
use super::scrollbars::{ScrollbarAxis, render_themed_div_scrollbar};

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
    fn from_shell(shell: &ShellRenderFrame<'_>) -> Self {
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
    shell: &ShellRenderFrame<'_>,
    loaded: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
    new_thread_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    let menu = surface.graph_thread_link_menu().active()?;
    let entity = cx.entity();
    let content = render_menu_content(
        shell,
        loaded,
        surface,
        menu.view(),
        new_thread_controls_disabled,
        cx,
    );
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
    if let Some(scrollbar) = render_themed_div_scrollbar(
        shell.style(),
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
    shell: &ShellRenderFrame<'_>,
    loaded: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
    view: &GraphThreadLinkMenuView,
    new_thread_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    let snapshot = surface.member_thread_inventory().snapshot();
    match view {
        GraphThreadLinkMenuView::Root => {
            render_node_action_menu(shell, loaded, surface, new_thread_controls_disabled, cx)
                .into_any_element()
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
    shell: &ShellRenderFrame<'_>,
    loaded: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
    new_thread_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let active_node_id = surface
        .graph_thread_link_menu()
        .active()
        .map(|open| open.node_id().clone());
    let graph = surface.graph_overlay().graph();
    let target_node = active_node_id
        .as_ref()
        .and_then(|node_id| graph.node(node_id));
    let target_exists = active_node_id
        .as_ref()
        .is_some_and(|node_id| graph.node(node_id).is_some());
    let target_is_checklist_item =
        target_node.is_some_and(|node| node.facets().has_checklist_item());
    let target_can_start_topic_decision = target_node
        .is_some_and(|node| node.facets().has_topic() && !node.facets().has_checklist_item());
    let decision_start_label = active_node_id
        .as_ref()
        .map(|node_id| decision_branch_start_label(&loaded.threaded_decision_state, node_id))
        .unwrap_or("Start Decision Branch");
    let topic_decision_disabled = active_node_id
        .as_ref()
        .and_then(|node_id| shell.topic_decision_start_disabled_reason(node_id));
    let has_active_decision_branch = active_node_id.as_ref().is_some_and(|node_id| {
        active_decision_branch_record_for_item(&loaded.threaded_decision_state, node_id).is_some()
    });
    let has_decision_handoff = active_node_id.as_ref().is_some_and(|node_id| {
        latest_handoff_record_for_item(&loaded.threaded_decision_state, node_id).is_some()
    });
    let has_checklist_retry = active_node_id.as_ref().is_some_and(|node_id| {
        checklist_update_retry_record_for_item(&loaded.threaded_decision_state, node_id).is_some()
    });
    let has_archive_retry = active_node_id.as_ref().is_some_and(|node_id| {
        archive_retry_record_for_item(&loaded.threaded_decision_state, node_id).is_some()
    });
    let decision_branch_disabled = active_node_id
        .as_ref()
        .and_then(|node_id| shell.decision_branch_start_disabled_reason(node_id));
    let active_branch_open_disabled = active_node_id
        .as_ref()
        .and_then(|node_id| shell.active_decision_branch_open_disabled_reason(node_id));
    let handoff_open_disabled = active_node_id
        .as_ref()
        .and_then(|node_id| shell.decision_handoff_open_disabled_reason(node_id));
    let checklist_retry_disabled = active_node_id
        .as_ref()
        .and_then(|node_id| shell.decision_checklist_update_retry_disabled_reason(node_id));
    let archive_retry_disabled = active_node_id
        .as_ref()
        .and_then(|node_id| shell.decision_archive_retry_disabled_reason(node_id));
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

    if target_can_start_topic_decision {
        if let Some(reason) = new_thread_controls_disabled {
            menu = menu.child(disabled_graph_action_message_row(
                shell,
                "graph-node-action-start-topic-decision-disabled-row",
                "Start Decision",
                reason.to_string(),
            ));
        } else if let Some(reason) = topic_decision_disabled {
            menu = menu.child(disabled_graph_action_message_row(
                shell,
                "graph-node-action-start-topic-decision-disabled-row",
                "Start Decision",
                reason,
            ));
        } else {
            menu = menu.child(action_row(
                shell,
                "graph-node-action-start-topic-decision-row",
                "Start Decision",
                cx.listener(ShellView::start_topic_decision_from_graph_action_menu),
            ));
        }
    }

    if target_is_checklist_item {
        if let Some(reason) = new_thread_controls_disabled {
            menu = menu.child(disabled_graph_action_message_row(
                shell,
                "graph-node-action-start-decision-branch-disabled-row",
                decision_start_label,
                reason.to_string(),
            ));
        } else if let Some(reason) = decision_branch_disabled {
            menu = menu.child(disabled_graph_action_message_row(
                shell,
                "graph-node-action-start-decision-branch-disabled-row",
                decision_start_label,
                reason,
            ));
        } else {
            menu = menu.child(action_row(
                shell,
                "graph-node-action-start-decision-branch-row",
                decision_start_label,
                cx.listener(ShellView::start_decision_branch_from_graph_action_menu),
            ));
        }

        if !has_active_decision_branch {
            menu = menu.child(disabled_graph_action_message_row(
                shell,
                "graph-node-action-open-active-decision-branch-disabled-row",
                "Open Active Branch",
                "This checklist item has no active decision branch.".to_string(),
            ));
        } else if let Some(reason) = active_branch_open_disabled {
            menu = menu.child(disabled_graph_action_message_row(
                shell,
                "graph-node-action-open-active-decision-branch-disabled-row",
                "Open Active Branch",
                reason,
            ));
        } else {
            menu = menu.child(action_row(
                shell,
                "graph-node-action-open-active-decision-branch-row",
                "Open Active Branch",
                cx.listener(ShellView::open_active_decision_branch_from_graph_action_menu),
            ));
        }

        if !has_decision_handoff {
            menu = menu.child(disabled_graph_action_message_row(
                shell,
                "graph-node-action-open-decision-handoff-disabled-row",
                "Open Handoff",
                "This decision has no parent handoff turn yet.".to_string(),
            ));
        } else if let Some(reason) = handoff_open_disabled {
            menu = menu.child(disabled_graph_action_message_row(
                shell,
                "graph-node-action-open-decision-handoff-disabled-row",
                "Open Handoff",
                reason,
            ));
        } else {
            menu = menu.child(action_row(
                shell,
                "graph-node-action-open-decision-handoff-row",
                "Open Handoff",
                cx.listener(ShellView::open_decision_handoff_from_graph_action_menu),
            ));
        }

        if has_checklist_retry {
            if let Some(reason) = checklist_retry_disabled {
                menu = menu.child(disabled_graph_action_message_row(
                    shell,
                    "graph-node-action-retry-decision-checklist-disabled-row",
                    "Retry Checklist Update",
                    reason,
                ));
            } else {
                menu = menu.child(action_row(
                    shell,
                    "graph-node-action-retry-decision-checklist-row",
                    "Retry Checklist Update",
                    cx.listener(ShellView::retry_decision_checklist_update_from_graph_action_menu),
                ));
            }
        }

        if has_archive_retry {
            if let Some(reason) = archive_retry_disabled {
                menu = menu.child(disabled_graph_action_message_row(
                    shell,
                    "graph-node-action-retry-decision-archive-disabled-row",
                    "Retry Branch Close",
                    reason,
                ));
            } else {
                menu = menu.child(action_row(
                    shell,
                    "graph-node-action-retry-decision-archive-row",
                    "Retry Branch Close",
                    cx.listener(ShellView::retry_decision_archive_from_graph_action_menu),
                ));
            }
        }

        if let Some(reason) = new_thread_controls_disabled {
            menu = menu.child(disabled_graph_action_message_row(
                shell,
                "graph-node-action-start-checklist-thread-disabled-row",
                "Start New Codex Thread",
                reason.to_string(),
            ));
        } else if graph_work_blocked {
            menu = menu.child(disabled_graph_work_row(shell, "Start New Codex Thread"));
        } else {
            menu = menu.child(action_row(
                shell,
                "graph-node-action-start-checklist-thread-row",
                "Start New Codex Thread",
                cx.listener(ShellView::start_checklist_item_thread_from_graph_action_menu),
            ));
        }
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
    shell: &ShellRenderFrame<'_>,
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
    shell: &ShellRenderFrame<'_>,
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

fn disabled_link_thread_row(shell: &ShellRenderFrame<'_>) -> impl IntoElement {
    let reason = "Select a workspace runtime environment before linking threads.".to_string();
    let tooltip_theme = LinkMenuTooltipTheme::from_shell(shell);
    disabled_secondary_button(
        shell,
        "graph-thread-link-disabled-link-thread",
        "Link thread",
    )
    .tooltip(move |_, cx| build_link_menu_tooltip(reason.clone(), tooltip_theme, cx))
}

fn disabled_delete_leaf_row(
    shell: &ShellRenderFrame<'_>,
    reason: &'static str,
) -> impl IntoElement {
    let tooltip_theme = LinkMenuTooltipTheme::from_shell(shell);
    disabled_secondary_button(shell, "graph-node-delete-row", "Delete")
        .tooltip(move |_, cx| build_link_menu_tooltip(reason.to_string(), tooltip_theme, cx))
}

fn disabled_graph_work_row(shell: &ShellRenderFrame<'_>, label: &'static str) -> impl IntoElement {
    disabled_graph_action_row(
        shell,
        "graph-node-action-disabled-graph-work-row",
        label,
        GRAPH_NODE_ACTION_BUSY_REASON,
    )
}

fn disabled_graph_action_row(
    shell: &ShellRenderFrame<'_>,
    id: &'static str,
    label: &'static str,
    reason: &'static str,
) -> impl IntoElement {
    let tooltip_theme = LinkMenuTooltipTheme::from_shell(shell);
    disabled_secondary_button(shell, id, label)
        .tooltip(move |_, cx| build_link_menu_tooltip(reason.to_string(), tooltip_theme, cx))
}

fn disabled_graph_action_message_row(
    shell: &ShellRenderFrame<'_>,
    id: &'static str,
    label: &'static str,
    reason: String,
) -> impl IntoElement {
    let tooltip_theme = LinkMenuTooltipTheme::from_shell(shell);
    disabled_secondary_button(shell, id, label)
        .tooltip(move |_, cx| build_link_menu_tooltip(reason.clone(), tooltip_theme, cx))
}

fn render_member_list(
    shell: &ShellRenderFrame<'_>,
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
    shell: &ShellRenderFrame<'_>,
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
    shell: &ShellRenderFrame<'_>,
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
    shell: &ShellRenderFrame<'_>,
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
    shell: &ShellRenderFrame<'_>,
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
    shell: &ShellRenderFrame<'_>,
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
