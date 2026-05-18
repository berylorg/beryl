use gpui::{
    AnyElement, AnyView, App, Context, DispatchPhase, InteractiveElement, KeyDownEvent,
    MouseDownEvent, Render, StatefulInteractiveElement, Window, anchored, canvas, div, prelude::*,
    px,
};

use crate::{
    BerylThemeRole,
    shell::{
        ConversationSurfaceState, ShellRenderFrame, ShellView,
        transcript_branch_menu_state::TranscriptBranchAction,
        transcript_edit_menu_state::{TranscriptEditDisabledReason, TranscriptEditMenuEntry},
    },
};

use super::common::{disabled_secondary_button, secondary_button};

#[derive(Clone)]
struct TranscriptTurnMenuTooltip {
    message: &'static str,
    background: gpui::Rgba,
    border: gpui::Rgba,
    foreground: gpui::Rgba,
}

pub(super) fn render_transcript_branch_menu_listeners(
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
                        view.handle_transcript_branch_menu_mouse_down(event, window, cx);
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
                        view.handle_transcript_branch_menu_key_down(event, window, cx)
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

pub(super) fn render_transcript_branch_menu(
    shell: &ShellRenderFrame<'_>,
    surface: &ConversationSurfaceState,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    let menu = surface.transcript_branch_menu().active()?;
    let entity = cx.entity();
    let branch_target_available = menu.branch_target().is_some();
    let edit_entry = menu.edit_entry().cloned();
    let image_target_available = menu.image_target().is_some();

    Some(
        anchored()
            .position(menu.position())
            .snap_to_window_with_margin(px(8.0))
            .child(
                div()
                    .on_children_prepainted(move |children, _, cx| {
                        let bounds = children.first().copied();
                        entity.update(cx, |view, cx| {
                            view.record_transcript_branch_menu_bounds(bounds, cx)
                        });
                    })
                    .child(
                        div()
                            .id("transcript-branch-menu-panel")
                            .w(px(236.0))
                            .occlude()
                            .rounded_lg()
                            .border_1()
                            .border_color(shell.role_border(
                                BerylThemeRole::TranscriptContextMenu,
                                shell.surface_border(),
                            ))
                            .bg(shell.role_background(
                                BerylThemeRole::TranscriptContextMenu,
                                shell.popup_surface_background(),
                            ))
                            .shadow_lg()
                            .p_2()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(menu_header(shell, "Turn"))
                            .when_some(edit_entry, |this, entry| {
                                this.child(edit_row(shell, entry, cx))
                            })
                            .when(branch_target_available, |this| {
                                this.child(branch_row(
                                    shell,
                                    "transcript-branch-switch-to-row",
                                    "Branch and switch to",
                                    TranscriptBranchAction::SwitchTo,
                                    cx,
                                ))
                                .child(branch_row(
                                    shell,
                                    "transcript-branch-background-row",
                                    "Branch in background",
                                    TranscriptBranchAction::Background,
                                    cx,
                                ))
                            })
                            .when(image_target_available, |this| {
                                this.child(copy_image_row(shell, cx))
                                    .child(save_image_as_row(shell, cx))
                            }),
                    ),
            )
            .into_any_element(),
    )
}

fn copy_image_row(shell: &ShellRenderFrame<'_>, cx: &mut Context<ShellView>) -> impl IntoElement {
    secondary_button(
        shell,
        "transcript-copy-image-row",
        "Copy image",
        cx.listener(ShellView::copy_transcript_image_from_menu),
    )
}

fn save_image_as_row(
    shell: &ShellRenderFrame<'_>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    secondary_button(
        shell,
        "transcript-save-image-as-row",
        "Save image as",
        cx.listener(ShellView::save_transcript_image_as_from_menu),
    )
}

fn edit_row(
    shell: &ShellRenderFrame<'_>,
    entry: TranscriptEditMenuEntry,
    cx: &mut Context<ShellView>,
) -> AnyElement {
    if let Some(reason) = entry.disabled_reason() {
        return disabled_edit_row(shell, reason).into_any_element();
    }

    secondary_button(
        shell,
        "transcript-edit-message-row",
        "Edit message",
        cx.listener(ShellView::edit_transcript_turn_from_menu),
    )
    .into_any_element()
}

fn disabled_edit_row(
    shell: &ShellRenderFrame<'_>,
    reason: TranscriptEditDisabledReason,
) -> impl IntoElement {
    let tooltip = TranscriptTurnMenuTooltip {
        message: reason.tooltip(),
        background: shell.popup_surface_background(),
        border: shell.surface_border(),
        foreground: shell.general_ui_foreground(),
    };
    disabled_secondary_button(shell, "transcript-edit-message-row", "Edit message")
        .tooltip(move |_, cx| build_transcript_turn_menu_tooltip(tooltip.clone(), cx))
}

fn branch_row(
    shell: &ShellRenderFrame<'_>,
    id: &'static str,
    label: &'static str,
    action: TranscriptBranchAction,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    secondary_button(
        shell,
        id,
        label,
        cx.listener(move |view, event, window, cx| match action {
            TranscriptBranchAction::SwitchTo => {
                view.branch_transcript_turn_and_switch_to(event, window, cx);
            }
            TranscriptBranchAction::Background => {
                view.branch_transcript_turn_in_background(event, window, cx);
            }
        }),
    )
}

fn menu_header(shell: &ShellRenderFrame<'_>, label: &str) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .text_xs()
        .font_family(
            shell.role_font_family(BerylThemeRole::TranscriptContextMenuHeaderText, "Inter"),
        )
        .font_weight(shell.role_font_weight(
            BerylThemeRole::TranscriptContextMenuHeaderText,
            gpui::FontWeight::SEMIBOLD,
        ))
        .text_color(shell.role_foreground(
            BerylThemeRole::TranscriptContextMenuHeaderText,
            shell.general_ui_foreground(),
        ))
        .child(label.to_string())
}

fn build_transcript_turn_menu_tooltip(tooltip: TranscriptTurnMenuTooltip, cx: &mut App) -> AnyView {
    cx.new(|_| tooltip).into()
}

impl Render for TranscriptTurnMenuTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(260.0))
            .rounded_md()
            .bg(self.background)
            .border_1()
            .border_color(self.border)
            .px_3()
            .py_2()
            .text_xs()
            .text_color(self.foreground)
            .child(self.message)
    }
}
