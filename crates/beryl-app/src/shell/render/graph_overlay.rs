use std::hash::{Hash, Hasher};

use gpui::{
    AnyElement, AnyView, App, Context, CursorStyle, DispatchPhase, ElementId, InteractiveElement,
    KeyDownEvent, MouseDownEvent, Render, StatefulInteractiveElement, Window, canvas, div,
    prelude::*, px, rgb,
};

use crate::shell::{
    ConversationSurfaceState, LoadedWorkspaceState, ScrollbarRegion, ShellView,
    column_selector::ColumnSelectorSurface,
    graph::{GraphColumnKey, GraphColumnState},
    graph_node_action_policy::semantic_node_summary_tooltip_allowed,
    layout,
};

mod rows;

use super::column_selector::render_column_selector_trail;
use super::common::card;
use super::scrollbars::{ScrollbarAxis, render_div_scrollbar};
use rows::render_graph_node_tree;

const GRAPH_OVERLAY_TOGGLE_KEYSTROKE: &str = "ctrl-shift-g";
const GRAPH_OVERLAY_COLUMN_WIDTH: f32 = 320.0;
const GRAPH_OVERLAY_COLUMN_GAP: f32 = 16.0;
const GRAPH_OVERLAY_ROW_INDENT: f32 = 18.0;
const GRAPH_OVERLAY_RESIZE_HANDLE_HEIGHT: f32 = 10.0;
const SOFT_LINK_ROW_MARKER: &str = "\u{219D}";
const CHECKLIST_ITEM_STATUS_TODO: &str = "\u{25A1}";
const CHECKLIST_ITEM_STATUS_IN_PROGRESS: &str = "\u{25A3}";
const CHECKLIST_ITEM_STATUS_DONE: &str = "\u{25A0}";

struct GraphSummaryTooltip {
    summary: String,
    theme: GraphSummaryTooltipTheme,
}

#[derive(Clone, Copy)]
struct GraphSummaryTooltipTheme {
    background: gpui::Rgba,
    border: gpui::Rgba,
    foreground: gpui::Rgba,
}

impl GraphSummaryTooltipTheme {
    fn from_shell(shell: &ShellView) -> Self {
        Self {
            background: shell.popup_surface_background(),
            border: shell.surface_border(),
            foreground: shell.general_ui_foreground(),
        }
    }
}

pub(super) fn render_graph_overlay_listeners(cx: &mut Context<ShellView>) -> impl IntoElement {
    let entity = cx.entity();

    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            window.on_key_event({
                let entity = entity.clone();
                move |event: &KeyDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }

                    let handled = entity.update(cx, |view, cx| {
                        view.handle_graph_overlay_key_down(event, window, cx)
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

pub(super) fn render_graph_overlay(
    shell: &ShellView,
    loaded_workspace: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
    composer_height: gpui::Pixels,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    let overlay = surface.graph_overlay();
    if !overlay.visible() {
        return None;
    }

    let viewport = window.viewport_size();
    let split = layout::split_layout(
        viewport.width,
        surface.checklist_sidebar_ratio(),
        surface.checklist_sidebar_visible(),
    );
    let overlay_width = split.left_width.max(px(layout::PANEL_MIN_WIDTH));
    let overlay_height = surface.graph_overlay_height(composer_height);
    let entity = cx.entity();

    let mut body = div()
        .size_full()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .child(render_overlay_header(shell, loaded_workspace, surface));

    if !overlay.graph_columns_available() {
        body = body.child(card(
            shell,
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(shell.general_ui_foreground())
                        .child("No semantic graph yet"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(shell.surface_muted_foreground())
                        .child(
                            "Semantic graph content appears here after graph-backed workspace activity creates durable nodes.",
                        ),
                ),
        ));
    } else {
        body = body.child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .child(render_graph_columns(shell, surface, cx)),
        );
    }

    let panel = div()
        .id("graph-overlay-panel")
        .size_full()
        .min_h(px(0.0))
        .overflow_hidden()
        .child(body);

    let shell = div()
        .id("graph-overlay-shell")
        .absolute()
        .top_0()
        .left_0()
        .w(overlay_width)
        .h(overlay_height)
        .min_h(px(0.0))
        .bg(shell.panel_surface_background())
        .border_b_1()
        .border_r_1()
        .border_color(shell.surface_border())
        .overflow_hidden()
        .occlude()
        .child(
            // Keep the outer shell absolutely layered over the conversation column.
            // The inner wrapper provides the relative positioning context for the resize handle.
            div()
                .relative()
                .size_full()
                .child(panel)
                .child(render_graph_overlay_resize_handle(shell, entity)),
        );

    Some(shell.into_any_element())
}

fn render_graph_overlay_resize_handle(
    shell: &ShellView,
    entity: gpui::Entity<ShellView>,
) -> impl IntoElement {
    div()
        .absolute()
        .left_0()
        .bottom_0()
        .w_full()
        .h(px(GRAPH_OVERLAY_RESIZE_HANDLE_HEIGHT))
        .cursor(CursorStyle::ResizeRow)
        .flex()
        .items_center()
        .justify_center()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _cx| {
                    window.on_mouse_event({
                        let entity = entity.clone();
                        move |event: &MouseDownEvent, _, _, cx| {
                            if !bounds.contains(&event.position) {
                                return;
                            }

                            entity.update(cx, |view, cx| {
                                view.begin_surface_graph_overlay_drag(bounds.bottom(), event, cx);
                            });
                        }
                    });
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .child(
            div()
                .w(px(56.0))
                .h(px(4.0))
                .rounded_full()
                .bg(shell.surface_border()),
        )
}

fn render_overlay_header(
    shell: &ShellView,
    loaded_workspace: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
) -> impl IntoElement {
    let overlay = surface.graph_overlay();
    let graph = overlay.graph();

    div()
        .flex()
        .items_start()
        .justify_between()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(shell.general_ui_foreground())
                        .child(format!(
                            "Semantic Graph: {}",
                            loaded_workspace.workspace.title()
                        )),
                )
                .child(render_overlay_header_status(shell, surface)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .items_end()
                .gap_1()
                .child(
                    div()
                        .text_xs()
                        .text_color(shell.surface_muted_foreground())
                        .child(format!("Toggle {GRAPH_OVERLAY_TOGGLE_KEYSTROKE}")),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(if overlay.mutation_pending() {
                            rgb(0xfde68a)
                        } else {
                            shell.surface_muted_foreground()
                        })
                        .child(format!(
                            "{} nodes  {} links  {} threads",
                            graph.node_count(),
                            graph.soft_link_count(),
                            graph.thread_ref_count()
                        )),
                ),
        )
}

fn render_overlay_header_status(
    shell: &ShellView,
    surface: &ConversationSurfaceState,
) -> AnyElement {
    let overlay = surface.graph_overlay();
    let (message, color) = if let Some(error) = overlay.last_error() {
        (format!("Graph issue: {error}"), rgb(0xfda4af))
    } else if let Some(status) = overlay.status_message() {
        (status.to_string(), rgb(0xfde68a))
    } else {
        (
            "Explorer columns open from node and soft-link selections.".to_string(),
            shell.surface_muted_foreground(),
        )
    };

    div()
        .max_w(px(420.0))
        .text_xs()
        .text_color(color)
        .whitespace_nowrap()
        .truncate()
        .child(message)
        .into_any_element()
}

fn render_graph_columns(
    shell: &ShellView,
    surface: &ConversationSurfaceState,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let overlay = surface.graph_overlay();
    let scroll_handle = surface.graph_columns_scroll_handle();
    let scrollbar_opacity = shell.scrollbar_opacity(&ScrollbarRegion::GraphColumns);
    let columns = overlay
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| {
            render_graph_column(shell, index, surface, column, cx).into_any_element()
        })
        .collect();

    render_column_selector_trail(
        ColumnSelectorSurface::GraphOverlay,
        "graph-overlay-columns",
        GRAPH_OVERLAY_COLUMN_WIDTH,
        GRAPH_OVERLAY_COLUMN_GAP,
        columns,
        scroll_handle,
        scrollbar_opacity,
        cx,
    )
}

fn render_graph_column(
    shell: &ShellView,
    column_index: usize,
    surface: &ConversationSurfaceState,
    column: &GraphColumnState,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let graph = surface.graph_overlay().graph();
    let scroll_handle = surface
        .graph_column_scroll_handle(column_index)
        .unwrap_or_default();
    let column_key = column.root_key().clone();
    let scrollbar_opacity =
        shell.scrollbar_opacity(&ScrollbarRegion::GraphColumn(column_key.clone()));
    let semantic_node_tooltips_allowed =
        semantic_node_summary_tooltip_allowed(surface.graph_thread_link_menu().is_open());
    let tooltip_theme = GraphSummaryTooltipTheme::from_shell(shell);
    debug_assert!(column.root_key().renders_fixed_header());
    let (header, body) = match column.root_key() {
        GraphColumnKey::RootLevel => {
            let header = render_graph_column_header(
                shell,
                column_index,
                column.root_key(),
                None,
                None,
                semantic_node_tooltips_allowed,
                tooltip_theme,
            );
            let mut root_rows = div().w_full().min_h_full().flex().flex_col().gap_2().p_3();
            for root_node in graph.root_nodes() {
                root_rows = root_rows.child(render_graph_node_tree(
                    shell,
                    column_index,
                    column,
                    surface,
                    graph,
                    root_node,
                    0,
                    semantic_node_tooltips_allowed,
                    cx,
                ));
            }
            (header, root_rows.into_any_element())
        }
        GraphColumnKey::Node(root_node_id) => {
            let root_node = graph
                .node(root_node_id)
                .expect("graph overlay columns are reconciled against the in-memory graph");
            let root_summary = root_node.summary().trim().to_string();
            let header = render_graph_column_header(
                shell,
                column_index,
                column.root_key(),
                Some(root_node.title().to_string()),
                Some(root_summary),
                semantic_node_tooltips_allowed,
                tooltip_theme,
            );
            let body = div()
                .w_full()
                .min_h_full()
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .child(render_graph_node_tree(
                    shell,
                    column_index,
                    column,
                    surface,
                    graph,
                    root_node,
                    0,
                    semantic_node_tooltips_allowed,
                    cx,
                ))
                .into_any_element();
            (header, body)
        }
    };

    div()
        .w(px(GRAPH_OVERLAY_COLUMN_WIDTH))
        .h_full()
        .min_h(px(0.0))
        .flex_none()
        .bg(shell.panel_surface_background())
        .border_1()
        .border_color(shell.surface_border())
        .overflow_hidden()
        .child(
            div()
                .size_full()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .child(header)
                .child({
                    let mut scroll_region = div()
                        .relative()
                        .flex_1()
                        .min_h(px(0.0))
                        .on_mouse_move(cx.listener({
                            let column_key = column_key.clone();
                            move |view, event, window, cx| {
                                view.note_graph_column_scrollbar_motion(
                                    column_key.clone(),
                                    event,
                                    window,
                                    cx,
                                );
                            }
                        }))
                        .on_scroll_wheel(cx.listener({
                            let column_key = column_key.clone();
                            move |view, event, window, cx| {
                                view.note_graph_column_scrollbar_scroll(
                                    column_key.clone(),
                                    event,
                                    window,
                                    cx,
                                );
                            }
                        }))
                        .child(
                            div()
                                .id(("graph-column-scroll", column_index))
                                .size_full()
                                .min_h(px(0.0))
                                .track_scroll(&scroll_handle)
                                .overflow_y_scroll()
                                .child(body),
                        );
                    if let Some(scrollbar) = render_div_scrollbar(
                        &scroll_handle,
                        ScrollbarAxis::Vertical,
                        scrollbar_opacity,
                    ) {
                        scroll_region = scroll_region.child(scrollbar);
                    }
                    scroll_region
                }),
        )
}

fn render_graph_column_header(
    shell: &ShellView,
    column_index: usize,
    column_key: &GraphColumnKey,
    title: Option<String>,
    summary: Option<String>,
    semantic_node_tooltips_allowed: bool,
    tooltip_theme: GraphSummaryTooltipTheme,
) -> AnyElement {
    let header_key = match column_key {
        GraphColumnKey::RootLevel => "__root_level",
        GraphColumnKey::Node(node_id) => node_id.as_str(),
    };
    let mut header_title = div()
        .id((
            ElementId::from(("graph-column-header", column_index)),
            stable_id_key(header_key),
        ))
        .min_w(px(0.0))
        .flex_1()
        .text_sm()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(shell.general_ui_foreground())
        .whitespace_nowrap()
        .truncate()
        .child(title.unwrap_or_else(|| " ".to_string()));

    if let Some(summary) = summary
        && semantic_node_tooltips_allowed
        && !summary.is_empty()
    {
        header_title = header_title
            .tooltip(move |_, cx| build_graph_summary_tooltip(summary.clone(), tooltip_theme, cx));
    }

    div()
        .w_full()
        .px_4()
        .py_2()
        .border_b_1()
        .border_color(shell.surface_border())
        .bg(shell.popup_surface_background())
        .child(div().flex().gap_2().items_center().child(header_title))
        .into_any_element()
}

fn build_graph_summary_tooltip(
    summary: String,
    theme: GraphSummaryTooltipTheme,
    cx: &mut App,
) -> AnyView {
    cx.new(|_| GraphSummaryTooltip { summary, theme }).into()
}

impl Render for GraphSummaryTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(280.0))
            .rounded_md()
            .bg(self.theme.background)
            .border_1()
            .border_color(self.theme.border)
            .px_3()
            .py_2()
            .text_xs()
            .text_color(self.theme.foreground)
            .child(self.summary.clone())
    }
}

fn stable_id_key(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish().to_string()
}
