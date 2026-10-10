use super::*;

pub(in crate::main_window::shell::host) fn render_strip(
    root: &MainWindowShellRoot,
    cx: &mut Context<MainWindowShellRoot>,
) -> AnyElement {
    let appearance = &root.controller.as_ref().unwrap().appearance;
    let status = &root.status_controls;
    let colors = super::super::model_controls::menu_colors(root);
    let remaining = status
        .snapshot
        .context
        .and_then(beryl_backend::ContextTokenUsage::remaining_percent);
    let text = format!("{}  View -/-", status.snapshot.state.label());
    let available = status.menu_available();
    let disabled = status.snapshot.state.active() && !available;
    let open = status.menu_anchor.is_some();
    let tooltip = if disabled {
        status.reason().to_owned()
    } else {
        text.clone()
    };
    let turn = div()
        .id("main-window-status-turn")
        .debug_selector(|| "main-window-status-turn".to_owned())
        .flex_1()
        .min_w_0()
        .px(px(10.))
        .h_full()
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .when(disabled, |segment| segment.text_color(gpui::rgb(0x94a3b8)))
        .when(open, |segment| {
            segment
                .bg(gpui::rgb(0xe2e8f0))
                .text_color(gpui::rgb(0x1d4ed8))
        })
        .when(available, |segment| {
            segment
                .cursor_pointer()
                .hover(|style| style.bg(gpui::rgb(0xeef2f7)))
                .active(|style| style.bg(gpui::rgb(0xe2e8f0)))
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.prevent_default()
                })
                .on_click(cx.listener(|root, _, window, cx| root.toggle_stop_menu(window, cx)))
        })
        .tooltip(move |_, cx| cx.new(|_| StatusTooltip(tooltip.clone())).into())
        .child(text);
    let divider = || {
        div()
            .w(px(1.))
            .h_full()
            .flex_none()
            .bg(appearance.separator)
    };
    let strip = div();
    #[cfg(feature = "test-faults")]
    let strip = {
        let rendered = status.context_rendered.clone();
        strip.on_children_prepainted(move |bounds, _, _| {
            rendered.set(bounds.get(2).copied().map(|bounds| (remaining, bounds)));
        })
    };
    let strip = strip
        .id("main-window-status-line")
        .debug_selector(|| "main-window-status-line".to_owned())
        .h(px(28.))
        .flex_none()
        .flex()
        .items_center()
        .overflow_hidden()
        .border_t_1()
        .border_color(appearance.separator)
        .bg(appearance.status)
        .text_size(px(12.))
        .text_color(gpui::rgb(0x475569))
        .child(super::super::model_controls::render_segment(root, cx))
        .child(divider())
        .child(
            div()
                .id("main-window-status-context")
                .debug_selector(move || {
                    remaining.map_or_else(
                        || "main-window-status-context:unknown".to_owned(),
                        |value| format!("main-window-status-context:{value}"),
                    )
                })
                .w(px(180.))
                .flex_none()
                .h_full()
                .px(px(10.))
                .flex()
                .items_center()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .when(status.compaction.anchor.is_some(), |segment| {
                    segment
                        .bg(colors.pressed)
                        .text_color(colors.pressed_foreground)
                })
                .when(root.compaction_menu_available(), |segment| {
                    segment
                        .cursor_pointer()
                        .hover(|style| style.bg(colors.hover).text_color(colors.hover_foreground))
                        .active(|style| {
                            style
                                .bg(colors.pressed)
                                .text_color(colors.pressed_foreground)
                        })
                        .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                            window.prevent_default()
                        })
                        .on_click(cx.listener(|root, _, window, cx| {
                            root.toggle_compaction_menu(window, cx)
                        }))
                })
                .when(
                    status.selection.is_some() && !root.compaction_menu_available(),
                    |segment| segment.text_color(colors.disabled),
                )
                .tooltip({
                    let reason = root.compaction_reason().to_owned();
                    move |_, cx| cx.new(|_| StatusTooltip(reason.clone())).into()
                })
                .child(remaining.map_or_else(
                    || "Context Unknown".to_owned(),
                    |value| format!("Context {value}%"),
                )),
        )
        .child(divider())
        .child(turn);
    strip.into_any_element()
}

pub(in crate::main_window::shell::host) fn render_menu(
    root: &MainWindowShellRoot,
    window: &Window,
    cx: &mut Context<MainWindowShellRoot>,
) -> Option<AnyElement> {
    if root.status_controls.compaction.anchor.is_some() {
        return super::manual_compaction::render_menu(root, window, cx);
    }
    let status = &root.status_controls;
    status.menu_anchor.as_ref()?;
    let enabled = status.command_enabled() && root.status_mutation_gate().is_none();
    let reason = root
        .status_mutation_gate()
        .unwrap_or_else(|| status.reason())
        .to_owned();
    let feedback = status
        .feedback()
        .map(|feedback| feedback_text(feedback.snapshot().state))
        .or(status.request_failure)
        .or(
            (status.request_pending == status.snapshot.origin && status.request_pending.is_some())
                .then_some("Requesting exact soft stop…"),
        );
    let viewport = window.viewport_size();
    let width = px(280.).min((viewport.width - px(16.)).max(px(0.)));
    let left = px(352.).min((viewport.width - width - px(8.)).max(px(0.)));
    let row = div()
        .id("main-window-soft-stop")
        .debug_selector(|| "main-window-soft-stop".to_owned())
        .w_full()
        .h(px(32.))
        .px(px(10.))
        .flex()
        .items_center()
        .rounded(px(3.))
        .text_color(if enabled {
            gpui::rgb(0x0f172a)
        } else {
            gpui::rgb(0x94a3b8)
        })
        .when(enabled, |row| {
            row.cursor_pointer()
                .hover(|style| style.bg(gpui::rgb(0xeef2f7)))
                .active(|style| style.bg(gpui::rgb(0xe2e8f0)))
                .on_click(cx.listener(|root, _, window, cx| root.activate_soft_stop(window, cx)))
        })
        .when(status.menu_focus.is_focused(window) && enabled, |row| {
            row.bg(gpui::rgb(0xdbeafe))
        })
        .when(!enabled, |row| {
            row.tooltip(move |_, cx| -> AnyView {
                cx.new(|_| StatusTooltip(reason.clone())).into()
            })
        })
        .child("Soft stop");
    Some(
        div()
            .id("main-window-turn-menu")
            .debug_selector(|| "main-window-turn-menu".to_owned())
            .absolute()
            .left(left)
            .bottom(px(32.))
            .w(width)
            .max_h((viewport.height - px(40.)).max(px(0.)))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .py(px(4.))
            .border_1()
            .border_color(gpui::rgb(0xcbd5e1))
            .rounded(px(6.))
            .bg(gpui::rgb(0xffffff))
            .text_size(px(13.))
            .text_color(gpui::rgb(0x0f172a))
            .track_focus(&status.menu_focus)
            .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => root.close_stop_menu(window, cx),
                    "enter" | "space" => root.activate_soft_stop(window, cx),
                    "up" | "down" | "home" | "end" => {
                        window.focus(&root.status_controls.menu_focus)
                    }
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .on_mouse_down_out(
                cx.listener(|root, event: &gpui::MouseDownEvent, window, cx| {
                    let size = window.viewport_size();
                    if event.position.x >= px(352.) && event.position.y >= size.height - px(28.) {
                        return;
                    }
                    root.close_stop_menu(window, cx);
                }),
            )
            .child(row)
            .children(feedback.map(|feedback| {
                div()
                    .id("main-window-stop-feedback")
                    .debug_selector(|| "main-window-stop-feedback".to_owned())
                    .px(px(10.))
                    .py(px(5.))
                    .text_size(px(12.))
                    .text_color(gpui::rgb(0x64748b))
                    .child(feedback)
            }))
            .into_any_element(),
    )
}

fn feedback_text(state: ExactStopFeedbackState) -> &'static str {
    match state {
        ExactStopFeedbackState::Waiting => {
            "Soft stop requested; awaiting the exact operation outcome."
        }
        ExactStopFeedbackState::DurableNondispatch => {
            "The durably admitted stop was not dispatched."
        }
        ExactStopFeedbackState::VolatileNondispatch => {
            "The volatile stop request was not dispatched. It cannot be retried."
        }
        ExactStopFeedbackState::RequestNotAdmitted => "The soft-stop request was not admitted.",
        ExactStopFeedbackState::Interrupted => "The exact operation was interrupted.",
        ExactStopFeedbackState::Completed => "The exact operation completed.",
        ExactStopFeedbackState::Failed => "The exact operation failed.",
        ExactStopFeedbackState::UnknownTerminal => {
            "The exact operation's terminal outcome is unknown."
        }
        ExactStopFeedbackState::AuthorityLost => "Exact operation authority was lost.",
    }
}

pub(super) struct StatusTooltip(pub(super) String);

impl Render for StatusTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(320.))
            .max_h(px(160.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(5.))
            .bg(gpui::rgb(0x111827))
            .text_color(gpui::rgb(0xffffff))
            .text_size(px(12.))
            .child(self.0.clone())
    }
}
