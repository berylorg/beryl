use super::*;
use crate::main_window::creation::command::{theme_color, theme_font};
use beryl_state::ThemePropertyId as Property;
use gpui::{
    AnyElement, AnyView, InteractiveElement, StatefulInteractiveElement, prelude::FluentBuilder,
};

pub(in crate::main_window::shell::host) fn render_command(
    root: &MainWindowShellRoot,
    window: &Window,
    cx: &mut Context<MainWindowShellRoot>,
) -> AnyElement {
    let generation = root.controller().expect("mounted controller").appearance();
    let font = theme_font(generation, "button.secondary.label", 13., 500.);
    let mut label_style = window.text_style();
    label_style.font_weight = gpui::FontWeight(font.weight);
    if let Some(family) = &font.family {
        label_style.font_family = family.clone();
    }
    let caption = "Running threads · 18446744073709551615";
    let width = window
        .text_system()
        .shape_line(
            caption.into(),
            px(font.size),
            &[label_style.to_run(caption.len())],
            None,
        )
        .width
        + px(26.);
    let enabled = root.running_threads_enabled();
    let attention = root.running_threads.attention > 0;
    let role = if !enabled {
        "button.secondary.disabled"
    } else {
        "button.secondary.normal"
    };
    let color = |role, property, fallback| theme_color(generation, role, property, fallback);
    let border = if enabled && attention {
        color("status.warning", Property::Color, 0xd97706)
    } else {
        color(role, Property::Border, 0xcbd5e1)
    };
    let label = root
        .running_threads
        .count
        .map(|count| format!("Running threads · {count}"))
        .unwrap_or_else(|| "Running threads".into());
    let hover = color("button.secondary.hover", Property::Background, 0xeef2f7);
    let pressed = color("button.secondary.pressed", Property::Background, 0xe2e8f0);
    let ring = color("focus.ring", Property::Color, 0x2563eb);
    let source = cx.weak_entity();
    div()
        .id("main-window-running-threads")
        .debug_selector(|| "main-window-running-threads".to_owned())
        .tab_stop(enabled)
        .h(px(32.))
        .w(width)
        .px(px(12.))
        .py(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .rounded(px(6.))
        .border_1()
        .border_color(border)
        .bg(color(role, Property::Background, 0xf8fafc))
        .text_color(color(
            if enabled {
                "button.secondary.label"
            } else {
                role
            },
            Property::Foreground,
            if enabled { 0x1f2937 } else { 0x94a3b8 },
        ))
        .text_size(px(font.size))
        .font_weight(gpui::FontWeight(font.weight))
        .when_some(font.family, |button, family| button.font_family(family))
        .when(enabled, |button| {
            button
                .track_focus(&root.running_threads.focus)
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .active(move |style| style.bg(pressed))
                .focus(move |style| style.border_color(ring))
                .on_click(cx.listener(|root, _, window, cx| root.open_running_threads(window, cx)))
                .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        root.open_running_threads(window, cx);
                        cx.stop_propagation();
                    }
                }))
        })
        .child(label)
        .tooltip(move |_, cx| -> AnyView {
            cx.new(|cx| RunningThreadsTooltip {
                source: source.clone(),
                _observer: source
                    .upgrade()
                    .map(|root| cx.observe(&root, |_, _, cx| cx.notify())),
            })
            .into()
        })
        .into_any_element()
}

struct RunningThreadsTooltip {
    source: gpui::WeakEntity<MainWindowShellRoot>,
    _observer: Option<gpui::Subscription>,
}

impl Render for RunningThreadsTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(root) = self.source.upgrade() else {
            return div().into_any_element();
        };
        let root = root.read(cx);
        let Some(controller) = root.controller() else {
            return div().into_any_element();
        };
        let appearance = controller.appearance();
        let text = if root.startup_interaction_gated() {
            "Beryl is preparing its windows.".to_owned()
        } else if root.shutdown_interaction_gated {
            "Application Exit is waiting for active work and durable state.".to_owned()
        } else if root.ordinary_close_interaction_gated {
            "This window is waiting for its draft and durable close state.".to_owned()
        } else if let Some(error) = &root.running_threads.failure {
            format!("Running threads is temporarily unavailable: {error}")
        } else if root.running_threads.attention > 0 {
            format!(
                "{} threads need attention. Inspect running work across all windows.",
                root.running_threads.attention
            )
        } else if root.running_threads.reader.is_none() {
            "The process work source is unavailable.".to_owned()
        } else {
            "Inspect running work across all windows.".to_owned()
        };
        div()
            .max_w(px(320.))
            .max_h(px(160.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(5.))
            .border_1()
            .bg(theme_color(
                appearance,
                "tooltip",
                Property::Background,
                0x111827,
            ))
            .border_color(theme_color(
                appearance,
                "tooltip",
                Property::Border,
                0x0f172a,
            ))
            .text_color(theme_color(
                appearance,
                "tooltip.text",
                Property::Foreground,
                0xffffff,
            ))
            .text_size(px(12.))
            .child(text)
            .into_any_element()
    }
}

pub(in crate::main_window::shell::host) fn render_picker(
    root: &MainWindowShellRoot,
    window: &Window,
) -> Option<AnyElement> {
    let picker = root.running_threads.picker.clone()?;
    Some(
        div()
            .id("main-window-running-threads-overlay")
            .absolute()
            .top(px(root.notice_chrome_height()))
            .right(px(12.))
            .max_w(window.viewport_size().width - px(24.))
            .max_h(
                (window.viewport_size().height - px(root.notice_chrome_height() + 12.)).max(px(0.)),
            )
            .child(picker)
            .into_any_element(),
    )
}
