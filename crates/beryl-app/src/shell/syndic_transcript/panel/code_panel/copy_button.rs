use super::*;
use crate::widgets::command_style as layout;

pub(super) fn render(
    code: &MountedCodePanel,
    host: &SyndicTranscriptHost,
    appearance: &super::super::code_appearance::CodeAppearance,
    commands_pending: bool,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> AnyElement {
    render_control(code, host, appearance, commands_pending, false, cx)
}

pub(super) fn render_save(
    code: &MountedCodePanel,
    host: &SyndicTranscriptHost,
    appearance: &super::super::code_appearance::CodeAppearance,
    commands_pending: bool,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> AnyElement {
    render_control(code, host, appearance, commands_pending, true, cx)
}

fn render_control(
    code: &MountedCodePanel,
    host: &SyndicTranscriptHost,
    appearance: &super::super::code_appearance::CodeAppearance,
    commands_pending: bool,
    saving: bool,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> AnyElement {
    let reason = if saving {
        super::save::unavailable(code, host, commands_pending)
    } else {
        super::commands::whole_copy_unavailable(code, host, commands_pending)
    };
    debug_assert!(layout::button_required_outer_height() <= layout::BUTTON_OUTER_HEIGHT);
    let enabled = reason.is_none();
    let theme = appearance.command_theme;
    let ring = appearance.selected_border;
    let state = if enabled {
        theme.normal
    } else {
        theme.disabled
    };
    let id = code.id;
    let focus = if saving {
        code.save_focus.clone()
    } else {
        code.copy_focus.clone()
    };
    let pointer_focus = focus.clone();
    let mut button = div()
        .id(SharedString::from(format!(
            "code-{}-{id}",
            if saving { "save" } else { "copy" }
        )))
        .track_focus(&focus)
        .tab_stop(true)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .h(px(layout::BUTTON_OUTER_HEIGHT))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .px(px(layout::BUTTON_HORIZONTAL_PADDING))
        .py(px(layout::BUTTON_VERTICAL_PADDING))
        .border_1()
        .border_color(state.border)
        .bg(state.background)
        .text_color(state.foreground)
        .font_family(appearance.command_font.family.clone())
        .font_weight(theme.font_weight)
        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
        .whitespace_nowrap()
        .child(if saving { "Save…" } else { "Copy" })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_, _, window, cx| {
                if enabled {
                    window.focus(&pointer_focus);
                }
                cx.stop_propagation();
            }),
        )
        .on_key_down(
            cx.listener(move |panel, event: &gpui::KeyDownEvent, window, cx| {
                if super::super::copy_keyboard::is_copy(event) {
                    panel.copy_code_key(id, event, window, cx);
                    cx.stop_propagation();
                } else if enabled
                    && !event.is_held
                    && matches!(event.keystroke.key.as_str(), "enter" | "space")
                {
                    if saving {
                        panel.begin_code_save(id, cx);
                    } else {
                        panel.begin_code_copy(id, false, cx);
                    }
                    cx.stop_propagation();
                }
            }),
        );
    if enabled {
        button = button
            .cursor_pointer()
            .hover(move |style| {
                style
                    .bg(theme.hover.background)
                    .border_color(theme.hover.border)
                    .text_color(theme.hover.foreground)
            })
            .active(move |style| {
                style
                    .bg(theme.active.background)
                    .border_color(theme.active.border)
            })
            .focus(move |style| style.border_color(ring))
            .on_click(cx.listener(move |panel, _, _, cx| {
                if saving {
                    panel.begin_code_save(id, cx);
                } else {
                    panel.begin_code_copy(id, false, cx);
                }
                cx.stop_propagation();
            }));
    } else if let Some(reason) = reason {
        button = button.tooltip(move |_, cx| cx.new(|_| CopyUnavailableTooltip { reason }).into());
    }
    button.into_any_element()
}

struct CopyUnavailableTooltip {
    reason: &'static str,
}

impl gpui::Render for CopyUnavailableTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("code-copy-unavailable-tooltip")
            .max_w(px(280.0))
            .p(px(8.0))
            .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
            .border_1()
            .border_color(gpui::rgb(0x475569))
            .bg(gpui::rgb(0x1e293b))
            .text_color(gpui::rgb(0xf8fafc))
            .text_size(px(13.0))
            .child(self.reason)
    }
}
