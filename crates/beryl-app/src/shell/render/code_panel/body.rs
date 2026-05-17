use gpui::{AnyElement, CursorStyle, Pixels, Rgba, StyledText, div, prelude::*, px};

use super::{
    CodePanelDisplayLine, CodePanelDisplaySpan, CodePanelDisplaySyntaxSpans, CodePanelScrollChrome,
    CodePanelSelectableLine, CodePanelSelection, CodePanelSyntaxTheme, CodePanelWrapMode,
    code_panel_styled_text_parts,
    scrolling::{ScrollbarAxes, render_scrollable_code_panel},
};

pub(super) fn render_code_panel_content(
    element_key: u64,
    display_lines: Vec<CodePanelDisplayLine>,
    syntax_spans: CodePanelDisplaySyntaxSpans,
    display_window: super::CodePanelDisplayWindow,
    max_display_text: String,
    wrap_mode: CodePanelWrapMode,
    foreground: Rgba,
    syntax_theme: CodePanelSyntaxTheme,
    scroll_chrome: Option<CodePanelScrollChrome>,
    content_height: Option<Pixels>,
    selection: Option<CodePanelSelection>,
) -> AnyElement {
    let selection_enabled = selection.is_some();
    match (wrap_mode, content_height) {
        (CodePanelWrapMode::Smart { .. }, None) => render_code_panel_text(
            display_lines,
            syntax_spans,
            display_window,
            max_display_text,
            wrap_mode,
            foreground,
            syntax_theme,
            selection,
            false,
        ),
        (CodePanelWrapMode::Smart { .. }, Some(content_height)) => render_scrollable_code_panel(
            element_key,
            render_code_panel_text(
                display_lines,
                syntax_spans,
                display_window,
                max_display_text,
                wrap_mode,
                foreground,
                syntax_theme,
                selection,
                true,
            ),
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
            render_code_panel_text(
                display_lines,
                syntax_spans,
                display_window,
                max_display_text,
                wrap_mode,
                foreground,
                syntax_theme,
                selection,
                false,
            ),
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
            render_code_panel_text(
                display_lines,
                syntax_spans,
                display_window,
                max_display_text,
                wrap_mode,
                foreground,
                syntax_theme,
                selection,
                true,
            ),
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
    syntax_spans: CodePanelDisplaySyntaxSpans,
    display_window: super::CodePanelDisplayWindow,
    max_display_text: String,
    wrap_mode: CodePanelWrapMode,
    foreground: Rgba,
    syntax_theme: CodePanelSyntaxTheme,
    selection: Option<CodePanelSelection>,
    fill_height: bool,
) -> AnyElement {
    let display_line_count = display_window.display_line_count;
    let mut text = div()
        .w_full()
        .min_w(px(0.0))
        .when(fill_height, |this| this.min_h_full())
        .flex()
        .flex_col()
        .gap_0();

    if matches!(wrap_mode, CodePanelWrapMode::NoWrap) && !max_display_text.is_empty() {
        text = text.child(render_code_panel_width_sentinel(
            max_display_text,
            &syntax_theme,
        ));
    }
    if display_window.top_spacer_height > Pixels::ZERO {
        text = text.child(
            div()
                .flex_none()
                .h(display_window.top_spacer_height)
                .w_full(),
        );
    }
    text = text.children(display_lines.into_iter().enumerate().map(|(offset, line)| {
        let display_line_index = display_window.range.start + offset;
        render_code_panel_line(
            display_line_index,
            display_line_count,
            line,
            syntax_spans.line_spans(offset),
            wrap_mode,
            foreground,
            &syntax_theme,
            selection.clone(),
        )
    }));
    if display_window.bottom_spacer_height > Pixels::ZERO {
        text = text.child(
            div()
                .flex_none()
                .h(display_window.bottom_spacer_height)
                .w_full(),
        );
    }

    text.into_any_element()
}

fn render_code_panel_line(
    display_line_index: usize,
    display_line_count: usize,
    line: CodePanelDisplayLine,
    syntax_spans: Vec<CodePanelDisplaySpan>,
    wrap_mode: CodePanelWrapMode,
    foreground: Rgba,
    syntax_theme: &CodePanelSyntaxTheme,
    selection: Option<CodePanelSelection>,
) -> AnyElement {
    let display_text_len = line.display_text.len();
    let (layout_text, runs) = code_panel_styled_text_parts(
        line.display_text.as_str(),
        syntax_spans.as_slice(),
        syntax_theme,
    );
    let styled_text = StyledText::new(layout_text).with_runs(runs);
    let text_layout = styled_text.layout().clone();
    let prepaint_action = selection.map(|selection| {
        (selection.line_prepaint_action)(CodePanelSelectableLine {
            display_line_index,
            display_line_count,
            raw_text: line.raw_text,
            break_before: line.break_before,
            display_text_len,
        })
    });

    let line = div()
        .w_full()
        .min_w(px(0.0))
        .text_size(px(syntax_theme.font_size()))
        .line_height(px(syntax_theme.line_height()))
        .font_family(syntax_theme.font_family().to_string())
        .font_weight(syntax_theme.font_weight())
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

fn render_code_panel_width_sentinel(
    max_display_text: String,
    syntax_theme: &CodePanelSyntaxTheme,
) -> impl IntoElement {
    div()
        .flex_none()
        .h(px(0.0))
        .overflow_hidden()
        .text_size(px(syntax_theme.font_size()))
        .line_height(px(syntax_theme.line_height()))
        .font_family(syntax_theme.font_family().to_string())
        .font_weight(syntax_theme.font_weight())
        .whitespace_nowrap()
        .child(max_display_text)
}
