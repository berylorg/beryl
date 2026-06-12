use std::time::Instant;

use gpui::{
    AnyElement, AnyView, App, Context, CursorStyle, DispatchPhase, Entity, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, Window, anchored, canvas,
    div, img, point, prelude::*, px, relative, rgba,
};

use crate::BerylThemeRole;
use crate::text_input::SingleLineInput;
use crate::{
    shell::{
        BackendUnavailableState, BlockedState, COMPOSER_KEY_CONTEXT, ComposerImagePopupMode,
        ConversationSurfaceState, IdleWorkspaceState, LoadedWorkspaceState, ReadyState,
        ScrollbarRegion, ShellRenderFrame, ShellView, SurfaceNotice,
        ThreadSelectorActivationTarget, composer_input_chrome,
        composer_measurement::ComposerInputMeasurementKey,
        image_preview_popup, layout,
        status_line::{self, StatusLineCellAction, StatusLineCellValueKind, StatusLineProjection},
        status_line::{StatusLineCellValueSegment, StatusLineCellValueSegmentKind},
        tool_activity::ToolActivityRowStatus,
    },
    thread_strip_breadcrumbs::{
        ThreadStripBreadcrumbSegment, TransientBranchParent, thread_strip_breadcrumb_trail,
    },
};

use super::common::{
    button, card, disabled_secondary_button, inline_notice, secondary_button,
    secondary_button_with_active_state, section_label, toolbar_controls_strip,
};
use super::graph_link_menu::{
    render_graph_thread_link_menu, render_graph_thread_link_menu_listeners,
};
use super::graph_overlay::{render_graph_overlay, render_graph_overlay_listeners};
use super::scrollbars::{
    ScrollDirection, ScrollbarAxis, ScrollbarInteraction, ScrollbarScrollState,
    render_interactive_vertical_scrollbar, render_themed_div_scrollbar,
};
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
    shell: &ShellRenderFrame<'_>,
    ready: &ReadyState,
    transcript_panel: &Entity<TranscriptPanel>,
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
        wsl_distro_input,
        workspace_picker_filter_input,
        workspace_rename_input,
        conversation_input,
        None,
        None,
        window,
        cx,
    )
}

pub(super) fn render_backend_unavailable_shell(
    shell: &ShellRenderFrame<'_>,
    unavailable: &BackendUnavailableState,
    transcript_panel: &Entity<TranscriptPanel>,
    wsl_distro_input: &Entity<SingleLineInput>,
    workspace_picker_filter_input: &Entity<SingleLineInput>,
    workspace_rename_input: &Entity<SingleLineInput>,
    conversation_input: &Entity<SingleLineInput>,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> gpui::AnyElement {
    let reason = unavailable.availability.unavailable_reason();
    let summary = reason
        .map(|reason| reason.summary())
        .unwrap_or("The backend for this runtime target is unavailable.");
    let title = reason
        .map(|reason| reason.title())
        .unwrap_or("Backend unavailable");
    let detail = reason
        .map(|reason| reason.detail())
        .unwrap_or("Beryl has not received detailed backend availability information.");
    let empty_next_steps: &[String] = &[];
    let next_steps = reason
        .map(|reason| reason.next_steps())
        .unwrap_or(empty_next_steps);
    let banner = div()
        .flex()
        .flex_col()
        .gap_3()
        .child(inline_notice(shell, summary, BerylThemeRole::NoticeError))
        .child(
            div()
                .text_sm()
                .text_color(
                    shell.role_foreground(BerylThemeRole::NoticeError, shell.surface_foreground()),
                )
                .child(format!(
                    "Target: {}",
                    unavailable.execution_target.display_label()
                )),
        )
        .child(button(
            shell,
            "retry-backend-unavailable-inline",
            "Retry Backend",
            cx.listener(ShellView::retry_workspace),
        ))
        .into_any_element();

    render_workspace_surface(
        shell,
        &unavailable.loaded_workspace,
        unavailable.loaded_workspace.workspace.title(),
        &unavailable.execution_target,
        None,
        "backend unavailable",
        &unavailable.surface,
        transcript_panel,
        wsl_distro_input,
        workspace_picker_filter_input,
        workspace_rename_input,
        conversation_input,
        shell.backend_controls_disabled_message(),
        Some((title, detail, next_steps, banner)),
        window,
        cx,
    )
}

pub(super) fn render_idle_workspace_shell(
    shell: &ShellRenderFrame<'_>,
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
    let workspace_members_scrollbar_visibility =
        shell.scrollbar_visibility_policy(&ScrollbarRegion::WorkspaceMembers, cx);
    let mut workspace_members_scroll_region = div()
        .relative()
        .size_full()
        .min_h(px(0.0))
        .on_mouse_move(cx.listener(ShellView::note_workspace_members_scrollbar_motion))
        .on_scroll_wheel(cx.listener(ShellView::note_workspace_members_scrollbar_scroll))
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
                        .child(section_label(shell, "Workspace Members"))
                        .when_some(loaded.startup_warning.as_ref(), |this, warning| {
                            this.child(inline_notice(shell, warning, BerylThemeRole::NoticeInfo))
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
                                        .font_weight(shell.role_font_weight(
                                            BerylThemeRole::ControlNoticeTitle,
                                            gpui::FontWeight::SEMIBOLD,
                                        ))
                                        .child("No runtime environment selected"),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(shell.surface_foreground())
                                        .child(format!(
                                            "Beryl opened the legacy semantic workspace '{}', but it does not have a selected runtime environment.",
                                            loaded.workspace.title()
                                        )),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(shell.surface_muted_foreground())
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
                                        .text_color(shell.surface_muted_foreground())
                                        .child(
                                            "Workspace switching stays in the toolbar Workspaces popup rather than a dedicated full-screen picker.",
                                        ),
                                ),
                        )),
                ),
        );
    if let Some(scrollbar) = render_themed_div_scrollbar(
        shell.style(),
        "idle-workspace-members-scrollbar",
        &workspace_members_scroll_handle,
        ScrollbarAxis::Vertical,
        workspace_members_scrollbar_visibility,
    ) {
        workspace_members_scroll_region = workspace_members_scroll_region.child(scrollbar);
    }

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
                        .text_color(shell.surface_muted_foreground())
                        .child("Runtime environment recovery required"),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .px_4()
                .py_4()
                .child(workspace_members_scroll_region),
        )
        .child(render_loaded_workspace_composer(
            shell,
            conversation_input,
            window,
            cx,
        ))
        .child(render_status_line(
            shell,
            StatusLineProjection::unknown(),
            cx,
        ));

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
    shell: &ShellRenderFrame<'_>,
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
                        .child(section_label(shell, "Workspace Surface"))
                        .when_some(loaded.startup_warning.as_ref(), |this, warning| {
                            this.child(inline_notice(shell, warning, BerylThemeRole::NoticeInfo))
                        })
                        .child(
                            div()
                                .text_lg()
                                .font_weight(shell.role_font_weight(
                                    BerylThemeRole::ControlNoticeTitle,
                                    gpui::FontWeight::SEMIBOLD,
                                ))
                                .child("Opening primary workspace member"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(shell.surface_foreground())
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
    shell: &ShellRenderFrame<'_>,
    blocked: &BlockedState,
    transcript_panel: &Entity<TranscriptPanel>,
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
            shell,
            &blocked.summary,
            BerylThemeRole::NoticeError,
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
        wsl_distro_input,
        workspace_picker_filter_input,
        workspace_rename_input,
        conversation_input,
        Some(blocked.summary.clone()),
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
    shell: &ShellRenderFrame<'_>,
    loaded_workspace: &LoadedWorkspaceState,
    _workspace_title: &str,
    execution_target: &beryl_model::workspace::WorkspaceId,
    _process_id: Option<u32>,
    _backend_label: &str,
    surface: &ConversationSurfaceState,
    transcript_panel: &Entity<TranscriptPanel>,
    _wsl_distro_input: &Entity<SingleLineInput>,
    workspace_picker_filter_input: &Entity<SingleLineInput>,
    workspace_rename_input: &Entity<SingleLineInput>,
    conversation_input: &Entity<SingleLineInput>,
    backend_controls_disabled: Option<String>,
    blocked: Option<(&'static str, &str, &[String], gpui::AnyElement)>,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> gpui::AnyElement {
    let new_thread_controls_disabled = shell.new_thread_controls_disabled_message();
    let thread_selector_controls_disabled = shell.thread_selector_controls_disabled_message();
    let toolbar = render_toolbar(
        shell,
        loaded_workspace,
        execution_target,
        surface,
        thread_selector_controls_disabled.as_deref(),
        cx,
    )
    .into_any_element();
    let thread_strip = render_thread_strip(
        shell,
        execution_target,
        &loaded_workspace.workspace_state,
        &loaded_workspace.threaded_decision_state,
        surface,
        new_thread_controls_disabled.as_deref(),
        thread_selector_controls_disabled.as_deref(),
        cx,
    )
    .into_any_element();
    let entity = cx.entity();
    let conversation_width = surface.transcript_width();
    let composer_enabled = backend_controls_disabled.is_none();
    let composer_measurement = measure_composer_input(
        shell,
        surface,
        conversation_input,
        conversation_width,
        composer_enabled,
        window,
        cx,
    );
    let composer_height = composer_measurement.composer_height;
    let split = render_split_surface(
        shell,
        transcript_panel,
        surface,
        conversation_input,
        &composer_measurement,
        backend_controls_disabled.as_deref(),
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

    let status_line = render_status_line(
        shell,
        if backend_controls_disabled.is_some() {
            StatusLineProjection::unknown()
        } else {
            surface.status_line_projection()
        },
        cx,
    );

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
        let mut detail_block =
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(shell.role_foreground(
                            BerylThemeRole::NoticeWarning,
                            shell.surface_foreground(),
                        ))
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(shell.surface_foreground())
                        .child(detail.to_string()),
                )
                .child(banner);
        for next_step in next_steps {
            detail_block = detail_block.child(
                div()
                    .text_sm()
                    .text_color(shell.surface_muted_foreground())
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
        if let Some(menu) = render_graph_thread_link_menu(
            shell,
            loaded_workspace,
            surface,
            new_thread_controls_disabled.as_deref(),
            cx,
        ) {
            body = body.child(menu);
        }
    }

    if surface.transcript_branch_menu().is_open() {
        body = body.child(render_transcript_branch_menu_listeners(cx));
        if let Some(menu) = render_transcript_branch_menu(shell, surface, cx) {
            body = body.child(menu);
        }
    }

    if surface.thread_selector().is_open() && thread_selector_controls_disabled.is_none() {
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
    if surface.status_line_operations().is_open() && backend_controls_disabled.is_none() {
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
    shell: &ShellRenderFrame<'_>,
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
    let notice_role = surface_notice_role(notice);

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
                                    .text_color(
                                        shell.role_foreground(
                                            notice_role,
                                            shell.surface_foreground(),
                                        ),
                                    )
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
                    .group("surface-notice-close")
                    .flex_none()
                    .w(px(layout::BUTTON_OUTER_HEIGHT))
                    .h(px(layout::BUTTON_OUTER_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
                    .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
                    .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
                    .font_weight(shell.secondary_button_theme().font_weight)
                    .text_color(shell.surface_muted_foreground())
                    .hover({
                        let hover_background = shell.role_background(
                            BerylThemeRole::PopupRowHover,
                            shell.row_surface_background(),
                        );
                        move |style| style.bg(hover_background)
                    })
                    .active({
                        let active_background = shell.role_background(
                            BerylThemeRole::PopupRowSelected,
                            shell.row_surface_background(),
                        );
                        move |style| style.bg(active_background)
                    })
                    .cursor(CursorStyle::PointingHand)
                    .child("X")
                    .on_click(cx.listener(ShellView::dismiss_surface_notice)),
            ),
    )
}

fn surface_notice_role(notice: &SurfaceNotice) -> BerylThemeRole {
    let title = notice.title().to_ascii_lowercase();
    if title.contains("error") || title.contains("failed") || title.contains("rejected") {
        BerylThemeRole::NoticeError
    } else if title.contains("warning") {
        BerylThemeRole::NoticeWarning
    } else if title.contains("success") {
        BerylThemeRole::NoticeSuccess
    } else {
        BerylThemeRole::NoticeInfo
    }
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
    shell: &ShellRenderFrame<'_>,
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
    shell: &ShellRenderFrame<'_>,
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
    shell: &ShellRenderFrame<'_>,
    index: usize,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    secondary_button(shell, ("composer-image-menu-row", index), label, on_click)
        .w_full()
        .justify_start()
}

fn render_composer_image_preview(shell: &ShellRenderFrame<'_>, label: &str) -> impl IntoElement {
    let image = shell.composer_image_preview_image();

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
                .border_color(
                    shell.role_border(BerylThemeRole::MediaBorder, shell.surface_border()),
                )
                .bg(shell.role_background(
                    BerylThemeRole::MediaPlaceholder,
                    shell.popup_surface_background(),
                ))
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

fn render_toolbar(
    shell: &ShellRenderFrame<'_>,
    loaded_workspace: &LoadedWorkspaceState,
    execution_target: &beryl_model::workspace::WorkspaceId,
    surface: &ConversationSurfaceState,
    thread_selector_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let selected_label =
        selected_thread_title_label(surface, &loaded_workspace.workspace_state, execution_target);
    let branch_breadcrumb_segments = toolbar_branch_breadcrumb_segments(
        shell,
        &loaded_workspace.workspace_state,
        surface,
        &selected_label,
    );

    toolbar_controls_strip(
        shell,
        div()
            .flex()
            .items_center()
            .w_full()
            .gap_2()
            .child(
                div()
                    .flex_initial()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(render_workspace_picker_button(shell, loaded_workspace, cx))
                    .when_some(branch_breadcrumb_segments, |this, segments| {
                        this.child(render_toolbar_branch_breadcrumbs(
                            shell,
                            segments,
                            thread_selector_controls_disabled,
                            cx,
                        ))
                    }),
            )
            .child(div().flex_1().min_w(px(0.0)))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(graph_toolbar_button(
                        shell,
                        surface.graph_overlay().visible(),
                        cx,
                    ))
                    .child(secondary_button(
                        shell,
                        "settings-toolbar",
                        "Settings",
                        cx.listener(ShellView::open_settings_window),
                    )),
            ),
    )
}

fn selected_thread_title_label(
    surface: &ConversationSurfaceState,
    workspace_state: &beryl_model::conversation::WorkspaceConversationState,
    workspace: &beryl_model::workspace::WorkspaceId,
) -> String {
    surface
        .selected_thread_display_label(workspace_state, workspace)
        .unwrap_or_else(|| "New conversation".to_string())
}

fn toolbar_branch_breadcrumb_segments(
    shell: &ShellRenderFrame<'_>,
    workspace_state: &beryl_model::conversation::WorkspaceConversationState,
    surface: &ConversationSurfaceState,
    selected_label: &str,
) -> Option<Vec<ThreadStripBreadcrumbSegment>> {
    let selected_thread_id = surface.selected_thread_id();
    let transient_branch_parent = shell
        .foreground_transcript_branch
        .as_ref()
        .and_then(|branch| {
            let branch_thread_id = branch.branch_thread_id()?;
            (selected_thread_id == Some(branch_thread_id.as_str())).then(|| {
                (
                    branch_thread_id.clone(),
                    beryl_model::conversation::ConversationThreadId::new(
                        branch.source_thread_id().to_string(),
                    ),
                )
            })
        });
    let transient_branch_parent =
        transient_branch_parent
            .as_ref()
            .map(
                |(child_thread_id, parent_thread_id)| TransientBranchParent {
                    child_thread_id,
                    parent_thread_id,
                },
            );
    let segments = thread_strip_breadcrumb_trail(
        workspace_state,
        selected_thread_id,
        selected_label,
        transient_branch_parent,
    )?
    .segments()
    .iter()
    .filter(|segment| !segment.active())
    .cloned()
    .collect::<Vec<_>>();

    (!segments.is_empty()).then_some(segments)
}

fn render_toolbar_branch_breadcrumbs(
    shell: &ShellRenderFrame<'_>,
    segments: Vec<ThreadStripBreadcrumbSegment>,
    thread_selector_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let mut row = div()
        .flex_initial()
        .max_w(px(layout::TOOLBAR_BREADCRUMB_TRAIL_MAX_WIDTH))
        .min_w(px(0.0))
        .h_full()
        .px(px(layout::BUTTON_BORDER_WIDTH))
        .flex()
        .items_center()
        .gap(px(layout::TOOLBAR_BREADCRUMB_GAP))
        .overflow_hidden();

    for (index, segment) in segments.into_iter().enumerate() {
        if index > 0 {
            row = row.child(
                div()
                    .flex_none()
                    .w(px(layout::TOOLBAR_BREADCRUMB_SEPARATOR_WIDTH))
                    .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
                    .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
                    .text_center()
                    .text_color(shell.surface_muted_foreground())
                    .child(">"),
            );
        }

        row = row.child(render_toolbar_parent_breadcrumb(
            shell,
            segment,
            index,
            thread_selector_controls_disabled,
            cx,
        ));
    }

    row
}

fn graph_toolbar_button(
    shell: &ShellRenderFrame<'_>,
    graph_visible: bool,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    secondary_button_with_active_state(
        shell,
        "graph-toolbar",
        "Graph",
        graph_visible,
        cx.listener(ShellView::toggle_graph_overlay),
    )
}

fn thread_navigation_button(
    shell: &ShellRenderFrame<'_>,
    id: &'static str,
    label: &'static str,
    disabled_reason: Option<String>,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    match disabled_reason {
        Some(reason) => {
            let tooltip = ThreadNavigationTooltip {
                message: reason,
                background: shell.popup_surface_background(),
                border: shell.surface_border(),
                foreground: shell.general_ui_foreground(),
            };
            disabled_secondary_button(shell, id, label)
                .w(px(layout::BUTTON_ICON_OUTER_WIDTH))
                .opacity(0.62)
                .tooltip(move |_, cx| build_thread_navigation_tooltip(tooltip.clone(), cx))
                .into_any_element()
        }
        None => secondary_button(shell, id, label, on_click)
            .w(px(layout::BUTTON_ICON_OUTER_WIDTH))
            .into_any_element(),
    }
}

#[derive(Clone)]
struct ThreadNavigationTooltip {
    message: String,
    background: gpui::Rgba,
    border: gpui::Rgba,
    foreground: gpui::Rgba,
}

fn build_thread_navigation_tooltip(tooltip: ThreadNavigationTooltip, cx: &mut App) -> AnyView {
    cx.new(|_| tooltip).into()
}

impl Render for ThreadNavigationTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(280.0))
            .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
            .bg(self.background)
            .border_1()
            .border_color(self.border)
            .px_3()
            .py_2()
            .text_xs()
            .text_color(self.foreground)
            .child(self.message.clone())
    }
}

fn render_thread_strip(
    shell: &ShellRenderFrame<'_>,
    workspace: &beryl_model::workspace::WorkspaceId,
    workspace_state: &beryl_model::conversation::WorkspaceConversationState,
    _threaded_decision_state: &beryl_model::threaded_decision::ThreadedDecisionState,
    surface: &ConversationSurfaceState,
    new_thread_controls_disabled: Option<&str>,
    thread_selector_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let entity = cx.entity();
    let new_thread_enabled = new_thread_controls_disabled.is_none();
    let pending_activation_progress = surface.pending_thread_activation_progress();
    let thread_selector_enabled =
        thread_selector_controls_disabled.is_none() && pending_activation_progress.is_none();
    let active_label = surface
        .pending_thread_activation_label()
        .map(str::to_string)
        .unwrap_or_else(|| selected_thread_title_label(surface, workspace_state, workspace));
    let backward_disabled_reason = shell.thread_navigation_backward_disabled_reason();
    let forward_disabled_reason = shell.thread_navigation_forward_disabled_reason();

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
        .child(if new_thread_enabled {
            thread_strip_action(
                shell,
                "thread-strip-new-thread",
                "New Thread",
                cx.listener(ShellView::start_new_thread),
            )
            .into_any_element()
        } else {
            disabled_secondary_button(shell, "thread-strip-new-thread", "New Thread")
                .into_any_element()
        })
        .child(thread_navigation_button(
            shell,
            "thread-navigation-backward-thread-strip",
            "<",
            backward_disabled_reason,
            cx.listener(|view, _event, window, cx| {
                view.activate_thread_navigation_backward(window, cx);
            }),
        ))
        .child(thread_navigation_button(
            shell,
            "thread-navigation-forward-thread-strip",
            ">",
            forward_disabled_reason,
            cx.listener(|view, _event, window, cx| {
                view.activate_thread_navigation_forward(window, cx);
            }),
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
                .child(render_thread_strip_active_thread_title(
                    shell,
                    entity,
                    active_label,
                    pending_activation_progress,
                    thread_selector_enabled,
                    surface.thread_selector().is_open(),
                    cx,
                )),
        )
}

fn render_thread_strip_active_thread_title(
    shell: &ShellRenderFrame<'_>,
    entity: Entity<ShellView>,
    active_label: String,
    pending_activation_progress: Option<f32>,
    thread_selector_enabled: bool,
    thread_selector_open: bool,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let progress_fill = shell.role_color(
        BerylThemeRole::PrimitiveAccentMarker,
        shell.general_ui_foreground(),
    );
    let text_color = if pending_activation_progress.is_some() {
        shell.role_foreground(
            BerylThemeRole::MainThreadStripActiveThreadLabel,
            shell.general_ui_foreground(),
        )
    } else if !thread_selector_enabled {
        shell.secondary_button_theme().disabled.foreground
    } else if thread_selector_open {
        shell.role_foreground(
            BerylThemeRole::MainThreadStripActiveThreadLabel,
            shell.secondary_button_theme().active.foreground,
        )
    } else {
        shell.role_foreground(
            BerylThemeRole::MainThreadStripActiveThreadLabel,
            shell.general_ui_foreground(),
        )
    };

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
                .relative()
                .overflow_hidden()
                .w_full()
                .h(px(layout::BUTTON_OUTER_HEIGHT))
                .px(px(layout::BUTTON_HORIZONTAL_PADDING))
                .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
                .bg(shell.role_background(
                    BerylThemeRole::MainThreadStripActiveThread,
                    gpui::rgba(0x00000000),
                ))
                .flex()
                .items_center()
                .when(thread_selector_enabled, |this| {
                    this.hover({
                        let theme = shell.secondary_button_theme();
                        move |style| style.bg(theme.hover.background)
                    })
                })
                .when(thread_selector_enabled, |this| {
                    this.active({
                        let theme = shell.secondary_button_theme();
                        move |style| style.bg(theme.active.background)
                    })
                })
                .when(thread_selector_enabled, |this| this.cursor_pointer())
                .when_some(pending_activation_progress, |this, progress| {
                    this.child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(relative(progress.clamp(0.0, 1.0)))
                            .bg(progress_fill)
                            .opacity(0.28),
                    )
                })
                .child(
                    div()
                        .relative()
                        .min_w(px(0.0))
                        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
                        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
                        .font_weight(shell.secondary_button_theme().font_weight)
                        .text_color(text_color)
                        .whitespace_nowrap()
                        .truncate()
                        .child(active_label),
                )
                .when(thread_selector_enabled, |this| {
                    this.on_click(cx.listener(ShellView::toggle_thread_selector))
                }),
        )
}

fn render_toolbar_parent_breadcrumb(
    shell: &ShellRenderFrame<'_>,
    breadcrumb: ThreadStripBreadcrumbSegment,
    index: usize,
    thread_selector_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let label = breadcrumb.label().to_string();
    let disabled_reason = thread_selector_controls_disabled
        .map(str::to_string)
        .or_else(|| breadcrumb.disabled_reason().map(str::to_string));
    let activation_target = if disabled_reason.is_none() && breadcrumb.activation_available() {
        breadcrumb
            .execution_target()
            .map(|execution_target| ThreadSelectorActivationTarget {
                thread_id: breadcrumb.thread_id().clone(),
                label: label.clone(),
                execution_target: execution_target.clone(),
            })
    } else {
        None
    };
    let theme = shell.secondary_button_theme();
    let button_state = if activation_target.is_some() {
        theme.normal
    } else {
        theme.disabled
    };

    let mut breadcrumb_button = div()
        .id(("toolbar-branch-parent-breadcrumb", index))
        .flex_initial()
        .max_w(px(layout::TOOLBAR_BREADCRUMB_BUTTON_MAX_WIDTH))
        .min_w(px(0.0))
        .h(px(layout::BUTTON_OUTER_HEIGHT))
        .px(px(layout::BUTTON_HORIZONTAL_PADDING))
        .py(px(layout::BUTTON_VERTICAL_PADDING))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(button_state.background)
        .border_1()
        .border_color(button_state.border)
        .flex()
        .items_center()
        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
        .font_weight(theme.font_weight)
        .text_color(button_state.foreground)
        .child(
            div()
                .min_w(px(0.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .truncate()
                .child(label.clone()),
        );

    if let Some(target) = activation_target {
        breadcrumb_button = breadcrumb_button
            .hover(move |style| {
                style
                    .bg(theme.hover.background)
                    .border_color(theme.hover.border)
            })
            .active(move |style| {
                style
                    .bg(theme.active.background)
                    .border_color(theme.active.border)
            })
            .cursor_pointer()
            .on_click(cx.listener(move |view, _event, window, cx| {
                view.activate_branch_breadcrumb_thread_target(target.clone(), window, cx);
            }));
    } else if let Some(reason) = disabled_reason {
        let tooltip_theme = ThreadStripBreadcrumbTooltipTheme::from_shell(shell);
        breadcrumb_button = breadcrumb_button.tooltip(move |_, cx| {
            build_thread_strip_breadcrumb_tooltip(reason.clone(), tooltip_theme, cx)
        });
    }

    breadcrumb_button
}

struct ThreadStripBreadcrumbTooltip {
    message: String,
    theme: ThreadStripBreadcrumbTooltipTheme,
}

#[derive(Clone, Copy)]
struct ThreadStripBreadcrumbTooltipTheme {
    background: gpui::Rgba,
    border: gpui::Rgba,
    foreground: gpui::Rgba,
}

impl ThreadStripBreadcrumbTooltipTheme {
    fn from_shell(shell: &ShellRenderFrame<'_>) -> Self {
        Self {
            background: shell.popup_surface_background(),
            border: shell.surface_border(),
            foreground: shell.general_ui_foreground(),
        }
    }
}

fn build_thread_strip_breadcrumb_tooltip(
    message: String,
    theme: ThreadStripBreadcrumbTooltipTheme,
    cx: &mut App,
) -> AnyView {
    cx.new(|_| ThreadStripBreadcrumbTooltip { message, theme })
        .into()
}

impl Render for ThreadStripBreadcrumbTooltip {
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
            .child(self.message.clone())
    }
}

fn render_status_line(
    shell: &ShellRenderFrame<'_>,
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
            StatusLineCellValueKind::TurnState => {
                Some(last_turn_state_color(shell, spec.value.as_str()))
            }
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
    shell: &ShellRenderFrame<'_>,
    label: &'static str,
    value_segments: Vec<StatusLineCellValueSegment>,
    value_color: Option<gpui::Rgba>,
    enabled: bool,
    action: StatusLineCellAction,
    cx: &mut Context<ShellView>,
) -> gpui::Div {
    let default_value_color = if enabled || matches!(action, StatusLineCellAction::None) {
        shell.status_line_value_foreground()
    } else {
        shell.surface_muted_foreground()
    };
    let resolved_value_color = value_color.unwrap_or(default_value_color);
    let mut cell = div()
        .h_full()
        .w(relative(1.0 / 3.0))
        .min_w(px(0.0))
        .px_4()
        .bg(shell.role_background(
            BerylThemeRole::StatusLineCell,
            shell.status_line_background(),
        ))
        .border_r_1()
        .border_color(shell.role_border(BerylThemeRole::StatusLineCell, shell.separator_color()))
        .flex()
        .items_center()
        .gap_2()
        .overflow_hidden()
        .child(
            div()
                .text_xs()
                .font_weight(
                    shell.role_font_weight(
                        BerylThemeRole::StatusLineLabel,
                        gpui::FontWeight::NORMAL,
                    ),
                )
                .text_color(shell.role_foreground(
                    BerylThemeRole::StatusLineLabel,
                    shell.status_line_title_foreground(),
                ))
                .whitespace_nowrap()
                .child(label),
        )
        .child(status_line_value(
            shell,
            value_segments,
            resolved_value_color,
            default_value_color,
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
    shell: &ShellRenderFrame<'_>,
    segments: Vec<StatusLineCellValueSegment>,
    value_color: gpui::Rgba,
    default_value_color: gpui::Rgba,
) -> gpui::Div {
    if let [segment] = segments.as_slice()
        && segment.kind == StatusLineCellValueSegmentKind::Value
    {
        return div()
            .flex_1()
            .min_w(px(0.0))
            .text_xs()
            .font_weight(
                shell.role_font_weight(BerylThemeRole::StatusLineValue, gpui::FontWeight::NORMAL),
            )
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
            StatusLineCellValueSegmentKind::Label => shell.role_foreground(
                BerylThemeRole::StatusLineLabel,
                shell.status_line_title_foreground(),
            ),
            StatusLineCellValueSegmentKind::Value => value_color,
            StatusLineCellValueSegmentKind::SecondaryValue => default_value_color,
        };
        let font_weight =
            match segment.kind {
                StatusLineCellValueSegmentKind::Label => shell
                    .role_font_weight(BerylThemeRole::StatusLineLabel, gpui::FontWeight::NORMAL),
                StatusLineCellValueSegmentKind::Value
                | StatusLineCellValueSegmentKind::SecondaryValue => shell
                    .role_font_weight(BerylThemeRole::StatusLineValue, gpui::FontWeight::NORMAL),
            };
        value = value.child(
            div()
                .text_xs()
                .font_weight(font_weight)
                .text_color(color)
                .whitespace_nowrap()
                .flex_none()
                .child(segment.text),
        );
    }

    value
}

fn last_turn_state_color(shell: &ShellRenderFrame<'_>, state: &str) -> gpui::Rgba {
    match state {
        "working" | "active" => shell.role_foreground(
            BerylThemeRole::StatusValueWorking,
            shell.status_line_value_foreground(),
        ),
        "compacting" => shell.role_foreground(
            BerylThemeRole::StatusValueCompacting,
            shell.status_line_value_foreground(),
        ),
        "ok" => shell.role_foreground(
            BerylThemeRole::StatusValueOk,
            shell.status_line_value_foreground(),
        ),
        "error" => shell.role_foreground(
            BerylThemeRole::StatusValueError,
            shell.status_line_value_foreground(),
        ),
        "pending" => shell.role_foreground(
            BerylThemeRole::StatusValuePending,
            shell.status_line_value_foreground(),
        ),
        "unavailable" => shell.role_foreground(
            BerylThemeRole::StatusValueUnavailable,
            shell.status_line_value_foreground(),
        ),
        "streaming" => shell.role_foreground(
            BerylThemeRole::StatusValueStreaming,
            shell.status_line_value_foreground(),
        ),
        _ => shell.status_line_value_foreground(),
    }
}

fn thread_strip_action(
    shell: &ShellRenderFrame<'_>,
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    secondary_button(shell, id, label, on_click)
}

fn render_split_surface(
    shell: &ShellRenderFrame<'_>,
    transcript_panel: &Entity<TranscriptPanel>,
    surface: &ConversationSurfaceState,
    conversation_input: &Entity<SingleLineInput>,
    composer_measurement: &layout::ComposerInputMeasurement,
    backend_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let entity = cx.entity();
    let composer_height = composer_measurement.composer_height;

    let left_panel = render_left_panel(shell, surface, transcript_panel, cx).into_any_element();
    let composer = render_composer(
        shell,
        conversation_input,
        composer_measurement,
        backend_controls_disabled,
        cx,
    )
    .into_any_element();
    let tool_activity_panel = render_tool_activity_panel(shell, surface, composer_height, cx);

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
            div().size_full().flex().gap_0().child(
                div()
                    .w_full()
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
            ),
        )
}

fn render_left_panel(
    shell: &ShellRenderFrame<'_>,
    surface: &ConversationSurfaceState,
    transcript_panel: &Entity<TranscriptPanel>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let cached_root = div();
    let cached_style = cached_root.size_full().min_h(px(0.0)).style().clone();
    let transcript_list_state = surface.transcript_list_state();
    let bounds = transcript_list_state.viewport_bounds();
    let max_offset = transcript_list_state.max_offset_for_scrollbar();
    let offset = transcript_list_state.scroll_px_offset_for_scrollbar();
    let scrollbar_visibility = shell.scrollbar_visibility_policy(&ScrollbarRegion::Transcript, cx);
    let shell_entity = cx.entity();
    let scrollbar_owner_update = {
        let transcript_list_state = transcript_list_state.clone();
        move |window: &mut Window, cx: &mut gpui::App| {
            shell_entity.update(cx, |view, cx| {
                view.note_transcript_scrollbar_owner_update(&transcript_list_state, window, cx);
            });
        }
    };
    let scrollbar_interaction = ScrollbarInteraction::new(
        {
            let transcript_list_state = transcript_list_state.clone();
            move || {
                Some(ScrollbarScrollState {
                    viewport_bounds: transcript_list_state.viewport_bounds(),
                    max_offset: transcript_list_state.max_offset_for_scrollbar(),
                    scroll_offset: {
                        let offset = transcript_list_state.scroll_px_offset_for_scrollbar();
                        point(px(0.0), -offset.y)
                    },
                })
            }
        },
        {
            let transcript_list_state = transcript_list_state.clone();
            move |scroll_offset| {
                transcript_list_state.set_offset_from_scrollbar(point(px(0.0), -scroll_offset));
            }
        },
        {
            let transcript_list_state = transcript_list_state.clone();
            move |direction, distance| {
                let distance = match direction {
                    ScrollDirection::Backward => -distance,
                    ScrollDirection::Forward => distance,
                };
                transcript_list_state.scroll_by(distance);
            }
        },
        {
            let transcript_list_state = transcript_list_state.clone();
            move || {
                transcript_list_state.scrollbar_drag_started();
            }
        },
        {
            let transcript_list_state = transcript_list_state;
            move || {
                transcript_list_state.scrollbar_drag_ended();
            }
        },
        scrollbar_owner_update,
    );

    let mut panel = div()
        .relative()
        .size_full()
        .min_h(px(0.0))
        .child(AnyView::from(transcript_panel.clone()).cached(cached_style));
    if let Some(scrollbar) = render_interactive_vertical_scrollbar(
        "transcript-scrollbar",
        bounds.size.height,
        max_offset.height,
        -offset.y,
        scrollbar_visibility,
        scrollbar_interaction,
    ) {
        panel = panel.child(scrollbar);
    }
    panel
}

fn render_tool_activity_panel(
    shell: &ShellRenderFrame<'_>,
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
    let scrollbar_visibility =
        shell.scrollbar_visibility_policy(&crate::shell::ScrollbarRegion::ToolActivity, cx);

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
        .bg(shell.role_background(
            BerylThemeRole::ActivityPanel,
            shell.status_line_background(),
        ))
        .border_t_1()
        .border_color(shell.role_color(
            BerylThemeRole::ActivityResizeHandle,
            shell.separator_color(),
        ))
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
            entity.clone(),
            panel_height,
            composer_height,
        ));

    if let Some(scrollbar) = render_themed_div_scrollbar(
        shell.style(),
        "tool-activity-scrollbar",
        &scroll_handle,
        ScrollbarAxis::Vertical,
        scrollbar_visibility,
    ) {
        panel = panel.child(scrollbar);
    }

    Some(panel.into_any_element())
}

fn render_tool_activity_resize_handle(
    shell: &ShellRenderFrame<'_>,
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
                .bg(shell.role_color(
                    BerylThemeRole::ActivityResizeHandle,
                    shell.separator_color(),
                )),
        )
}

fn render_tool_activity_row(
    shell: &ShellRenderFrame<'_>,
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
        .bg(shell.role_background(BerylThemeRole::ActivityRow, shell.status_line_background()))
        .border_b_1()
        .border_color(shell.role_border(BerylThemeRole::ActivityRow, shell.separator_color()))
        .flex()
        .items_center()
        .gap_2()
        .overflow_hidden()
        .child(tool_activity_status_disc(shell, status))
        .child(tool_activity_label(shell, "Agent"))
        .child(
            div()
                .max_w(relative(0.35))
                .min_w(px(0.0))
                .text_xs()
                .font_weight(
                    shell.role_font_weight(BerylThemeRole::ActivityValue, gpui::FontWeight::NORMAL),
                )
                .text_color(shell.role_foreground(
                    BerylThemeRole::ActivityValue,
                    shell.status_line_value_foreground(),
                ))
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
                .font_weight(
                    shell.role_font_weight(BerylThemeRole::ActivityValue, gpui::FontWeight::NORMAL),
                )
                .text_color(shell.role_foreground(
                    BerylThemeRole::ActivityValue,
                    shell.status_line_value_foreground(),
                ))
                .whitespace_nowrap()
                .truncate()
                .child(tool_display_value),
        )
}

fn tool_activity_label(shell: &ShellRenderFrame<'_>, label: &'static str) -> impl IntoElement {
    div()
        .text_xs()
        .font_weight(
            shell.role_font_weight(BerylThemeRole::ActivityLabel, gpui::FontWeight::NORMAL),
        )
        .text_color(shell.role_foreground(
            BerylThemeRole::ActivityLabel,
            shell.status_line_title_foreground(),
        ))
        .whitespace_nowrap()
        .child(label)
}

fn tool_activity_status_disc(
    shell: &ShellRenderFrame<'_>,
    status: ToolActivityRowStatus,
) -> impl IntoElement {
    let color = match status {
        ToolActivityRowStatus::Running => shell.role_color(
            BerylThemeRole::ActivityIndicatorRunning,
            shell.status_line_value_foreground(),
        ),
        ToolActivityRowStatus::FinishedOk => shell.role_color(
            BerylThemeRole::ActivityIndicatorOk,
            shell.status_line_value_foreground(),
        ),
        ToolActivityRowStatus::FinishedError => shell.role_color(
            BerylThemeRole::ActivityIndicatorError,
            shell.status_line_value_foreground(),
        ),
    };

    div()
        .w(px(10.0))
        .h(px(10.0))
        .rounded_full()
        .flex_none()
        .bg(color)
}

fn measure_composer_input(
    shell: &ShellRenderFrame<'_>,
    surface: &ConversationSurfaceState,
    conversation_input: &Entity<SingleLineInput>,
    conversation_column_width: gpui::Pixels,
    enabled: bool,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> layout::ComposerInputMeasurement {
    let available_height = surface
        .layout_bounds
        .map(|bounds| bounds.size.height)
        .unwrap_or_else(|| px(layout::WINDOW_MIN_HEIGHT));
    let viewport_height = window.viewport_size().height;
    let key = ComposerInputMeasurementKey::new(
        shell.composer_input_revision(),
        shell.composer_image_atom_revision(),
        conversation_column_width,
        available_height,
        viewport_height,
        window.scale_factor(),
        shell.style().revision(),
        enabled,
        surface.transcript_edit_mode().is_some(),
    );

    let measurement_started = Instant::now();
    let measurement = shell.cached_composer_input_measurement(key, || {
        measure_uncached_composer_input(
            conversation_input,
            conversation_column_width,
            available_height,
            viewport_height,
            window,
            cx,
        )
    });
    shell.record_composer_measurement_cost(measurement_started.elapsed());
    measurement
}

fn measure_uncached_composer_input(
    conversation_input: &Entity<SingleLineInput>,
    conversation_column_width: gpui::Pixels,
    available_height: gpui::Pixels,
    viewport_height: gpui::Pixels,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> layout::ComposerInputMeasurement {
    let initial_bounds =
        layout::composer_text_input_bounds(conversation_column_width, available_height);
    let initial_geometry = conversation_input
        .read(cx)
        .measure_geometry(initial_bounds, window);
    let initial_measurement =
        layout::composer_input_measurement(available_height, viewport_height, &initial_geometry);
    if initial_measurement.input_render_height >= initial_measurement.text_content_height {
        return initial_measurement;
    }

    let final_geometry = conversation_input
        .read(cx)
        .measure_geometry(initial_measurement.input_bounds, window);

    layout::composer_input_measurement(available_height, viewport_height, &final_geometry)
}

fn render_composer(
    shell: &ShellRenderFrame<'_>,
    conversation_input: &Entity<SingleLineInput>,
    composer_measurement: &layout::ComposerInputMeasurement,
    backend_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let enabled = backend_controls_disabled.is_none();
    conversation_input.update(cx, |input, cx| input.set_enabled(enabled, cx));
    let mut composer = div()
        .relative()
        .w_full()
        .h(composer_measurement.composer_height)
        .min_h(px(layout::COMPOSER_MIN_HEIGHT))
        .bg(shell.input_panel_background())
        .border_t_1()
        .border_color(shell.separator_color());
    if enabled {
        composer = composer
            .key_context(COMPOSER_KEY_CONTEXT)
            .on_action(cx.listener(ShellView::queue_turn_from_composer_action))
            .on_action(cx.listener(ShellView::queue_turn_from_composer_text_enter_action))
            .on_action(cx.listener(ShellView::copy_composer_selection_action))
            .on_action(cx.listener(ShellView::cut_composer_selection_action))
            .on_action(cx.listener(ShellView::paste_composer_clipboard_image_action))
            .on_action(cx.listener(ShellView::browse_composer_history_previous_action))
            .on_action(cx.listener(ShellView::browse_composer_history_next_action))
            .on_action(cx.listener(ShellView::jump_transcript_turn_up_action))
            .on_action(cx.listener(ShellView::jump_transcript_turn_down_action));
    }
    composer.child(render_composer_input_area(
        shell,
        composer_measurement.input_render_height,
        composer_measurement.text_top_padding,
        conversation_input,
        backend_controls_disabled,
        cx,
    ))
}

fn render_composer_input_area(
    shell: &ShellRenderFrame<'_>,
    input_render_height: gpui::Pixels,
    text_top_padding: gpui::Pixels,
    conversation_input: &Entity<SingleLineInput>,
    backend_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    composer_input_chrome::composer_input_area(composer_input_scroll_region(
        shell,
        input_render_height,
        text_top_padding,
        conversation_input,
        backend_controls_disabled,
        cx,
    ))
}

fn composer_input_scroll_region(
    shell: &ShellRenderFrame<'_>,
    input_render_height: gpui::Pixels,
    text_top_padding: gpui::Pixels,
    conversation_input: &Entity<SingleLineInput>,
    backend_controls_disabled: Option<&str>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let enabled = backend_controls_disabled.is_none();
    conversation_input.update(cx, |input, cx| input.set_enabled(enabled, cx));
    let focus_input = conversation_input.clone();
    let mut region = composer_input_chrome::composer_input_scroll_region(
        input_render_height,
        text_top_padding,
        conversation_input,
    )
    .bg(if enabled {
        shell.input_background()
    } else {
        shell.secondary_button_theme().disabled.background
    })
    .border_color(if enabled {
        shell.input_border()
    } else {
        shell.secondary_button_theme().disabled.border
    })
    .text_color(if enabled {
        shell.input_foreground()
    } else {
        shell.secondary_button_theme().disabled.foreground
    });
    if enabled {
        region = region.cursor(CursorStyle::IBeam).on_mouse_down(
            MouseButton::Left,
            move |_, window, cx| {
                let focus_handle = focus_input.read(cx).tab_focus_handle();
                window.focus(&focus_handle);
            },
        );
    }
    region.when(!enabled, |this| {
        this.child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(shell.role_background(
                    BerylThemeRole::SurfaceRowDisabled,
                    shell.secondary_button_theme().disabled.background,
                )),
        )
    })
}

fn render_loaded_workspace_composer(
    shell: &ShellRenderFrame<'_>,
    conversation_input: &Entity<SingleLineInput>,
    window: &mut Window,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let composer_height = px(layout::DEFAULT_COMPOSER_HEIGHT);
    let initial_bounds =
        layout::composer_text_input_bounds(px(layout::WINDOW_MIN_WIDTH), composer_height);
    let initial_geometry = conversation_input
        .read(cx)
        .measure_geometry(initial_bounds, window);
    let initial_measurement =
        layout::composer_input_measurement_for_height(composer_height, &initial_geometry);
    let final_geometry = conversation_input
        .read(cx)
        .measure_geometry(initial_measurement.input_bounds, window);
    let composer_measurement =
        layout::composer_input_measurement_for_height(composer_height, &final_geometry);

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
            composer_measurement.input_render_height,
            composer_measurement.text_top_padding,
            conversation_input,
            Some("Workspace backend is not ready."),
            cx,
        ))
}
