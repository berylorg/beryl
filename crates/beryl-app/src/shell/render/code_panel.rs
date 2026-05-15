use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use gpui::{
    AnyElement, App, Bounds, CursorStyle, FontWeight, MouseDownEvent, Overflow, Pixels, Rgba,
    ScrollHandle, TextLayout, TextRun, Window, canvas, div, prelude::*, px, rems, rgb,
};

#[path = "code_panel/body.rs"]
mod body;
#[path = "code_panel/projection.rs"]
mod projection;
#[path = "code_panel/scrolling.rs"]
mod scrolling;
#[path = "code_panel/styled_text.rs"]
mod styled_text;
#[path = "code_panel/syntax_projection.rs"]
mod syntax_projection;

use super::scrollbars::ScrollbarVisibilityPolicy;
use crate::shell::{layout, syntax_highlighting::SyntaxHighlight};

use body::render_code_panel_content;
#[allow(unused_imports)]
pub(crate) use projection::{
    CodePanelDisplayLine, CodePanelDisplayProjection, CodePanelDisplayWindow,
    code_panel_display_lines, code_panel_display_window, smart_wrap_for_columns,
};
#[allow(unused_imports)]
pub(crate) use scrolling::{
    ScrollbarAxes, code_panel_offset_after_scroll_delta, code_panel_scroll_overflow,
    code_panel_stops_scroll_wheel_propagation,
};
pub(crate) use styled_text::{CodePanelSyntaxTheme, code_panel_styled_text_parts};
#[allow(unused_imports)]
pub(crate) use syntax_projection::{
    CodePanelDisplaySpan, CodePanelDisplaySyntaxSpans, code_panel_display_line_syntax_spans,
    code_panel_display_line_syntax_spans_for_window,
};

pub(crate) const CODE_FONT_FAMILY: &str = "Consolas";
const CODE_FONT_SIZE_REM: f32 = 0.875;
const CODE_PANEL_RESIZE_HANDLE_HEIGHT: f32 = 10.0;
const CODE_PANEL_ESTIMATED_LINE_HEIGHT: f32 = 20.0;
const CODE_PANEL_VISIBLE_LINE_CAP: usize = 12;
const CODE_PANEL_CONTENT_VERTICAL_PADDING: f32 = 24.0;
const CODE_PANEL_OVERSCAN_LINES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CodePanelWrapMode {
    Smart { columns: usize },
    NoWrap,
}

#[derive(Clone, Debug)]
pub(crate) enum CodePanelDisplayProjectionInput {
    BuildInline,
    Pending,
    Ready(Arc<CodePanelDisplayProjection>),
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
    pub display_line_index: usize,
    pub display_line_count: usize,
    pub raw_text: String,
    pub break_before: usize,
    pub display_text_len: usize,
}

#[derive(Clone)]
pub(crate) struct CodePanelScrollChrome {
    pub handle: ScrollHandle,
    pub scrollbar_visibility: ScrollbarVisibilityPolicy,
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
    display_projection: CodePanelDisplayProjectionInput,
    chrome: CodePanelChrome,
    foreground: Rgba,
    syntax_theme: Option<CodePanelSyntaxTheme>,
    syntax_highlight: Option<&SyntaxHighlight>,
    header: Option<CodePanelHeader>,
    scroll_chrome: Option<CodePanelScrollChrome>,
    resize: Option<CodePanelResize>,
    selection: Option<CodePanelSelection>,
) -> AnyElement {
    let element_key = code_panel_element_key(element_id.clone(), source, language);
    let inline_projection;
    let display_projection = match display_projection {
        CodePanelDisplayProjectionInput::BuildInline => {
            inline_projection = CodePanelDisplayProjection::new(source, wrap_mode);
            Some(&inline_projection)
        }
        CodePanelDisplayProjectionInput::Pending => None,
        CodePanelDisplayProjectionInput::Ready(ref projection) => Some(projection.as_ref()),
    };
    let syntax_theme = syntax_theme.unwrap_or_else(|| CodePanelSyntaxTheme::plain(foreground));
    let plain_syntax_highlight;
    let syntax_highlight = match syntax_highlight {
        Some(syntax_highlight) => syntax_highlight,
        None => {
            plain_syntax_highlight = SyntaxHighlight::plain();
            &plain_syntax_highlight
        }
    };
    let content_height = resize.as_ref().map(|resize| {
        display_projection.map_or_else(
            || resolved_pending_code_panel_height(resize),
            |projection| {
                resolved_resizable_code_panel_height_for_line_count(
                    projection.display_line_count(),
                    resize,
                )
            },
        )
    });
    let display_window = display_projection.map_or_else(
        || CodePanelDisplayWindow::pending(content_height.unwrap_or(Pixels::ZERO)),
        |projection| {
            projection.display_window(
                content_height,
                scroll_chrome.as_ref(),
                CODE_PANEL_OVERSCAN_LINES,
            )
        },
    );
    let display_lines = display_projection.map_or_else(Vec::new, |projection| {
        projection.display_lines_for_window(display_window.range.clone())
    });
    let syntax_spans = if display_projection.is_some() {
        CodePanelDisplaySyntaxSpans::new_for_window(
            display_lines.as_slice(),
            syntax_highlight.tokens(),
        )
    } else {
        CodePanelDisplaySyntaxSpans::Plain
    };
    let max_display_text = display_projection
        .map(|projection| projection.max_display_text().to_string())
        .unwrap_or_default();

    let content = render_code_panel_content(
        element_key,
        display_lines,
        syntax_spans,
        display_window,
        max_display_text,
        wrap_mode,
        foreground,
        syntax_theme,
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

#[allow(dead_code)]
pub(crate) fn estimated_resizable_code_panel_height(
    display_text: &str,
    min_height: Pixels,
    max_height: Option<Pixels>,
) -> Pixels {
    let visible_line_count = display_text.replace("\r\n", "\n").lines().count().max(1);
    estimated_resizable_code_panel_height_for_line_count(visible_line_count, min_height, max_height)
}

fn resolved_resizable_code_panel_height_for_line_count(
    display_line_count: usize,
    resize: &CodePanelResize,
) -> Pixels {
    let desired_height = resize.current_height.unwrap_or_else(|| {
        estimated_resizable_code_panel_height_for_line_count(
            display_line_count,
            resize.min_height,
            resize.max_height,
        )
    });

    clamp_resizable_code_panel_height(desired_height, resize.min_height, resize.max_height)
}

fn resolved_pending_code_panel_height(resize: &CodePanelResize) -> Pixels {
    let desired_height = resize.current_height.unwrap_or_else(|| {
        estimated_resizable_code_panel_height_for_line_count(
            CODE_PANEL_VISIBLE_LINE_CAP,
            resize.min_height,
            resize.max_height,
        )
    });

    clamp_resizable_code_panel_height(desired_height, resize.min_height, resize.max_height)
}

fn estimated_resizable_code_panel_height_for_line_count(
    display_line_count: usize,
    min_height: Pixels,
    max_height: Option<Pixels>,
) -> Pixels {
    let visible_line_count = display_line_count.max(1).min(CODE_PANEL_VISIBLE_LINE_CAP);
    let estimated_height = px(CODE_PANEL_CONTENT_VERTICAL_PADDING)
        + (px(CODE_PANEL_ESTIMATED_LINE_HEIGHT) * visible_line_count as f32);
    clamp_resizable_code_panel_height(estimated_height, min_height, max_height)
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
