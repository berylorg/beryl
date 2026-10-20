use super::*;
use crate::main_window::creation::command::{theme_color, theme_font};
use beryl_state::ThemePropertyId as Property;
use gpui::{
    AnyElement, AnyView, AppContext as _, InteractiveElement as _, IntoElement as _,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, prelude::FluentBuilder,
};

impl MainWindowShellRoot {
    fn settings_disabled_reason(&self, app: &App) -> Option<&'static str> {
        if self.shutdown_interaction_gated || self.ordinary_close_interaction_gated {
            Some("Settings is unavailable while this window is closing.")
        } else {
            crate::settings_command::SettingsCommandRoute::disabled_reason(app)
        }
    }

    fn open_settings(&self, app: &mut App) {
        if self.settings_disabled_reason(app).is_none() {
            if let Err(error) = crate::settings_command::SettingsCommandRoute::open(app) {
                tracing::warn!(%error, "Settings window could not open");
            }
        }
    }
}

pub(super) fn render(
    root: &MainWindowShellRoot,
    cx: &mut Context<MainWindowShellRoot>,
) -> AnyElement {
    let generation = root.controller().expect("mounted shell").appearance();
    let color = |role, property, fallback| theme_color(generation, role, property, fallback);
    let enabled = root.settings_disabled_reason(cx).is_none();
    let role = if enabled {
        "button.secondary.normal"
    } else {
        "button.secondary.disabled"
    };
    let font = theme_font(generation, "button.secondary.label", 13., 500.);
    let hover = color("button.secondary.hover", Property::Background, 0xeef2f7);
    let hover_border = color("button.secondary.hover", Property::Border, 0x94a3b8);
    let hover_text = color("button.secondary.hover", Property::Foreground, 0x1f2937);
    let pressed = color("button.secondary.pressed", Property::Background, 0xe2e8f0);
    let ring = color("focus.ring", Property::Color, 0x2563eb);
    let source = cx.weak_entity();
    div()
        .id("main-window-settings")
        .debug_selector(|| "main-window-settings".to_owned())
        .track_focus(&root.settings_focus)
        .tab_stop(enabled)
        .w(root.controller().unwrap().metrics.settings_width)
        .h(px(sizing::CONTROL_HEIGHT))
        .px(px(12.))
        .py(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .rounded(px(6.))
        .border_1()
        .border_color(color(role, Property::Border, 0xcbd5e1))
        .bg(color(role, Property::Background, 0xf8fafc))
        .text_color(color(
            if enabled {
                "button.secondary.label"
            } else {
                role
            },
            Property::Foreground,
            0x1f2937,
        ))
        .text_size(px(font.size))
        .font_weight(gpui::FontWeight(font.weight))
        .when_some(font.family, |button, family| button.font_family(family))
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| {
                    style
                        .bg(hover)
                        .border_color(hover_border)
                        .text_color(hover_text)
                })
                .active(move |style| style.bg(pressed))
                .focus(move |style| style.border_color(ring))
                .on_click(cx.listener(|root, _, _, cx| {
                    cx.stop_propagation();
                    root.open_settings(cx);
                }))
                .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        cx.stop_propagation();
                        root.open_settings(cx);
                    }
                }))
        })
        .child("Settings")
        .tooltip(move |_, cx| -> AnyView {
            cx.new(|cx| SettingsTooltip {
                source: source.clone(),
                _observer: source
                    .upgrade()
                    .map(|root| cx.observe(&root, |_, _, cx| cx.notify())),
            })
            .into()
        })
        .into_any_element()
}

struct SettingsTooltip {
    source: gpui::WeakEntity<MainWindowShellRoot>,
    _observer: Option<gpui::Subscription>,
}

impl gpui::Render for SettingsTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(root) = self.source.upgrade() else {
            return div().into_any_element();
        };
        let root = root.read(cx);
        let Some(controller) = root.controller() else {
            return div().into_any_element();
        };
        let generation = controller.appearance();
        let font = theme_font(generation, "tooltip.text", 12., 400.);
        div()
            .px(px(8.))
            .py(px(6.))
            .rounded(px(6.))
            .border_1()
            .bg(theme_color(
                generation,
                "tooltip",
                Property::Background,
                0x111827,
            ))
            .border_color(theme_color(
                generation,
                "tooltip",
                Property::Border,
                0x475569,
            ))
            .text_color(theme_color(
                generation,
                "tooltip.text",
                Property::Foreground,
                0xf8fafc,
            ))
            .text_size(px(font.size))
            .font_weight(gpui::FontWeight(font.weight))
            .when_some(font.family, |element, family| element.font_family(family))
            .child(root.settings_disabled_reason(cx).unwrap_or("Open Settings"))
            .into_any_element()
    }
}
