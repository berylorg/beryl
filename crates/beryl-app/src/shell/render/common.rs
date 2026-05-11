use gpui::{
    App, Context, CursorStyle, ElementId, Entity, MouseButton, ScrollHandle, SharedString, Window,
    div, prelude::*, px, rgb,
};

use crate::shell::{ChromeButtonTheme, ShellView, layout};
use crate::text_input::SingleLineInput;

use super::scrollbars::{ScrollbarAxis, render_div_scrollbar};

pub(super) fn startup_shell_frame(
    shell: &ShellView,
    scroll_handle: &ScrollHandle,
    scrollbar_opacity: f32,
    on_scrollbar_mouse_move: impl Fn(&gpui::MouseMoveEvent, &mut Window, &mut App) + 'static,
    on_scrollbar_scroll_wheel: impl Fn(&gpui::ScrollWheelEvent, &mut Window, &mut App) + 'static,
    title: &'static str,
    subtitle: &'static str,
    body: impl IntoElement,
    actions: impl IntoElement,
) -> impl IntoElement {
    let mut scroll_region = div()
        .relative()
        .flex_1()
        .min_h(px(0.0))
        .on_mouse_move(on_scrollbar_mouse_move)
        .on_scroll_wheel(on_scrollbar_scroll_wheel)
        .child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .id("beryl-shell-scroll")
                .track_scroll(scroll_handle)
                .overflow_scroll()
                .child(
                    div()
                        .w_full()
                        .min_h_full()
                        .p_6()
                        .flex()
                        .flex_col()
                        .gap_5()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_3xl()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .child(title),
                                )
                                .child(div().text_sm().text_color(rgb(0x94a3b8)).child(subtitle)),
                        )
                        .child(card(shell, body)),
                ),
        );
    if let Some(vertical_scrollbar) =
        render_div_scrollbar(scroll_handle, ScrollbarAxis::Vertical, scrollbar_opacity)
    {
        scroll_region = scroll_region.child(vertical_scrollbar);
    }
    if let Some(horizontal_scrollbar) =
        render_div_scrollbar(scroll_handle, ScrollbarAxis::Horizontal, scrollbar_opacity)
    {
        scroll_region = scroll_region.child(horizontal_scrollbar);
    }

    div()
        .size_full()
        .bg(shell.general_ui_background())
        .text_color(shell.general_ui_foreground())
        .child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(toolbar_strip(
                    shell,
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_lg()
                                .font_weight(gpui::FontWeight::BOLD)
                                .child("Beryl"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x94a3b8))
                                .child(title.to_string()),
                        ),
                    actions,
                ))
                .child(scroll_region),
        )
}

pub(super) fn toolbar_strip(
    shell: &ShellView,
    leading: impl IntoElement,
    actions: impl IntoElement,
) -> impl IntoElement {
    fixed_strip(shell, px(layout::TOOLBAR_STRIP_HEIGHT), leading, actions)
}

pub(super) fn toolbar_controls_strip(
    shell: &ShellView,
    controls: impl IntoElement,
) -> impl IntoElement {
    div()
        .w_full()
        .h(px(layout::TOOLBAR_STRIP_HEIGHT))
        .px_4()
        .bg(shell.toolbar_background())
        .border_b_1()
        .border_color(shell.separator_color())
        .flex()
        .items_center()
        .gap_3()
        .child(controls)
}

pub(super) fn section_label(text: &'static str) -> impl IntoElement {
    div()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(0x93c5fd))
        .child(text)
}

pub(super) fn info_line(label: &str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x94a3b8))
                .child(label.to_string()),
        )
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xf8fafc))
                .child(value.to_string()),
        )
}

pub(super) fn inline_notice(
    message: &str,
    background: gpui::Rgba,
    foreground: gpui::Rgba,
) -> impl IntoElement {
    div()
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(background)
        .border_1()
        .border_color(foreground)
        .p_3()
        .text_sm()
        .text_color(foreground)
        .child(message.to_string())
}

pub(super) fn card(shell: &ShellView, content: impl IntoElement) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .bg(shell.panel_surface_background())
        .border_1()
        .border_color(shell.surface_border())
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .child(content)
}

pub(super) fn framed_text_input(
    shell: &ShellView,
    input: &Entity<SingleLineInput>,
) -> impl IntoElement {
    let focus_input = input.clone();

    div()
        .w_full()
        .h(px(38.0))
        .overflow_hidden()
        .px_3()
        .pt(px(4.0))
        .pb(px(8.0))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(shell.input_background())
        .border_1()
        .border_color(shell.input_border())
        .text_color(shell.input_foreground())
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            let focus_handle = focus_input.read(cx).tab_focus_handle();
            window.focus(&focus_handle);
        })
        .child(input.clone())
}

pub(super) fn panel_shell(shell: &ShellView, content: impl IntoElement) -> impl IntoElement {
    div()
        .size_full()
        .min_h(px(0.0))
        .bg(shell.transcript_shell_background())
        .border_1()
        .border_color(shell.separator_color())
        .text_color(shell.transcript_shell_foreground())
        .overflow_hidden()
        .child(content)
}

pub(super) fn button(
    shell: &ShellView,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let theme = shell.primary_button_theme();
    themed_button(theme, ChromeButtonVisualState::Normal, id, label, on_click)
}

pub(super) fn secondary_button(
    shell: &ShellView,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let theme = shell.secondary_button_theme();
    themed_button(theme, ChromeButtonVisualState::Normal, id, label, on_click)
}

pub(super) fn secondary_button_with_active_state(
    shell: &ShellView,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let theme = shell.secondary_button_theme();
    let visual_state = if active {
        ChromeButtonVisualState::Active
    } else {
        ChromeButtonVisualState::Normal
    };
    themed_button(theme, visual_state, id, label, on_click)
}

pub(super) fn disabled_secondary_button(
    shell: &ShellView,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let theme = shell.secondary_button_theme();
    themed_button_base(theme, ChromeButtonVisualState::Disabled, id, label)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChromeButtonVisualState {
    Normal,
    Active,
    Disabled,
}

impl ChromeButtonVisualState {
    fn theme_state(self, theme: ChromeButtonTheme) -> crate::shell::ChromeButtonStateTheme {
        match self {
            Self::Normal => theme.normal,
            Self::Active => theme.active,
            Self::Disabled => theme.disabled,
        }
    }

    fn interactive(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

fn themed_button(
    theme: ChromeButtonTheme,
    visual_state: ChromeButtonVisualState,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    themed_button_base(theme, visual_state, id, label)
        .on_click(move |event, window, cx| on_click(event, window, cx))
}

fn themed_button_base(
    theme: ChromeButtonTheme,
    visual_state: ChromeButtonVisualState,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    let state = visual_state.theme_state(theme);
    debug_assert!(layout::button_required_outer_height() <= layout::BUTTON_OUTER_HEIGHT);
    div()
        .id(id)
        .h(px(layout::BUTTON_OUTER_HEIGHT))
        .px(px(layout::BUTTON_HORIZONTAL_PADDING))
        .py(px(layout::BUTTON_VERTICAL_PADDING))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(state.background)
        .border_1()
        .border_color(state.border)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
        .text_color(state.foreground)
        .whitespace_nowrap()
        .child(label)
        .when(visual_state.interactive(), move |button| {
            button
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
                        .text_color(theme.active.foreground)
                })
                .cursor_pointer()
        })
}

pub(super) fn primary_actions(shell: &ShellView, cx: &mut Context<ShellView>) -> impl IntoElement {
    div().flex().gap_3().child(button(
        shell,
        "close-beryl",
        "Close Beryl",
        cx.listener(ShellView::quit),
    ))
}

fn fixed_strip(
    shell: &ShellView,
    height: gpui::Pixels,
    leading: impl IntoElement,
    actions: impl IntoElement,
) -> impl IntoElement {
    div()
        .w_full()
        .h(height)
        .px_4()
        .bg(shell.toolbar_background())
        .border_b_1()
        .border_color(shell.separator_color())
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .child(leading),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap_3()
                .child(actions),
        )
}
