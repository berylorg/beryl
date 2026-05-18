use gpui::{
    App, Context, CursorStyle, ElementId, Entity, MouseButton, ScrollHandle, SharedString, Window,
    div, prelude::*, px,
};

use crate::BerylThemeRole;
use crate::shell::{
    ChromeButtonTheme, ShellRenderFrame, ShellRenderStyleSnapshot, ShellView, layout,
};
use crate::text_input::SingleLineInput;

use super::scrollbars::{ScrollbarAxis, ScrollbarVisibilityPolicy, render_themed_div_scrollbar};

pub(super) fn startup_shell_frame(
    shell: &ShellRenderFrame<'_>,
    scroll_handle: &ScrollHandle,
    scrollbar_visibility: ScrollbarVisibilityPolicy,
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
                                        .font_weight(shell.role_font_weight(
                                            BerylThemeRole::AppWindowTitle,
                                            gpui::FontWeight::BOLD,
                                        ))
                                        .text_color(shell.role_foreground(
                                            BerylThemeRole::AppWindowTitle,
                                            shell.general_ui_foreground(),
                                        ))
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(shell.surface_muted_foreground())
                                        .child(subtitle),
                                ),
                        )
                        .child(card(shell, body)),
                ),
        );
    if let Some(vertical_scrollbar) = render_themed_div_scrollbar(
        shell.style(),
        "beryl-shell-scrollbar-vertical",
        scroll_handle,
        ScrollbarAxis::Vertical,
        scrollbar_visibility.clone(),
    ) {
        scroll_region = scroll_region.child(vertical_scrollbar);
    }
    if let Some(horizontal_scrollbar) = render_themed_div_scrollbar(
        shell.style(),
        "beryl-shell-scrollbar-horizontal",
        scroll_handle,
        ScrollbarAxis::Horizontal,
        scrollbar_visibility,
    ) {
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
                        .items_center()
                        .gap_2()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .flex_none()
                                .text_sm()
                                .font_weight(shell.role_font_weight(
                                    BerylThemeRole::MainToolbarTitle,
                                    gpui::FontWeight::BOLD,
                                ))
                                .text_color(shell.role_foreground(
                                    BerylThemeRole::MainToolbarTitle,
                                    shell.general_ui_foreground(),
                                ))
                                .child("Beryl"),
                        )
                        .child(
                            div()
                                .min_w(px(0.0))
                                .text_xs()
                                .text_color(shell.surface_muted_foreground())
                                .whitespace_nowrap()
                                .truncate()
                                .child(title.to_string()),
                        ),
                    actions,
                ))
                .child(scroll_region),
        )
}

pub(super) fn toolbar_strip(
    shell: &ShellRenderFrame<'_>,
    leading: impl IntoElement,
    actions: impl IntoElement,
) -> impl IntoElement {
    fixed_strip(shell, px(layout::TOOLBAR_STRIP_HEIGHT), leading, actions)
}

pub(super) fn toolbar_controls_strip(
    shell: &ShellRenderFrame<'_>,
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

pub(super) fn section_label(shell: &ShellRenderFrame<'_>, text: &'static str) -> impl IntoElement {
    div()
        .text_xs()
        .font_weight(shell.role_font_weight(
            BerylThemeRole::ControlListHeader,
            gpui::FontWeight::SEMIBOLD,
        ))
        .text_color(shell.role_foreground(
            BerylThemeRole::ControlListHeader,
            shell.status_line_value_foreground(),
        ))
        .child(text)
}

pub(super) fn info_line(
    shell: &ShellRenderFrame<'_>,
    label: &str,
    value: &str,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(shell.surface_muted_foreground())
                .child(label.to_string()),
        )
        .child(
            div()
                .text_sm()
                .text_color(shell.surface_foreground())
                .child(value.to_string()),
        )
}

pub(super) fn inline_notice(
    shell: &ShellRenderFrame<'_>,
    message: &str,
    role: BerylThemeRole,
) -> impl IntoElement {
    let background = shell.role_background(role, shell.popup_surface_background());
    let border = shell.role_border(role, shell.surface_border());
    let foreground = shell.role_foreground(role, shell.surface_foreground());

    div()
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(background)
        .border_1()
        .border_color(border)
        .p_3()
        .text_sm()
        .text_color(foreground)
        .child(message.to_string())
}

pub(super) fn card(shell: &ShellRenderFrame<'_>, content: impl IntoElement) -> impl IntoElement {
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
    shell: &ShellRenderFrame<'_>,
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

pub(super) fn panel_shell_with_style(
    style: &ShellRenderStyleSnapshot,
    content: impl IntoElement,
) -> impl IntoElement {
    div()
        .size_full()
        .min_h(px(0.0))
        .bg(style.transcript_shell_background())
        .border_1()
        .border_color(style.separator_color())
        .text_color(style.transcript_shell_foreground())
        .overflow_hidden()
        .child(content)
}

pub(super) fn button(
    shell: &ShellRenderFrame<'_>,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let theme = shell.primary_button_theme();
    themed_button(theme, ChromeButtonVisualState::Normal, id, label, on_click)
}

pub(super) fn secondary_button(
    shell: &ShellRenderFrame<'_>,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let theme = shell.secondary_button_theme();
    themed_button(theme, ChromeButtonVisualState::Normal, id, label, on_click)
}

pub(super) fn secondary_fixed_label_button(
    shell: &ShellRenderFrame<'_>,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    possible_labels: &'static [&'static str],
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let theme = shell.secondary_button_theme();
    themed_fixed_label_button(
        theme,
        ChromeButtonVisualState::Normal,
        id,
        label,
        possible_labels,
        on_click,
    )
}

pub(super) fn secondary_labeled_cycle_button_with_active_state(
    shell: &ShellRenderFrame<'_>,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    value_label: impl Into<SharedString>,
    possible_value_labels: &'static [&'static str],
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    let value_label = value_label.into();
    let id = id.into();
    let theme = shell.secondary_button_theme();
    let visual_state = if active {
        ChromeButtonVisualState::Active
    } else {
        ChromeButtonVisualState::Normal
    };
    let button_state = visual_state.theme_state(theme);
    let divider_group_id: SharedString = format!("{id}-divider").into();
    let divider_element_id: SharedString = format!("{divider_group_id}-element").into();
    themed_button_container(theme, visual_state, id)
        .group(divider_group_id.clone())
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .px(px(layout::BUTTON_HORIZONTAL_PADDING))
                .flex()
                .items_center()
                .justify_center()
                .child(label),
        )
        .child(
            div()
                .h_full()
                .w(px(1.0))
                .bg(button_state.border)
                .id(divider_element_id)
                .group_hover(divider_group_id.clone(), move |style| {
                    style.bg(theme.hover.border)
                })
                .group_active(divider_group_id, move |style| style.bg(theme.active.border)),
        )
        .child(
            div()
                .h_full()
                .child(fixed_label_slot(value_label, possible_value_labels)),
        )
        .on_click(move |event, window, cx| on_click(event, window, cx))
}

pub(super) fn disabled_secondary_button(
    shell: &ShellRenderFrame<'_>,
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

fn themed_fixed_label_button(
    theme: ChromeButtonTheme,
    visual_state: ChromeButtonVisualState,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    possible_labels: &'static [&'static str],
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    themed_fixed_label_button_base(theme, visual_state, id, label, possible_labels)
        .on_click(move |event, window, cx| on_click(event, window, cx))
}

fn themed_button_base(
    theme: ChromeButtonTheme,
    visual_state: ChromeButtonVisualState,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    themed_button_container(theme, visual_state, id)
        .px(px(layout::BUTTON_HORIZONTAL_PADDING))
        .py(px(layout::BUTTON_VERTICAL_PADDING))
        .child(label)
}

fn themed_fixed_label_button_base(
    theme: ChromeButtonTheme,
    visual_state: ChromeButtonVisualState,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    possible_labels: &'static [&'static str],
) -> gpui::Stateful<gpui::Div> {
    let label = label.into();
    themed_button_container(theme, visual_state, id).child(fixed_label_slot(label, possible_labels))
}

fn fixed_label_slot(
    label: SharedString,
    possible_labels: &'static [&'static str],
) -> impl IntoElement {
    debug_assert!(!possible_labels.is_empty());
    let mut reserved_labels = div()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .overflow_hidden()
        .opacity(0.0);
    for possible_label in possible_labels {
        reserved_labels = reserved_labels.child(
            div()
                .px(px(layout::BUTTON_HORIZONTAL_PADDING))
                .flex()
                .items_center()
                .justify_center()
                .child(*possible_label),
        );
    }

    div()
        .relative()
        .h_full()
        .overflow_hidden()
        .child(reserved_labels)
        .child(
            div()
                .absolute()
                .inset_0()
                .px(px(layout::BUTTON_HORIZONTAL_PADDING))
                .flex()
                .items_center()
                .justify_center()
                .child(label),
        )
}

fn themed_button_container(
    theme: ChromeButtonTheme,
    visual_state: ChromeButtonVisualState,
    id: impl Into<ElementId>,
) -> gpui::Stateful<gpui::Div> {
    let state = visual_state.theme_state(theme);
    debug_assert!(layout::button_required_outer_height() <= layout::BUTTON_OUTER_HEIGHT);
    div()
        .id(id)
        .flex_none()
        .h(px(layout::BUTTON_OUTER_HEIGHT))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(state.background)
        .border_1()
        .border_color(state.border)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
        .font_weight(theme.font_weight)
        .text_color(state.foreground)
        .whitespace_nowrap()
        .when(visual_state.interactive(), move |button| {
            button
                .hover(move |style| {
                    style
                        .bg(theme.hover.background)
                        .border_color(theme.hover.border)
                })
                .active(move |style| {
                    style
                        .bg(theme.active.background)
                        .border_color(theme.active.border)
                })
                .cursor_pointer()
        })
}

pub(super) fn primary_actions(
    shell: &ShellRenderFrame<'_>,
    cx: &mut Context<ShellView>,
) -> impl IntoElement {
    div().flex().gap_3().child(button(
        shell,
        "close-beryl",
        "Close Beryl",
        cx.listener(ShellView::quit),
    ))
}

fn fixed_strip(
    shell: &ShellRenderFrame<'_>,
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
