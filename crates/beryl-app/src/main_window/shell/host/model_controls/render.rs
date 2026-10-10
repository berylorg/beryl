use super::*;
use crate::main_window::creation::command::theme_color;
use crate::widgets::anchored_context_menu::{self as widget, MenuColors};
use beryl_state::ThemePropertyId as Property;

fn menu_colors(root: &MainWindowShellRoot) -> MenuColors {
    let appearance = root.controller().expect("mounted controller").appearance();
    let color = |role, property, fallback| theme_color(appearance, role, property, fallback);
    MenuColors {
        background: color("popup.surface", Property::Background, 0xffffff),
        foreground: color("popup.surface", Property::Foreground, 0x111827),
        border: color("popup.surface", Property::Border, 0xcbd5e1),
        hover: color("popup.row.hover", Property::Background, 0xeef2f7),
        hover_foreground: color("interaction.hover", Property::Foreground, 0x0f172a),
        pressed: color("interaction.pressed", Property::Background, 0xe2e8f0),
        pressed_foreground: color("interaction.pressed", Property::Foreground, 0x0f172a),
        focused: color("interaction.focused", Property::Background, 0xdbeafe),
        focused_foreground: color("interaction.focused", Property::Foreground, 0x0f172a),
        selected: color("popup.row.selected", Property::Background, 0xeff6ff),
        selected_foreground: color("interaction.selected", Property::Foreground, 0x1d4ed8),
        disabled: color("interaction.disabled", Property::Foreground, 0x94a3b8),
        header: color("popup.header", Property::Foreground, 0x64748b),
        checkmark: color("accent_marker", Property::Color, 0x2563eb),
        tooltip_background: color("tooltip", Property::Background, 0x111827),
        tooltip_foreground: color("tooltip.text", Property::Foreground, 0xffffff),
    }
}

pub(in crate::main_window::shell::host) fn render_segment(
    root: &MainWindowShellRoot,
    cx: &mut Context<MainWindowShellRoot>,
) -> AnyElement {
    let controls = &root.model_controls;
    let text = controls.text();
    let command = controls.selection.is_some() && controls.known();
    let reason = root.model_disabled_reason();
    let enabled = command && reason.is_none();
    let tooltip = if command {
        reason.map(str::to_owned).unwrap_or_else(|| text.clone())
    } else {
        text.clone()
    };
    let weak = cx.weak_entity();
    let colors = menu_colors(root);
    let appearance = root.controller().expect("mounted controller").appearance();
    let foreground = theme_color(
        appearance,
        "status.line.value",
        Property::Foreground,
        0x111827,
    );
    div()
        .id("main-window-status-model")
        .debug_selector(|| "main-window-status-model".to_owned())
        .relative()
        .w(px(170.))
        .h_full()
        .flex_none()
        .px(px(10.))
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_color(foreground)
        .when(command && reason.is_some(), |segment| {
            segment.text_color(colors.disabled)
        })
        .when(controls.popup.is_some(), |segment| {
            segment
                .bg(colors.pressed)
                .text_color(colors.pressed_foreground)
        })
        .when(enabled, |segment| {
            segment
                .cursor_pointer()
                .hover(move |style| style.bg(colors.hover).text_color(colors.hover_foreground))
                .active(move |style| {
                    style
                        .bg(colors.pressed)
                        .text_color(colors.pressed_foreground)
                })
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.prevent_default()
                })
                .on_click(cx.listener(|root, _, window, cx| root.toggle_model_menu(window, cx)))
        })
        .tooltip(move |_, cx| cx.new(|_| ModelTooltip(tooltip.clone(), colors)).into())
        .child(text)
        .child(
            gpui::canvas(
                move |bounds, _, app| {
                    let _ = weak.update(app, |root, cx| {
                        let changed = root.model_controls.anchor_bounds != Some(bounds);
                        root.model_controls.anchor_bounds = Some(bounds);
                        if changed && root.model_controls.popup.is_some() {
                            cx.notify();
                        }
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .left(px(0.))
            .top(px(0.))
            .size_full(),
        )
        .into_any_element()
}

pub(in crate::main_window::shell::host) fn render_menu(
    root: &MainWindowShellRoot,
    window: &Window,
    cx: &mut Context<MainWindowShellRoot>,
) -> Option<AnyElement> {
    let popup = root.model_controls.popup.as_ref()?;
    let anchor = root.model_controls.anchor_bounds?;
    Some(widget::render(
        &popup.widget,
        anchor,
        popup.row_count(),
        menu_colors(root),
        ("main-window-model-menu", "main-window-model-options"),
        rows::prepare_range,
        rows::event,
        window,
        cx,
    ))
}

struct ModelTooltip(String, MenuColors);
impl Render for ModelTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(320.))
            .max_h(px(160.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(5.))
            .bg(self.1.tooltip_background)
            .text_color(self.1.tooltip_foreground)
            .text_size(px(12.))
            .child(self.0.clone())
    }
}

impl MainWindowShellRoot {
    pub(crate) fn model_menu_diagnostics(&self) -> Option<ModelMenuDiagnostics> {
        let popup = self.model_controls.popup.as_ref()?;
        Some(ModelMenuDiagnostics {
            total: popup.row_count(),
            realized: popup.widget.realized,
            range: popup.widget.range.clone(),
            overscan: widget::OVERSCAN,
            row_height: widget::ROW_HEIGHT,
            scroll_offset: popup.widget.scroll.0.borrow().base_handle.offset().y,
            focused: popup.widget.has_focus(),
            selected: popup.collection.selected.is_some(),
            reconciliation_micros: popup.widget.reconciliation_micros,
        })
    }

    #[cfg(test)]
    pub(crate) fn test_model_menu_diagnostics(
        &self,
    ) -> Option<(usize, usize, std::ops::Range<usize>, usize, bool, bool)> {
        let diagnostics = self.model_menu_diagnostics()?;
        Some((
            diagnostics.total,
            diagnostics.realized,
            diagnostics.range,
            diagnostics.overscan,
            diagnostics.focused,
            diagnostics.selected,
        ))
    }

    #[cfg(test)]
    pub(crate) fn test_model_menu_bounds(
        &self,
        window: &Window,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        let popup = self.model_controls.popup.as_ref()?;
        Some(popup.widget.bounds(
            self.model_controls.anchor_bounds?,
            popup.row_count(),
            window.viewport_size(),
        ))
    }
}
