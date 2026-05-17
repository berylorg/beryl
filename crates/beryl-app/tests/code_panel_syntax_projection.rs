#[path = "../src/shell/layout.rs"]
pub(crate) mod layout;

#[path = "../src/shell/syntax_highlighting.rs"]
pub(crate) mod syntax_highlighting;

mod shell {
    pub(crate) use crate::layout;
    pub(crate) use crate::syntax_highlighting;

    pub(crate) struct ShellRenderStyleSnapshot;

    impl ShellRenderStyleSnapshot {
        pub(crate) fn scrollbar_thumb_color(&self) -> u32 {
            0x000000
        }
    }

    pub(crate) struct ShellView;

    impl ShellView {
        pub(crate) fn scrollbar_thumb_color(&self) -> u32 {
            0x000000
        }
    }
}

#[path = "../src/shell/render/code_panel.rs"]
mod code_panel;

#[path = "../src/shell/render/scrollbars.rs"]
mod scrollbars;

use std::ops::Range;

use code_panel::{CodePanelDisplaySpan, CodePanelWrapMode};
use syntax_highlighting::{SyntaxToken, SyntaxTokenRole, highlight_syntax};

fn token(role: SyntaxTokenRole, range: Range<usize>) -> SyntaxToken {
    SyntaxToken::new(role, range)
}

fn span(role: SyntaxTokenRole, range: Range<usize>) -> CodePanelDisplaySpan {
    CodePanelDisplaySpan { role, range }
}

#[test]
fn projects_source_span_inside_single_display_line() {
    let display_lines = code_panel::code_panel_display_lines(
        "alpha **bold**",
        CodePanelWrapMode::Smart { columns: 80 },
    );
    let spans = code_panel::code_panel_display_line_syntax_spans(
        &display_lines,
        &[token(SyntaxTokenRole::MarkupStrongDelimiter, 6..8)],
    );

    assert_eq!(
        spans,
        vec![vec![span(SyntaxTokenRole::MarkupStrongDelimiter, 6..8)]]
    );
}

#[test]
fn projects_token_across_smart_wrap_segments() {
    let display_lines =
        code_panel::code_panel_display_lines("abcdef", CodePanelWrapMode::Smart { columns: 2 });
    let spans = code_panel::code_panel_display_line_syntax_spans(
        &display_lines,
        &[token(SyntaxTokenRole::MarkupCodeSpan, 1..5)],
    );

    assert_eq!(
        spans,
        vec![
            vec![span(SyntaxTokenRole::MarkupCodeSpan, 1..2)],
            vec![span(SyntaxTokenRole::MarkupCodeSpan, 0..2)],
            vec![span(SyntaxTokenRole::MarkupCodeSpan, 0..1)],
        ]
    );
}

#[test]
fn projects_token_across_no_wrap_lines() {
    let display_lines =
        code_panel::code_panel_display_lines("alpha\nbeta", CodePanelWrapMode::NoWrap);
    let spans = code_panel::code_panel_display_line_syntax_spans(
        &display_lines,
        &[token(SyntaxTokenRole::MarkupCodeBlock, 2..8)],
    );

    assert_eq!(
        spans,
        vec![
            vec![span(SyntaxTokenRole::MarkupCodeBlock, 2..5)],
            vec![span(SyntaxTokenRole::MarkupCodeBlock, 0..2)],
        ]
    );
}

#[test]
fn projects_crlf_source_ranges_without_drift() {
    let display_lines =
        code_panel::code_panel_display_lines("a\r\n**b**", CodePanelWrapMode::NoWrap);
    let spans = code_panel::code_panel_display_line_syntax_spans(
        &display_lines,
        &[token(SyntaxTokenRole::MarkupStrongDelimiter, 3..5)],
    );

    assert_eq!(display_lines[0].source_range, 0..1);
    assert_eq!(display_lines[1].source_range, 3..8);
    assert_eq!(
        spans,
        vec![
            vec![],
            vec![span(SyntaxTokenRole::MarkupStrongDelimiter, 0..2)],
        ]
    );
}

#[test]
fn preserves_empty_display_lines_without_spans() {
    let display_lines = code_panel::code_panel_display_lines("a\n\nb", CodePanelWrapMode::NoWrap);
    let spans = code_panel::code_panel_display_line_syntax_spans(
        &display_lines,
        &[token(SyntaxTokenRole::MarkupCodeBlock, 0..4)],
    );

    assert_eq!(display_lines[1].display_text, "");
    assert_eq!(display_lines[1].source_range, 2..2);
    assert!(spans[1].is_empty());
}

#[test]
fn syntax_projection_does_not_change_display_lines_used_for_selection() {
    let source = "alpha **bold** gamma";
    let display_lines =
        code_panel::code_panel_display_lines(source, CodePanelWrapMode::Smart { columns: 8 });
    let before = display_lines.clone();
    let highlight = highlight_syntax(source, Some("markdown"));
    let spans =
        code_panel::code_panel_display_line_syntax_spans(&display_lines, highlight.tokens());

    assert_eq!(display_lines, before);
    assert_eq!(
        display_lines
            .iter()
            .map(|line| (
                line.raw_text.as_str(),
                line.break_before,
                line.source_range.clone()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("alpha ", 1, 0..6),
            ("**bold**", 0, 6..14),
            (" gamma", 0, 14..20),
        ]
    );
    assert!(spans.iter().any(|line| !line.is_empty()));
}

#[test]
fn projects_multiline_markdown_constructs_after_wrapping() {
    let source = "```markdown\n# heading\n```\n";
    let highlight = highlight_syntax(source, Some("markdown"));
    let display_lines =
        code_panel::code_panel_display_lines(source, CodePanelWrapMode::Smart { columns: 4 });
    let spans =
        code_panel::code_panel_display_line_syntax_spans(&display_lines, highlight.tokens());

    assert_eq!(
        spans,
        vec![
            vec![
                span(SyntaxTokenRole::MarkupFenceDelimiter, 0..3),
                span(SyntaxTokenRole::MarkupFenceInfo, 3..4),
            ],
            vec![span(SyntaxTokenRole::MarkupFenceInfo, 0..4)],
            vec![span(SyntaxTokenRole::MarkupFenceInfo, 0..3)],
            vec![span(SyntaxTokenRole::MarkupCodeBlock, 0..2)],
            vec![span(SyntaxTokenRole::MarkupCodeBlock, 0..4)],
            vec![span(SyntaxTokenRole::MarkupCodeBlock, 0..3)],
            vec![span(SyntaxTokenRole::MarkupFenceDelimiter, 0..3)],
            vec![],
        ]
    );
}

#[test]
fn projects_syntax_only_for_visible_display_window() {
    let source = (0..1_000)
        .map(|index| format!("line {index:04} **bold**"))
        .collect::<Vec<_>>()
        .join("\n");
    let projection =
        code_panel::CodePanelDisplayProjection::new(source.as_str(), CodePanelWrapMode::NoWrap);
    let visible_lines = projection.display_lines_for_window(500..506);
    let inside_range = visible_lines[2].source_range.start + 10..visible_lines[2].source_range.end;
    let outside_before = projection.display_lines()[10].source_range.clone();
    let outside_after = projection.display_lines()[900].source_range.clone();

    let spans = code_panel::code_panel_display_line_syntax_spans_for_window(
        visible_lines.as_slice(),
        &[
            token(SyntaxTokenRole::MarkupCodeSpan, outside_before),
            token(SyntaxTokenRole::MarkupStrongDelimiter, inside_range.clone()),
            token(SyntaxTokenRole::MarkupCodeSpan, outside_after),
        ],
    );

    assert_eq!(spans.len(), visible_lines.len());
    assert!(spans[0].is_empty());
    assert!(spans[1].is_empty());
    assert_eq!(
        spans[2],
        vec![span(
            SyntaxTokenRole::MarkupStrongDelimiter,
            inside_range.start - visible_lines[2].source_range.start
                ..inside_range.end - visible_lines[2].source_range.start
        )]
    );
    assert!(spans[3].is_empty());
    assert!(spans[4].is_empty());
    assert!(spans[5].is_empty());
}
