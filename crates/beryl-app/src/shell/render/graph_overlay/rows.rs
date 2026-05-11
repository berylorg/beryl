use beryl_model::{
    conversation::WorkspaceConversationState,
    semantic_graph::{
        ChecklistItemStatus, SemanticGraph, SemanticNode, SemanticNodeId, SoftLink, ThreadRef,
    },
    workspace::WorkspaceId,
};
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement, MouseButton, StatefulInteractiveElement,
    div, prelude::*, px, rgb,
};

use crate::shell::{
    ConversationSurfaceState, ShellView,
    graph::{DEFAULT_GRAPH_COLUMN_EXPANDED_DEPTH, GraphColumnSelection, GraphColumnState},
    layout,
    thread_selection::graph_thread_ref_availability,
};

use super::{
    CHECKLIST_ITEM_STATUS_DONE, CHECKLIST_ITEM_STATUS_IN_PROGRESS, CHECKLIST_ITEM_STATUS_TODO,
    GRAPH_OVERLAY_ROW_INDENT, GraphSummaryTooltipTheme, SOFT_LINK_ROW_MARKER,
    build_graph_summary_tooltip, stable_id_key,
};

struct GraphNodePalette {
    background: gpui::Rgba,
    selected_background: gpui::Rgba,
    border: gpui::Rgba,
    selected_border: gpui::Rgba,
}

pub(super) fn render_graph_node_tree(
    shell: &ShellView,
    workspace_state: &WorkspaceConversationState,
    implicit_home_execution_target: Option<&WorkspaceId>,
    column_index: usize,
    column: &GraphColumnState,
    surface: &ConversationSurfaceState,
    graph: &SemanticGraph,
    node: &SemanticNode,
    depth: usize,
    semantic_node_tooltips_allowed: bool,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    let children = graph.child_nodes_of(node.id());
    let soft_links: Vec<_> = graph.soft_links_from(node.id()).collect();
    let thread_refs: Vec<_> = graph.thread_refs_for_node(node.id()).collect();
    let expanded = column.is_expanded(node.id(), depth < DEFAULT_GRAPH_COLUMN_EXPANDED_DEPTH);
    let has_attached_rows = !soft_links.is_empty() || !thread_refs.is_empty();
    let selected = matches!(
        column.selection(),
        Some(GraphColumnSelection::Node(selected_node_id)) if selected_node_id == node.id()
    );
    let pending = surface
        .graph_overlay()
        .node_has_pending_optimistic_mutation(node.id());

    let mut rows = div()
        .w_full()
        .flex()
        .flex_col()
        .gap_1()
        .child(render_graph_node_row(
            shell,
            column_index,
            node,
            depth,
            !children.is_empty() || has_attached_rows,
            expanded,
            selected,
            pending,
            semantic_node_tooltips_allowed,
            cx,
        ));

    if expanded {
        for soft_link in soft_links {
            let link_selected = matches!(
                column.selection(),
                Some(GraphColumnSelection::SoftLink { link_id, .. }) if link_id == soft_link.id()
            );
            rows = rows.child(render_graph_soft_link_row(
                shell,
                column_index,
                graph,
                soft_link,
                depth + 1,
                link_selected,
                cx,
            ));
        }

        for thread_ref in thread_refs {
            rows = rows.child(render_graph_thread_ref_row(
                shell,
                workspace_state,
                implicit_home_execution_target,
                column_index,
                thread_ref,
                depth + 1,
                cx,
            ));
        }

        for child in children {
            rows = rows.child(render_graph_node_tree(
                shell,
                workspace_state,
                implicit_home_execution_target,
                column_index,
                column,
                surface,
                graph,
                child,
                depth + 1,
                semantic_node_tooltips_allowed,
                cx,
            ));
        }
    }

    rows.into_any_element()
}

fn render_graph_node_row(
    shell: &ShellView,
    column_index: usize,
    node: &SemanticNode,
    depth: usize,
    has_children: bool,
    expanded: bool,
    selected: bool,
    pending: bool,
    semantic_node_tooltips_allowed: bool,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let node_id = node.id().clone();
    let node_id_for_context_menu = node.id().clone();
    let summary = node.summary().trim().to_string();
    let palette = graph_node_palette(node);
    let status = node.checklist_item_status();
    let tooltip_theme = GraphSummaryTooltipTheme::from_shell(shell);
    let mut title_hitbox = div()
        .id((
            ElementId::from(("graph-node-row", column_index)),
            stable_id_key(node.id().as_str()),
        ))
        .flex_1()
        .min_w(px(0.0))
        .cursor_pointer()
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap_0()
                .when_some(status, |title_row, status| {
                    title_row.child(render_checklist_item_status_marker(shell, status))
                })
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(shell.general_ui_foreground())
                        .whitespace_nowrap()
                        .truncate()
                        .child(node.title().to_string()),
                ),
        )
        .on_click(cx.listener(move |view, event, window, cx| {
            view.handle_graph_node_click(column_index, node_id.clone(), event, window, cx);
        }));
    if semantic_node_tooltips_allowed && !summary.is_empty() {
        title_hitbox = title_hitbox
            .tooltip(move |_, cx| build_graph_summary_tooltip(summary.clone(), tooltip_theme, cx));
    }
    div()
        .w_full()
        .pl(px(depth as f32 * GRAPH_OVERLAY_ROW_INDENT))
        .child(
            div()
                .w_full()
                .rounded_md()
                .bg(if selected {
                    palette.selected_background
                } else {
                    palette.background
                })
                .border_1()
                .border_color(if selected {
                    palette.selected_border
                } else {
                    palette.border
                })
                .opacity(if pending { 0.72 } else { 1.0 })
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |view, event, window, cx| {
                        view.open_graph_node_thread_link_menu(
                            column_index,
                            node_id_for_context_menu.clone(),
                            event,
                            window,
                            cx,
                        );
                    }),
                )
                .px_3()
                .py_2()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(render_expander(
                            shell,
                            column_index,
                            node.id().clone(),
                            depth,
                            has_children,
                            expanded,
                            cx,
                        ))
                        .child(title_hitbox),
                ),
        )
}

fn render_graph_soft_link_row(
    shell: &ShellView,
    column_index: usize,
    graph: &SemanticGraph,
    soft_link: &SoftLink,
    depth: usize,
    selected: bool,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let link_id = soft_link.id().clone();
    let target_id = soft_link.target_id().clone();
    let target = graph
        .node(soft_link.target_id())
        .expect("graph overlay renders only valid soft-link targets");
    let target_summary = target.summary().trim().to_string();
    let primary = shell.primary_button_theme();
    let secondary = shell.secondary_button_theme();
    let background = if selected {
        primary.normal.background
    } else {
        shell.row_surface_background()
    };
    let border = if selected {
        primary.normal.border
    } else {
        shell.surface_border()
    };
    let foreground = if selected {
        primary.normal.foreground
    } else {
        rgb(0xbfdbfe)
    };
    let hover_background = if selected {
        primary.hover.background
    } else {
        secondary.hover.background
    };
    let tooltip_theme = GraphSummaryTooltipTheme::from_shell(shell);
    let mut row = div()
        .id((
            ElementId::from(("graph-soft-link-row", column_index)),
            stable_id_key(soft_link.id().as_str()),
        ))
        .w_full()
        .rounded_md()
        .bg(background)
        .border_1()
        .border_color(border)
        .px_3()
        .py_2()
        .cursor_pointer()
        .hover(move |style| style.bg(hover_background))
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .flex_none()
                        .text_sm()
                        .text_color(foreground)
                        .child(format!("{SOFT_LINK_ROW_MARKER} ")),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(foreground)
                        .whitespace_nowrap()
                        .truncate()
                        .child(target.title().to_string()),
                ),
        )
        .on_click(cx.listener(move |view, event, window, cx| {
            view.select_graph_soft_link(
                column_index,
                link_id.clone(),
                target_id.clone(),
                event,
                window,
                cx,
            );
        }));
    if !target_summary.is_empty() {
        row = row.tooltip(move |_, cx| {
            build_graph_summary_tooltip(target_summary.clone(), tooltip_theme, cx)
        });
    }

    div()
        .w_full()
        .pl(px(depth as f32 * GRAPH_OVERLAY_ROW_INDENT))
        .child(row)
}

fn render_graph_thread_ref_row(
    shell: &ShellView,
    workspace_state: &WorkspaceConversationState,
    implicit_home_execution_target: Option<&WorkspaceId>,
    column_index: usize,
    thread_ref: &ThreadRef,
    depth: usize,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let thread_ref_id = thread_ref.id().clone();
    let thread_id = thread_ref.thread_id().as_str().to_string();
    let execution_target = thread_ref.execution_target().clone();
    let label = thread_ref.label().to_string();
    let availability =
        graph_thread_ref_availability(workspace_state, thread_ref, implicit_home_execution_target);
    let invalid_reason = availability.reason().map(str::to_string);
    let secondary = shell.secondary_button_theme();
    let border_color = if availability.is_openable() {
        shell.surface_border()
    } else {
        rgb(0xf87171)
    };

    div()
        .w_full()
        .pl(px(depth as f32 * GRAPH_OVERLAY_ROW_INDENT))
        .child(
            div()
                .id((
                    ElementId::from("graph-thread-ref-row"),
                    stable_id_key(thread_ref.id().as_str()),
                ))
                .w_full()
                .rounded_md()
                .bg(shell.row_surface_background())
                .border_1()
                .border_color(border_color)
                .cursor_pointer()
                .hover(move |style| style.bg(secondary.hover.background))
                .p_3()
                .on_click(cx.listener(move |view, event, window, cx| {
                    view.select_graph_thread_ref(
                        thread_ref_id.clone(),
                        thread_id.clone(),
                        execution_target.clone(),
                        label.clone(),
                        event,
                        window,
                        cx,
                    );
                }))
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap_2()
                        .child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(rgb(0xfde68a))
                                        .child(format!("thread {}", thread_ref.label())),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(shell.surface_muted_foreground())
                                        .child(format!(
                                            "{}  {}",
                                            thread_ref
                                                .execution_target()
                                                .runtime_mode()
                                                .display_name(),
                                            thread_ref.thread_id().as_str()
                                        )),
                                ),
                        )
                        .when_some(invalid_reason, |this, reason| {
                            this.child(render_invalid_thread_ref_actions(
                                shell,
                                column_index,
                                thread_ref.id().clone(),
                                reason,
                                cx,
                            ))
                        }),
                ),
        )
}

fn render_invalid_thread_ref_actions(
    shell: &ShellView,
    column_index: usize,
    thread_ref_id: beryl_model::semantic_graph::ThreadRefId,
    reason: String,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let indicator_reason = reason.clone();
    let tooltip_theme = GraphSummaryTooltipTheme::from_shell(shell);
    let button_theme = shell.secondary_button_theme();

    div()
        .flex_none()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .id((
                    ElementId::from("graph-thread-ref-invalid-indicator"),
                    stable_id_key(thread_ref_id.as_str()),
                ))
                .h(px(20.0))
                .w(px(20.0))
                .rounded_full()
                .border_1()
                .border_color(rgb(0xf87171))
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(0xfca5a5))
                .child("!")
                .tooltip(move |_, cx| {
                    build_graph_summary_tooltip(indicator_reason.clone(), tooltip_theme, cx)
                }),
        )
        .child(
            div()
                .id((
                    ElementId::from("graph-thread-ref-rebind-row"),
                    stable_id_key(thread_ref_id.as_str()),
                ))
                .h(px(24.0))
                .px_2()
                .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
                .border_1()
                .border_color(button_theme.normal.border)
                .bg(button_theme.normal.background)
                .flex()
                .items_center()
                .text_xs()
                .text_color(button_theme.normal.foreground)
                .cursor_pointer()
                .hover(move |style| {
                    style
                        .bg(button_theme.hover.background)
                        .border_color(button_theme.hover.border)
                        .text_color(button_theme.hover.foreground)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, event, window, cx| {
                        view.open_graph_thread_ref_rebind_menu(
                            column_index,
                            thread_ref_id.clone(),
                            event,
                            window,
                            cx,
                        );
                    }),
                )
                .on_click(|_, _, cx| cx.stop_propagation())
                .child("Rebind"),
        )
}

fn render_expander(
    shell: &ShellView,
    column_index: usize,
    node_id: SemanticNodeId,
    depth: usize,
    has_children: bool,
    expanded: bool,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    if !has_children {
        return div()
            .w(px(layout::BUTTON_ICON_OUTER_WIDTH))
            .into_any_element();
    }
    let secondary = shell.secondary_button_theme();

    div()
        .id((
            ElementId::from(("graph-expander", depth)),
            stable_id_key(node_id.as_str()),
        ))
        .w(px(layout::BUTTON_ICON_OUTER_WIDTH))
        .h(px(layout::BUTTON_OUTER_HEIGHT))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(secondary.normal.background)
        .border_1()
        .border_color(secondary.normal.border)
        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
        .text_color(secondary.normal.foreground)
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .hover(move |style| style.bg(secondary.hover.background))
        .child(if expanded { "-" } else { "+" })
        .on_click(cx.listener(move |view, event, window, cx| {
            view.toggle_graph_node_expansion(
                column_index,
                node_id.clone(),
                depth,
                event,
                window,
                cx,
            );
        }))
        .into_any_element()
}

fn render_checklist_item_status_marker(
    shell: &ShellView,
    status: ChecklistItemStatus,
) -> impl IntoElement {
    div()
        .flex_none()
        .text_sm()
        .text_color(shell.general_ui_foreground())
        .child(format!("{} ", checklist_item_status_glyph(status)))
}

fn graph_node_palette(node: &SemanticNode) -> GraphNodePalette {
    if node.facets().has_checklist() {
        GraphNodePalette {
            background: rgb(0x134e4a),
            selected_background: rgb(0x0f766e),
            border: rgb(0x0f766e),
            selected_border: rgb(0x99f6e4),
        }
    } else if node.facets().has_checklist_item() {
        GraphNodePalette {
            background: rgb(0x1f2937),
            selected_background: rgb(0x374151),
            border: rgb(0x334155),
            selected_border: rgb(0xf8fafc),
        }
    } else {
        GraphNodePalette {
            background: rgb(0x172554),
            selected_background: rgb(0x1d4ed8),
            border: rgb(0x1d4ed8),
            selected_border: rgb(0xbfdbfe),
        }
    }
}

fn checklist_item_status_glyph(status: ChecklistItemStatus) -> &'static str {
    match status {
        ChecklistItemStatus::Todo => CHECKLIST_ITEM_STATUS_TODO,
        ChecklistItemStatus::InProgress => CHECKLIST_ITEM_STATUS_IN_PROGRESS,
        ChecklistItemStatus::Done => CHECKLIST_ITEM_STATUS_DONE,
    }
}
