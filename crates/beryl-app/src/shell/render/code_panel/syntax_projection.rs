use std::ops::Range;

use crate::shell::syntax_highlighting::{SyntaxToken, SyntaxTokenRole};

use super::projection::CodePanelDisplayLine;

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct CodePanelDisplaySpan {
    pub role: SyntaxTokenRole,
    pub range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CodePanelDisplaySyntaxSpans {
    Plain,
    Highlighted(Vec<Vec<CodePanelDisplaySpan>>),
}

impl CodePanelDisplaySyntaxSpans {
    #[allow(dead_code)]
    pub(crate) fn new(display_lines: &[CodePanelDisplayLine], tokens: &[SyntaxToken]) -> Self {
        if tokens.is_empty() {
            Self::Plain
        } else {
            Self::Highlighted(code_panel_display_line_syntax_spans(display_lines, tokens))
        }
    }

    pub(crate) fn new_for_window(
        display_lines: &[CodePanelDisplayLine],
        tokens: &[SyntaxToken],
    ) -> Self {
        if tokens.is_empty() || display_lines.is_empty() {
            Self::Plain
        } else {
            Self::Highlighted(code_panel_display_line_syntax_spans_for_window(
                display_lines,
                tokens,
            ))
        }
    }

    #[allow(dead_code)]
    pub(crate) fn is_plain(&self) -> bool {
        matches!(self, Self::Plain)
    }

    pub(super) fn line_spans(&self, index: usize) -> Vec<CodePanelDisplaySpan> {
        match self {
            Self::Plain => Vec::new(),
            Self::Highlighted(spans) => spans.get(index).cloned().unwrap_or_default(),
        }
    }
}

#[allow(dead_code)]
pub(crate) fn code_panel_display_line_syntax_spans(
    display_lines: &[CodePanelDisplayLine],
    tokens: &[SyntaxToken],
) -> Vec<Vec<CodePanelDisplaySpan>> {
    let mut spans = vec![Vec::new(); display_lines.len()];

    for token in tokens {
        let token_range = token.range();
        if token_range.is_empty() {
            continue;
        }

        let mut line_index =
            display_lines.partition_point(|line| line.source_range.end <= token_range.start);
        while let Some(line) = display_lines.get(line_index) {
            if line.source_range.start >= token_range.end {
                break;
            }

            let Some(overlap) = intersect_ranges(token_range.clone(), line.source_range.clone())
            else {
                line_index += 1;
                continue;
            };
            let local_range =
                overlap.start - line.source_range.start..overlap.end - line.source_range.start;
            if line.display_text.get(local_range.clone()).is_none() {
                line_index += 1;
                continue;
            }
            spans[line_index].push(CodePanelDisplaySpan {
                role: token.role(),
                range: local_range,
            });
            line_index += 1;
        }
    }

    for line_spans in &mut spans {
        line_spans.sort_by_key(|span| (span.range.start, span.range.end));
    }

    spans
}

#[allow(dead_code)]
pub(crate) fn code_panel_display_line_syntax_spans_for_window(
    display_lines: &[CodePanelDisplayLine],
    tokens: &[SyntaxToken],
) -> Vec<Vec<CodePanelDisplaySpan>> {
    let mut spans = vec![Vec::new(); display_lines.len()];
    let Some(first_line) = display_lines.first() else {
        return spans;
    };
    let Some(last_line) = display_lines.last() else {
        return spans;
    };
    let window_start = first_line.source_range.start;
    let window_end = last_line.source_range.end;
    if window_start >= window_end {
        return spans;
    }

    let mut token_index = tokens.partition_point(|token| token.range().end <= window_start);
    while let Some(token) = tokens.get(token_index) {
        let token_range = token.range();
        if token_range.start >= window_end {
            break;
        }
        if token_range.is_empty() {
            token_index += 1;
            continue;
        }

        let mut line_index =
            display_lines.partition_point(|line| line.source_range.end <= token_range.start);
        while let Some(line) = display_lines.get(line_index) {
            if line.source_range.start >= token_range.end {
                break;
            }

            let Some(overlap) = intersect_ranges(token_range.clone(), line.source_range.clone())
            else {
                line_index += 1;
                continue;
            };
            let local_range =
                overlap.start - line.source_range.start..overlap.end - line.source_range.start;
            if line.display_text.get(local_range.clone()).is_some() {
                spans[line_index].push(CodePanelDisplaySpan {
                    role: token.role(),
                    range: local_range,
                });
            }
            line_index += 1;
        }
        token_index += 1;
    }

    for line_spans in &mut spans {
        line_spans.sort_by_key(|span| (span.range.start, span.range.end));
    }

    spans
}

fn intersect_ranges(left: Range<usize>, right: Range<usize>) -> Option<Range<usize>> {
    let start = left.start.max(right.start);
    let end = left.end.min(right.end);
    (start < end).then_some(start..end)
}
