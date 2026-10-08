use super::*;
use crate::main_window::creation::command::{theme_color, theme_font};
use beryl_state::ThemePropertyId as Property;
use gpui::{
    AnyElement, AnyView, InteractiveElement, StatefulInteractiveElement, prelude::FluentBuilder,
};

pub(in crate::main_window::shell::host) fn render_command(
    root: &MainWindowShellRoot,
    _: &Window,
    cx: &mut Context<MainWindowShellRoot>,
) -> AnyElement {
    let appearance = root.controller().expect("mounted controller").appearance();
    let font = theme_font(appearance, "button.secondary.label", 13., 500.);
    let color = |role, property, fallback| theme_color(appearance, role, property, fallback);
    let secondary_enabled = root.setup_enabled()
        && !root.runtime_setup.pending()
        && root
            .runtime_setup
            .services
            .as_ref()
            .is_some_and(|services| services.current());
    let attention = root
        .controller()
        .is_some_and(|controller| controller.is_threadless());
    let background = color("button.secondary.normal", Property::Background, 0xf8fafc);
    let foreground = color("button.secondary.label", Property::Foreground, 0x1f2937);
    let separator = color("button.secondary.normal", Property::Border, 0xcbd5e1);
    let ring = color("focus.ring", Property::Color, 0x2563eb);
    let source = cx.weak_entity();
    let primary_source = source.clone();
    let secondary = div()
        .id("main-window-new-thread-secondary")
        .debug_selector(|| "main-window-new-thread-secondary".into())
        .track_focus(&root.runtime_setup.focus)
        .tab_stop(true)
        .h_full()
        .w(px(32.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .border_l_1()
        .border_color(separator)
        .bg(if secondary_enabled && attention {
            color("button.secondary.attention", Property::Background, 0xdbeafe)
        } else {
            background
        })
        .text_color(if secondary_enabled {
            foreground
        } else {
            color("button.secondary.disabled", Property::Foreground, 0x94a3b8)
        })
        .focus(move |style| style.border_color(ring))
        .when(secondary_enabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(gpui::rgb(0xeef2f7)))
                .on_click(cx.listener(|root, _, window, cx| root.open_runtime_setup(window, cx)))
                .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        root.open_runtime_setup(window, cx);
                        cx.stop_propagation();
                    }
                }))
        })
        .tooltip(move |_, cx| -> AnyView {
            cx.new(|cx| SetupCommandTooltip {
                source: source.clone(),
                primary: false,
                _observer: source
                    .upgrade()
                    .map(|root| cx.observe(&root, |_, _, cx| cx.notify())),
            })
            .into()
        })
        .child("…");
    let primary = div()
        .id("main-window-new-thread-primary")
        .debug_selector(|| "main-window-new-thread-primary".into())
        .track_focus(&root.runtime_setup.primary_focus)
        .tab_stop(true)
        .h_full()
        .px(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .bg(background)
        .text_color(color(
            "button.secondary.disabled",
            Property::Foreground,
            0x94a3b8,
        ))
        .focus(move |style| style.border_color(ring))
        .tooltip(move |_, cx| -> AnyView {
            cx.new(|cx| SetupCommandTooltip {
                source: primary_source.clone(),
                primary: true,
                _observer: primary_source
                    .upgrade()
                    .map(|root| cx.observe(&root, |_, _, cx| cx.notify())),
            })
            .into()
        })
        .child("New Thread");
    div()
        .id("main-window-new-thread-split-button")
        .h(px(32.))
        .flex_none()
        .flex()
        .rounded(px(6.))
        .border_1()
        .border_color(separator)
        .overflow_hidden()
        .text_size(px(font.size))
        .font_weight(gpui::FontWeight(font.weight))
        .when_some(font.family, |button, family| button.font_family(family))
        .child(primary)
        .child(secondary)
        .into_any_element()
}

struct SetupCommandTooltip {
    source: gpui::WeakEntity<MainWindowShellRoot>,
    primary: bool,
    _observer: Option<gpui::Subscription>,
}

impl Render for SetupCommandTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self
            .source
            .upgrade()
            .map(|root| {
                let root = root.read(cx);
                if self.primary {
                    if root
                        .controller()
                        .is_some_and(|controller| controller.is_threadless())
                    {
                        "Add a runtime with the … button before creating a thread.".into()
                    } else {
                        "New thread creation is not available yet.".into()
                    }
                } else if let Some(error) = &root.runtime_setup.unavailable {
                    error.clone()
                } else if root.runtime_setup.pending() {
                    "Runtime setup is waiting for its original request.".into()
                } else if !root.setup_enabled() {
                    "Runtime setup is waiting for this window's current work.".into()
                } else if root.runtime_setup.services.is_none() {
                    "Runtime setup services are unavailable.".into()
                } else {
                    "Choose runtime and root".into()
                }
            })
            .unwrap_or_default();
        div()
            .max_w(px(320.))
            .max_h(px(160.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(5.))
            .bg(gpui::rgb(0x111827))
            .text_color(gpui::rgb(0xffffff))
            .text_size(px(12.))
            .child(text)
    }
}

pub(in crate::main_window::shell::host) fn render_picker(
    root: &MainWindowShellRoot,
    window: &Window,
) -> Option<AnyElement> {
    let picker = root.runtime_setup.picker.clone()?;
    Some(
        div()
            .id("main-window-runtime-setup-overlay")
            .absolute()
            .top(px(root.notice_chrome_height()))
            .left(px(12.))
            .max_w((window.viewport_size().width - px(24.)).max(px(0.)))
            .max_h(
                (window.viewport_size().height - px(root.notice_chrome_height() + 12.)).max(px(0.)),
            )
            .child(picker)
            .into_any_element(),
    )
}
