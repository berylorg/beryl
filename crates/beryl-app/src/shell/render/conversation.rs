use std::sync::Arc;

use gpui::{
    AnyElement, AnyView, Context, CursorStyle, DispatchPhase, Entity, Image, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, ScrollHandle, Window,
    anchored, canvas, div, img, point, prelude::*, px, relative, rgb, rgba,
};

use crate::shell::{
    BlockedState, COMPOSER_KEY_CONTEXT, ComposerImagePopupMode, ConversationSurfaceState,
    IdleWorkspaceState, LoadedWorkspaceState, ReadyState, ShellView, SurfaceNotice,
    image_preview_popup, layout,
    status_line::{self, StatusLineCellAction, StatusLineCellValueKind, StatusLineProjection},
    status_line::{StatusLineCellValueSegment, StatusLineCellValueSegmentKind},
    tool_activity::ToolActivityRowStatus,
};
use crate::text_input::SingleLineInput;

use super::checklist_sidebar::{
    ChecklistSidebarPanel, render_checklist_thread_start_menu,
    render_checklist_thread_start_menu_listeners,
};
use super::common::{
    button, card, inline_notice, secondary_button, secondary_button_with_active_state,
    section_label, toolbar_controls_strip,
};
use super::graph_link_menu::{
    render_graph_thread_link_menu, render_graph_thread_link_menu_listeners,
};
use super::graph_overlay::{render_graph_overlay, render_graph_overlay_listeners};
use super::scrollbars::{ScrollbarAxis, render_div_scrollbar};
use super::status_operation::{render_status_operation_listeners, render_status_operation_popup};
use super::thread_selector::{render_thread_selector_listeners, render_thread_selector_overlay};
use super::transcript::TranscriptPanel;
use super::transcript_branch_menu::{
    render_transcript_branch_menu, render_transcript_branch_menu_listeners,
};
use super::workspace_picker::{
    render_workspace_picker_button, render_workspace_picker_listeners,
    render_workspace_picker_overlay,
};

pub(super) fn render_ready_shell(
    shell: &ShellView,
    ready: &ReadyState,
    transcript_panel: &Entity<TranscriptPanel>,
    checklist_sidebar_panel: &Entity<ChecklistSidebarPanel>,
    wsl_distro_input: &Entity<SingleLineInput>,
    workspace_picker_filter_input: &Entity<SingleLineInput>,
    workspace_rename_input: &Entity<SingleLineInput>,
    conversation_input: &Entity<SingleLineInput>,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> gpui::AnyElement {
    render_workspace_surface(
        shell,
        &ready.loaded_workspace,
        ready.loaded_workspace.workspace.title(),
        &ready.execution_target,
        ready.process_id,
        ready.report.initialize().user_agent.as_str(),
        &ready.surface,
        transcript_panel,
        checklist_sidebar_panel,
        wsl_distro_input,
        workspace_picker_filter_input,
        workspace_rename_input,
        conversation_input,
        None,
        window,
        cx,
    )
}

pub(super) fn render_idle_workspace_shell(
    shell: &ShellView,
    idle: &IdleWorkspaceState,
    _wsl_distro_input: &Entity<SingleLineInput>,
    workspace_picker_filter_input: &Entity<SingleLineInput>,
    workspace_rename_input: &Entity<SingleLineInput>,
    conversation_input: &Entity<SingleLineInput>,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> gpui::AnyElement {
    let loaded = &idle.loaded_workspace;
    let workspace_members_scroll_handle = loaded.workspace_members_scroll_handle();

    let mut root = div()
        .size_full()
        .relative()
        .bg(shell.general_ui_background())
        .text_color(shell.general_ui_foreground())
        .flex()
        .flex_col()
        .child(toolbar_controls_strip(
            shell,
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(render_workspace_picker_button(shell, loaded, cx))
                .child(secondary_button(
                    shell,
                    "settings-toolbar",
                    "Settings",
                    cx.listener(ShellView::open_settings_window),
                )),
        ))
        .child(
            div()
                .w_full()
                .h(px(layout::THREAD_STRIP_HEIGHT))
                .bg(shell.conversation_thread_strip_background())
                .border_b_1()
                .border_color(shell.separator_color())
                .flex()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .px_4()
                        .text_sm()
                        .text_color(rgb(0xcbd5e1))
                        .child("Runtime environment recovery required"),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .px_4()
                .py_4()
                .child(
                    div()
                        .id("idle-workspace-members-scroll")
                        .size_full()
                        .min_h(px(0.0))
                        .track_scroll(&workspace_members_scroll_handle)
                        .overflow_y_scroll()
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .gap_4()
                                .child(section_label("Workspace Members"))
                                .when_some(loaded.startup_warning.as_ref(), |this, warning| {
                                    this.child(inline_notice(warning, rgb(0x172554), rgb(0xbfdbfe)))
                                })
                                .child(card(
                                    shell,
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_3()
                                        .child(
                                            div()
                                                .text_lg()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .child("No runtime environment selected"),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(rgb(0xcbd5e1))
                                                .child(format!(
                                                    "Beryl opened the legacy semantic workspace '{}', but it does not have a selected runtime environment.",
                                                    loaded.workspace.title()
                                                )),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(rgb(0x94a3b8))
                                                .child(
                                                    "Select a host-Windows or WSL-Linux runtime in Workspaces before starting a transcript in this workspace.",
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .gap_3()
                                                .child(secondary_button(
                                                    shell,
                                                    "workspaces-inline",
                                                    "Workspaces",
                                                    cx.listener(ShellView::toggle_workspace_picker),
                                                )),
                                        )
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(rgb(0x94a3b8))
                                                .child(
                                                    "Workspace switching stays in the toolbar Workspaces popup rather than a dedicated full-screen picker.",
                                                ),
                                        ),
                                )),
                        ),
                ),
        )
        .child(render_loaded_workspace_composer(
            shell,
            conversation_input,
            window,
            cx,
        ))
        .child(render_status_line(shell, StatusLineProjection::unknown(), cx));

    if loaded.workspace_picker.is_open() {
        root = root.child(render_workspace_picker_listeners(cx));
        if let Some(overlay) = render_workspace_picker_overlay(
            shell,
            loaded,
            workspace_picker_filter_input,
            workspace_rename_input,
            window,
            cx,
        ) {
            root = root.child(overlay);
        }
    }
    root.into_any_element()
}

pub(super) fn render_loaded_workspace_shell(
    shell: &ShellView,
    loaded: &LoadedWorkspaceState,
    _host_path_input: &Entity<SingleLineInput>,
    _wsl_distro_input: &Entity<SingleLineInput>,
    _wsl_path_input: &Entity<SingleLineInput>,
    workspace_picker_filter_input: &Entity<SingleLineInput>,
    workspace_rename_input: &Entity<SingleLineInput>,
    conversation_input: &Entity<SingleLineInput>,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> gpui::AnyElement {
    let mut root = div()
        .size_full()
        .relative()
        .bg(shell.general_ui_background())
        .text_color(shell.general_ui_foreground())
        .flex()
        .flex_col()
        .child(toolbar_controls_strip(
            shell,
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(render_workspace_picker_button(shell, loaded, cx))
                .child(secondary_button(
                    shell,
                    "settings-toolbar",
                    "Settings",
                    cx.listener(ShellView::open_settings_window),
                )),
        ))
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .px_4()
                .py_4()
                .child(card(
                    shell,
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(section_label("Workspace Surface"))
                        .when_some(loaded.startup_warning.as_ref(), |this, warning| {
                            this.child(inline_notice(warning, rgb(0x172554), rgb(0xbfdbfe)))
                        })
                        .child(
                            div()
                                .text_lg()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("Opening primary workspace member"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xcbd5e1))
                                .child(format!(
                                    "Beryl loaded '{}'. Member management stays in the Workspaces popup while the workspace surface opens.",
                                    loaded.workspace.title()
                                )),
                        ),
                )),
        )
        .child(render_loaded_workspace_composer(
            shell,
            conversation_input,
            window,
            cx,
        ))
        .child(render_status_line(shell, StatusLineProjection::unknown(), cx));

    if loaded.workspace_picker.is_open() {
        root = root.child(render_workspace_picker_listeners(cx));
        if let Some(overlay) = render_workspace_picker_overlay(
            shell,
            loaded,
            workspace_picker_filter_input,
            workspace_rename_input,
            window,
            cx,
        ) {
            root = root.child(overlay);
        }
    }
    root.into_any_element()
}

pub(super) fn render_blocked_shell(
    shell: &ShellView,
    blocked: &BlockedState,
    transcript_panel: &Entity<TranscriptPanel>,
    checklist_sidebar_panel: &Entity<ChecklistSidebarPanel>,
    wsl_distro_input: &Entity<SingleLineInput>,
    workspace_picker_filter_input: &Entity<SingleLineInput>,
    workspace_rename_input: &Entity<SingleLineInput>,
    conversation_input: &Entity<SingleLineInput>,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> gpui::AnyElement {
    let Some(surface) = blocked.surface.as_ref() else {
        return div().into_any_element();
    };
    let Some(loaded_workspace) = blocked.loaded_workspace.as_ref() else {
        return div().into_any_element();
    };

    let banner = div()
        .flex()
        .flex_col()
        .gap_3()
        .child(inline_notice(
            &blocked.summary,
            rgb(0x3f1d1d),
            rgb(0xfecaca),
        ))
        .child(
            div()
                .flex()
                .gap_3()
                .child(button(
                    shell,
                    "retry-backend-inline",
                    "Retry Backend",
                    cx.listener(ShellView::retry_workspace),
                ))
                .child(secondary_button(
                    shell,
                    "close-beryl-inline",
                    "Close Beryl",
                    cx.listener(ShellView::quit),
                )),
        )
        .into_any_element();

    render_workspace_surface(
        shell,
        loaded_workspace,
        blocked
            .loaded_workspace
            .as_ref()
            .map(|loaded| loaded.workspace.title())
            .unwrap_or("Beryl"),
        &blocked.target.workspace(),
        None,
        "backend unavailable",
        surface,
        transcript_panel,
        checklist_sidebar_panel,
        wsl_distro_input,
        workspace_picker_filter_input,
        workspace_rename_input,
        conversation_input,
        Some((
            blocked.title,
            blocked.detail.as_str(),
            blocked.next_steps.as_slice(),
            banner,
        )),
        window,
        cx,
    )
}

fn render_workspace_surface(
    shell: &ShellView,
    loaded_workspace: &LoadedWorkspaceState,
    workspace_title: &str,
    execution_target: &beryl_model::workspace::WorkspaceId,
    _process_id: Option<u32>,
    _backend_label: &str,
    surface: &ConversationSurfaceState,
    transcript_panel: &Entity<TranscriptPanel>,
    checklist_sidebar_panel: &Entity<ChecklistSidebarPanel>,
    _wsl_distro_input: &Entity<SingleLineInput>,
    workspace_picker_filter_input: &Entity<SingleLineInput>,
    workspace_rename_input: &Entity<SingleLineInput>,
    conversation_input: &Entity<SingleLineInput>,
    blocked: Option<(&'static str, &str, &[String], gpui::AnyElement)>,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> gpui::AnyElement {
    let toolbar = render_toolbar(
        shell,
        loaded_workspace,
        workspace_title,
        execution_target,
        surface,
        blocked.is_some(),
        cx,
    )
    .into_any_element();
    let thread_strip = render_thread_strip(
        shell,
        execution_target,
        &loaded_workspace.workspace_state,
        surface,
        cx,
    )
    .into_any_element();
    let entity = cx.entity();
    let conversation_width = surface.transcript_width();
    let composer_height =
        composer_height_for_input(surface, conversation_input, conversation_width, window, cx);
    let split = render_split_surface(
        shell,
        transcript_panel,
        surface,
        conversation_input,
        checklist_sidebar_panel,
        window,
        cx,
    )
    .into_any_element();
    let main_region = div()
        .relative()
        .flex_1()
        .min_h(px(
            layout::MAIN_REGION_MIN_HEIGHT + layout::COMPOSER_MIN_HEIGHT
        ))
        .child(
            canvas(|bounds, _, _| bounds, {
                let entity = entity.clone();
                move |bounds, _, window, cx| {
                    entity.update(cx, |view, cx| view.record_surface_layout_bounds(bounds, cx));
                    window.on_mouse_event({
                        let entity = entity.clone();
                        move |event: &MouseMoveEvent, _, _, cx| {
                            if !event.dragging() {
                                return;
                            }

                            entity.update(cx, |view, cx| view.update_surface_drag(event, cx));
                        }
                    });
                    window.on_mouse_event({
                        let entity = entity.clone();
                        move |event: &MouseUpEvent, _, _, cx| {
                            entity.update(cx, |view, cx| view.end_surface_drag(event, cx));
                        }
                    });
                }
            })
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .child(div().size_full().flex().child(split))
        .into_any_element();

    let mut workspace_body = div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(
            layout::MAIN_REGION_MIN_HEIGHT + layout::COMPOSER_MIN_HEIGHT
        ))
        .child(thread_strip)
        .child(main_region);
    workspace_body = workspace_body.child(render_graph_overlay_listeners(cx));
    if let Some(overlay) = render_graph_overlay(
        shell,
        loaded_workspace,
        surface,
        composer_height,
        window,
        cx,
    ) {
        workspace_body = workspace_body.child(overlay);
    }

    let status_line = render_status_line(shell, surface.status_line_projection(), cx);

    let mut body = div()
        .size_full()
        .relative()
        .bg(shell.general_ui_background())
        .text_color(shell.general_ui_foreground())
        .flex()
        .flex_col()
        .child(toolbar)
        .child(workspace_body)
        .child(status_line);

    if let Some((title, detail, next_steps, banner)) = blocked {
        let mut detail_block = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xfde68a))
                    .child(title.to_string()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xe2e8f0))
                    .child(detail.to_string()),
            )
            .child(banner);
        for next_step in next_steps {
            detail_block = detail_block.child(
                div()
                    .text_sm()
                    .text_color(rgb(0xcbd5e1))
                    .child(format!("Next: {next_step}")),
            );
        }

        body = body.child(
            div()
                .absolute()
                .top(px(layout::TOOLBAR_STRIP_HEIGHT
                    + layout::THREAD_STRIP_HEIGHT
                    + 16.0))
                .left_4()
                .right_4()
                .flex()
                .justify_end()
                .child(
                    div()
                        .w_full()
                        .max_w(px(420.0))
                        .child(card(shell, detail_block)),
                ),
        );
    } else if let Some(notice) = surface.notice() {
        body = body.child(
            div()
                .absolute()
                .top(px(layout::TOOLBAR_STRIP_HEIGHT
                    + layout::THREAD_STRIP_HEIGHT
                    + 16.0))
                .left_4()
                .right_4()
                .flex()
                .justify_end()
                .child(
                    div()
                        .w_full()
                        .max_w(px(420.0))
                        .child(render_surface_notice(shell, notice, window, cx)),
                ),
        );
    }

    if surface.graph_thread_link_menu().is_open() {
        body = body.child(render_graph_thread_link_menu_listeners(cx));
        if let Some(menu) = render_graph_thread_link_menu(shell, loaded_workspace, surface, cx) {
            body = body.child(menu);
        }
    }

    if surface.transcript_branch_menu().is_open() {
        body = body.child(render_transcript_branch_menu_listeners(cx));
        if let Some(menu) = render_transcript_branch_menu(shell, surface, cx) {
            body = body.child(menu);
        }
    }

    if surface.checklist_thread_start_menu().is_open() {
        body = body.child(render_checklist_thread_start_menu_listeners(cx));
        if let Some(menu) = render_checklist_thread_start_menu(shell, loaded_workspace, surface, cx)
        {
            body = body.child(menu);
        }
    }

    if surface.thread_selector().is_open() {
        body = body.child(render_thread_selector_listeners(cx));
        if let Some(overlay) =
            render_thread_selector_overlay(shell, loaded_workspace, surface, window, cx)
        {
            body = body.child(overlay);
        }
    }

    if loaded_workspace.workspace_picker.is_open() {
        body = body.child(render_workspace_picker_listeners(cx));
        if let Some(overlay) = render_workspace_picker_overlay(
            shell,
            loaded_workspace,
            workspace_picker_filter_input,
            workspace_rename_input,
            window,
            cx,
        ) {
            body = body.child(overlay);
        }
    }
    if surface.status_line_operations().is_open() {
        body = body.child(render_status_operation_listeners(cx));
        if let Some(popup) =
            render_status_operation_popup(shell, surface, shell.status_model_cache(), cx)
        {
            body = body.child(popup);
        }
    }

    if shell.composer_image_popup().is_some() {
        body = body.child(render_composer_image_popup_listeners(cx));
        if let Some(popup) = render_composer_image_popup(shell, cx) {
            body = body.child(popup);
        }
    }

    if surface.transcript_edit_mode().is_some() {
        body = body.child(render_transcript_edit_mode_listeners(cx));
    }

    body.into_any_element()
}

fn render_transcript_edit_mode_listeners(cx: &mut Context<ShellView>) -> impl IntoElement {
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
                        view.handle_transcript_edit_mode_key_down(event, window, cx)
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

fn render_surface_notice(
    shell: &ShellView,
    notice: &SurfaceNotice,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let notice_text = notice.selectable_text();
    shell.sync_surface_notice_text_input(notice.id(), &notice_text, cx);

    let notice_width = (window.viewport_size().width - px(32.0)).min(px(420.0));
    let notice_text_width = (notice_width - px(32.0) - px(24.0) - px(8.0)).max(px(120.0));
    let visual_line_count = crate::text_input::wrapped_visual_line_count_for_width(
        &notice_text,
        notice_text_width,
        window,
    )
    .clamp(1, 10);
    let input_height = window.line_height() * visual_line_count as f32;
    let focus_input = shell.surface_notice_text_input.clone();
    let has_detail = !notice.detail().is_empty();

    card(
        shell,
        div()
            .occlude()
            .flex()
            .items_start()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h(input_height)
                    .min_h(input_height)
                    .relative()
                    .cursor(CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        let focus_handle = focus_input.read(cx).tab_focus_handle();
                        window.focus(&focus_handle);
                    })
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .gap_0()
                            .text_sm()
                            .child(
                                div()
                                    .text_color(rgb(0xfbbf24))
                                    .child(notice.title().to_string()),
                            )
                            .when(has_detail, |this| {
                                this.child(
                                    div()
                                        .text_color(shell.surface_foreground())
                                        .child(notice.detail().to_string()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .text_sm()
                            .text_color(rgba(0x00000000))
                            .child(shell.surface_notice_text_input.clone()),
                    ),
            )
            .child(
                div()
                    .id("surface-notice-close")
                    .w(px(layout::BUTTON_OUTER_HEIGHT))
                    .h(px(layout::BUTTON_OUTER_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
                    .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
                    .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
                    .text_color(shell.surface_muted_foreground())
                    .hover(|style| style.bg(rgba(0x33415599)))
                    .active(|style| style.bg(rgba(0x475569cc)))
                    .cursor(CursorStyle::PointingHand)
                    .child("X")
                    .on_click(cx.listener(ShellView::dismiss_surface_notice)),
            ),
    )
}

fn render_composer_image_popup_listeners(cx: &mut Context<ShellView>) -> impl IntoElement {
    let entity = cx.entity();
    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            window.on_mouse_event({
                let entity = entity.clone();
                move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }

                    entity.update(cx, |view, cx| {
                        view.handle_composer_image_popup_mouse_down(event, window, cx);
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
                        view.handle_composer_image_popup_key_down(event, window, cx)
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

fn render_composer_image_popup(
    shell: &ShellView,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    let popup = shell.composer_image_popup()?;
    let entity = cx.entity();
    let content = match popup.mode {
        ComposerImagePopupMode::Menu => {
            render_composer_image_menu(shell, popup.label.as_str(), cx).into_any_element()
        }
        ComposerImagePopupMode::Preview => {
            render_composer_image_preview(shell, popup.label.as_str()).into_any_element()
        }
    };

    Some(
        anchored()
            .position(popup.position)
            .snap_to_window_with_margin(px(8.0))
            .child(
                div()
                    .on_children_prepainted(move |children, _, cx| {
                        let bounds = children.first().copied();
                        entity.update(cx, |view, cx| {
                            view.record_composer_image_popup_bounds(bounds, cx);
                        });
                    })
                    .child(content),
            )
            .into_any_element(),
    )
}

fn render_composer_image_menu(
    shell: &ShellView,
    label: &str,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    div()
        .id("composer-image-marker-menu")
        .w(px(180.0))
        .occlude()
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .border_1()
        .border_color(shell.surface_border())
        .bg(shell.popup_surface_background())
        .shadow_lg()
        .p_1()
        .child(
            div()
                .px_3()
                .py_2()
                .text_xs()
                .text_color(shell.surface_muted_foreground())
                .child(format!("Image {label}")),
        )
        .child(composer_image_menu_row(
            shell,
            0,
            "View",
            cx.listener(ShellView::view_composer_image_from_popup),
        ))
        .child(composer_image_menu_row(
            shell,
            1,
            "Remove",
            cx.listener(ShellView::remove_composer_image_from_popup),
        ))
}

fn composer_image_menu_row(
    shell: &ShellView,
    index: usize,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    secondary_button(shell, ("composer-image-menu-row", index), label, on_click)
        .w_full()
        .justify_start()
}

fn render_composer_image_preview(shell: &ShellView, label: &str) -> impl IntoElement {
    let image = shell
        .composer_image_preview_data()
        .map(|data| Arc::new(Image::from_bytes(data.format(), data.bytes().to_vec())));

    div()
        .id("composer-image-preview-popup")
        .w(image_preview_popup::popup_width())
        .h(image_preview_popup::popup_height())
        .occlude()
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .border_1()
        .border_color(shell.surface_border())
        .bg(shell.popup_surface_background())
        .shadow_lg()
        .p_3()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(shell.surface_muted_foreground())
                .child(format!("Image {label}")),
        )
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
                .border_1()
                .border_color(shell.surface_border())
                .bg(rgb(0x020617))
                .relative()
                .overflow_hidden()
                .child(match image {
                    Some(image) => img(image)
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                    None => div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(shell.surface_muted_foreground())
                        .child("Image data is no longer available")
                        .into_any_element(),
                }),
        )
}

fn activity_mode_button(
    shell: &ShellView,
    label: &'static str,
    active: bool,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    secondary_button_with_active_state(
        shell,
        "activity-mode",
        label,
        active,
        cx.listener(ShellView::cycle_tool_activity_panel_mode),
    )
}

fn render_toolbar(
    shell: &ShellView,
    loaded_workspace: &LoadedWorkspaceState,
    _workspace_title: &str,
    _execution_target: &beryl_model::workspace::WorkspaceId,
    surface: &ConversationSurfaceState,
    blocked: bool,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    toolbar_controls_strip(
        shell,
        div()
            .flex()
            .items_center()
            .gap_3()
            .child(render_workspace_picker_button(shell, loaded_workspace, cx))
            .child(activity_mode_button(
                shell,
                surface.tool_activity_panel_mode().label(),
                surface.tool_activity_panel_visible(),
                cx,
            ))
            .child(secondary_button(
                shell,
                "toggle-graph-overlay",
                if surface.graph_overlay().visible() {
                    "Hide Graph"
                } else {
                    "Graph"
                },
                cx.listener(ShellView::toggle_graph_overlay),
            ))
            .child(secondary_button(
                shell,
                "toggle-checklist-sidebar",
                if surface.checklist_sidebar_visible() {
                    "Hide Checklist"
                } else {
                    "Show Checklist"
                },
                cx.listener(ShellView::toggle_checklist_sidebar),
            ))
            .child(secondary_button(
                shell,
                "settings-toolbar",
                "Settings",
                cx.listener(ShellView::open_settings_window),
            ))
            .when(blocked, |this| {
                this.child(button(
                    shell,
                    "retry-backend-toolbar",
                    "Retry Backend",
                    cx.listener(ShellView::retry_workspace),
                ))
            }),
    )
}

fn render_thread_strip(
    shell: &ShellView,
    workspace: &beryl_model::workspace::WorkspaceId,
    workspace_state: &beryl_model::conversation::WorkspaceConversationState,
    surface: &ConversationSurfaceState,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let entity = cx.entity();
    let active_label = surface
        .pending_thread_activation_label()
        .map(|label| format!("Opening {label}"))
        .or_else(|| surface.selected_thread_display_label(workspace_state, workspace))
        .unwrap_or_else(|| "New conversation".to_string());

    div()
        .w_full()
        .h(px(layout::THREAD_STRIP_HEIGHT))
        .bg(shell.conversation_thread_strip_background())
        .border_b_1()
        .border_color(shell.separator_color())
        .flex()
        .items_center()
        .gap_3()
        .px_4()
        .child(thread_strip_action(
            shell,
            "thread-strip-new-thread",
            "New Thread",
            cx.listener(ShellView::start_new_thread),
        ))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .flex()
                .items_center()
                .gap_3()
                .overflow_hidden()
                .when(
                    matches!(
                        workspace.runtime_mode(),
                        beryl_model::workspace::RuntimeMode::WslLinux { .. }
                    ),
                    |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x7dd3fc))
                                .child(workspace.runtime_mode().display_name()),
                        )
                    },
                )
                .child(
                    div()
                        .on_children_prepainted(move |children, _, cx| {
                            let bounds = children.first().copied();
                            entity.update(cx, |view, cx| {
                                view.record_thread_selector_anchor_bounds(bounds, cx)
                            });
                        })
                        .flex_1()
                        .min_w(px(0.0))
                        .h_full()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .id("thread-strip-active-thread-title")
                                .w_full()
                                .h(px(layout::BUTTON_OUTER_HEIGHT))
                                .px(px(layout::BUTTON_HORIZONTAL_PADDING))
                                .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
                                .flex()
                                .items_center()
                                .hover({
                                    let theme = shell.secondary_button_theme();
                                    move |style| style.bg(theme.hover.background)
                                })
                                .active({
                                    let theme = shell.secondary_button_theme();
                                    move |style| style.bg(theme.active.background)
                                })
                                .cursor_pointer()
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
                                        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
                                        .text_color(if surface.thread_selector().is_open() {
                                            rgb(0x7dd3fc)
                                        } else {
                                            rgb(0xe2e8f0)
                                        })
                                        .whitespace_nowrap()
                                        .truncate()
                                        .child(active_label),
                                )
                                .on_click(cx.listener(ShellView::toggle_thread_selector)),
                        ),
                ),
        )
}

fn render_status_line(
    shell: &ShellView,
    status: StatusLineProjection,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let model_reasoning_enabled =
        shell.status_line_model_reasoning_interactive(status.model_reasoning_available);
    let context_enabled = shell.status_line_context_interactive(status.context_operation_available);
    let turn_operations_enabled =
        shell.status_line_turn_operations_interactive(status.cancellable_active_turn.is_some());
    let cells = status_line::status_line_cell_specs(
        status,
        model_reasoning_enabled,
        context_enabled,
        turn_operations_enabled,
    );

    let mut line = div()
        .w_full()
        .h(px(layout::STATUS_LINE_HEIGHT))
        .bg(shell.status_line_background())
        .border_t_1()
        .border_color(shell.separator_color())
        .flex()
        .items_center()
        .overflow_hidden();

    for spec in cells {
        let value_color = match spec.value_kind {
            StatusLineCellValueKind::Default => None,
            StatusLineCellValueKind::TurnState => Some(last_turn_state_color(spec.value.as_str())),
        };
        line = line.child(status_line_cell(
            shell,
            spec.label,
            spec.value_segments,
            value_color,
            spec.enabled,
            spec.action,
            cx,
        ));
    }

    line
}

fn status_line_cell(
    shell: &ShellView,
    label: &'static str,
    value_segments: Vec<StatusLineCellValueSegment>,
    value_color: Option<gpui::Rgba>,
    enabled: bool,
    action: StatusLineCellAction,
    cx: &mut Context<ShellView>,
) -> gpui::Div {
    let resolved_value_color = value_color.unwrap_or_else(|| {
        if enabled || matches!(action, StatusLineCellAction::None) {
            shell.status_line_value_foreground()
        } else {
            shell.surface_muted_foreground()
        }
    });
    let mut cell = div()
        .h_full()
        .w(relative(1.0 / 3.0))
        .min_w(px(0.0))
        .px_4()
        .border_r_1()
        .border_color(shell.separator_color())
        .flex()
        .items_center()
        .gap_2()
        .overflow_hidden()
        .child(
            div()
                .text_xs()
                .text_color(shell.status_line_title_foreground())
                .whitespace_nowrap()
                .child(label),
        )
        .child(status_line_value(
            shell,
            value_segments,
            resolved_value_color,
        ));

    if enabled {
        let theme = shell.secondary_button_theme();
        cell = cell
            .cursor_pointer()
            .hover(move |style| style.bg(theme.hover.background));
        cell = match action {
            StatusLineCellAction::ModelReasoning => cell.on_mouse_down(
                MouseButton::Left,
                cx.listener(ShellView::open_status_model_reasoning_popup),
            ),
            StatusLineCellAction::Context => cell.on_mouse_down(
                MouseButton::Left,
                cx.listener(ShellView::open_status_context_popup),
            ),
            StatusLineCellAction::TurnOperations => cell.on_mouse_down(
                MouseButton::Left,
                cx.listener(ShellView::open_status_turn_operations_popup),
            ),
            StatusLineCellAction::None => cell,
        };
    }

    cell
}

fn status_line_value(
    shell: &ShellView,
    segments: Vec<StatusLineCellValueSegment>,
    value_color: gpui::Rgba,
) -> gpui::Div {
    if let [segment] = segments.as_slice()
        && segment.kind == StatusLineCellValueSegmentKind::Value
    {
        return div()
            .flex_1()
            .min_w(px(0.0))
            .text_xs()
            .text_color(value_color)
            .whitespace_nowrap()
            .truncate()
            .child(segment.text.clone());
    }

    let mut value = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap_2()
        .overflow_hidden();
    for segment in segments {
        let color = match segment.kind {
            StatusLineCellValueSegmentKind::Label => shell.status_line_title_foreground(),
            StatusLineCellValueSegmentKind::Value => value_color,
        };
        value = value.child(
            div()
                .text_xs()
                .text_color(color)
                .whitespace_nowrap()
                .flex_none()
                .child(segment.text),
        );
    }

    value
}

fn last_turn_state_color(state: &str) -> gpui::Rgba {
    match state {
        "working" | "compacting" => rgb(0x93c5fd),
        "ok" => rgb(0x86efac),
        "error" => rgb(0xfca5a5),
        _ => rgb(0xe2e8f0),
    }
}

fn thread_strip_action(
    shell: &ShellView,
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    secondary_button(shell, id, label, on_click)
}

fn render_split_surface(
    shell: &ShellView,
    transcript_panel: &Entity<TranscriptPanel>,
    surface: &ConversationSurfaceState,
    conversation_input: &Entity<SingleLineInput>,
    checklist_sidebar_panel: &Entity<ChecklistSidebarPanel>,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let entity = cx.entity();
    let total_width = surface
        .split_bounds
        .or(surface.layout_bounds)
        .map(|bounds| bounds.size.width)
        .unwrap_or_else(|| px(layout::WINDOW_MIN_WIDTH));
    let split_layout = layout::split_layout(
        total_width,
        surface.checklist_sidebar_ratio,
        surface.checklist_sidebar_visible(),
    );
    let visible_width = (split_layout.left_width + split_layout.right_width).max(px(1.0));
    let left_ratio = if surface.checklist_sidebar_visible() {
        split_layout.left_width / visible_width
    } else {
        1.0
    };
    let right_ratio = if surface.checklist_sidebar_visible() {
        split_layout.right_width / visible_width
    } else {
        0.0
    };
    let composer_height = composer_height_for_input(
        surface,
        conversation_input,
        split_layout.left_width,
        window,
        cx,
    );

    let left_panel = render_left_panel(transcript_panel).into_any_element();
    let composer = render_composer(
        shell,
        surface,
        conversation_input,
        split_layout.left_width,
        composer_height,
        window,
        cx,
    )
    .into_any_element();
    let tool_activity_panel = render_tool_activity_panel(shell, surface, composer_height, cx);
    let checklist_sidebar = surface
        .checklist_sidebar_visible()
        .then(|| render_checklist_sidebar_panel(checklist_sidebar_panel).into_any_element());

    div()
        .relative()
        .w_full()
        .flex_1()
        .min_h(px(
            layout::MAIN_REGION_MIN_HEIGHT + layout::COMPOSER_MIN_HEIGHT
        ))
        .child(
            canvas(|bounds, _, _| bounds, {
                let entity = entity.clone();
                move |bounds, _, _, cx| {
                    entity.update(cx, |view, cx| view.record_surface_split_bounds(bounds, cx));
                }
            })
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .child(
            div()
                .size_full()
                .flex()
                .gap_0()
                .child(
                    div()
                        .w(relative(left_ratio))
                        .min_w(px(layout::PANEL_MIN_WIDTH))
                        .h_full()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex_1()
                                .min_h(px(layout::MAIN_REGION_MIN_HEIGHT))
                                .child(left_panel),
                        )
                        .when_some(tool_activity_panel, |this, panel| this.child(panel))
                        .child(composer),
                )
                .when_some(checklist_sidebar, |this, checklist_sidebar| {
                    this.child(render_checklist_sidebar_divider(shell, cx))
                        .child(
                            div()
                                .w(relative(right_ratio))
                                .min_w(px(layout::PANEL_MIN_WIDTH))
                                .h_full()
                                .child(checklist_sidebar),
                        )
                }),
        )
}

fn render_checklist_sidebar_divider(
    shell: &ShellView,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let entity = cx.entity();

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
                        view.begin_surface_divider_drag(bounds.left(), event, cx);
                    });
                }
            });
        },
    )
    .w(px(layout::PANEL_DIVIDER_WIDTH))
    .h_full()
    .cursor(CursorStyle::ResizeColumn)
    .bg(shell.input_background())
    .border_x_1()
    .border_color(shell.separator_color())
}

fn render_left_panel(transcript_panel: &Entity<TranscriptPanel>) -> impl IntoElement {
    let cached_root = div();
    let cached_style = cached_root.size_full().min_h(px(0.0)).style().clone();
    AnyView::from(transcript_panel.clone()).cached(cached_style)
}

fn render_tool_activity_panel(
    shell: &ShellView,
    surface: &ConversationSurfaceState,
    composer_height: gpui::Pixels,
    cx: &mut Context<ShellView>,
) -> Option<gpui::AnyElement> {
    if !surface.tool_activity_panel_visible() {
        return None;
    }

    let entity = cx.entity();
    let panel_height = surface.tool_activity_panel_height_for_layout(composer_height);
    let scroll_handle = surface.tool_activity_scroll_handle();
    let row_count = surface.tool_activity_row_count();
    let row_window = layout::tool_activity_row_window(
        row_count,
        panel_height,
        -scroll_handle.offset().y,
        layout::TOOL_ACTIVITY_OVERSCAN_ROWS,
    );
    let rows = surface.tool_activity_row_window(row_window.range.clone());
    let scrollbar_opacity = shell.scrollbar_opacity(&crate::shell::ScrollbarRegion::ToolActivity);

    let mut row_list = div()
        .w_full()
        .h(row_window.content_height)
        .min_h(row_window.content_height)
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .h(row_window.top_spacer_height)
                .min_h(row_window.top_spacer_height),
        );
    for (index, row) in rows {
        row_list = row_list.child(render_tool_activity_row(
            shell,
            index,
            row.agent_label.clone(),
            row.tool_display_value.clone(),
            row.status,
        ));
    }
    row_list = row_list.child(
        div()
            .w_full()
            .h(row_window.bottom_spacer_height)
            .min_h(row_window.bottom_spacer_height),
    );

    let mut panel = div()
        .relative()
        .w_full()
        .h(panel_height)
        .min_h(panel_height)
        .bg(shell.status_line_background())
        .border_t_1()
        .border_color(shell.separator_color())
        .overflow_hidden()
        .on_mouse_move(cx.listener(ShellView::note_tool_activity_scrollbar_motion))
        .on_scroll_wheel(cx.listener(ShellView::note_tool_activity_scrollbar_scroll))
        .child(
            div()
                .id("tool-activity-scroll")
                .size_full()
                .min_h(px(0.0))
                .track_scroll(&scroll_handle)
                .overflow_y_scroll()
                .child(row_list),
        )
        .child(render_tool_activity_resize_handle(
            shell,
            entity,
            panel_height,
            composer_height,
        ));

    if let Some(scrollbar) =
        render_div_scrollbar(&scroll_handle, ScrollbarAxis::Vertical, scrollbar_opacity)
    {
        panel = panel.child(scrollbar);
    }

    Some(panel.into_any_element())
}

fn render_tool_activity_resize_handle(
    shell: &ShellView,
    entity: gpui::Entity<ShellView>,
    panel_height: gpui::Pixels,
    composer_height: gpui::Pixels,
) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h(px(layout::TOOL_ACTIVITY_RESIZE_HANDLE_HEIGHT))
        .cursor(CursorStyle::ResizeRow)
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
                                view.begin_surface_tool_activity_panel_drag(
                                    bounds.top(),
                                    bounds.top() + panel_height,
                                    composer_height,
                                    event,
                                    cx,
                                );
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
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(1.0))
                .bg(shell.separator_color()),
        )
}

fn render_tool_activity_row(
    shell: &ShellView,
    index: usize,
    agent_label: String,
    tool_display_value: String,
    status: ToolActivityRowStatus,
) -> impl IntoElement {
    div()
        .id(("tool-activity-row", index))
        .h(px(layout::TOOL_ACTIVITY_ROW_HEIGHT))
        .min_h(px(layout::TOOL_ACTIVITY_ROW_HEIGHT))
        .w_full()
        .px_4()
        .border_b_1()
        .border_color(shell.separator_color())
        .flex()
        .items_center()
        .gap_2()
        .overflow_hidden()
        .child(tool_activity_status_disc(status))
        .child(tool_activity_label(shell, "Agent"))
        .child(
            div()
                .max_w(relative(0.35))
                .min_w(px(0.0))
                .text_xs()
                .text_color(shell.status_line_value_foreground())
                .whitespace_nowrap()
                .truncate()
                .child(agent_label),
        )
        .child(tool_activity_label(shell, "Activity"))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_xs()
                .text_color(shell.status_line_value_foreground())
                .whitespace_nowrap()
                .truncate()
                .child(tool_display_value),
        )
}

fn tool_activity_label(shell: &ShellView, label: &'static str) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(shell.status_line_title_foreground())
        .whitespace_nowrap()
        .child(label)
}

fn tool_activity_status_disc(status: ToolActivityRowStatus) -> impl IntoElement {
    let color = match status {
        ToolActivityRowStatus::Running => rgb(0x22c55e),
        ToolActivityRowStatus::FinishedOk => rgb(0x64748b),
        ToolActivityRowStatus::FinishedError => rgb(0xef4444),
    };

    div()
        .w(px(10.0))
        .h(px(10.0))
        .rounded_full()
        .flex_none()
        .bg(color)
}

fn render_checklist_sidebar_panel(
    checklist_sidebar_panel: &Entity<ChecklistSidebarPanel>,
) -> impl IntoElement {
    let cached_root = div();
    let cached_style = cached_root.size_full().min_h(px(0.0)).style().clone();
    AnyView::from(checklist_sidebar_panel.clone()).cached(cached_style)
}

fn composer_height_for_input(
    surface: &ConversationSurfaceState,
    conversation_input: &Entity<SingleLineInput>,
    conversation_column_width: gpui::Pixels,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> gpui::Pixels {
    let available_height = surface
        .layout_bounds
        .map(|bounds| bounds.size.height)
        .unwrap_or_else(|| px(layout::WINDOW_MIN_HEIGHT));
    let text_width = layout::composer_text_width(conversation_column_width);
    let visual_line_count = crate::text_input::wrapped_visual_line_count_for_width(
        conversation_input.read(cx).text(),
        text_width,
        window,
    );
    layout::composer_height_for_visual_lines(
        available_height,
        window.viewport_size().height,
        window.line_height(),
        visual_line_count,
    )
}

fn render_composer(
    shell: &ShellView,
    surface: &ConversationSurfaceState,
    conversation_input: &Entity<SingleLineInput>,
    conversation_column_width: gpui::Pixels,
    composer_height: gpui::Pixels,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let text_width = layout::composer_text_width(conversation_column_width);
    let (draft_text, cursor_offset) = {
        let input = conversation_input.read(cx);
        (input.text().to_string(), input.cursor_offset())
    };
    let visual_line_count =
        crate::text_input::wrapped_visual_line_count_for_width(&draft_text, text_width, window);
    let scroll_handle = surface.composer_scroll_handle();
    let visible_input_height =
        (composer_height - px(layout::COMPOSER_OUTER_VERTICAL_PADDING)).max(px(0.0));
    let visible_text_height =
        (visible_input_height - px(layout::COMPOSER_INPUT_VERTICAL_CHROME)).max(px(0.0));
    let text_content_height =
        layout::composer_input_content_height(window.line_height(), visual_line_count);
    let scroll_content_height = layout::composer_input_scroll_content_height(
        window.line_height(),
        visual_line_count,
        visible_text_height,
    );
    let text_top_padding = layout::composer_input_centered_text_top_padding(
        window.line_height(),
        visual_line_count,
        visible_text_height,
    );
    if surface.should_reveal_composer_cursor(
        &draft_text,
        cursor_offset,
        text_width,
        scroll_content_height,
        visible_text_height,
    ) {
        reveal_composer_cursor(
            &scroll_handle,
            &draft_text,
            cursor_offset,
            text_width,
            scroll_content_height,
            visible_text_height,
            window,
        );
    }
    let scrollbar_opacity = shell.scrollbar_opacity(&crate::shell::ScrollbarRegion::Composer);

    div()
        .relative()
        .w_full()
        .h(composer_height)
        .min_h(px(layout::COMPOSER_MIN_HEIGHT))
        .key_context(COMPOSER_KEY_CONTEXT)
        .on_action(cx.listener(ShellView::queue_turn_from_composer_action))
        .on_action(cx.listener(ShellView::queue_turn_from_composer_text_enter_action))
        .on_action(cx.listener(ShellView::copy_composer_selection_action))
        .on_action(cx.listener(ShellView::cut_composer_selection_action))
        .on_action(cx.listener(ShellView::paste_composer_clipboard_image_action))
        .on_action(cx.listener(ShellView::browse_composer_history_previous_action))
        .on_action(cx.listener(ShellView::browse_composer_history_next_action))
        .on_action(cx.listener(ShellView::jump_transcript_turn_up_action))
        .on_action(cx.listener(ShellView::jump_transcript_turn_down_action))
        .bg(shell.input_panel_background())
        .border_t_1()
        .border_color(shell.separator_color())
        .child(render_composer_input_area(
            shell,
            scroll_handle,
            scrollbar_opacity,
            scroll_content_height,
            text_content_height,
            text_top_padding,
            conversation_input,
            cx,
        ))
}

fn render_composer_input_area(
    shell: &ShellView,
    scroll_handle: ScrollHandle,
    scrollbar_opacity: f32,
    scroll_content_height: gpui::Pixels,
    text_content_height: gpui::Pixels,
    text_top_padding: gpui::Pixels,
    conversation_input: &Entity<SingleLineInput>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    div()
        .absolute()
        .top(px(layout::COMPOSER_OUTER_VERTICAL_PADDING / 2.0))
        .bottom(px(layout::COMPOSER_OUTER_VERTICAL_PADDING / 2.0))
        .left(px(layout::COMPOSER_OUTER_HORIZONTAL_PADDING / 2.0))
        .right(px(layout::COMPOSER_OUTER_HORIZONTAL_PADDING / 2.0))
        .min_h(px(0.0))
        .flex()
        .items_end()
        .child(composer_input_scroll_region(
            shell,
            scroll_handle,
            scrollbar_opacity,
            scroll_content_height,
            text_content_height,
            text_top_padding,
            conversation_input,
            cx,
        ))
}

fn composer_input_scroll_region(
    shell: &ShellView,
    scroll_handle: gpui::ScrollHandle,
    scrollbar_opacity: f32,
    scroll_content_height: gpui::Pixels,
    text_content_height: gpui::Pixels,
    text_top_padding: gpui::Pixels,
    conversation_input: &Entity<SingleLineInput>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let focus_input = conversation_input.clone();
    let mut scroll_region = div()
        .relative()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .min_h(px(0.0))
        .px_3()
        .pt(px(4.0))
        .pb(px(8.0))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(shell.input_background())
        .border_1()
        .border_color(shell.input_border())
        .text_color(shell.input_foreground())
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            let focus_handle = focus_input.read(cx).tab_focus_handle();
            window.focus(&focus_handle);
        })
        .on_mouse_move(cx.listener(ShellView::note_composer_scrollbar_motion))
        .on_scroll_wheel(cx.listener(ShellView::note_composer_scrollbar_scroll))
        .child(
            div()
                .id("conversation-composer-scroll")
                .size_full()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .track_scroll(&scroll_handle)
                .child(
                    div()
                        .w_full()
                        .min_w(px(0.0))
                        .h(scroll_content_height)
                        .min_h(scroll_content_height)
                        .flex()
                        .flex_col()
                        .child(div().w_full().h(text_top_padding).min_h(text_top_padding))
                        .child(
                            div()
                                .w_full()
                                .min_w(px(0.0))
                                .h(text_content_height)
                                .min_h(text_content_height)
                                .child(conversation_input.clone()),
                        ),
                ),
        );
    if let Some(scrollbar) =
        render_div_scrollbar(&scroll_handle, ScrollbarAxis::Vertical, scrollbar_opacity)
    {
        scroll_region = scroll_region.child(scrollbar);
    }
    scroll_region
}

fn reveal_composer_cursor(
    scroll_handle: &gpui::ScrollHandle,
    text: &str,
    cursor_offset: usize,
    text_width: gpui::Pixels,
    input_content_height: gpui::Pixels,
    visible_input_height: gpui::Pixels,
    window: &mut Window,
) {
    if visible_input_height <= px(0.0) || input_content_height <= visible_input_height {
        scroll_handle.set_offset(point(px(0.0), px(0.0)));
        return;
    }

    let cursor_offset = cursor_offset.min(text.len());
    let prefix = &text[..cursor_offset];
    let cursor_visual_lines =
        crate::text_input::wrapped_visual_line_count_for_width(prefix, text_width, window).max(1);
    let line_height = window.line_height();
    let cursor_bottom = line_height * cursor_visual_lines as f32;
    let cursor_top = (cursor_bottom - line_height).max(px(0.0));
    let current_top = -scroll_handle.offset().y;
    let current_bottom = current_top + visible_input_height;
    let desired_top = if cursor_top < current_top {
        cursor_top
    } else if cursor_bottom > current_bottom {
        cursor_bottom - visible_input_height
    } else {
        current_top
    };
    let max_top = (input_content_height - visible_input_height).max(px(0.0));
    let next_top = desired_top.clamp(px(0.0), max_top);
    if (next_top - current_top).abs() > px(0.5) {
        scroll_handle.set_offset(point(px(0.0), -next_top));
    }
}

fn render_loaded_workspace_composer(
    shell: &ShellView,
    conversation_input: &Entity<SingleLineInput>,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let composer_height = px(layout::DEFAULT_COMPOSER_HEIGHT);
    let text_width = layout::composer_text_width(px(layout::WINDOW_MIN_WIDTH));
    let visual_line_count = crate::text_input::wrapped_visual_line_count_for_width(
        conversation_input.read(cx).text(),
        text_width,
        window,
    );
    let visible_input_height =
        (composer_height - px(layout::COMPOSER_OUTER_VERTICAL_PADDING)).max(px(0.0));
    let visible_text_height =
        (visible_input_height - px(layout::COMPOSER_INPUT_VERTICAL_CHROME)).max(px(0.0));
    let text_content_height =
        layout::composer_input_content_height(window.line_height(), visual_line_count);
    let scroll_content_height = layout::composer_input_scroll_content_height(
        window.line_height(),
        visual_line_count,
        visible_text_height,
    );
    let text_top_padding = layout::composer_input_centered_text_top_padding(
        window.line_height(),
        visual_line_count,
        visible_text_height,
    );

    div()
        .relative()
        .w_full()
        .h(composer_height)
        .min_h(px(layout::COMPOSER_MIN_HEIGHT))
        .bg(shell.input_panel_background())
        .border_t_1()
        .border_color(shell.separator_color())
        .child(render_composer_input_area(
            shell,
            ScrollHandle::new(),
            0.0,
            scroll_content_height,
            text_content_height,
            text_top_padding,
            conversation_input,
            cx,
        ))
}
