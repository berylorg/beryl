use super::*;
use gpui::{IntoElement, MouseButton, Render, Styled, div, prelude::*, rgb};

impl Render for StartupSurface {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .id("startup-surface")
            .debug_selector(|| "startup-surface".into())
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(16.))
            .p(px(24.))
            .bg(rgb(0xf8fafc))
            .text_color(rgb(0x1f2937))
            .text_size(px(14.))
            .on_key_down(cx.listener(Self::key))
            .child(div().text_size(px(20.)).child(if self.detail.is_some() {
                "Beryl couldn't open its data"
            } else {
                "Beryl is already open"
            }));
        if let Some(detail) = &self.detail {
            root = root.child(
                div()
                    .id("startup-detail")
                    .debug_selector(|| "startup-detail".into())
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(0xcbd5e1))
                    .rounded_md()
                    .p(px(12.))
                    .child(detail.clone()),
            );
        } else {
            root = root
                .child("Another Beryl process owns this home.")
                .child(div().child(format!(
                    "Exiting automatically in {} seconds…",
                    self.remaining_seconds
                )));
        }
        let mut commands = div().flex().justify_center().gap(px(12.));
        if self.detail.is_some() {
            let disabled = self.pending.is_some() || self.exited;
            let focus = self.retry_focus.clone();
            let mut retry = button(
                "startup-retry",
                if self.pending.is_some() {
                    "Retrying…"
                } else {
                    "Retry"
                },
                &focus,
                disabled,
                true,
            )
            .on_click(cx.listener(|surface, _, _, cx| {
                surface.request_retry(cx);
            }));
            if !disabled {
                retry =
                    retry.on_mouse_down(MouseButton::Left, move |_, window, _| focus.focus(window));
            } else {
                let explanation = if self.exited {
                    "Beryl is exiting."
                } else {
                    "A retry is already in progress."
                };
                retry = retry.tooltip(move |_, cx| cx.new(|_| Explanation(explanation)).into());
            }
            commands = commands.child(retry);
        }
        let focus = self.exit_focus.clone();
        root.child(
            commands.child(
                button("startup-exit", "Exit", &focus, false, false)
                    .on_mouse_down(MouseButton::Left, move |_, window, _| focus.focus(window))
                    .on_click(cx.listener(|surface, _, _, cx| surface.request_exit(cx))),
            ),
        )
    }
}

fn button(
    id: &'static str,
    label: &'static str,
    focus: &FocusHandle,
    disabled: bool,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .track_focus(focus)
        .h(px(32.))
        .px(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .rounded(px(6.))
        .text_size(px(13.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .bg(rgb(if disabled {
            0xf1f5f9
        } else if primary {
            0xdbeafe
        } else {
            0xf8fafc
        }))
        .text_color(rgb(if disabled { 0x94a3b8 } else { 0x1f2937 }))
        .border_color(rgb(if primary && !disabled {
            0x2563eb
        } else {
            0xcbd5e1
        }))
        .when(!disabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(rgb(0xeef2f7)).border_color(rgb(0x94a3b8)))
                .active(|style| style.bg(rgb(0xe2e8f0)).border_color(rgb(0x64748b)))
                .focus(|style| {
                    style.shadow(vec![
                        gpui::BoxShadow {
                            color: rgb(0x2563eb).into(),
                            offset: gpui::point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            spread_radius: px(4.),
                        },
                        gpui::BoxShadow {
                            color: rgb(0xf8fafc).into(),
                            offset: gpui::point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            spread_radius: px(2.),
                        },
                    ])
                })
        })
        .child(label)
}

struct Explanation(&'static str);
impl Render for Explanation {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("startup-command-explanation")
            .debug_selector(|| "startup-command-explanation".into())
            .max_w(px(320.))
            .max_h(px(160.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(5.))
            .border_1()
            .border_color(rgb(0x0f172a))
            .shadow(vec![gpui::BoxShadow {
                color: gpui::rgba(0x0f172a38).into(),
                offset: gpui::point(px(0.), px(10.)),
                blur_radius: px(24.),
                spread_radius: px(0.),
            }])
            .bg(rgb(0x111827))
            .text_color(rgb(0xffffff))
            .text_size(px(12.))
            .line_height(px(16.))
            .child(self.0)
    }
}
