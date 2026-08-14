use gpui::{AnyElement, div, prelude::*};

use crate::shell::transcript_markdown::{
    InlineRenderFragment, InlineRenderLine, InlineRenderRole, InlineRenderStyle, MarkdownSourceSpan,
};

pub(super) fn fallback_inline_lines(source: &str, role: InlineRenderRole) -> Vec<InlineRenderLine> {
    fallback_inline_lines_inner(source, role, None)
}

pub(super) fn fallback_inline_lines_with_source_span(
    source: &str,
    role: InlineRenderRole,
    source_start: usize,
) -> Vec<InlineRenderLine> {
    fallback_inline_lines_inner(source, role, Some(source_start))
}

fn fallback_inline_lines_inner(
    source: &str,
    role: InlineRenderRole,
    source_start: Option<usize>,
) -> Vec<InlineRenderLine> {
    let style = InlineRenderStyle {
        role,
        link: false,
        emphasis: false,
        strong: false,
        fallback: true,
        atom: false,
    };

    if source.is_empty() {
        return vec![InlineRenderLine {
            fragments: Vec::new(),
        }];
    }

    let source = source.replace("\r\n", "\n").replace('\r', "\n");
    let mut line_source_start = source_start;

    source
        .split('\n')
        .map(|line| {
            let display_source_span = line_source_start
                .and_then(|start| MarkdownSourceSpan::new(start, start + line.len()));
            line_source_start = line_source_start.map(|start| start + line.len() + 1);
            if line.is_empty() {
                InlineRenderLine {
                    fragments: Vec::new(),
                }
            } else {
                InlineRenderLine {
                    fragments: vec![InlineRenderFragment {
                        text: line.to_string(),
                        style,
                        source_span: display_source_span,
                        display_source_span,
                        copy_prefix: String::new(),
                        copy_suffix: String::new(),
                        copy_replacement: None,
                    }],
                }
            }
        })
        .collect()
}

pub(super) fn empty_line() -> AnyElement {
    div().text_sm().child(" ").into_any_element()
}
