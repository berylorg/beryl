use super::*;
use beryl_state::{ThemePropertyId as Property, ThemeValue};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AnyView, AppContext, Context, FocusHandle, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window,
    div, point, px, rgb,
};

pub(in crate::main_window) fn render(
    root: &MainWindowShellRoot,
    focus: &FocusHandle,
    cx: &mut Context<MainWindowShellRoot>,
) -> AnyElement {
    let reason = root.new_window_disabled_reason(cx);
    let enabled = reason.is_none();
    let generation = root
        .controller()
        .expect("mounted shell controller")
        .appearance()
        .clone();
    let color = |role, property, fallback| theme_color(&generation, role, property, fallback);
    let role = if enabled {
        "button.secondary.normal"
    } else {
        "button.secondary.disabled"
    };
    let background = color(role, Property::Background, 0xf8fafc);
    let foreground = color(
        if enabled {
            "button.secondary.label"
        } else {
            role
        },
        Property::Foreground,
        if enabled { 0x1f2937 } else { 0x94a3b8 },
    );
    let border = color(role, Property::Border, 0xcbd5e1);
    let hover = color("button.secondary.hover", Property::Background, 0xeef2f7);
    let hover_border = color("button.secondary.hover", Property::Border, 0x94a3b8);
    let hover_foreground = color("button.secondary.hover", Property::Foreground, 0x1f2937);
    let pressed = color("button.secondary.pressed", Property::Background, 0xe2e8f0);
    let pressed_border = color("button.secondary.pressed", Property::Border, 0x64748b);
    let ring = color("focus.ring", Property::Color, 0x2563eb);
    let toolbar = color("main.toolbar", Property::Background, 0xf8fafc);
    let font = theme_font(&generation, "button.secondary.label", 13., 500.);
    let mut button = div()
        .id("main-window-new-window")
        .debug_selector(|| "main-window-new-window".to_owned())
        .track_focus(focus)
        .tab_stop(enabled)
        .h(px(32.))
        .px(px(12.))
        .py(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .rounded(px(6.))
        .border_1()
        .border_color(border)
        .bg(background)
        .text_color(foreground)
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
                        .text_color(hover_foreground)
                })
                .active(move |style| style.bg(pressed).border_color(pressed_border))
                .focus(move |style| {
                    style.shadow(vec![
                        gpui::BoxShadow {
                            color: toolbar.into(),
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            spread_radius: px(2.),
                        },
                        gpui::BoxShadow {
                            color: ring.into(),
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            spread_radius: px(4.),
                        },
                    ])
                })
                .on_click(cx.listener(|root, _, _, cx| {
                    cx.stop_propagation();
                    root.invoke_new_window(cx);
                }))
                .on_key_down(cx.listener(|root, event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        cx.stop_propagation();
                        root.invoke_new_window(cx);
                    }
                }))
        })
        .child("New Window");
    let source = cx.weak_entity();
    button = button.tooltip(move |_, cx| -> AnyView {
        cx.new(|cx| CreationTooltip {
            source: source.clone(),
            _observer: source
                .upgrade()
                .map(|root| cx.observe(&root, |_, _, cx| cx.notify())),
        })
        .into()
    });
    button.into_any_element()
}

fn theme_color(
    generation: &crate::theme_runtime::AppearanceGeneration,
    role: &str,
    property: Property,
    fallback: u32,
) -> gpui::Rgba {
    let value = generation
        .prepared()
        .appearance()
        .roles()
        .iter()
        .find(|(id, _)| id.as_str() == role)
        .and_then(|(_, style)| style.property(property));
    if let Some(ThemeValue::Color(value)) = value {
        let [r, g, b] = value.rgb();
        rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
    } else {
        rgb(fallback)
    }
}

struct CreationFont {
    family: Option<SharedString>,
    size: f32,
    weight: f32,
}

fn theme_font(
    generation: &crate::theme_runtime::AppearanceGeneration,
    role: &str,
    size: f32,
    weight: f32,
) -> CreationFont {
    let style = generation
        .prepared()
        .appearance()
        .roles()
        .iter()
        .find(|(id, _)| id.as_str() == role)
        .map(|(_, style)| style);
    let property = |property| style.and_then(|style| style.property(property));
    CreationFont {
        family: match property(Property::FontFamily) {
            Some(ThemeValue::FontFamily(family)) => {
                Some(SharedString::from(family.as_str().to_owned()))
            }
            _ => None,
        },
        size: match property(Property::FontSize) {
            Some(ThemeValue::LogicalPixels(value)) => value.get(),
            _ => size,
        },
        weight: match property(Property::FontWeight) {
            Some(ThemeValue::FontWeight(value)) => f32::from(value.get()),
            _ => weight,
        },
    }
}

struct CreationTooltip {
    source: gpui::WeakEntity<MainWindowShellRoot>,
    _observer: Option<gpui::Subscription>,
}
impl Render for CreationTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(root) = self.source.upgrade() else {
            return div().into_any_element();
        };
        let root = root.read(cx);
        let Some(controller) = root.controller() else {
            return div().into_any_element();
        };
        let generation = controller.appearance();
        let text = root
            .new_window_disabled_reason(cx)
            .unwrap_or_else(|| "New Window (Ctrl+Shift+N)".to_owned());
        let color = |role, property, fallback| theme_color(generation, role, property, fallback);
        let font = theme_font(generation, "tooltip.text", 12., 400.);
        div()
            .id("main-window-new-window-tooltip")
            .debug_selector(|| "main-window-new-window-tooltip".to_owned())
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
            .child(text)
            .into_any_element()
    }
}
