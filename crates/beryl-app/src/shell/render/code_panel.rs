use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use gpui::{
    AnyElement, App, Bounds, CursorStyle, Font, FontStyle, FontWeight, IsZero, MouseButton,
    MouseDownEvent, Overflow, Pixels, Point, Rgba, ScrollHandle, SharedString, Size,
    StatefulInteractiveElement, StyledText, TextLayout, TextRun, Window, canvas, div, point,
    prelude::*, px, rems, rgb,
};

use crate::shell::layout;

use super::scrollbars::{ScrollbarAxis, render_div_scrollbar};

pub(crate) const CODE_FONT_FAMILY: &str = "Consolas";
const CODE_FONT_SIZE_REM: f32 = 0.875;
const CODE_PANEL_RESIZE_HANDLE_HEIGHT: f32 = 10.0;
const CODE_PANEL_ESTIMATED_LINE_HEIGHT: f32 = 20.0;
const CODE_PANEL_VISIBLE_LINE_CAP: usize = 12;
const CODE_PANEL_CONTENT_VERTICAL_PADDING: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodePanelWrapMode {
    Smart { columns: usize },
    NoWrap,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CodePanelChrome {
    Bordered { background: Rgba, border: Rgba },
}

pub(crate) type CodePanelAction = Arc<dyn Fn(&mut App)>;
pub(crate) type CodePanelResizeStartAction = Arc<dyn Fn(Pixels, Pixels, &MouseDownEvent, &mut App)>;
pub(crate) type CodePanelLinePrepaintAction = Arc<dyn Fn(Bounds<Pixels>, TextLayout, &mut App)>;

#[derive(Clone)]
pub(crate) struct CodePanelHeaderAction {
    pub key: String,
    pub label: String,
    pub active: bool,
    pub on_click: CodePanelAction,
}

#[derive(Clone, Default)]
pub(crate) struct CodePanelHeader {
    pub title: Option<String>,
    pub leading_actions: Vec<CodePanelHeaderAction>,
    pub trailing_actions: Vec<CodePanelHeaderAction>,
}

#[derive(Clone)]
pub(crate) struct CodePanelResize {
    pub current_height: Option<Pixels>,
    pub min_height: Pixels,
    pub max_height: Option<Pixels>,
    pub on_resize_start: CodePanelResizeStartAction,
}

#[derive(Clone)]
pub(crate) struct CodePanelSelection {
    pub line_prepaint_action: Arc<dyn Fn(CodePanelSelectableLine) -> CodePanelLinePrepaintAction>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodePanelSelectableLine {
    pub raw_text: String,
    pub break_before: usize,
    pub display_text_len: usize,
}

#[derive(Clone)]
pub(crate) struct CodePanelScrollChrome {
    pub handle: ScrollHandle,
    pub scrollbar_opacity: f32,
    pub on_activity: Option<CodePanelAction>,
    pub on_select: Option<CodePanelAction>,
    pub vertical_wheel_ownership: CodePanelVerticalWheelOwnership,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodePanelVerticalWheelOwnership {
    Panel,
    Parent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CodePanelScrollOverflow {
    pub horizontal: Overflow,
    pub vertical: Overflow,
}

pub(crate) fn render_code_panel(
    element_id: Option<String>,
    source: &str,
    language: Option<&str>,
    wrap_mode: CodePanelWrapMode,
    chrome: CodePanelChrome,
    foreground: Rgba,
    header: Option<CodePanelHeader>,
    scroll_chrome: Option<CodePanelScrollChrome>,
    resize: Option<CodePanelResize>,
    selection: Option<CodePanelSelection>,
) -> AnyElement {
    let element_key = code_panel_element_key(element_id.clone(), source, language);
    let display_lines = code_panel_display_lines(source, wrap_mode);
    let display_text = match wrap_mode {
        CodePanelWrapMode::Smart { columns } => smart_wrap_for_columns(source, columns),
        CodePanelWrapMode::NoWrap => source.to_string(),
    };
    let content_height = resize
        .as_ref()
        .map(|resize| resolved_resizable_code_panel_height(display_text.as_str(), resize));

    let content = render_code_panel_content(
        element_key,
        display_lines,
        wrap_mode,
        foreground,
        scroll_chrome,
        content_height,
        selection,
    );

    let CodePanelChrome::Bordered { background, border } = chrome;
    let header_title = header
        .as_ref()
        .and_then(|header| header.title.clone())
        .or_else(|| language.map(str::to_string));
    let has_header = header
        .as_ref()
        .is_some_and(|header| code_panel_has_header(header, header_title.as_deref()));

    let mut panel = div()
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .bg(background)
        .border_1()
        .border_color(border)
        .overflow_hidden()
        .flex()
        .flex_col();

    if let Some(header) =
        header.filter(|header| code_panel_has_header(header, header_title.as_deref()))
    {
        panel = panel.child(render_code_panel_header(
            element_key,
            header_title.as_deref(),
            background,
            border,
            header,
        ));
    }

    panel
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .p_3()
                .when(has_header, |this| this.border_t_1().border_color(border))
                .child(content),
        )
        .when_some(
            resize.zip(content_height),
            |panel, (resize, content_height)| {
                panel.child(render_code_panel_resize_handle(
                    background,
                    border,
                    content_height,
                    resize.on_resize_start,
                ))
            },
        )
        .into_any_element()
}

fn code_panel_element_key(element_id: Option<String>, source: &str, language: Option<&str>) -> u64 {
    let mut hasher = DefaultHasher::new();
    match element_id {
        Some(element_id) => element_id.hash(&mut hasher),
        None => {
            source.hash(&mut hasher);
            language.hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn code_panel_has_header(header: &CodePanelHeader, header_title: Option<&str>) -> bool {
    header_title.is_some_and(|title| !title.is_empty())
        || !header.leading_actions.is_empty()
        || !header.trailing_actions.is_empty()
}

fn render_code_panel_header(
    element_key: u64,
    title: Option<&str>,
    muted_background: Rgba,
    border: Rgba,
    header: CodePanelHeader,
) -> impl IntoElement {
    let CodePanelHeader {
        leading_actions,
        trailing_actions,
        ..
    } = header;

    div()
        .w_full()
        .min_w(px(0.0))
        .px_3()
        .py_2()
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .children(leading_actions.into_iter().map({
                    move |action| {
                        code_panel_header_button(element_key, muted_background, border, action)
                            .into_any_element()
                    }
                })),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(0x94a3b8))
                .child(title.unwrap_or_default().to_string()),
        )
        .child(div().flex().items_center().justify_end().gap_2().children(
            trailing_actions.into_iter().map({
                move |action| {
                    code_panel_header_button(element_key, muted_background, border, action)
                        .into_any_element()
                }
            }),
        ))
}

fn code_panel_header_button(
    element_key: u64,
    muted_background: Rgba,
    border: Rgba,
    action: CodePanelHeaderAction,
) -> impl IntoElement {
    let CodePanelHeaderAction {
        key,
        label,
        active,
        on_click,
    } = action;

    div()
        .id((
            "code-panel-header-action",
            code_panel_header_action_key(element_key, key.as_str()),
        ))
        .flex_none()
        .h(px(layout::BUTTON_OUTER_HEIGHT))
        .px(px(layout::BUTTON_HORIZONTAL_PADDING))
        .py(px(layout::BUTTON_VERTICAL_PADDING))
        .rounded(px(layout::ROUNDED_WIDGET_CORNER_RADIUS))
        .flex()
        .items_center()
        .justify_center()
        .bg(if active {
            rgb(0x1e3a8a)
        } else {
            muted_background
        })
        .border_1()
        .border_color(if active { rgb(0x60a5fa) } else { border })
        .hover(|style| style.bg(if active { rgb(0x1d4ed8) } else { rgb(0x0f172a) }))
        .text_size(px(layout::BUTTON_LABEL_FONT_SIZE))
        .line_height(px(layout::BUTTON_LABEL_LINE_HEIGHT))
        .text_color(if active { rgb(0xf8fafc) } else { rgb(0xcbd5e1) })
        .cursor_pointer()
        .child(label)
        .on_click(move |_, _, cx| on_click(cx))
}

fn code_panel_header_action_key(element_key: u64, action_name: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    element_key.hash(&mut hasher);
    action_name.hash(&mut hasher);
    hasher.finish()
}

fn render_code_panel_content(
    element_key: u64,
    display_lines: Vec<CodePanelDisplayLine>,
    wrap_mode: CodePanelWrapMode,
    foreground: Rgba,
    scroll_chrome: Option<CodePanelScrollChrome>,
    content_height: Option<Pixels>,
    selection: Option<CodePanelSelection>,
) -> AnyElement {
    let selection_enabled = selection.is_some();
    match (wrap_mode, content_height) {
        (CodePanelWrapMode::Smart { .. }, None) => {
            render_code_panel_text(display_lines, wrap_mode, foreground, selection, false)
        }
        (CodePanelWrapMode::Smart { .. }, Some(content_height)) => render_scrollable_code_panel(
            element_key,
            render_code_panel_text(display_lines, wrap_mode, foreground, selection, true),
            ScrollbarAxes {
                horizontal: false,
                vertical: true,
            },
            scroll_chrome,
            Some(content_height),
            selection_enabled,
        ),
        (CodePanelWrapMode::NoWrap, None) => render_scrollable_code_panel(
            element_key,
            render_code_panel_text(display_lines, wrap_mode, foreground, selection, false),
            ScrollbarAxes {
                horizontal: true,
                vertical: false,
            },
            scroll_chrome,
            None,
            selection_enabled,
        ),
        (CodePanelWrapMode::NoWrap, Some(content_height)) => render_scrollable_code_panel(
            element_key,
            render_code_panel_text(display_lines, wrap_mode, foreground, selection, true),
            ScrollbarAxes {
                horizontal: true,
                vertical: true,
            },
            scroll_chrome,
            Some(content_height),
            selection_enabled,
        ),
    }
}

fn render_code_panel_text(
    display_lines: Vec<CodePanelDisplayLine>,
    wrap_mode: CodePanelWrapMode,
    foreground: Rgba,
    selection: Option<CodePanelSelection>,
    fill_height: bool,
) -> AnyElement {
    div()
        .w_full()
        .min_w(px(0.0))
        .when(fill_height, |this| this.min_h_full())
        .flex()
        .flex_col()
        .gap_0()
        .children(
            display_lines
                .into_iter()
                .map(|line| render_code_panel_line(line, wrap_mode, foreground, selection.clone())),
        )
        .into_any_element()
}

fn render_code_panel_line(
    line: CodePanelDisplayLine,
    wrap_mode: CodePanelWrapMode,
    foreground: Rgba,
    selection: Option<CodePanelSelection>,
) -> AnyElement {
    let display_text_len = line.display_text.len();
    let layout_text = if line.display_text.is_empty() {
        " ".to_string()
    } else {
        line.display_text
    };
    let styled_text = code_panel_styled_text(layout_text, foreground);
    let text_layout = styled_text.layout().clone();
    let prepaint_action = selection.map(|selection| {
        (selection.line_prepaint_action)(CodePanelSelectableLine {
            raw_text: line.raw_text,
            break_before: line.break_before,
            display_text_len,
        })
    });

    let line = div()
        .w_full()
        .min_w(px(0.0))
        .text_sm()
        .font_family(CODE_FONT_FAMILY)
        .text_color(foreground)
        .child(styled_text);
    let line = match wrap_mode {
        CodePanelWrapMode::Smart { .. } => line.whitespace_normal(),
        CodePanelWrapMode::NoWrap => line.whitespace_nowrap(),
    };

    line.when_some(prepaint_action, |line, prepaint_action| {
        line.cursor(CursorStyle::IBeam)
            .on_children_prepainted(move |bounds, _, cx| {
                let Some(bounds) = bounds.first().copied() else {
                    return;
                };
                prepaint_action(bounds, text_layout.clone(), cx);
            })
    })
    .into_any_element()
}

fn code_panel_styled_text(text: String, foreground: Rgba) -> StyledText {
    let len = text.len();
    StyledText::new(text).with_runs(vec![TextRun {
        len,
        font: Font {
            family: SharedString::from(CODE_FONT_FAMILY),
            features: Default::default(),
            fallbacks: None,
            weight: FontWeight(400.0),
            style: FontStyle::Normal,
        },
        color: foreground.into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }])
}

#[derive(Clone, Copy)]
pub(crate) struct ScrollbarAxes {
    pub(crate) horizontal: bool,
    pub(crate) vertical: bool,
}

fn render_scrollable_code_panel(
    element_key: u64,
    content: impl IntoElement,
    axes: ScrollbarAxes,
    scroll_chrome: Option<CodePanelScrollChrome>,
    content_height: Option<Pixels>,
    selection_enabled: bool,
) -> AnyElement {
    let mut scrollable = div()
        .id(("code-panel-scroll", element_key))
        .w_full()
        .min_w(px(0.0));

    if let Some(content_height) = content_height {
        scrollable = scrollable.h(content_height);
    }

    let vertical_wheel_ownership = scroll_chrome
        .as_ref()
        .map_or(CodePanelVerticalWheelOwnership::Panel, |scroll_chrome| {
            scroll_chrome.vertical_wheel_ownership
        });
    let overflow = code_panel_scroll_overflow(axes, vertical_wheel_ownership);
    scrollable.style().overflow.x = Some(overflow.horizontal);
    scrollable.style().overflow.y = Some(overflow.vertical);

    if axes.horizontal {
        scrollable.style().restrict_scroll_to_axis = Some(true);
    }

    match scroll_chrome {
        Some(scroll_chrome) => {
            let CodePanelScrollChrome {
                handle,
                scrollbar_opacity,
                on_activity,
                on_select,
                vertical_wheel_ownership,
            } = scroll_chrome;
            let stop_scroll_wheel_propagation =
                code_panel_stops_scroll_wheel_propagation(axes, vertical_wheel_ownership);
            let mut scroll_region = div()
                .relative()
                .w_full()
                .min_w(px(0.0))
                .when_some(content_height, |this, content_height| {
                    this.h(content_height)
                })
                .on_mouse_move({
                    let on_activity = on_activity.clone();
                    move |_, _, cx| {
                        if let Some(on_activity) = on_activity.as_ref() {
                            on_activity(cx);
                        }
                    }
                })
                .when_some(on_select, |this, on_select| {
                    this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        on_select(cx);
                        if !selection_enabled {
                            cx.stop_propagation();
                        }
                    })
                })
                .on_scroll_wheel({
                    let on_activity = on_activity.clone();
                    move |_, _, cx| {
                        if let Some(on_activity) = on_activity.as_ref() {
                            on_activity(cx);
                        }
                        if stop_scroll_wheel_propagation {
                            cx.stop_propagation();
                        }
                    }
                })
                .child(scrollable.track_scroll(&handle).child(content));
            if axes.vertical {
                if let Some(scrollbar) =
                    render_div_scrollbar(&handle, ScrollbarAxis::Vertical, scrollbar_opacity)
                {
                    scroll_region = scroll_region.child(scrollbar);
                }
            }
            if axes.horizontal {
                if let Some(scrollbar) =
                    render_div_scrollbar(&handle, ScrollbarAxis::Horizontal, scrollbar_opacity)
                {
                    scroll_region = scroll_region.child(scrollbar);
                }
            }
            scroll_region.into_any_element()
        }
        None => scrollable.child(content).into_any_element(),
    }
}

pub(crate) fn code_panel_scroll_overflow(
    axes: ScrollbarAxes,
    vertical_wheel_ownership: CodePanelVerticalWheelOwnership,
) -> CodePanelScrollOverflow {
    CodePanelScrollOverflow {
        horizontal: if axes.horizontal {
            Overflow::Scroll
        } else {
            Overflow::Visible
        },
        vertical: if axes.vertical {
            match vertical_wheel_ownership {
                CodePanelVerticalWheelOwnership::Panel => Overflow::Scroll,
                CodePanelVerticalWheelOwnership::Parent => Overflow::Hidden,
            }
        } else {
            Overflow::Visible
        },
    }
}

pub(crate) fn code_panel_stops_scroll_wheel_propagation(
    axes: ScrollbarAxes,
    vertical_wheel_ownership: CodePanelVerticalWheelOwnership,
) -> bool {
    axes.vertical && vertical_wheel_ownership == CodePanelVerticalWheelOwnership::Panel
}

pub(crate) fn code_panel_offset_after_scroll_delta(
    current_offset: Point<Pixels>,
    max_offset: Size<Pixels>,
    delta: Point<Pixels>,
) -> Point<Pixels> {
    let mut delta_x = delta.x;
    let mut delta_y = delta.y;
    if !delta_x.is_zero() && !delta_y.is_zero() {
        if delta_x.abs() > delta_y.abs() {
            delta_y = Pixels::ZERO;
        } else {
            delta_x = Pixels::ZERO;
        }
    }

    point(
        (current_offset.x + delta_x).clamp(-max_offset.width, px(0.0)),
        (current_offset.y + delta_y).clamp(-max_offset.height, px(0.0)),
    )
}

fn render_code_panel_resize_handle(
    background: Rgba,
    border: Rgba,
    content_height: Pixels,
    on_resize_start: CodePanelResizeStartAction,
) -> impl IntoElement {
    div()
        .relative()
        .w_full()
        .h(px(CODE_PANEL_RESIZE_HANDLE_HEIGHT))
        .bg(background)
        .border_t_1()
        .border_color(border)
        .cursor(CursorStyle::ResizeRow)
        .flex()
        .items_center()
        .justify_center()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _cx| {
                    window.on_mouse_event({
                        let on_resize_start = on_resize_start.clone();
                        move |event: &MouseDownEvent, _, _, cx| {
                            if !bounds.contains(&event.position) {
                                return;
                            }

                            let content_top = bounds.top() - content_height;
                            on_resize_start(content_top, content_height, event, cx);
                        }
                    });
                },
            )
            .size_full()
            .absolute()
            .top_0()
            .left_0(),
        )
        .child(
            div()
                .w(px(56.0))
                .h(px(4.0))
                .rounded_full()
                .bg(rgb(0x334155)),
        )
}

pub(crate) fn estimated_resizable_code_panel_height(
    display_text: &str,
    min_height: Pixels,
    max_height: Option<Pixels>,
) -> Pixels {
    let visible_line_count = display_text
        .replace("\r\n", "\n")
        .lines()
        .count()
        .max(1)
        .min(CODE_PANEL_VISIBLE_LINE_CAP);
    let estimated_height = px(CODE_PANEL_CONTENT_VERTICAL_PADDING)
        + (px(CODE_PANEL_ESTIMATED_LINE_HEIGHT) * visible_line_count as f32);
    clamp_resizable_code_panel_height(estimated_height, min_height, max_height)
}

fn resolved_resizable_code_panel_height(display_text: &str, resize: &CodePanelResize) -> Pixels {
    let desired_height = resize.current_height.unwrap_or_else(|| {
        estimated_resizable_code_panel_height(display_text, resize.min_height, resize.max_height)
    });

    clamp_resizable_code_panel_height(desired_height, resize.min_height, resize.max_height)
}

pub(crate) fn clamp_resizable_code_panel_height(
    height: Pixels,
    min_height: Pixels,
    max_height: Option<Pixels>,
) -> Pixels {
    let mut clamped = height.max(min_height.max(Pixels::ZERO));
    if let Some(max_height) = max_height {
        clamped = clamped.min(max_height.max(min_height));
    }
    clamped
}

pub(crate) fn smart_wrap_columns_for_width(available_width: Pixels, window: &Window) -> usize {
    if available_width <= px(0.0) {
        return 1;
    }

    let char_width = code_char_width(window).max(px(1.0));
    ((available_width / char_width).floor() as usize).max(1)
}

pub(crate) fn smart_wrap_for_columns(text: &str, columns: usize) -> String {
    if text.is_empty() || columns == 0 {
        return text.to_string();
    }

    split_code_panel_source_lines(text)
        .into_iter()
        .map(|line| wrap_line_segments_for_columns(line.as_str(), columns).join("\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodePanelDisplayLine {
    pub display_text: String,
    pub raw_text: String,
    pub break_before: usize,
}

pub(crate) fn code_panel_display_lines(
    source: &str,
    wrap_mode: CodePanelWrapMode,
) -> Vec<CodePanelDisplayLine> {
    let source_lines = split_code_panel_source_lines(source);
    let mut display_lines = Vec::new();

    for source_line in source_lines {
        let segments = match wrap_mode {
            CodePanelWrapMode::Smart { columns } if columns > 0 => {
                wrap_line_segments_for_columns(source_line.as_str(), columns)
            }
            CodePanelWrapMode::Smart { .. } | CodePanelWrapMode::NoWrap => {
                vec![source_line.clone()]
            }
        };

        for (segment_index, segment) in segments.into_iter().enumerate() {
            display_lines.push(CodePanelDisplayLine {
                display_text: segment.clone(),
                raw_text: segment,
                break_before: usize::from(segment_index == 0),
            });
        }
    }

    if display_lines.is_empty() {
        display_lines.push(CodePanelDisplayLine {
            display_text: String::new(),
            raw_text: String::new(),
            break_before: 0,
        });
    }

    display_lines
}

fn split_code_panel_source_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }

    text.replace("\r\n", "\n")
        .split('\n')
        .map(str::to_string)
        .collect()
}

fn code_char_width(window: &Window) -> Pixels {
    let mut font = window.text_style().font();
    font.family = CODE_FONT_FAMILY.into();
    let run = TextRun {
        len: 1,
        font,
        color: window.text_style().color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(
            "0".into(),
            rems(CODE_FONT_SIZE_REM).to_pixels(window.rem_size()),
            &[run],
            None,
        )
        .width
}

fn wrap_line_segments_for_columns(line: &str, columns: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }
    if columns == 0 {
        return vec![line.to_string()];
    }

    let chars: Vec<char> = line.chars().collect();
    let mut segments = Vec::new();
    let mut start = 0usize;

    while start < chars.len() {
        let remaining = chars.len() - start;
        if remaining <= columns {
            segments.push(chars[start..].iter().collect());
            break;
        }

        let window_end = start + columns;
        let break_index = (start..window_end)
            .rev()
            .find(|&index| matches!(chars[index], ' ' | ',' | ';'))
            .unwrap_or(window_end - 1);

        segments.push(chars[start..=break_index].iter().collect());
        start = break_index + 1;
    }

    segments
}
