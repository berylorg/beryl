use beryl_model::conversation::PrimaryWorkspaceMember;
use beryl_model::workspace::{RuntimeMode, WorkspaceMember};
use gpui::{
    AnyElement, Context, DispatchPhase, Entity, InteractiveElement, KeyDownEvent, KeyUpEvent,
    MouseButton, MouseDownEvent, StatefulInteractiveElement, Window, anchored, canvas, div, point,
    prelude::*, px, rgb,
};

use crate::shell::{LoadedWorkspaceState, ScrollbarRegion, ShellView, layout, workspace_picker};
use crate::text_input::SingleLineInput;

use super::code_panel::CODE_FONT_FAMILY;
use super::common::{
    button, disabled_secondary_button, framed_text_input, inline_notice, secondary_button,
};
use super::scrollbars::{ScrollbarAxis, ScrollbarVisibilityPolicy, render_div_scrollbar};
use super::workspace_picker_row_menu::{
    render_workspace_row_action_menu, render_workspace_row_action_trigger,
};

const RUNTIME_SELECTOR_ARROW: &str = "\u{25be}";

pub(super) fn render_workspace_picker_button(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let entity = cx.entity();
    let label = if loaded.workspace_picker.is_open() {
        "Workspaces"
    } else {
        "Workspaces"
    };

    div()
        .on_children_prepainted(move |children, _, cx| {
            let bounds = children.first().copied();
            entity.update(cx, |view, cx| {
                view.record_workspace_picker_anchor_bounds(bounds, cx)
            });
        })
        .child(secondary_button(
            shell,
            "workspace-picker-button",
            label,
            cx.listener(ShellView::toggle_workspace_picker),
        ))
        .into_any_element()
}

pub(super) fn render_workspace_picker_listeners(cx: &mut Context<ShellView>) -> impl IntoElement {
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
                        view.handle_workspace_picker_mouse_down(event, window, cx);
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
                        view.handle_workspace_picker_key_down(event, window, cx)
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
                        view.handle_workspace_picker_key_up(event, window, cx)
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

pub(super) fn render_workspace_picker_overlay(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    workspace_filter_input: &Entity<SingleLineInput>,
    workspace_rename_input: &Entity<SingleLineInput>,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    if !loaded.workspace_picker.is_open() {
        return None;
    }

    let anchor_bounds = loaded.workspace_picker.anchor_bounds()?;
    let viewport_size = window.viewport_size();
    let entity = cx.entity();
    let picker_scroll_handle = loaded.workspace_picker_scroll_handle();
    let members_scroll_handle = loaded.workspace_members_scroll_handle();
    let picker_scrollbar_visibility =
        shell.scrollbar_visibility_policy(&ScrollbarRegion::WorkspacePicker, cx);
    let members_scrollbar_visibility =
        shell.scrollbar_visibility_policy(&ScrollbarRegion::WorkspaceMembers, cx);
    let filter_text = workspace_filter_input.read(cx).text().to_string();
    let visible_workspace_indices = workspace_picker::filtered_workspace_indices(
        &loaded.known_workspaces,
        &loaded.workspace_picker_member_paths,
        &filter_text,
    );
    let member_item_count = workspace_picker::workspace_picker_member_list_item_count(
        loaded.explicit_members().len(),
        loaded.selected_runtime().is_some(),
    );
    let runtime_selector_dropdown_row_count = if loaded
        .workspace_picker
        .runtime_selector_dropdown_is_open()
    {
        workspace_picker::runtime_selector_dropdown_row_count(loaded.runtime_selector_distro_list())
    } else {
        0
    };
    let popup_layout = workspace_picker::popup_layout(
        visible_workspace_indices.len(),
        member_item_count,
        runtime_selector_dropdown_row_count,
        viewport_size.width,
        viewport_size.height,
    );

    let rows = visible_workspace_indices.iter().enumerate().fold(
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(render_create_workspace_row(shell, cx)),
        |list, (visible_index, workspace_index)| {
            let workspace = &loaded.known_workspaces[*workspace_index];
            list.child(render_workspace_row(
                shell,
                workspace_picker::workspace_item_index(visible_index),
                *workspace_index,
                workspace,
                loaded
                    .workspace_picker_member_paths
                    .get(workspace.id())
                    .map_or(&[][..], Vec::as_slice),
                loaded,
                workspace_rename_input,
                cx,
            ))
        },
    );

    let workspaces_column = render_workspaces_column(
        shell,
        loaded,
        workspace_filter_input,
        rows,
        picker_scroll_handle,
        picker_scrollbar_visibility,
        popup_layout.workspaces_column_width,
        popup_layout.workspaces_list_height,
        cx,
    );
    let members_column = render_members_column(
        shell,
        loaded,
        members_scroll_handle,
        members_scrollbar_visibility,
        popup_layout.members_column_width,
        popup_layout.members_list_height,
        popup_layout.runtime_selector_dropdown_height,
        cx,
    );

    let picker_popup = anchored()
        .position(anchor_bounds.bottom_left())
        .offset(point(px(0.0), px(layout::WORKSPACE_PICKER_OFFSET_Y)))
        .snap_to_window_with_margin(px(layout::WORKSPACE_PICKER_MARGIN))
        .child(
            div()
                .on_children_prepainted(move |children, _, cx| {
                    let bounds = children.first().copied();
                    entity.update(cx, |view, cx| {
                        view.record_workspace_picker_bounds(bounds, cx)
                    });
                })
                .child(
                    div()
                        .w(popup_layout.width)
                        .h(popup_layout.height)
                        .flex()
                        .occlude()
                        .overflow_hidden()
                        .bg(shell.popup_surface_background())
                        .border_1()
                        .border_color(shell.surface_border())
                        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
                        .shadow_lg()
                        .child(
                            div()
                                .h_full()
                                .flex()
                                .child(workspaces_column)
                                .child(
                                    div()
                                        .h_full()
                                        .w(popup_layout.divider_width)
                                        .bg(shell.separator_color()),
                                )
                                .child(members_column),
                        ),
                ),
        )
        .into_any_element();

    let mut overlay = div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(picker_popup);
    if let Some(row_action_menu) = render_workspace_row_action_menu(shell, loaded, cx) {
        overlay = overlay.child(row_action_menu);
    }
    if let Some(member_action_menu) = render_workspace_member_action_menu(shell, loaded, cx) {
        overlay = overlay.child(member_action_menu);
    }

    Some(overlay.into_any_element())
}

fn render_workspaces_column(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    workspace_filter_input: &Entity<SingleLineInput>,
    rows: gpui::Div,
    picker_scroll_handle: gpui::ScrollHandle,
    scrollbar_visibility: ScrollbarVisibilityPolicy,
    column_width: gpui::Pixels,
    list_height: gpui::Pixels,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    div()
        .w(column_width)
        .h_full()
        .flex()
        .flex_col()
        .min_w(px(0.0))
        .child(render_column_header(
            shell,
            "Workspaces",
            "Most recently opened workspaces first.",
        ))
        .when_some(loaded.workspace_picker_notice(), |this, notice| {
            this.child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(shell.separator_color())
                    .child(inline_notice(notice, rgb(0x3f1d1d), rgb(0xfecaca))),
            )
        })
        .child(
            div()
                .h(px(layout::WORKSPACE_PICKER_FILTER_HEIGHT))
                .px_4()
                .py_3()
                .border_b_1()
                .border_color(shell.separator_color())
                .child(framed_text_input(shell, workspace_filter_input)),
        )
        .child(render_scrollable_workspace_rows(
            rows,
            picker_scroll_handle,
            scrollbar_visibility,
            list_height,
            cx,
        ))
        .into_any_element()
}

fn render_members_column(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    members_scroll_handle: gpui::ScrollHandle,
    scrollbar_visibility: ScrollbarVisibilityPolicy,
    column_width: gpui::Pixels,
    list_height: gpui::Pixels,
    runtime_selector_dropdown_height: gpui::Pixels,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    let dropdown_open = loaded.workspace_picker.runtime_selector_dropdown_is_open();

    div()
        .w(column_width)
        .h_full()
        .flex()
        .flex_col()
        .min_w(px(0.0))
        .child(render_column_header(
            shell,
            "Members",
            "Active workspace filesystem roots.",
        ))
        .when_some(loaded.workspace_members_notice(), |this, notice| {
            this.child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(shell.separator_color())
                    .child(inline_notice(notice, rgb(0x3f1d1d), rgb(0xfecaca))),
            )
        })
        .child(
            div()
                .relative()
                .w_full()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .child(render_runtime_selector_control(shell, loaded, cx))
                .child(render_scrollable_member_rows(
                    shell,
                    loaded,
                    members_scroll_handle,
                    scrollbar_visibility,
                    list_height,
                    cx,
                ))
                .when(dropdown_open, |this| {
                    this.child(render_runtime_selector_dropdown(
                        shell,
                        loaded,
                        runtime_selector_dropdown_height,
                        cx,
                    ))
                }),
        )
        .into_any_element()
}

fn render_column_header(
    shell: &ShellView,
    title: &'static str,
    subtitle: &'static str,
) -> impl IntoElement {
    div()
        .h(px(layout::WORKSPACE_PICKER_HEADER_HEIGHT))
        .px_4()
        .border_b_1()
        .border_color(shell.separator_color())
        .flex()
        .items_center()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .min_w(px(0.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(shell.general_ui_foreground())
                        .whitespace_normal()
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(shell.surface_muted_foreground())
                        .whitespace_normal()
                        .child(subtitle),
                ),
        )
}

fn render_scrollable_workspace_rows(
    rows: gpui::Div,
    picker_scroll_handle: gpui::ScrollHandle,
    scrollbar_visibility: ScrollbarVisibilityPolicy,
    list_height: gpui::Pixels,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let mut scroll_region = div()
        .relative()
        .h(list_height)
        .min_h(px(0.0))
        .on_mouse_move(cx.listener(ShellView::note_workspace_picker_scrollbar_motion))
        .on_scroll_wheel(cx.listener(ShellView::note_workspace_picker_scrollbar_scroll))
        .child(
            div()
                .id("workspace-picker-list")
                .size_full()
                .min_h(px(0.0))
                .track_scroll(&picker_scroll_handle)
                .overflow_y_scroll()
                .child(rows),
        );
    if let Some(scrollbar) = render_div_scrollbar(
        "workspace-picker-scrollbar",
        &picker_scroll_handle,
        ScrollbarAxis::Vertical,
        scrollbar_visibility,
    ) {
        scroll_region = scroll_region.child(scrollbar);
    }
    scroll_region
}

fn render_runtime_selector_control(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let dropdown_open = loaded.workspace_picker.runtime_selector_dropdown_is_open();
    let entity = cx.entity();

    div()
        .h(px(layout::WORKSPACE_PICKER_MEMBERS_CONTROL_HEIGHT))
        .px(px(layout::WORKSPACE_PICKER_MEMBERS_CONTROL_PADDING_X))
        .py(px(layout::WORKSPACE_PICKER_MEMBERS_CONTROL_PADDING_Y))
        .border_b_1()
        .border_color(shell.separator_color())
        .child(div().relative().w_full().child({
            let trigger = render_runtime_selector_trigger(shell, loaded, dropdown_open);
            let trigger = trigger
                .cursor_pointer()
                .hover({
                    let theme = shell.secondary_button_theme();
                    move |style| style.bg(theme.hover.background)
                })
                .on_click(cx.listener(ShellView::toggle_workspace_runtime_selector_dropdown));
            div()
                .on_children_prepainted(move |children, _, cx| {
                    let bounds = children.first().copied();
                    entity.update(cx, |view, cx| {
                        view.record_workspace_runtime_selector_trigger_bounds(bounds, cx)
                    });
                })
                .child(trigger)
        }))
}

fn render_runtime_selector_trigger(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    dropdown_open: bool,
) -> gpui::Stateful<gpui::Div> {
    let theme = shell.secondary_button_theme();
    let label = runtime_selector_current_label(loaded);
    let detail = "Used for new attachments and home fallback.";
    let foreground = theme.normal.foreground;
    let border = if dropdown_open {
        theme.active.border
    } else {
        theme.normal.border
    };
    let background = if dropdown_open {
        theme.active.background
    } else {
        theme.normal.background
    };

    div()
        .id("workspace-runtime-selector-trigger")
        .h(px(layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_TRIGGER_HEIGHT))
        .w_full()
        .px(px(layout::BUTTON_HORIZONTAL_PADDING))
        .py(px(layout::BUTTON_VERTICAL_PADDING))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .when(dropdown_open, |this| {
            this.rounded_bl(px(0.0)).rounded_br(px(0.0))
        })
        .bg(background)
        .border_1()
        .border_color(border)
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_col()
                .min_w(px(0.0))
                .child(
                    div()
                        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
                        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
                        .font_weight(theme.font_weight)
                        .text_color(foreground)
                        .whitespace_nowrap()
                        .truncate()
                        .child(label),
                )
                .child(
                    div()
                        .text_xs()
                        .line_height(px(
                            layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_DETAIL_LINE_HEIGHT,
                        ))
                        .text_color(shell.surface_muted_foreground())
                        .whitespace_nowrap()
                        .truncate()
                        .child(detail),
                ),
        )
        .child(
            div()
                .flex_none()
                .w(px(
                    layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_ARROW_SLOT_WIDTH,
                ))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(
                    layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_ARROW_FONT_SIZE,
                ))
                .text_color(foreground)
                .child(RUNTIME_SELECTOR_ARROW),
        )
}

fn render_create_add_plus_marker(shell: &ShellView, enabled: bool) -> impl IntoElement {
    let color = if enabled {
        rgb(0x72e4b8)
    } else {
        shell.surface_muted_foreground()
    };

    div()
        .flex_none()
        .w(px(layout::WORKSPACE_PICKER_CREATE_ADD_PLUS_SLOT_WIDTH))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(layout::WORKSPACE_PICKER_CREATE_ADD_PLUS_FONT_SIZE))
        .line_height(px(layout::WORKSPACE_PICKER_CREATE_ADD_PLUS_FONT_SIZE))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(color)
        .child(
            div()
                .relative()
                .top(px(layout::WORKSPACE_PICKER_CREATE_ADD_PLUS_GLYPH_Y_OFFSET))
                .child("+"),
        )
}

fn render_runtime_selector_dropdown(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    dropdown_height: gpui::Pixels,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let entity = cx.entity();
    let distro_list = loaded.runtime_selector_distro_list();
    let dropdown = loaded.workspace_picker.runtime_selector_dropdown();
    let scroll_handle = loaded.workspace_runtime_selector_scroll_handle();
    let scrollbar_visibility =
        shell.scrollbar_visibility_policy(&ScrollbarRegion::WorkspaceRuntimeSelector, cx);
    let mut dropdown_panel = div()
        .id("workspace-runtime-selector-dropdown")
        .relative()
        .w_full()
        .h(dropdown_height)
        .min_h(px(0.0))
        .occlude()
        .overflow_hidden()
        .bg(shell.popup_surface_background())
        .border_1()
        .border_color(shell.secondary_button_theme().active.border)
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .rounded_tl(px(0.0))
        .rounded_tr(px(0.0))
        .shadow_lg()
        .on_mouse_move(cx.listener(ShellView::note_workspace_runtime_selector_scrollbar_motion))
        .on_scroll_wheel(cx.listener(ShellView::note_workspace_runtime_selector_scrollbar_scroll))
        .child(
            div()
                .id("workspace-runtime-selector-dropdown-scroll")
                .size_full()
                .min_h(px(0.0))
                .track_scroll(&scroll_handle)
                .overflow_y_scroll()
                .child(render_runtime_selector_row(
                    shell,
                    loaded,
                    workspace_picker::RuntimeSelectorRow::HostWindows,
                    0,
                    dropdown.highlighted_index() == 0,
                    cx,
                ))
                .children(render_runtime_selector_wsl_rows(
                    shell,
                    loaded,
                    dropdown.highlighted_index(),
                    cx,
                    distro_list,
                )),
        );
    if let Some(scrollbar) = render_div_scrollbar(
        "workspace-runtime-selector-scrollbar",
        &scroll_handle,
        ScrollbarAxis::Vertical,
        scrollbar_visibility,
    ) {
        dropdown_panel = dropdown_panel.child(scrollbar);
    }

    div()
        .absolute()
        .top(px(
            layout::WORKSPACE_PICKER_RUNTIME_SELECTOR_DROPDOWN_COLUMN_TOP,
        ))
        .left(px(layout::WORKSPACE_PICKER_MEMBERS_CONTROL_PADDING_X))
        .right(px(layout::WORKSPACE_PICKER_MEMBERS_CONTROL_PADDING_X))
        .on_children_prepainted(move |children, _, cx| {
            let bounds = children.first().copied();
            entity.update(cx, |view, cx| {
                view.record_workspace_runtime_selector_dropdown_bounds(bounds, cx)
            });
        })
        .child(dropdown_panel)
}

fn render_runtime_selector_wsl_rows(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    highlighted_index: usize,
    cx: &mut Context<ShellView>,
    distro_list: &workspace_picker::RuntimeSelectorDistroList,
) -> Vec<AnyElement> {
    match distro_list.status() {
        workspace_picker::RuntimeSelectorDistroListStatus::Loading => {
            vec![render_runtime_selector_status_row(
                shell,
                "Loading WSL distros...",
            )]
        }
        workspace_picker::RuntimeSelectorDistroListStatus::Failed(error) => {
            vec![render_runtime_selector_status_row(
                shell,
                format!("WSL distros unavailable: {error}"),
            )]
        }
        workspace_picker::RuntimeSelectorDistroListStatus::Loaded
        | workspace_picker::RuntimeSelectorDistroListStatus::NotLoaded
            if distro_list.distro_names().is_empty() =>
        {
            vec![render_runtime_selector_status_row(
                shell,
                "No WSL distros available",
            )]
        }
        workspace_picker::RuntimeSelectorDistroListStatus::Loaded
        | workspace_picker::RuntimeSelectorDistroListStatus::NotLoaded => distro_list
            .distro_names()
            .iter()
            .enumerate()
            .map(|(index, distro_name)| {
                let item_index = index + 1;
                render_runtime_selector_row(
                    shell,
                    loaded,
                    workspace_picker::RuntimeSelectorRow::WslDistro {
                        distro_name: distro_name.clone(),
                    },
                    item_index,
                    highlighted_index == item_index,
                    cx,
                )
            })
            .collect(),
    }
}

fn render_runtime_selector_status_row(shell: &ShellView, text: impl Into<String>) -> AnyElement {
    div()
        .h(px(layout::WORKSPACE_PICKER_RUNTIME_DROPDOWN_ROW_HEIGHT))
        .px_3()
        .border_b_1()
        .border_color(shell.separator_color())
        .flex()
        .items_center()
        .text_xs()
        .text_color(shell.surface_muted_foreground())
        .whitespace_normal()
        .child(text.into())
        .into_any_element()
}

fn render_runtime_selector_row(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    row: workspace_picker::RuntimeSelectorRow,
    item_index: usize,
    highlighted: bool,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    let runtime = workspace_picker::runtime_selector_row_runtime(&row);
    let selected = loaded
        .selected_runtime()
        .is_some_and(|current| current == &runtime);
    let label = workspace_picker::runtime_selector_row_label(&row);
    let background = if highlighted || selected {
        shell.row_surface_background()
    } else {
        shell.popup_surface_background()
    };
    let foreground = if highlighted {
        shell.general_ui_foreground()
    } else {
        shell.surface_muted_foreground()
    };

    div()
        .id(("workspace-runtime-selector-row", item_index))
        .relative()
        .h(px(layout::WORKSPACE_PICKER_RUNTIME_DROPDOWN_ROW_HEIGHT))
        .w_full()
        .px_3()
        .border_b_1()
        .border_color(shell.separator_color())
        .bg(background)
        .flex()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .hover({
            let hover_background = shell.row_surface_background();
            move |style| style.bg(hover_background)
        })
        .when(selected, |this| {
            this.child(render_workspace_active_marker(shell))
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_sm()
                .text_color(foreground)
                .whitespace_nowrap()
                .truncate()
                .child(label),
        )
        .on_click(cx.listener(move |view, _, window, cx| {
            view.select_workspace_runtime(runtime.clone(), window, cx);
            if let Some(loaded) = view.loaded_workspace_mut() {
                loaded.workspace_picker.close_runtime_selector_dropdown();
            }
        }))
        .into_any_element()
}

fn runtime_selector_current_label(loaded: &LoadedWorkspaceState) -> String {
    match loaded.selected_runtime() {
        Some(RuntimeMode::HostWindows) => "host-Windows".to_string(),
        Some(RuntimeMode::WslLinux { distro_name }) => format!("WSL: {distro_name}"),
        None => "Select runtime environment".to_string(),
    }
}

fn render_scrollable_member_rows(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    members_scroll_handle: gpui::ScrollHandle,
    scrollbar_visibility: ScrollbarVisibilityPolicy,
    list_height: gpui::Pixels,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let mut scroll_region = div()
        .relative()
        .h(list_height)
        .min_h(px(0.0))
        .on_mouse_move(cx.listener(ShellView::note_workspace_members_scrollbar_motion))
        .on_scroll_wheel(cx.listener(ShellView::note_workspace_members_scrollbar_scroll))
        .child(
            div()
                .id("workspace-picker-members-list")
                .size_full()
                .min_h(px(0.0))
                .track_scroll(&members_scroll_handle)
                .overflow_y_scroll()
                .child(render_member_rows(shell, loaded, cx)),
        );
    if let Some(scrollbar) = render_div_scrollbar(
        "workspace-members-scrollbar",
        &members_scroll_handle,
        ScrollbarAxis::Vertical,
        scrollbar_visibility,
    ) {
        scroll_region = scroll_region.child(scrollbar);
    }
    scroll_region
}

fn render_member_rows(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let mut rows = div()
        .w_full()
        .flex()
        .flex_col()
        .child(render_attach_member_row(shell, loaded, cx));

    if loaded.selected_runtime().is_none() {
        return rows;
    }

    if !loaded.workspace_state.has_available_explicit_members() {
        rows = rows.child(render_implicit_home_member_row(shell, loaded));
    }
    for (index, member) in loaded.explicit_members().iter().enumerate() {
        rows = rows.child(render_explicit_member_row(shell, loaded, index, member, cx));
    }

    rows
}

fn render_attach_member_row(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    let enabled =
        loaded.selected_runtime().is_some() && !loaded.workspace_members.path_prompt_active();
    let label = if loaded.workspace_members.path_prompt_active() {
        "Choosing member directory..."
    } else {
        "Attach member"
    };
    let secondary = shell.secondary_button_theme();
    let foreground = if enabled {
        shell.general_ui_foreground()
    } else {
        shell.surface_muted_foreground()
    };
    let hover_background = shell.row_surface_background();
    let mut row = div()
        .id("workspace-picker-attach-member")
        .w_full()
        .min_h(px(layout::WORKSPACE_PICKER_MEMBERS_ATTACH_ROW_HEIGHT))
        .px_4()
        .py_3()
        .bg(shell.popup_surface_background())
        .border_b_1()
        .border_color(shell.separator_color())
        .flex()
        .items_center()
        .text_sm()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(foreground)
        .child(render_create_add_plus_marker(shell, enabled))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .whitespace_normal()
                .child(label),
        );
    if enabled {
        row = row
            .cursor_pointer()
            .hover(move |style| style.bg(hover_background))
            .active(move |style| style.bg(secondary.active.background))
            .on_click(cx.listener(ShellView::prompt_attach_workspace_member));
    }
    row.into_any_element()
}

fn render_implicit_home_member_row(shell: &ShellView, loaded: &LoadedWorkspaceState) -> AnyElement {
    member_row_shell(
        shell,
        "workspace-picker-implicit-home-member",
        true,
        false,
        "Home directory".to_string(),
        loaded.implicit_home_path_display_text(),
        layout::WORKSPACE_PICKER_MEMBERS_ROW_HEIGHT,
        None,
        None,
    )
    .into_any_element()
}

fn render_explicit_member_row(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    index: usize,
    member: &WorkspaceMember,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    let primary = explicit_member_is_primary(loaded, member);
    let member_id = member.id().clone();
    member_row_shell(
        shell,
        ("workspace-picker-member-row", index),
        primary,
        false,
        explicit_member_display_label(member),
        member.canonical_path().display().to_string(),
        layout::WORKSPACE_PICKER_MEMBERS_ROW_HEIGHT,
        None,
        Some(render_member_row_action_trigger(shell, index, member_id, cx).into_any_element()),
    )
    .into_any_element()
}

fn member_row_shell(
    shell: &ShellView,
    id: impl Into<gpui::ElementId>,
    primary: bool,
    interactive: bool,
    label: String,
    detail: String,
    row_height: f32,
    leading: Option<AnyElement>,
    action: Option<AnyElement>,
) -> gpui::Stateful<gpui::Div> {
    let background = shell.popup_surface_background();
    let hover_background = shell.row_surface_background();
    let mut row = div()
        .id(id)
        .relative()
        .w_full()
        .min_h(px(row_height))
        .bg(background)
        .border_b_1()
        .border_color(shell.separator_color())
        .px_4()
        .py_3()
        .flex()
        .items_start()
        .justify_between()
        .gap_3()
        .when(primary, |this| {
            this.child(render_workspace_active_marker(shell))
        })
        .when_some(leading, |this, leading| this.child(leading))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(shell.general_ui_foreground())
                        .whitespace_normal()
                        .child(label),
                )
                .child(
                    div()
                        .text_xs()
                        .font_family(CODE_FONT_FAMILY)
                        .text_color(shell.surface_muted_foreground())
                        .whitespace_normal()
                        .child(detail),
                ),
        )
        .when_some(action, |this, action| {
            this.child(div().flex_none().child(action))
        });
    if interactive {
        row = row
            .cursor_pointer()
            .hover(move |style| style.bg(hover_background));
    }
    row
}

fn render_member_row_action_trigger(
    shell: &ShellView,
    index: usize,
    member_id: beryl_model::workspace::WorkspaceMemberId,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let secondary = shell.secondary_button_theme();

    div()
        .id(("workspace-picker-member-action-menu-trigger", index))
        .flex_none()
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
        .font_weight(secondary.font_weight)
        .text_color(secondary.normal.foreground)
        .cursor_pointer()
        .hover(move |style| {
            style
                .bg(secondary.hover.background)
                .border_color(secondary.hover.border)
        })
        .active(move |style| {
            style
                .bg(secondary.active.background)
                .border_color(secondary.active.border)
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, event, window, cx| {
                view.open_workspace_member_action_menu(member_id.clone(), event, window, cx);
            }),
        )
        .on_click(|_, _, cx| cx.stop_propagation())
        .child("...")
}

fn render_workspace_member_action_menu(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    let menu = loaded.workspace_picker.member_action_menu_active()?;
    let member_id = menu.member_id().clone();
    let position = menu.position();
    let member = loaded
        .explicit_members()
        .iter()
        .find(|member| member.id() == &member_id)?;
    let member_path = member.canonical_path().display().to_string();
    let primary = explicit_member_is_primary(loaded, member);
    let available = member.is_available();
    let entity = cx.entity();

    let mut content = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(shell, "Member"));
    if primary {
        content = content.child(disabled_secondary_button(
            shell,
            "workspace-picker-member-menu-primary",
            "Primary",
        ));
    } else if available {
        let primary_member_id = member_id.clone();
        content = content.child(
            secondary_button(
                shell,
                "workspace-picker-member-menu-make-primary",
                "Make primary",
                cx.listener(move |view, event, window, cx| {
                    view.make_workspace_member_primary(
                        primary_member_id.clone(),
                        event,
                        window,
                        cx,
                    );
                    if let Some(loaded) = view.loaded_workspace_mut() {
                        loaded.workspace_picker.close_member_action_menu();
                    }
                }),
            )
            .w_full()
            .justify_start(),
        );
    } else {
        content = content.child(disabled_secondary_button(
            shell,
            "workspace-picker-member-menu-path-not-found",
            "Path not found",
        ));
    }
    content = content.child(
        secondary_button(
            shell,
            "workspace-picker-member-menu-detach",
            "Detach",
            cx.listener(move |view, event, window, cx| {
                view.prompt_detach_workspace_member(
                    member_id.clone(),
                    member_path.clone(),
                    event,
                    window,
                    cx,
                );
                if let Some(loaded) = view.loaded_workspace_mut() {
                    loaded.workspace_picker.close_member_action_menu();
                }
            }),
        )
        .w_full()
        .justify_start(),
    );

    Some(
        anchored()
            .position(position)
            .snap_to_window_with_margin(px(8.0))
            .child(
                div()
                    .on_children_prepainted(move |children, _, cx| {
                        let bounds = children.first().copied();
                        entity.update(cx, |view, cx| {
                            view.record_workspace_member_action_menu_bounds(bounds, cx);
                        });
                    })
                    .child(
                        div()
                            .id("workspace-picker-member-action-menu-panel")
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

fn explicit_member_is_primary(loaded: &LoadedWorkspaceState, member: &WorkspaceMember) -> bool {
    loaded
        .workspace_state
        .primary_member()
        .is_some_and(|primary_member| match primary_member {
            PrimaryWorkspaceMember::Explicit(primary_member) => primary_member.id() == member.id(),
            PrimaryWorkspaceMember::ImplicitHome(_) => false,
        })
}

fn explicit_member_display_label(member: &WorkspaceMember) -> String {
    let label = member
        .canonical_path()
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| member.canonical_path().display().to_string());
    if member.is_available() {
        label
    } else {
        format!("{label} - path not found")
    }
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

fn render_workspace_row(
    shell: &ShellView,
    item_index: usize,
    workspace_index: usize,
    workspace: &beryl_model::workspace::BerylWorkspaceManifest,
    explicit_member_paths: &[String],
    loaded: &LoadedWorkspaceState,
    workspace_rename_input: &Entity<SingleLineInput>,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    let current = workspace.id() == loaded.workspace.id();
    let background = shell.popup_surface_background();
    let border = shell.separator_color();
    let title_color = shell.general_ui_foreground();
    let member_path_color = shell.surface_muted_foreground();
    let rename_editor_open_for_row = loaded
        .workspace_picker
        .rename_editor_open_for(workspace.id());
    let row_accepts_activation =
        workspace_picker::workspace_row_accepts_activation(rename_editor_open_for_row);

    let row = div()
        .id(("workspace-picker-row", workspace_index))
        .relative()
        .w_full()
        .min_h(px(layout::WORKSPACE_PICKER_ROW_HEIGHT))
        .bg(background)
        .border_b_1()
        .border_color(border)
        .when(current, |this| {
            this.child(render_workspace_active_marker(shell))
        });

    if !row_accepts_activation {
        return row
            .child(div().w_full().px_4().py_3().child(render_rename_editor(
                shell,
                workspace_rename_input,
                cx,
            )))
            .into_any_element();
    }

    let hover_background = shell.row_surface_background();

    row.cursor_pointer()
        .hover(move |style| style.bg(hover_background))
        .on_click(cx.listener(move |view, _, window, cx| {
            let _ = view.activate_workspace_picker_item(item_index, window, cx);
        }))
        .child(
            div()
                .w_full()
                .px_4()
                .py_3()
                .child(render_workspace_row_summary(
                    shell,
                    workspace_index,
                    workspace,
                    explicit_member_paths,
                    title_color,
                    member_path_color,
                    cx,
                )),
        )
        .into_any_element()
}

fn render_workspace_active_marker(shell: &ShellView) -> impl IntoElement {
    div()
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(px(3.0))
        .bg(shell.primary_button_theme().active.background)
}

fn render_workspace_row_summary(
    shell: &ShellView,
    index: usize,
    workspace: &beryl_model::workspace::BerylWorkspaceManifest,
    explicit_member_paths: &[String],
    title_color: gpui::Rgba,
    member_path_color: gpui::Rgba,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap_3()
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .min_w(px(0.0))
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(title_color)
                        .whitespace_normal()
                        .child(workspace.title().to_string()),
                )
                .when(!explicit_member_paths.is_empty(), |this| {
                    this.child(render_workspace_member_paths(
                        explicit_member_paths,
                        member_path_color,
                    ))
                }),
        )
        .child(div().flex_none().child(render_workspace_row_action_trigger(
            shell,
            index,
            workspace.id().clone(),
            cx,
        )))
}

fn render_workspace_member_paths(
    explicit_member_paths: &[String],
    text_color: gpui::Rgba,
) -> impl IntoElement {
    explicit_member_paths
        .iter()
        .fold(div().w_full().flex().flex_col().gap_1(), |paths, path| {
            paths.child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .text_xs()
                    .font_family(CODE_FONT_FAMILY)
                    .text_color(text_color)
                    .whitespace_normal()
                    .child(path.clone()),
            )
        })
}

fn render_rename_editor(
    shell: &ShellView,
    workspace_rename_input: &Entity<SingleLineInput>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .child(framed_text_input(shell, workspace_rename_input)),
        )
        .child(button(
            shell,
            "workspace-picker-save-rename",
            "Save",
            cx.listener(ShellView::submit_workspace_rename),
        ))
        .child(secondary_button(
            shell,
            "workspace-picker-cancel-rename",
            "Cancel",
            cx.listener(ShellView::cancel_workspace_rename),
        ))
}

fn render_create_workspace_row(shell: &ShellView, cx: &mut Context<ShellView>) -> impl IntoElement {
    let secondary = shell.secondary_button_theme();
    let background = shell.popup_surface_background();
    let foreground = shell.general_ui_foreground();
    let border = shell.separator_color();
    let hover_background = shell.row_surface_background();
    let active_background = secondary.active.background;

    div()
        .id("workspace-picker-create-new")
        .w_full()
        .min_h(px(layout::WORKSPACE_PICKER_CREATE_ROW_HEIGHT))
        .px_4()
        .py_3()
        .bg(background)
        .border_b_1()
        .border_color(border)
        .flex()
        .items_center()
        .text_sm()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(foreground)
        .cursor_pointer()
        .hover(move |style| style.bg(hover_background))
        .active(move |style| style.bg(active_background))
        .on_click(cx.listener(move |view, _, window, cx| {
            let _ = view.activate_workspace_picker_item(
                workspace_picker::CREATE_NEW_ITEM_INDEX,
                window,
                cx,
            );
        }))
        .child(render_create_add_plus_marker(shell, true))
        .child(
            div()
                .min_w(px(0.0))
                .flex_1()
                .whitespace_normal()
                .child("Create new workspace"),
        )
}
