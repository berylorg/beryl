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
    let color = |role, property, fallback| theme_color(appearance, role, property, fallback);
    let ready = root
        .thread_switcher
        .reader
        .as_ref()
        .is_some_and(|reader| reader.is_ready());
    let reason = root.switcher_disabled_reason(cx);
    let enabled = ready && reason.is_none();
    let open = root.thread_switcher.picker.is_some();
    let background = color(
        if open {
            "thread-selector-trigger.open"
        } else {
            "thread-selector-trigger"
        },
        Property::Background,
        if open { 0x1e293b } else { 0x172033 },
    );
    let hover = color(
        "thread-selector-trigger.hover",
        Property::Background,
        0x1e293b,
    );
    let pressed = color(
        "thread-selector-trigger.pressed",
        Property::Background,
        0x263449,
    );
    let ring = color("thread-selector-trigger.focused", Property::Color, 0x38bdf8);
    let font = theme_font(appearance, "thread-selector-trigger.title", 13., 600.);
    let source = cx.weak_entity();
    div()
        .id("main-window-thread-selector")
        .debug_selector(|| "main-window-thread-selector".into())
        .flex_1()
        .min_w_0()
        .h(px(32.))
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(12.))
        .rounded(px(6.))
        .border_1()
        .border_color(color("thread-selector-trigger", Property::Border, 0x334155))
        .bg(background)
        .when(ready, |trigger| {
            trigger
                .track_focus(&root.thread_switcher.focus)
                .tab_stop(true)
                .focus(move |style| style.border_color(ring))
        })
        .when(!enabled, |trigger| {
            trigger.opacity(if ready { 0.68 } else { 0.55 })
        })
        .when(enabled, |trigger| {
            trigger
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .active(move |style| style.bg(pressed))
                .on_click(cx.listener(|root, _, window, cx| root.open_thread_switcher(window, cx)))
                .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
                        root.open_thread_switcher(window, cx);
                        cx.stop_propagation();
                    }
                }))
        })
        .tooltip(move |_, cx| -> AnyView {
            cx.new(|_| SwitcherTooltip {
                root: source.clone(),
            })
            .into()
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(font.size))
                .font_weight(gpui::FontWeight(font.weight))
                .text_color(color(
                    "thread-selector-trigger.title",
                    Property::Foreground,
                    0xf1f5f9,
                ))
                .when_some(font.family, |title, family| title.font_family(family))
                .child(root.thread_switcher.title.clone()),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.))
                .text_size(px(10.))
                .font_weight(gpui::FontWeight(700.))
                .text_color(color(
                    "thread-selector-trigger.flyout-affordance",
                    Property::Foreground,
                    0x7dd3fc,
                ))
                .child("Switch thread")
                .child("▾"),
        )
        .into_any_element()
}

struct SwitcherTooltip {
    root: gpui::WeakEntity<MainWindowShellRoot>,
}
impl Render for SwitcherTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self
            .root
            .upgrade()
            .map(|root| {
                let root = root.read(cx);
                root.thread_switcher
                    .disabled_reason
                    .clone()
                    .or_else(|| root.thread_switcher.failure.clone())
                    .unwrap_or_else(|| root.thread_switcher.title.clone())
            })
            .unwrap_or_default();
        div()
            .max_w(px(420.))
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
    let picker = root.thread_switcher.picker.clone()?;
    Some(
        div()
            .id("main-window-thread-switcher-overlay")
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
