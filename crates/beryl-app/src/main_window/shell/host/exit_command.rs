use super::*;
use crate::main_window::creation::command::{theme_color, theme_font};
use beryl_state::ThemePropertyId as Property;
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, AnyView, InteractiveElement, StatefulInteractiveElement};

impl MainWindowShellRoot {
    pub(crate) fn set_exit_disabled_reason(
        &mut self,
        reason: Option<&'static str>,
        cx: &mut Context<Self>,
    ) {
        if self.exit_disabled_reason != reason {
            self.exit_disabled_reason = reason;
            cx.notify();
        }
    }

    fn exit_presentation(&self) -> (&'static str, &'static str) {
        #[cfg(target_os = "windows")]
        if self.blocked_shutdown.is_some() {
            return (
                "Quit Anyway",
                "Quit Beryl immediately; cleanup is incomplete.",
            );
        }
        if self.shutdown_interaction_gated {
            (
                "Exiting…",
                "Application Exit is waiting for active work and durable state.",
            )
        } else if self.startup_interaction_gated() {
            ("Exit", "Beryl is preparing its windows.")
        } else {
            (
                "Exit",
                self.exit_disabled_reason
                    .unwrap_or("Application Exit is not available."),
            )
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn test_exit_presentation(&self) -> (&'static str, &'static str) {
        self.exit_presentation()
    }
}

pub(super) fn render(
    root: &MainWindowShellRoot,
    window: &Window,
    cx: &mut Context<MainWindowShellRoot>,
) -> AnyElement {
    let generation = root
        .controller()
        .expect("mounted shell controller")
        .appearance();
    let color = |role, property, fallback| theme_color(generation, role, property, fallback);
    let font = theme_font(generation, "button.secondary.label", 13., 500.);
    let mut label_style = window.text_style();
    label_style.font_weight = gpui::FontWeight(font.weight);
    if let Some(family) = &font.family {
        label_style.font_family = family.clone();
    }
    let label_width = ["Exit", "Exiting…", "Quit Anyway"]
        .into_iter()
        .map(|label| {
            window
                .text_system()
                .shape_line(
                    label.into(),
                    px(font.size),
                    &[label_style.to_run(label.len())],
                    None,
                )
                .width
        })
        .fold(px(0.), |width, next| width.max(next));
    let source = cx.weak_entity();
    #[cfg(target_os = "windows")]
    let blocked = root.blocked_shutdown.is_some();
    #[cfg(not(target_os = "windows"))]
    let blocked = false;
    let hover = color("button.secondary.hover", Property::Background, 0xeef2f7);
    let hover_border = color("button.secondary.hover", Property::Border, 0x94a3b8);
    let hover_foreground = color("button.secondary.hover", Property::Foreground, 0x1f2937);
    let pressed = color("button.secondary.pressed", Property::Background, 0xe2e8f0);
    let pressed_border = color("button.secondary.pressed", Property::Border, 0x64748b);
    let ring = color("focus.ring", Property::Color, 0x2563eb);
    let toolbar = color("main.toolbar", Property::Background, 0xf8fafc);
    div()
        .id("main-window-exit")
        .debug_selector(|| "main-window-exit".to_owned())
        .tab_stop(blocked)
        .when(blocked, |button| {
            button
                .track_focus(&root.exit_focus)
                .cursor_pointer()
                .hover(move |style| {
                    style
                        .bg(hover)
                        .border_color(hover_border)
                        .text_color(hover_foreground)
                })
                .active(move |style| style.bg(pressed).border_color(pressed_border))
                .focus(move |style| {
                    style.shadow(vec![
                        gpui::BoxShadow {
                            color: toolbar.into(),
                            offset: gpui::point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            spread_radius: px(2.),
                        },
                        gpui::BoxShadow {
                            color: ring.into(),
                            offset: gpui::point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            spread_radius: px(4.),
                        },
                    ])
                })
                .on_click(cx.listener(|root, _, _, cx| {
                    #[cfg(target_os = "windows")]
                    let _ = root.request_blocked_quit(cx);
                }))
                .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                        #[cfg(target_os = "windows")]
                        let _ = root.request_blocked_quit(cx);
                        cx.stop_propagation();
                    }
                }))
        })
        .h(px(32.))
        .w(label_width + px(26.))
        .px(px(12.))
        .py(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .whitespace_nowrap()
        .overflow_hidden()
        .rounded(px(6.))
        .border_1()
        .border_color(color(
            if blocked {
                "button.secondary.normal"
            } else {
                "button.secondary.disabled"
            },
            Property::Border,
            0xcbd5e1,
        ))
        .bg(color(
            if blocked {
                "button.secondary.normal"
            } else {
                "button.secondary.disabled"
            },
            Property::Background,
            if blocked { 0xf8fafc } else { 0xf1f5f9 },
        ))
        .text_color(color(
            if blocked {
                "button.secondary.label"
            } else {
                "button.secondary.disabled"
            },
            Property::Foreground,
            if blocked { 0x1f2937 } else { 0x94a3b8 },
        ))
        .text_size(px(font.size))
        .font_weight(gpui::FontWeight(font.weight))
        .when_some(font.family, |button, family| button.font_family(family))
        .when(root.shutdown_interaction_gated && !blocked, |button| {
            button.opacity(0.72)
        })
        .child(root.exit_presentation().0)
        .tooltip(move |_, cx| -> AnyView {
            cx.new(|cx| ExitTooltip {
                source: source.clone(),
                _observer: source
                    .upgrade()
                    .map(|root| cx.observe(&root, |_, _, cx| cx.notify())),
            })
            .into()
        })
        .into_any_element()
}

struct ExitTooltip {
    source: gpui::WeakEntity<MainWindowShellRoot>,
    _observer: Option<gpui::Subscription>,
}

impl Render for ExitTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(root) = self.source.upgrade() else {
            return div().into_any_element();
        };
        let root = root.read(cx);
        let Some(controller) = root.controller() else {
            return div().into_any_element();
        };
        let generation = controller.appearance();
        let color = |role, property, fallback| theme_color(generation, role, property, fallback);
        let font = theme_font(generation, "tooltip.text", 12., 400.);
        div()
            .id("main-window-exit-tooltip")
            .debug_selector(|| "main-window-exit-tooltip".to_owned())
            .max_w(px(320.))
            .max_h(px(160.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(5.))
            .border_1()
            .border_color(color("tooltip", Property::Border, 0x0f172a))
            .bg(color("tooltip", Property::Background, 0x111827))
            .text_color(color("tooltip.text", Property::Foreground, 0xffffff))
            .text_size(px(font.size))
            .font_weight(gpui::FontWeight(font.weight))
            .when_some(font.family, |tooltip, family| tooltip.font_family(family))
            .child(root.exit_presentation().1)
            .into_any_element()
    }
}
