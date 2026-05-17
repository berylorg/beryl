use gpui::{
    AnyElement, Context, DispatchPhase, ElementId, InteractiveElement, KeyDownEvent,
    MouseDownEvent, ScrollHandle, StatefulInteractiveElement, Window, anchored, canvas, div, point,
    prelude::*, px,
};

use crate::{
    BerylThemeRole,
    member_thread_inventory::MemberThreadInventoryGroup,
    shell::{
        ConversationSurfaceState, LoadedWorkspaceState, ScrollbarRegion, ShellRenderFrame,
        ShellView,
        column_selector::ColumnSelectorSurface,
        layout,
        thread_selector::{
            ThreadSelectorColumnKey, ThreadSelectorColumnState, ThreadSelectorProjectionThread,
            ThreadSelectorSelection,
        },
    },
};

use super::{
    column_selector::render_column_selector_trail,
    scrollbars::{ScrollbarAxis, ScrollbarVisibilityPolicy, render_themed_div_scrollbar},
};

const THREAD_SELECTOR_COLUMN_WIDTH: f32 = 284.0;
const THREAD_SELECTOR_COLUMN_GAP: f32 = 16.0;
const THREAD_SELECTOR_PREFERRED_WIDTH: f32 = 620.0;
const THREAD_SELECTOR_MIN_WIDTH: f32 = 320.0;
const THREAD_SELECTOR_MAX_WIDTH: f32 = 720.0;
const THREAD_SELECTOR_MAX_HEIGHT_RATIO: f32 = 0.52;
const THREAD_SELECTOR_MIN_HEIGHT: f32 = 220.0;
const THREAD_SELECTOR_PREFERRED_HEIGHT: f32 = 420.0;
const THREAD_SELECTOR_CHILD_COUNT_DIGIT_WIDTH: f32 = 8.0;
const THREAD_SELECTOR_CHILD_COUNT_HORIZONTAL_PADDING: f32 = 16.0;

pub(super) fn render_thread_selector_listeners(cx: &mut Context<ShellView>) -> impl IntoElement {
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
                        view.handle_thread_selector_mouse_down(event, window, cx);
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
                        view.handle_thread_selector_key_down(event, window, cx)
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

pub(super) fn render_thread_selector_overlay(
    shell: &ShellRenderFrame<'_>,
    loaded: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    if !surface.thread_selector().is_open() {
        return None;
    }

    let anchor_bounds = surface.thread_selector().anchor_bounds()?;
    let viewport_size = window.viewport_size();
    let width = thread_selector_popup_width(viewport_size.width);
    let height = thread_selector_popup_height(viewport_size.height);
    let entity = cx.entity();

    Some(
        anchored()
            .position(anchor_bounds.bottom_left())
            .offset(point(px(0.0), px(6.0)))
            .snap_to_window_with_margin(px(8.0))
            .child(
                div()
                    .on_children_prepainted(move |children, _, cx| {
                        let bounds = children.first().copied();
                        entity.update(cx, |view, cx| {
                            view.record_thread_selector_bounds(bounds, cx)
                        });
                    })
                    .child(
                        div()
                            .id("thread-selector-panel")
                            .w(width)
                            .h(height)
                            .flex()
                            .flex_col()
                            .occlude()
                            .overflow_hidden()
                            .border_1()
                            .bg(shell.popup_surface_background())
                            .border_color(shell.surface_border())
                            .rounded_lg()
                            .shadow_lg()
                            .child(render_header(shell, loaded, surface))
                            .child(render_columns(shell, surface, cx)),
                    ),
            )
            .into_any_element(),
    )
}

fn thread_selector_popup_width(viewport_width: gpui::Pixels) -> gpui::Pixels {
    px(THREAD_SELECTOR_PREFERRED_WIDTH)
        .min(viewport_width - px(16.0))
        .max(px(THREAD_SELECTOR_MIN_WIDTH.min(THREAD_SELECTOR_MAX_WIDTH)))
        .min(px(THREAD_SELECTOR_MAX_WIDTH))
}

fn thread_selector_popup_height(viewport_height: gpui::Pixels) -> gpui::Pixels {
    let max_height = (viewport_height * THREAD_SELECTOR_MAX_HEIGHT_RATIO).max(px(0.0));
    px(THREAD_SELECTOR_PREFERRED_HEIGHT)
        .min(max_height)
        .max(px(THREAD_SELECTOR_MIN_HEIGHT).min(max_height))
}

fn render_header(
    shell: &ShellRenderFrame<'_>,
    loaded: &LoadedWorkspaceState,
    surface: &ConversationSurfaceState,
) -> impl IntoElement {
    let inventory = surface.member_thread_inventory();
    let mut status = if inventory.refreshing() {
        Some("Refreshing thread list...".to_string())
    } else {
        inventory.last_error().map(str::to_string)
    };
    if loaded.selected_runtime().is_none() {
        status = Some("Select a runtime environment before opening existing threads.".to_string());
    }

    let mut header = div()
        .w_full()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(shell.surface_border())
        .bg(shell.panel_surface_background())
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_weight(shell.role_font_weight(
                            BerylThemeRole::ThreadSelectorSurface,
                            gpui::FontWeight::SEMIBOLD,
                        ))
                        .text_color(shell.general_ui_foreground())
                        .child("Threads"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(shell.surface_muted_foreground())
                        .whitespace_nowrap()
                        .truncate()
                        .child(format!(
                            "{} member groups",
                            surface.member_thread_inventory().snapshot().groups().len()
                        )),
                ),
        );

    if let Some(status) = status {
        header = header.child(
            div()
                .max_w(px(280.0))
                .text_xs()
                .text_color(shell.surface_muted_foreground())
                .whitespace_nowrap()
                .truncate()
                .child(status),
        );
    }

    header
}

fn render_columns(
    shell: &ShellRenderFrame<'_>,
    surface: &ConversationSurfaceState,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let scroll_handle = surface.thread_selector_columns_scroll_handle();
    let scrollbar_visibility =
        shell.scrollbar_visibility_policy(&ScrollbarRegion::ThreadSelectorColumns, cx);
    let columns = surface
        .thread_selector()
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| render_column(shell, index, surface, column, cx).into_any_element())
        .collect();

    div()
        .flex_1()
        .min_h(px(0.0))
        .p_3()
        .child(render_column_selector_trail(
            shell,
            ColumnSelectorSurface::ThreadSelector,
            "thread-selector-columns",
            THREAD_SELECTOR_COLUMN_WIDTH,
            THREAD_SELECTOR_COLUMN_GAP,
            columns,
            scroll_handle,
            scrollbar_visibility,
            cx,
        ))
}

fn render_column(
    shell: &ShellRenderFrame<'_>,
    column_index: usize,
    surface: &ConversationSurfaceState,
    column: &ThreadSelectorColumnState,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let column_key = column.root_key().clone();
    let scroll_handle = surface
        .thread_selector_column_scroll_handle(column_index)
        .unwrap_or_else(ScrollHandle::new);
    let scrollbar_visibility = shell.scrollbar_visibility_policy(
        &ScrollbarRegion::ThreadSelectorColumn(column_key.clone()),
        cx,
    );
    let header_label = column_header_label(surface, &column_key);

    div()
        .w(px(THREAD_SELECTOR_COLUMN_WIDTH))
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
                .child(
                    div()
                        .w_full()
                        .px_4()
                        .py_2()
                        .border_b_1()
                        .border_color(shell.surface_border())
                        .bg(shell.popup_surface_background())
                        .child(
                            div()
                                .min_w(px(0.0))
                                .text_sm()
                                .font_weight(shell.role_font_weight(
                                    BerylThemeRole::ThreadSelectorSurface,
                                    gpui::FontWeight::SEMIBOLD,
                                ))
                                .text_color(shell.general_ui_foreground())
                                .whitespace_nowrap()
                                .truncate()
                                .child(header_label),
                        ),
                )
                .child(render_column_rows(
                    shell,
                    surface,
                    column_index,
                    column,
                    column_key,
                    scroll_handle,
                    scrollbar_visibility,
                    cx,
                )),
        )
}

fn render_column_rows(
    shell: &ShellRenderFrame<'_>,
    surface: &ConversationSurfaceState,
    column_index: usize,
    column: &ThreadSelectorColumnState,
    column_key: ThreadSelectorColumnKey,
    scroll_handle: ScrollHandle,
    scrollbar_visibility: ScrollbarVisibilityPolicy,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let viewport_height = thread_selector_row_viewport_height(&scroll_handle);
    let scroll_offset = -scroll_handle.offset().y;
    let mut rows = div().w_full().p_3();
    let snapshot = surface.member_thread_inventory().snapshot();

    match &column_key {
        ThreadSelectorColumnKey::Members => {
            rows = rows.child(render_member_rows(
                shell,
                column_index,
                column,
                snapshot.groups(),
                viewport_height,
                scroll_offset,
                cx,
            ));
        }
        ThreadSelectorColumnKey::Threads { .. } => {
            rows = rows.child(render_thread_group_rows(
                shell,
                surface,
                column_index,
                &column_key,
                viewport_height,
                scroll_offset,
                cx,
            ));
        }
    }

    let mut scroll_region = div()
        .relative()
        .flex_1()
        .min_h(px(0.0))
        .on_mouse_move(cx.listener({
            let column_key = column_key.clone();
            move |view, event, window, cx| {
                view.note_thread_selector_column_scrollbar_motion(
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
                view.note_thread_selector_column_scrollbar_scroll(
                    column_key.clone(),
                    event,
                    window,
                    cx,
                );
            }
        }))
        .child(
            div()
                .id((
                    ElementId::from(("thread-selector-column-scroll", column_index)),
                    thread_selector_column_stable_key(&column_key),
                ))
                .size_full()
                .min_h(px(0.0))
                .track_scroll(&scroll_handle)
                .overflow_y_scroll()
                .child(rows),
        );
    if let Some(scrollbar) = render_themed_div_scrollbar(
        shell.style(),
        (
            ElementId::from(("thread-selector-column-scrollbar", column_index)),
            thread_selector_column_stable_key(&column_key),
        ),
        &scroll_handle,
        ScrollbarAxis::Vertical,
        scrollbar_visibility,
    ) {
        scroll_region = scroll_region.child(scrollbar);
    }
    scroll_region
}

fn thread_selector_row_viewport_height(scroll_handle: &ScrollHandle) -> gpui::Pixels {
    let viewport_height = scroll_handle.bounds().size.height;
    if viewport_height > px(0.0) {
        viewport_height
    } else {
        px(THREAD_SELECTOR_PREFERRED_HEIGHT)
    }
}

fn thread_selector_row_list(row_window: &layout::ThreadSelectorRowWindow) -> gpui::Div {
    div()
        .w_full()
        .h(row_window.content_height)
        .min_h(row_window.content_height)
        .flex()
        .flex_col()
        .child(thread_selector_spacer(row_window.top_spacer_height))
}

fn append_thread_selector_row_gap(
    rows: gpui::Div,
    row_index: usize,
    row_count: usize,
) -> gpui::Div {
    if row_index + 1 < row_count {
        rows.child(thread_selector_spacer(px(layout::THREAD_SELECTOR_ROW_GAP)))
    } else {
        rows
    }
}

fn thread_selector_spacer(height: gpui::Pixels) -> gpui::Div {
    div().w_full().h(height).min_h(height)
}

fn render_member_rows(
    shell: &ShellRenderFrame<'_>,
    column_index: usize,
    column: &ThreadSelectorColumnState,
    groups: &[MemberThreadInventoryGroup],
    viewport_height: gpui::Pixels,
    scroll_offset: gpui::Pixels,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    if groups.is_empty() {
        return disabled_row(shell, "No available members").into_any_element();
    }

    let row_window = layout::thread_selector_row_window(
        groups.len(),
        viewport_height,
        scroll_offset,
        layout::THREAD_SELECTOR_OVERSCAN_ROWS,
    );
    let mut rows = thread_selector_row_list(&row_window);
    for index in row_window.range.clone() {
        if let Some(group) = groups.get(index) {
            rows = rows.child(render_member_row(
                shell,
                column_index,
                index,
                column,
                group,
                cx,
            ));
            rows = append_thread_selector_row_gap(rows, index, groups.len());
        }
    }
    rows.child(thread_selector_spacer(row_window.bottom_spacer_height))
        .into_any_element()
}

fn render_thread_group_rows(
    shell: &ShellRenderFrame<'_>,
    surface: &ConversationSurfaceState,
    column_index: usize,
    column_key: &ThreadSelectorColumnKey,
    viewport_height: gpui::Pixels,
    scroll_offset: gpui::Pixels,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    let snapshot = surface.member_thread_inventory().snapshot();
    let ThreadSelectorColumnKey::Threads {
        member_key,
        parent_thread_id,
    } = column_key
    else {
        return disabled_row(shell, "No threads").into_any_element();
    };
    if snapshot.group(member_key).is_none() {
        return disabled_row(shell, "Member unavailable").into_any_element();
    }

    let projection = surface.thread_selector().projection();
    let row_ids = projection.row_ids_for_column(column_key);
    if row_ids.is_empty() {
        let label = if parent_thread_id.is_some() {
            "No forks"
        } else {
            "No threads"
        };
        return disabled_row(shell, label).into_any_element();
    }

    let child_count_cell_width = projection
        .child_count_digit_count_for_column(column_key)
        .map(thread_child_count_cell_width);
    let row_window = layout::thread_selector_row_window(
        row_ids.len(),
        viewport_height,
        scroll_offset,
        layout::THREAD_SELECTOR_OVERSCAN_ROWS,
    );

    let mut rows = thread_selector_row_list(&row_window);
    for index in row_window.range.clone() {
        let Some(thread_id) = row_ids.get(index) else {
            continue;
        };
        let Some(thread) = projection.thread(member_key, thread_id) else {
            continue;
        };
        let child_count = projection.direct_child_count(member_key, thread.thread_id());
        rows = rows.child(render_thread_row(
            shell,
            surface,
            column_index,
            index,
            thread,
            child_count,
            child_count_cell_width,
            cx,
        ));
        rows = append_thread_selector_row_gap(rows, index, row_ids.len());
    }
    rows.child(thread_selector_spacer(row_window.bottom_spacer_height))
        .into_any_element()
}

fn render_member_row(
    shell: &ShellRenderFrame<'_>,
    column_index: usize,
    row_index: usize,
    column: &ThreadSelectorColumnState,
    group: &MemberThreadInventoryGroup,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let member_key = group.key().clone();
    let selected = column.selection() == Some(&ThreadSelectorSelection::Member(member_key.clone()));
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
        shell.general_ui_foreground()
    };
    let count_foreground = if selected {
        primary.normal.foreground
    } else {
        shell.surface_muted_foreground()
    };
    let hover_background = if selected {
        primary.hover.background
    } else {
        secondary.hover.background
    };

    div()
        .id((
            ElementId::from(("thread-selector-member-row", column_index)),
            row_index.to_string(),
        ))
        .h(px(layout::THREAD_SELECTOR_ROW_HEIGHT))
        .min_h(px(layout::THREAD_SELECTOR_ROW_HEIGHT))
        .w_full()
        .rounded_md()
        .px_3()
        .py_2()
        .bg(background)
        .border_1()
        .border_color(border)
        .cursor_pointer()
        .hover(move |style| style.bg(hover_background))
        .child(
            div()
                .h_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w(px(0.0))
                        .flex_1()
                        .text_sm()
                        .text_color(foreground)
                        .whitespace_nowrap()
                        .truncate()
                        .child(group.label().to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(count_foreground)
                        .child(group.threads().len().to_string()),
                ),
        )
        .on_click(cx.listener(move |view, event, window, cx| {
            view.select_thread_selector_member(column_index, member_key.clone(), event, window, cx);
        }))
}

fn render_thread_row(
    shell: &ShellRenderFrame<'_>,
    surface: &ConversationSurfaceState,
    column_index: usize,
    row_index: usize,
    thread: &ThreadSelectorProjectionThread,
    child_count: usize,
    child_count_cell_width: Option<gpui::Pixels>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let thread_id = thread.thread_id().clone();
    let row_state = surface
        .thread_selector()
        .thread_row_state(column_index, &thread_id);
    let secondary = shell.secondary_button_theme();
    let background = if row_state.active {
        shell.role_background(
            BerylThemeRole::StatusValueOk,
            shell.row_surface_background(),
        )
    } else if row_state.selected {
        shell.role_background(
            BerylThemeRole::ThreadSelectorRowSelected,
            shell.primary_button_theme().normal.background,
        )
    } else {
        shell.row_surface_background()
    };
    let border = if row_state.active {
        shell.role_border(
            BerylThemeRole::StatusValueOk,
            shell.primary_button_theme().active.border,
        )
    } else if row_state.selected {
        shell.role_border(
            BerylThemeRole::ThreadSelectorRowSelected,
            shell.primary_button_theme().normal.border,
        )
    } else {
        shell.surface_border()
    };
    let foreground = if row_state.selected && !row_state.active {
        shell.role_foreground(
            BerylThemeRole::ThreadSelectorRowSelected,
            shell.primary_button_theme().normal.foreground,
        )
    } else {
        shell.general_ui_foreground()
    };
    let count_foreground = if row_state.active || row_state.selected {
        foreground
    } else {
        shell.surface_muted_foreground()
    };
    let separator = if row_state.active {
        shell.role_border(
            BerylThemeRole::StatusValueOk,
            shell.primary_button_theme().active.border,
        )
    } else if row_state.selected {
        shell.role_border(
            BerylThemeRole::ThreadSelectorRowSelected,
            shell.primary_button_theme().normal.border,
        )
    } else {
        shell.surface_border()
    };
    let hover_background = if row_state.active {
        shell.role_background(BerylThemeRole::SurfaceRowHover, secondary.hover.background)
    } else if row_state.selected {
        shell.role_background(
            BerylThemeRole::ThreadSelectorRowSelected,
            shell.primary_button_theme().hover.background,
        )
    } else {
        secondary.hover.background
    };

    let mut content = div().min_w(px(0.0)).h_full().flex().child(
        div()
            .min_w(px(0.0))
            .flex_1()
            .px_3()
            .py_2()
            .text_sm()
            .text_color(foreground)
            .whitespace_nowrap()
            .truncate()
            .child(thread.title().to_string()),
    );

    if let (true, Some(cell_width)) = (child_count > 0, child_count_cell_width) {
        content = content
            .child(div().h_full().w(px(1.0)).flex_none().bg(separator))
            .child(
                div()
                    .w(cell_width)
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_xs()
                    .text_color(count_foreground)
                    .child(child_count.to_string()),
            );
    }

    div()
        .id((
            ElementId::from(("thread-selector-thread-row", column_index)),
            row_index.to_string(),
        ))
        .h(px(layout::THREAD_SELECTOR_ROW_HEIGHT))
        .min_h(px(layout::THREAD_SELECTOR_ROW_HEIGHT))
        .w_full()
        .rounded_md()
        .bg(background)
        .border_1()
        .border_color(border)
        .cursor_pointer()
        .hover(move |style| style.bg(hover_background))
        .child(content)
        .on_click(cx.listener(move |view, event, window, cx| {
            view.select_thread_selector_thread(column_index, thread_id.clone(), event, window, cx);
        }))
}

fn thread_child_count_cell_width(digit_count: usize) -> gpui::Pixels {
    px(THREAD_SELECTOR_CHILD_COUNT_HORIZONTAL_PADDING
        + THREAD_SELECTOR_CHILD_COUNT_DIGIT_WIDTH * digit_count.max(1) as f32)
}

fn column_header_label(
    surface: &ConversationSurfaceState,
    column_key: &ThreadSelectorColumnKey,
) -> String {
    let snapshot = surface.member_thread_inventory().snapshot();
    match column_key {
        ThreadSelectorColumnKey::Members => "Members".to_string(),
        ThreadSelectorColumnKey::Threads {
            member_key,
            parent_thread_id: None,
        } => snapshot
            .group(member_key)
            .map(|group| group.label().to_string())
            .unwrap_or_else(|| "Threads".to_string()),
        ThreadSelectorColumnKey::Threads {
            member_key,
            parent_thread_id: Some(parent_thread_id),
        } => surface
            .thread_selector()
            .projection()
            .thread(member_key, parent_thread_id)
            .map(|thread| thread.title().to_string())
            .unwrap_or_else(|| "Forks".to_string()),
    }
}

fn disabled_row(shell: &ShellRenderFrame<'_>, label: &str) -> gpui::Div {
    let background = shell.role_background(
        BerylThemeRole::ThreadSelectorRowUnavailable,
        shell.row_surface_background(),
    );
    let border = shell.role_border(
        BerylThemeRole::ThreadSelectorRowUnavailable,
        shell.surface_border(),
    );
    let foreground = shell.role_foreground(
        BerylThemeRole::ThreadSelectorRowUnavailable,
        shell.surface_muted_foreground(),
    );
    div()
        .rounded_md()
        .px_3()
        .py_2()
        .border_1()
        .bg(background)
        .border_color(border)
        .text_sm()
        .text_color(foreground)
        .child(label.to_string())
}

fn thread_selector_column_stable_key(column_key: &ThreadSelectorColumnKey) -> String {
    match column_key {
        ThreadSelectorColumnKey::Members => "members".to_string(),
        ThreadSelectorColumnKey::Threads {
            member_key,
            parent_thread_id,
        } => {
            let member = match member_key {
                crate::member_thread_inventory::MemberThreadInventoryMemberKey::ImplicitHome => {
                    "implicit_home".to_string()
                }
                crate::member_thread_inventory::MemberThreadInventoryMemberKey::Explicit(id) => {
                    id.as_str().to_string()
                }
            };
            match parent_thread_id {
                Some(thread_id) => format!("threads-{member}-fork-{}", thread_id.as_str()),
                None => format!("threads-{member}-root"),
            }
        }
    }
}
