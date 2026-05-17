use gpui::{Context, Entity, SharedString, Window, div, prelude::*, px};

use gpui::ScrollHandle;

use crate::BerylThemeRole;
use crate::shell::{
    BlockedState, DiscoveringState, OpeningState, PickerState, RetryTarget, ScrollbarRegion,
    ShellRenderFrame, ShellState, ShellView, WorkspaceChoice, layout,
};
use crate::text_input::SingleLineInput;

use super::common::{
    button, card, framed_text_input, info_line, inline_notice, primary_actions, section_label,
    startup_shell_frame,
};

pub(super) fn render_startup_shell(
    shell: &ShellRenderFrame<'_>,
    state: &ShellState,
    scroll_handle: &ScrollHandle,
    host_path_input: &Entity<SingleLineInput>,
    wsl_path_input: &Entity<SingleLineInput>,
    cx: &mut Context<ShellView>,
) -> gpui::AnyElement {
    let startup_scrollbar_visibility =
        shell.scrollbar_visibility_policy(&ScrollbarRegion::Startup, cx);
    match state {
        crate::shell::ShellState::Discovering(discovering) => startup_shell_frame(
            shell,
            scroll_handle,
            startup_scrollbar_visibility,
            cx.listener(ShellView::note_startup_scrollbar_motion),
            cx.listener(ShellView::note_startup_scrollbar_scroll),
            "Beryl",
            "Beryl is loading semantic workspace startup state before the main workspace shell appears.",
            render_discovering(shell, discovering),
            primary_actions(shell, cx),
        )
        .into_any_element(),
        crate::shell::ShellState::Picker(picker) => {
            let body =
                render_picker(shell, picker, host_path_input, wsl_path_input, cx).into_any_element();
            let actions = primary_actions(shell, cx).into_any_element();
            startup_shell_frame(
                shell,
                scroll_handle,
                startup_scrollbar_visibility,
                cx.listener(ShellView::note_startup_scrollbar_motion),
                cx.listener(ShellView::note_startup_scrollbar_scroll),
                "Beryl",
                "Select one workspace to bind to this window. Beryl keeps startup and recovery separate from the ready conversation surface.",
                body,
                actions,
            )
            .into_any_element()
        }
        crate::shell::ShellState::Opening(opening) => startup_shell_frame(
            shell,
            scroll_handle,
            startup_scrollbar_visibility,
            cx.listener(ShellView::note_startup_scrollbar_motion),
            cx.listener(ShellView::note_startup_scrollbar_scroll),
            "Beryl",
            "Beryl is opening the selected workspace and preparing the managed backend before the conversation surface can appear.",
            render_opening(shell, opening),
            primary_actions(shell, cx),
        )
        .into_any_element(),
        crate::shell::ShellState::Blocked(blocked) => startup_shell_frame(
            shell,
            scroll_handle,
            startup_scrollbar_visibility,
            cx.listener(ShellView::note_startup_scrollbar_motion),
            cx.listener(ShellView::note_startup_scrollbar_scroll),
            "Beryl",
            "Startup and recovery failures still block workspace activation until the managed backend can be used safely.",
            render_blocked(shell, blocked),
            div()
                .flex()
                .gap_3()
                .child(button(
                    shell,
                    "retry-backend",
                    retry_label(&blocked.target),
                    cx.listener(ShellView::retry_workspace),
                ))
                .child(button(
                    shell,
                    "close-beryl",
                    "Close Beryl",
                    cx.listener(ShellView::quit),
                )),
        )
        .into_any_element(),
        crate::shell::ShellState::WorkspaceIdle(_)
        | crate::shell::ShellState::WorkspaceLoaded(_)
        | crate::shell::ShellState::BackendUnavailable(_)
        | crate::shell::ShellState::Ready(_) => {
            div().into_any_element()
        }
    }
}

pub(super) fn workspace_choice_origin(choice: &WorkspaceChoice) -> String {
    match (
        choice.thread_count > 0,
        choice.remembered_rank.is_some(),
        choice.last_opened,
    ) {
        (true, true, true) => {
            "discovered thread history, remembered startup metadata, last opened".to_string()
        }
        (true, true, false) => {
            "discovered thread history and remembered startup metadata".to_string()
        }
        (true, false, true) => "discovered thread history and last opened".to_string(),
        (false, true, true) => "remembered startup metadata and last opened".to_string(),
        (true, false, false) => "discovered thread history".to_string(),
        (false, true, false) => "remembered startup metadata".to_string(),
        (false, false, true) => "last opened workspace".to_string(),
        (false, false, false) => "workspace selection".to_string(),
    }
}

fn render_discovering(
    shell: &ShellRenderFrame<'_>,
    discovering: &DiscoveringState,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(section_label(shell, "Startup Discovery"))
        .child(
            div()
                .text_lg()
                .font_weight(shell.role_font_weight(
                    BerylThemeRole::NoticeInfo,
                    gpui::FontWeight::SEMIBOLD,
                ))
                .text_color(shell.role_foreground(
                    BerylThemeRole::NoticeInfo,
                    shell.surface_foreground(),
                ))
                .child("Resolving the startup workspace"),
        )
        .child(
            div()
                .text_sm()
                .text_color(shell.surface_foreground())
                .child("Beryl is loading shared startup state from the configured Beryl home directory and selecting or creating the semantic workspace that should open in this window."),
        )
        .child(info_line(shell, "Current step", &discovering.detail))
}

fn render_picker(
    shell: &ShellRenderFrame<'_>,
    picker: &PickerState,
    host_path_input: &Entity<SingleLineInput>,
    wsl_path_input: &Entity<SingleLineInput>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let mut body = div()
        .flex()
        .flex_col()
        .gap_4()
        .child(section_label(shell, "Workspace Picker"))
        .child(
            div()
                .text_sm()
                .text_color(shell.surface_foreground())
                .child("Select one discovered workspace or open a new workspace explicitly. Once you open one, this Beryl window stays bound to that workspace until you close it."),
        );

    if let Some(notice) = &picker.notice {
        body = body.child(inline_notice(shell, notice, BerylThemeRole::NoticeError));
    }
    if let Some(warning) = &picker.model.metadata_warning {
        body = body.child(inline_notice(shell, warning, BerylThemeRole::NoticeInfo));
    }

    if !picker.model.choices.is_empty() {
        body = body.child(section_label(shell, "Known Workspaces"));
        for (index, choice) in picker.model.choices.iter().enumerate() {
            let mut card_body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(info_line(
                    shell,
                    "Workspace",
                    &choice.workspace.display_label(),
                ))
                .child(info_line(
                    shell,
                    "Known by",
                    &workspace_choice_origin(choice),
                ));

            if choice.thread_count > 0 {
                card_body = card_body.child(info_line(
                    shell,
                    "Discovered threads",
                    &choice.thread_count.to_string(),
                ));
            }
            if let Some(preview) = &choice.latest_preview {
                card_body = card_body.child(info_line(shell, "Latest preview", preview));
            }

            let workspace = choice.workspace.clone();
            body = body.child(card(
                shell,
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(card_body)
                    .child(button(
                        shell,
                        ("open-workspace", index),
                        "Open Workspace",
                        cx.listener(move |view, event, window, cx| {
                            let _ = event;
                            view.open_workspace_choice(workspace.clone(), window, cx);
                        }),
                    )),
            ));
        }
    }

    body = body.child(section_label(shell, "Open New Host Workspace"));
    if picker.model.host_available {
        body = body.child(card(
            shell,
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(info_line(shell, "Runtime mode", "host-windows"))
                .child(framed_text_input(shell, host_path_input))
                .child(button(
                    shell,
                    "open-host-workspace",
                    "Open Host Workspace",
                    cx.listener(ShellView::open_host_path),
                )),
        ));
    } else if let Some(issue) = &picker.model.host_issue {
        body = body.child(inline_notice(shell, issue, BerylThemeRole::NoticeError));
    }

    body = body.child(section_label(shell, "Open New WSL Workspace"));
    if !picker.model.available_wsl_distros.is_empty() {
        let mut distro_row = div().flex().flex_wrap().gap_2();
        for distro_name in &picker.model.available_wsl_distros {
            let selected =
                picker.model.selected_wsl_distro.as_deref() == Some(distro_name.as_str());
            let distro = distro_name.clone();
            let chip = distro_chip(
                shell,
                SharedString::from(distro_name.clone()),
                selected,
                cx.listener(move |view, event, window, cx| {
                    view.select_wsl_distro(&distro, event, window, cx);
                }),
            );
            distro_row = distro_row.child(chip);
        }

        body = body.child(card(
            shell,
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(info_line(shell, "Runtime mode", "wsl-linux"))
                .child(distro_row)
                .child(framed_text_input(shell, wsl_path_input))
                .child(button(
                    shell,
                    "open-wsl-workspace",
                    "Open WSL Workspace",
                    cx.listener(ShellView::open_wsl_path),
                )),
        ));
    } else {
        body = body.child(
            div()
                .text_sm()
                .text_color(shell.surface_muted_foreground())
                .child("No WSL distro is currently available for managed backend launch."),
        );
    }

    if let Some(error) = &picker.model.wsl_listing_error {
        body = body.child(inline_notice(shell, error, BerylThemeRole::NoticeError));
    }
    for (distro_name, reason) in &picker.model.unavailable_wsl {
        body = body.child(info_line(
            shell,
            &format!("WSL distro unavailable: {distro_name}"),
            reason,
        ));
    }

    body
}

fn render_opening(shell: &ShellRenderFrame<'_>, opening: &OpeningState) -> impl IntoElement {
    let mut body = div()
        .flex()
        .flex_col()
        .gap_3()
        .child(section_label(shell, "Workspace Startup"))
        .child(
            div()
                .text_lg()
                .font_weight(
                    shell.role_font_weight(BerylThemeRole::NoticeInfo, gpui::FontWeight::SEMIBOLD),
                )
                .text_color(
                    shell.role_foreground(BerylThemeRole::NoticeInfo, shell.surface_foreground()),
                )
                .child("Opening the selected workspace"),
        )
        .child(info_line(shell, "Attempt", &opening.attempt.to_string()))
        .child(info_line(shell, "Target", &opening.workspace_label))
        .child(info_line(shell, "Current step", &opening.detail));

    if let Some(progress) = &opening.progress
        && let Some(detail) = progress.detail()
    {
        body = body.child(info_line(shell, "Backend probe detail", detail));
    }

    if let Some(previous_failure) = &opening.previous_failure {
        let retry_message = match previous_failure.stage {
            Some(stage) => format!(
                "Retrying after {} during {}: {}",
                previous_failure.title,
                stage.display_label(),
                previous_failure.summary
            ),
            None => format!(
                "Retrying after {}: {}",
                previous_failure.title, previous_failure.summary
            ),
        };
        body = body.child(inline_notice(
            shell,
            &retry_message,
            BerylThemeRole::NoticeInfo,
        ));
    }

    body
}

fn render_blocked(shell: &ShellRenderFrame<'_>, blocked: &BlockedState) -> impl IntoElement {
    let mut body = div()
        .flex()
        .flex_col()
        .gap_3()
        .child(section_label(
            shell,
            if blocked.disconnect {
                "Disconnect Recovery"
            } else {
                "Blocking Error"
            },
        ))
        .child(
            div()
                .text_lg()
                .font_weight(
                    shell.role_font_weight(BerylThemeRole::NoticeError, gpui::FontWeight::SEMIBOLD),
                )
                .text_color(
                    shell.role_foreground(BerylThemeRole::NoticeError, shell.surface_foreground()),
                )
                .child(blocked.title),
        )
        .child(info_line(shell, "Workspace", &blocked.workspace_label))
        .child(info_line(shell, "Attempt", &blocked.attempt.to_string()))
        .child(
            div()
                .text_sm()
                .text_color(shell.surface_foreground())
                .child(blocked.summary.clone()),
        )
        .child(
            div()
                .text_sm()
                .text_color(
                    shell
                        .role_foreground(BerylThemeRole::NoticeWarning, shell.surface_foreground()),
                )
                .child(blocked.detail.clone()),
        );

    if let Some(stage) = blocked.stage {
        body = body.child(info_line(shell, "Failed step", stage.display_label()));
    }
    for next_step in &blocked.next_steps {
        body = body.child(
            div()
                .text_sm()
                .text_color(shell.surface_muted_foreground())
                .child(format!("Next: {next_step}")),
        );
    }

    body
}

fn retry_label(target: &RetryTarget) -> &'static str {
    match target {
        RetryTarget::Startup => "Retry Startup",
        RetryTarget::WorkspacePrimary
        | RetryTarget::Workspace(_)
        | RetryTarget::HostPath(_)
        | RetryTarget::WslPath { .. } => "Retry Backend",
    }
}

fn distro_chip(
    shell: &ShellRenderFrame<'_>,
    label: impl Into<SharedString>,
    selected: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let label = label.into();
    let primary = shell.primary_button_theme();
    let secondary = shell.secondary_button_theme();
    let background = if selected {
        primary.active.background
    } else {
        secondary.normal.background
    };
    let hover = if selected {
        primary.hover.background
    } else {
        secondary.hover.background
    };
    let active = if selected {
        primary.active.background
    } else {
        secondary.active.background
    };
    let foreground = if selected {
        primary.active.foreground
    } else {
        secondary.normal.foreground
    };

    div()
        .id(label.clone())
        .flex_none()
        .h(px(layout::BUTTON_OUTER_HEIGHT))
        .px(px(layout::BUTTON_HORIZONTAL_PADDING))
        .py(px(layout::BUTTON_VERTICAL_PADDING))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .flex()
        .items_center()
        .justify_center()
        .bg(background)
        .hover(move |style| style.bg(hover))
        .active(move |style| style.bg(active))
        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
        .font_weight(secondary.font_weight)
        .text_color(foreground)
        .cursor_pointer()
        .child(label)
        .on_click(move |event, window, cx| on_click(event, window, cx))
}
