use gpui::{Font, FontStyle, FontWeight, Rgba, SharedString, TextRun};

use crate::shell::syntax_highlighting::SyntaxTokenRole;

use super::{CODE_FONT_FAMILY, syntax_projection::CodePanelDisplaySpan};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CodePanelSyntaxTheme {
    pub plain_foreground: Rgba,
    pub structural_foreground: Rgba,
    pub heading_foreground: Rgba,
    pub emphasis_foreground: Rgba,
    pub strong_emphasis_foreground: Rgba,
    pub code_foreground: Rgba,
    pub link_foreground: Rgba,
    pub escape_foreground: Rgba,
}

impl CodePanelSyntaxTheme {
    pub(crate) fn plain(foreground: Rgba) -> Self {
        Self {
            plain_foreground: foreground,
            structural_foreground: foreground,
            heading_foreground: foreground,
            emphasis_foreground: foreground,
            strong_emphasis_foreground: foreground,
            code_foreground: foreground,
            link_foreground: foreground,
            escape_foreground: foreground,
        }
    }

    fn foreground_for_role(self, role: SyntaxTokenRole) -> Rgba {
        match role {
            SyntaxTokenRole::MarkupHeadingMarker | SyntaxTokenRole::SyntaxSectionHeader => {
                self.heading_foreground
            }
            SyntaxTokenRole::MarkupEmphasisDelimiter => self.emphasis_foreground,
            SyntaxTokenRole::MarkupStrongDelimiter => self.strong_emphasis_foreground,
            SyntaxTokenRole::MarkupCodeBlock | SyntaxTokenRole::MarkupCodeSpan => {
                self.code_foreground
            }
            SyntaxTokenRole::MarkupFenceInfo
            | SyntaxTokenRole::MarkupLinkText
            | SyntaxTokenRole::MarkupLinkDestination
            | SyntaxTokenRole::SyntaxKey => self.link_foreground,
            SyntaxTokenRole::SyntaxString => self.emphasis_foreground,
            SyntaxTokenRole::SyntaxBoolean
            | SyntaxTokenRole::SyntaxNull
            | SyntaxTokenRole::SyntaxDateTime
            | SyntaxTokenRole::SyntaxError => self.strong_emphasis_foreground,
            SyntaxTokenRole::SyntaxNumber => self.code_foreground,
            SyntaxTokenRole::Escape | SyntaxTokenRole::SyntaxEscape => self.escape_foreground,
            SyntaxTokenRole::MarkupQuoteMarker
            | SyntaxTokenRole::MarkupListMarker
            | SyntaxTokenRole::MarkupThematicBreak
            | SyntaxTokenRole::MarkupFenceDelimiter
            | SyntaxTokenRole::MarkupCodeSpanDelimiter
            | SyntaxTokenRole::MarkupImageMarker
            | SyntaxTokenRole::MarkupPunctuation
            | SyntaxTokenRole::MarkupHtml
            | SyntaxTokenRole::SyntaxStructuralPunctuation
            | SyntaxTokenRole::SyntaxComment
            | SyntaxTokenRole::SyntaxAssignment => self.structural_foreground,
        }
    }
}

#[allow(dead_code)]
pub(crate) fn code_panel_styled_text_parts(
    display_text: &str,
    syntax_spans: &[CodePanelDisplaySpan],
    syntax_theme: CodePanelSyntaxTheme,
) -> (String, Vec<TextRun>) {
    let layout_text = if display_text.is_empty() {
        " ".to_string()
    } else {
        display_text.to_string()
    };
    let mut runs = Vec::new();

    if display_text.is_empty() {
        runs.push(code_panel_text_run(
            layout_text.len(),
            syntax_theme.plain_foreground,
        ));
        return (layout_text, runs);
    }

    let mut sorted_spans: Vec<_> = syntax_spans.iter().collect();
    sorted_spans.sort_by_key(|span| (span.range.start, span.range.end));

    let mut cursor = 0usize;
    for span in sorted_spans {
        let range = span.range.clone();
        if range.end <= cursor || display_text.get(range.clone()).is_none() {
            continue;
        }

        if range.start > cursor {
            runs.push(code_panel_text_run(
                range.start - cursor,
                syntax_theme.plain_foreground,
            ));
        }

        let start = range.start.max(cursor);
        let end = range.end;
        if start < end {
            runs.push(code_panel_text_run(
                end - start,
                syntax_theme.foreground_for_role(span.role),
            ));
            cursor = end;
        }
    }

    if cursor < display_text.len() {
        runs.push(code_panel_text_run(
            display_text.len() - cursor,
            syntax_theme.plain_foreground,
        ));
    }

    if runs.is_empty() {
        runs.push(code_panel_text_run(
            layout_text.len(),
            syntax_theme.plain_foreground,
        ));
    }

    (layout_text, runs)
}

fn code_panel_text_run(len: usize, foreground: Rgba) -> TextRun {
    TextRun {
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
    }
}
