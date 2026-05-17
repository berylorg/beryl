use beryl_backend::{HardStopTarget, ModelInfo};
use gpui::{
    AnyElement, Context, DispatchPhase, InteractiveElement, KeyDownEvent, KeyUpEvent, MouseButton,
    MouseDownEvent, MouseUpEvent, StatefulInteractiveElement, Window, anchored, canvas, div,
    prelude::*, px, relative,
};

use std::time::Instant;

use crate::{
    BerylThemeRole,
    shell::{
        ConversationSurfaceState, ScrollbarRegion, ShellRenderFrame, ShellView, layout,
        status_operation_state::{
            HardStopRequestSummary, StatusLineOperationKind, StatusModelListCache,
            reasoning_effort_for_model_selection,
        },
    },
};

use super::common::{disabled_secondary_button, secondary_button};
use super::scrollbars::{ScrollbarAxis, render_themed_div_scrollbar};

pub(super) fn render_status_operation_listeners(cx: &mut Context<ShellView>) -> impl IntoElement {
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
                        view.handle_status_operation_mouse_down(event, window, cx);
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
                        view.handle_status_operation_key_down(event, window, cx)
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
                        view.handle_status_operation_key_up(event, window, cx)
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

pub(super) fn render_status_operation_popup(
    shell: &ShellRenderFrame<'_>,
    surface: &ConversationSurfaceState,
    model_cache: &StatusModelListCache,
    cx: &mut Context<ShellView>,
) -> Option<AnyElement> {
    let menu = surface.status_line_operations().active()?;
    let entity = cx.entity();
    let content = match menu.kind() {
        StatusLineOperationKind::ModelReasoning => {
            render_model_reasoning_menu(shell, surface, model_cache, cx).into_any_element()
        }
        StatusLineOperationKind::Context => {
            render_context_menu(shell, surface, cx).into_any_element()
        }
        StatusLineOperationKind::TurnOperations => {
            render_turn_operations_menu(shell, surface, cx).into_any_element()
        }
    };
    let scroll_handle = surface.status_operation_scroll_handle();
    let scrollbar_visibility =
        shell.scrollbar_visibility_policy(&ScrollbarRegion::StatusOperation, cx);
    let mut panel = div()
        .id("status-operation-popup")
        .relative()
        .w(px(340.0))
        .max_h(px(420.0))
        .overflow_hidden()
        .occlude()
        .rounded_lg()
        .border_1()
        .border_color(shell.surface_border())
        .bg(shell.popup_surface_background())
        .shadow_lg()
        .on_mouse_move(cx.listener(ShellView::note_status_operation_scrollbar_motion))
        .on_scroll_wheel(cx.listener(ShellView::note_status_operation_scrollbar_scroll))
        .child(
            div()
                .id("status-operation-popup-scroll")
                .w_full()
                .max_h(px(420.0))
                .min_h(px(0.0))
                .track_scroll(&scroll_handle)
                .overflow_y_scroll()
                .p_2()
                .child(content),
        );
    if let Some(scrollbar) = render_themed_div_scrollbar(
        shell.style(),
        "status-operation-scrollbar",
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
                            view.record_status_operation_bounds(bounds, cx);
                        });
                    })
                    .child(panel),
            )
            .into_any_element(),
    )
}

fn render_model_reasoning_menu(
    shell: &ShellRenderFrame<'_>,
    surface: &ConversationSurfaceState,
    model_cache: &StatusModelListCache,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let (current_model, current_reasoning) = surface.current_status_model_reasoning();
    let mut menu = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(shell, "Model / Reasoning"));

    if model_cache.loading() {
        return menu.child(status_row(shell, "Loading models...".to_string()));
    }
    if let Some(error) = model_cache.last_error() {
        return menu
            .child(status_row(shell, error.to_string()))
            .child(action_row(
                shell,
                "status-model-retry-row",
                "Retry",
                cx.listener(ShellView::retry_status_model_list),
            ));
    }

    let Some(models) = model_cache.models() else {
        return menu.child(status_row(shell, "Loading models...".to_string()));
    };
    let visible_models = models
        .iter()
        .filter(|model| !model.hidden)
        .collect::<Vec<_>>();
    if visible_models.is_empty() {
        return menu.child(disabled_row(shell, "No models"));
    }

    menu = menu.child(section_header(shell, "Model"));
    for (index, model) in visible_models.iter().enumerate() {
        let selected = current_model
            .as_deref()
            .is_some_and(|current| model_matches(model, current));
        menu = menu.child(model_row(shell, index, model, selected, cx));
    }

    menu = menu.child(section_header(shell, "Reasoning"));
    let selected_model = current_model
        .as_deref()
        .and_then(|current| model_cache.find_model(current))
        .or_else(|| {
            visible_models
                .iter()
                .copied()
                .find(|model| model.is_default)
        })
        .or_else(|| visible_models.first().copied());
    match selected_model {
        Some(model) if model.supported_reasoning_efforts.is_empty() => {
            menu.child(disabled_row(shell, "No reasoning choices"))
        }
        Some(model) => {
            let selected_effort =
                reasoning_effort_for_model_selection(model, current_reasoning.as_deref());
            let mut list = menu;
            for (index, effort) in model.supported_reasoning_efforts.iter().enumerate() {
                list = list.child(reasoning_row(
                    shell,
                    index,
                    model.model.clone(),
                    effort.clone(),
                    selected_effort.as_deref() == Some(effort.as_str()),
                    cx,
                ));
            }
            list
        }
        None => menu.child(disabled_row(shell, "Select a model")),
    }
}

fn render_context_menu(
    shell: &ShellRenderFrame<'_>,
    surface: &ConversationSurfaceState,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let available = surface.status_line_projection().context_operation_available;
    let enabled = shell.status_line_context_interactive(available);
    let mut menu = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(shell, "Context"));

    if enabled {
        menu = menu.child(action_row(
            shell,
            "status-context-compact-row",
            "Compact",
            cx.listener(ShellView::compact_selected_thread_from_status_popup),
        ));
    } else {
        menu = menu.child(disabled_action_row(
            shell,
            "status-context-compact-row",
            "Compact",
        ));
    }
    menu
}

fn render_turn_operations_menu(
    shell: &ShellRenderFrame<'_>,
    surface: &ConversationSurfaceState,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let projection = surface.status_line_projection();
    let target = projection.cancellable_active_turn.as_ref();
    let hard_stop_targets = projection.hard_stop_targets.as_ref();
    let enabled = shell.status_line_turn_operations_interactive(target.is_some());
    let stop_in_flight = surface.status_line_operations().stop_request_in_flight();
    let hard_stop_enabled =
        enabled && hard_stop_targets.is_some_and(|targets| !targets.targets.is_empty());
    let hard_stop_hold_progress = hard_stop_targets.and_then(|targets| {
        surface
            .status_line_operations()
            .hard_stop_hold_progress_for_target(&targets.selected_turn, Instant::now())
    });
    let mut menu = div()
        .flex()
        .flex_col()
        .gap_1()
        .child(menu_header(shell, "Turn"));

    if let Some(error) = surface
        .status_line_operations()
        .turn_stop_request_error(target)
    {
        menu = menu.child(status_row(shell, error.to_string()));
    }

    if let Some(summary) = surface
        .status_line_operations()
        .hard_stop_request_summary()
        .filter(|summary| {
            target.is_some_and(|target| summary.selected_turn == *target)
                || hard_stop_targets
                    .is_some_and(|targets| summary.selected_turn == targets.selected_turn)
        })
    {
        menu = menu.child(hard_stop_summary_row(shell, summary));
        for failure in summary.failures.iter().take(3) {
            menu = menu.child(status_row(
                shell,
                format!(
                    "{} failed: {}",
                    hard_stop_target_label(&failure.target),
                    failure.message
                ),
            ));
        }
        if summary.failures.len() > 3 {
            menu = menu.child(status_row(
                shell,
                format!(
                    "{} more hard-stop targets failed.",
                    summary.failures.len() - 3
                ),
            ));
        }
    }

    if stop_in_flight {
        menu.child(disabled_action_row(
            shell,
            "status-turn-soft-stop-row",
            "Stopping...",
        ))
        .child(disabled_action_row(
            shell,
            "status-turn-hard-stop-row",
            "Hard stop",
        ))
    } else if enabled {
        menu.child(action_row(
            shell,
            "status-turn-soft-stop-row",
            "Soft stop",
            cx.listener(ShellView::stop_selected_turn_from_status_popup),
        ))
        .child(if hard_stop_enabled {
            hard_stop_hold_row(
                shell,
                hard_stop_hold_progress,
                cx.listener(ShellView::begin_hard_stop_hold_from_status_popup),
                cx.listener(ShellView::cancel_hard_stop_hold_from_status_popup),
                cx.listener(ShellView::cancel_hard_stop_hold_from_status_popup),
                cx.listener(ShellView::cancel_hard_stop_hold_on_hover_change),
                cx.listener(ShellView::begin_hard_stop_keyboard_hold_from_status_popup),
                cx.listener(ShellView::cancel_hard_stop_keyboard_hold_from_status_popup),
            )
            .into_any_element()
        } else {
            disabled_action_row(shell, "status-turn-hard-stop-row", "Hard stop").into_any_element()
        })
    } else {
        if target.is_some() {
            menu.child(disabled_action_row(
                shell,
                "status-turn-soft-stop-row",
                "Soft stop",
            ))
        } else {
            menu.child(disabled_row(shell, "No active turn"))
        }
    }
}

fn hard_stop_summary_row(
    shell: &ShellRenderFrame<'_>,
    summary: &HardStopRequestSummary,
) -> impl IntoElement {
    if let Some(error) = summary.request_error.as_ref() {
        return status_row(shell, format!("Hard stop failed: {error}"));
    }

    if summary.failures.is_empty() {
        return status_row(shell, "Hard stop requested.".to_string());
    }

    status_row(
        shell,
        format!(
            "Hard stop partially failed: {} of {} target{} failed.",
            summary.failures.len(),
            summary.target_count,
            if summary.target_count == 1 { "" } else { "s" }
        ),
    )
}

fn hard_stop_target_label(target: &HardStopTarget) -> &'static str {
    match target {
        HardStopTarget::Turn { .. } => "Turn interrupt",
        HardStopTarget::CommandExecution { .. } => "Command termination",
        HardStopTarget::BackgroundTerminals { .. } => "Background terminal cleanup",
    }
}

fn model_row(
    shell: &ShellRenderFrame<'_>,
    index: usize,
    model: &ModelInfo,
    selected: bool,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let model = model.clone();
    let secondary = shell.secondary_button_theme();
    div()
        .id(("status-model-row", index))
        .rounded_md()
        .px_2()
        .py_2()
        .when(selected, |row| row.bg(shell.row_surface_background()))
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
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_sm()
                                .text_color(shell.general_ui_foreground())
                                .whitespace_nowrap()
                                .truncate()
                                .child(model.display_name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(shell.surface_muted_foreground())
                                .whitespace_nowrap()
                                .truncate()
                                .child(model.model.clone()),
                        ),
                )
                .when(selected, |row| {
                    row.child(
                        div()
                            .text_xs()
                            .text_color(shell.role_foreground(
                                BerylThemeRole::StatusValueOk,
                                shell.status_line_value_foreground(),
                            ))
                            .child("Selected"),
                    )
                }),
        )
        .on_click(cx.listener(move |view, event, window, cx| {
            view.select_status_model(model.clone(), event, window, cx);
        }))
}

fn reasoning_row(
    shell: &ShellRenderFrame<'_>,
    index: usize,
    model: String,
    effort: String,
    selected: bool,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    let secondary = shell.secondary_button_theme();
    div()
        .id(("status-reasoning-row", index))
        .rounded_md()
        .px_2()
        .py_2()
        .when(selected, |row| row.bg(shell.row_surface_background()))
        .cursor_pointer()
        .hover(move |style| style.bg(secondary.hover.background))
        .text_sm()
        .text_color(shell.general_ui_foreground())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(effort.clone())
                .when(selected, |row| {
                    row.child(
                        div()
                            .text_xs()
                            .text_color(shell.role_foreground(
                                BerylThemeRole::StatusValueOk,
                                shell.status_line_value_foreground(),
                            ))
                            .child("Selected"),
                    )
                }),
        )
        .on_click(cx.listener(move |view, event, window, cx| {
            view.select_status_reasoning_effort(model.clone(), effort.clone(), event, window, cx);
        }))
}

fn action_row(
    shell: &ShellRenderFrame<'_>,
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    secondary_button(shell, id, label, on_click)
}

fn hard_stop_hold_row(
    shell: &ShellRenderFrame<'_>,
    progress: Option<f32>,
    on_mouse_down: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    on_mouse_up: impl Fn(&MouseUpEvent, &mut Window, &mut gpui::App) + 'static,
    on_mouse_up_out: impl Fn(&MouseUpEvent, &mut Window, &mut gpui::App) + 'static,
    on_hover: impl Fn(&bool, &mut Window, &mut gpui::App) + 'static,
    on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut gpui::App) + 'static,
    on_key_up: impl Fn(&KeyUpEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let secondary = shell.secondary_button_theme();
    let progress = progress.unwrap_or(0.0).clamp(0.0, 1.0);
    div()
        .id("status-turn-hard-stop-row")
        .relative()
        .overflow_hidden()
        .h(px(layout::BUTTON_OUTER_HEIGHT))
        .px(px(layout::BUTTON_HORIZONTAL_PADDING))
        .py(px(layout::BUTTON_VERTICAL_PADDING))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(secondary.normal.background)
        .border_1()
        .border_color(secondary.normal.border)
        .flex()
        .items_center()
        .cursor_pointer()
        .tab_index(0)
        .hover(move |style| style.bg(secondary.hover.background))
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(relative(progress))
                .bg(shell.role_background(
                    BerylThemeRole::StatusValueError,
                    secondary.active.background,
                )),
        )
        .child(
            div()
                .relative()
                .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
                .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
                .font_weight(secondary.font_weight)
                .text_color(secondary.normal.foreground)
                .child("Hard stop"),
        )
        .on_mouse_down(MouseButton::Left, on_mouse_down)
        .on_mouse_up(MouseButton::Left, on_mouse_up)
        .on_mouse_up_out(MouseButton::Left, on_mouse_up_out)
        .on_hover(on_hover)
        .on_key_down(on_key_down)
        .on_key_up(on_key_up)
}

fn disabled_action_row(
    shell: &ShellRenderFrame<'_>,
    id: &'static str,
    label: &'static str,
) -> impl IntoElement {
    disabled_secondary_button(shell, id, label)
}

fn disabled_row(shell: &ShellRenderFrame<'_>, label: &str) -> impl IntoElement {
    div()
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .px_2()
        .py_2()
        .text_sm()
        .text_color(shell.surface_muted_foreground())
        .child(label.to_string())
}

fn status_row(shell: &ShellRenderFrame<'_>, message: String) -> impl IntoElement {
    div()
        .rounded_md()
        .px_2()
        .py_2()
        .text_xs()
        .text_color(shell.surface_muted_foreground())
        .child(message)
}

fn menu_header(shell: &ShellRenderFrame<'_>, label: &str) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .text_xs()
        .font_weight(
            shell.role_font_weight(BerylThemeRole::PopupSurface, gpui::FontWeight::SEMIBOLD),
        )
        .text_color(shell.general_ui_foreground())
        .child(label.to_string())
}

fn section_header(shell: &ShellRenderFrame<'_>, label: &str) -> impl IntoElement {
    div()
        .px_2()
        .pt_2()
        .pb_1()
        .text_xs()
        .text_color(shell.status_line_title_foreground())
        .child(label.to_string())
}

fn model_matches(model: &ModelInfo, value: &str) -> bool {
    model.model == value || model.id == value || model.display_name == value
}
