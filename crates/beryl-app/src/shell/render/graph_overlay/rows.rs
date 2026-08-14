use beryl_model::{
    conversation::WorkspaceConversationState,
    semantic_graph::{
        ChecklistItemStatus, SemanticGraph, SemanticNode, SemanticNodeId, SoftLink, ThreadRef,
    },
    workspace::WorkspaceId,
};
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement, MouseButton, StatefulInteractiveElement,
    div, prelude::*, px,
};

use crate::{
    BerylThemeRole,
    shell::{
        ConversationSurfaceState, ShellRenderFrame, ShellView,
        graph::{DEFAULT_GRAPH_COLUMN_EXPANDED_DEPTH, GraphColumnSelection, GraphColumnState},
        layout,
        thread_selection::graph_thread_ref_availability,
    },
};

use super::{
    CHECKLIST_ITEM_STATUS_DONE, CHECKLIST_ITEM_STATUS_IN_PROGRESS, CHECKLIST_ITEM_STATUS_TODO,
    GRAPH_OVERLAY_ROW_INDENT, GraphRoleStyle, GraphSummaryTooltipTheme, SOFT_LINK_ROW_MARKER,
    build_graph_summary_tooltip, graph_role_style, stable_id_key,
};

struct GraphNodePalette {
    normal_surface: GraphRoleStyle,
    normal_text: GraphRoleStyle,
    selected_surface: GraphRoleStyle,
    selected_text: GraphRoleStyle,
}

impl GraphNodePalette {
    fn background(&self, selected: bool) -> gpui::Rgba {
        self.surface_style(selected).background
    }

    fn border(&self, selected: bool) -> gpui::Rgba {
        self.surface_style(selected).border
    }

    fn foreground(&self, selected: bool) -> gpui::Rgba {
        self.text_style(selected).foreground
    }

    fn font_weight(&self, selected: bool) -> gpui::FontWeight {
        self.text_style(selected).font_weight
    }

    fn surface_style(&self, selected: bool) -> GraphRoleStyle {
        if selected {
            self.selected_surface
        } else {
            self.normal_surface
        }
    }

    fn text_style(&self, selected: bool) -> GraphRoleStyle {
        if selected {
            self.selected_text
        } else {
            self.normal_text
        }
    }
}

pub(super) fn render_graph_node_tree(
    shell: &ShellRenderFrame<'_>,
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
    shell: &ShellRenderFrame<'_>,
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
    let palette = graph_node_palette(shell, node);
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
                        .font_weight(palette.font_weight(selected))
                        .text_color(palette.foreground(selected))
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
                .bg(palette.background(selected))
                .border_1()
                .border_color(palette.border(selected))
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
    shell: &ShellRenderFrame<'_>,
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
    let normal_style = graph_role_style(shell, BerylThemeRole::GraphRowSoftLink);
    let normal_text_style = graph_role_style(shell, BerylThemeRole::GraphRowSoftLinkText);
    let selected_style = graph_role_style(shell, BerylThemeRole::GraphRowSelected);
    let selected_text_style = graph_role_style(shell, BerylThemeRole::GraphRowSelectedText);
    let hover_style = graph_role_style(shell, BerylThemeRole::GraphRowHover);
    let surface_style = if selected {
        selected_style
    } else {
        normal_style
    };
    let text_style = if selected {
        selected_text_style
    } else {
        normal_text_style
    };
    let hover_background = if selected {
        selected_style.background
    } else {
        hover_style.background
    };
    let tooltip_theme = GraphSummaryTooltipTheme::from_shell(shell);
    let mut row = div()
        .id((
            ElementId::from(("graph-soft-link-row", column_index)),
            stable_id_key(soft_link.id().as_str()),
        ))
        .w_full()
        .rounded_md()
        .bg(surface_style.background)
        .border_1()
        .border_color(surface_style.border)
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
                        .text_color(text_style.foreground)
                        .child(format!("{SOFT_LINK_ROW_MARKER} ")),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .text_sm()
                        .font_weight(text_style.font_weight)
                        .text_color(text_style.foreground)
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
    shell: &ShellRenderFrame<'_>,
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
    let normal_style = graph_role_style(shell, BerylThemeRole::GraphRowThreadRef);
    let normal_text_style = graph_role_style(shell, BerylThemeRole::GraphRowThreadRefText);
    let meta_text_style = graph_role_style(shell, BerylThemeRole::GraphRowThreadRefMeta);
    let invalid_style = graph_role_style(shell, BerylThemeRole::GraphRowInvalid);
    let invalid_text_style = graph_role_style(shell, BerylThemeRole::GraphRowInvalidText);
    let hover_style = graph_role_style(shell, BerylThemeRole::GraphRowHover);
    let row_surface_style = if availability.is_openable() {
        normal_style
    } else {
        invalid_style
    };
    let row_text_style = if availability.is_openable() {
        normal_text_style
    } else {
        invalid_text_style
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
                .bg(row_surface_style.background)
                .border_1()
                .border_color(row_surface_style.border)
                .cursor_pointer()
                .hover(move |style| style.bg(hover_style.background))
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
                                        .font_weight(row_text_style.font_weight)
                                        .text_color(row_text_style.foreground)
                                        .child(format!("thread {}", thread_ref.label())),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(meta_text_style.foreground)
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
    shell: &ShellRenderFrame<'_>,
    column_index: usize,
    thread_ref_id: beryl_model::semantic_graph::ThreadRefId,
    reason: String,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let indicator_reason = reason.clone();
    let tooltip_theme = GraphSummaryTooltipTheme::from_shell(shell);
    let button_theme = shell.secondary_button_theme();
    let invalid_style = graph_role_style(shell, BerylThemeRole::GraphRowInvalid);
    let invalid_text_style = graph_role_style(shell, BerylThemeRole::GraphRowInvalidText);

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
                .border_color(invalid_style.border)
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .font_weight(invalid_text_style.font_weight)
                .text_color(invalid_text_style.foreground)
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
                .flex_none()
                .h(px(layout::BUTTON_OUTER_HEIGHT))
                .px(px(layout::BUTTON_HORIZONTAL_PADDING))
                .py(px(layout::BUTTON_VERTICAL_PADDING))
                .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
                .border_1()
                .border_color(button_theme.normal.border)
                .bg(button_theme.normal.background)
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
                .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
                .font_weight(button_theme.font_weight)
                .text_color(button_theme.normal.foreground)
                .cursor_pointer()
                .hover(move |style| {
                    style
                        .bg(button_theme.hover.background)
                        .border_color(button_theme.hover.border)
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
    shell: &ShellRenderFrame<'_>,
    column_index: usize,
    node_id: SemanticNodeId,
    depth: usize,
    has_children: bool,
    expanded: bool,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    if !has_children {
        return div()
            .flex_none()
            .w(px(layout::BUTTON_ICON_OUTER_WIDTH))
            .into_any_element();
    }
    let secondary = shell.secondary_button_theme();

    div()
        .id((
            ElementId::from(("graph-expander", depth)),
            stable_id_key(node_id.as_str()),
        ))
        .flex_none()
        .w(px(layout::BUTTON_ICON_OUTER_WIDTH))
        .h(px(layout::BUTTON_OUTER_HEIGHT))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(secondary.normal.background)
        .border_1()
        .border_color(secondary.normal.border)
        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
        .font_weight(secondary.font_weight)
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
    shell: &ShellRenderFrame<'_>,
    status: ChecklistItemStatus,
) -> impl IntoElement {
    let status_color = shell.role_color(checklist_status_role(status), shell.surface_foreground());
    div()
        .flex_none()
        .text_sm()
        .text_color(status_color)
        .child(format!("{} ", checklist_item_status_glyph(status)))
}

fn graph_node_palette(shell: &ShellRenderFrame<'_>, node: &SemanticNode) -> GraphNodePalette {
    let surface_role = if node.facets().has_checklist() {
        BerylThemeRole::GraphRowChecklist
    } else if node.facets().has_checklist_item() {
        BerylThemeRole::GraphRowChecklistItem
    } else {
        BerylThemeRole::GraphRowTopic
    };
    let text_role = if node.facets().has_checklist() {
        BerylThemeRole::GraphRowChecklistText
    } else if node.facets().has_checklist_item() {
        BerylThemeRole::GraphRowChecklistItemText
    } else {
        BerylThemeRole::GraphRowTopicText
    };
    GraphNodePalette {
        normal_surface: graph_role_style(shell, surface_role),
        normal_text: graph_role_style(shell, text_role),
        selected_surface: graph_role_style(shell, BerylThemeRole::GraphRowSelected),
        selected_text: graph_role_style(shell, BerylThemeRole::GraphRowSelectedText),
    }
}

fn checklist_status_role(status: ChecklistItemStatus) -> BerylThemeRole {
    match status {
        ChecklistItemStatus::Todo => BerylThemeRole::ChecklistStatusTodo,
        ChecklistItemStatus::InProgress => BerylThemeRole::ChecklistStatusInProgress,
        ChecklistItemStatus::Done => BerylThemeRole::ChecklistStatusDone,
    }
}

fn checklist_item_status_glyph(status: ChecklistItemStatus) -> &'static str {
    match status {
        ChecklistItemStatus::Todo => CHECKLIST_ITEM_STATUS_TODO,
        ChecklistItemStatus::InProgress => CHECKLIST_ITEM_STATUS_IN_PROGRESS,
        ChecklistItemStatus::Done => CHECKLIST_ITEM_STATUS_DONE,
    }
}
