use gpui::{Font, FontStyle, FontWeight, Rgba, SharedString, TextRun};

use crate::shell::syntax_highlighting::SyntaxTokenRole;

use super::{
    DEFAULT_CODE_FONT_FAMILY, DEFAULT_CODE_FONT_SIZE, syntax_projection::CodePanelDisplaySpan,
};

const SYNTAX_TOKEN_ROLE_COUNT: usize = 29;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CodePanelSyntaxTheme {
    plain_foreground: Rgba,
    role_foregrounds: [Rgba; SYNTAX_TOKEN_ROLE_COUNT],
    font_family: String,
    font_size: f32,
    font_weight: FontWeight,
}

impl CodePanelSyntaxTheme {
    pub(crate) fn plain(foreground: Rgba) -> Self {
        Self::from_role_foregrounds(
            foreground,
            DEFAULT_CODE_FONT_FAMILY,
            DEFAULT_CODE_FONT_SIZE,
            FontWeight(400.0),
            |_| foreground,
        )
    }

    pub(crate) fn from_role_foregrounds(
        plain_foreground: Rgba,
        font_family: impl Into<String>,
        font_size: f32,
        font_weight: FontWeight,
        foreground_for_role: impl Fn(SyntaxTokenRole) -> Rgba,
    ) -> Self {
        let mut role_foregrounds = [plain_foreground; SYNTAX_TOKEN_ROLE_COUNT];
        for role in SYNTAX_TOKEN_ROLES {
            role_foregrounds[syntax_token_role_index(*role)] = foreground_for_role(*role);
        }
        Self {
            plain_foreground,
            role_foregrounds,
            font_family: font_family.into(),
            font_size,
            font_weight,
        }
    }

    pub(crate) fn plain_foreground(&self) -> Rgba {
        self.plain_foreground
    }

    pub(crate) fn foreground_for_role(&self, role: SyntaxTokenRole) -> Rgba {
        self.role_foregrounds[syntax_token_role_index(role)]
    }

    pub(crate) fn font_family(&self) -> &str {
        self.font_family.as_str()
    }

    pub(crate) fn font_size(&self) -> f32 {
        self.font_size
    }

    pub(crate) fn font_weight(&self) -> FontWeight {
        self.font_weight
    }

    pub(crate) fn line_height(&self) -> f32 {
        (self.font_size + 7.0).max(self.font_size)
    }
}

#[allow(dead_code)]
pub(crate) fn code_panel_styled_text_parts(
    display_text: &str,
    syntax_spans: &[CodePanelDisplaySpan],
    syntax_theme: &CodePanelSyntaxTheme,
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
            syntax_theme.plain_foreground(),
            syntax_theme,
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
                syntax_theme.plain_foreground(),
                syntax_theme,
            ));
        }

        let start = range.start.max(cursor);
        let end = range.end;
        if start < end {
            runs.push(code_panel_text_run(
                end - start,
                syntax_theme.foreground_for_role(span.role),
                syntax_theme,
            ));
            cursor = end;
        }
    }

    if cursor < display_text.len() {
        runs.push(code_panel_text_run(
            display_text.len() - cursor,
            syntax_theme.plain_foreground(),
            syntax_theme,
        ));
    }

    if runs.is_empty() {
        runs.push(code_panel_text_run(
            layout_text.len(),
            syntax_theme.plain_foreground(),
            syntax_theme,
        ));
    }

    (layout_text, runs)
}

fn code_panel_text_run(
    len: usize,
    foreground: Rgba,
    syntax_theme: &CodePanelSyntaxTheme,
) -> TextRun {
    TextRun {
        len,
        font: Font {
            family: SharedString::from(syntax_theme.font_family().to_string()),
            features: Default::default(),
            fallbacks: None,
            weight: syntax_theme.font_weight(),
            style: FontStyle::Normal,
        },
        color: foreground.into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

const SYNTAX_TOKEN_ROLES: &[SyntaxTokenRole] = &[
    SyntaxTokenRole::MarkupHeadingMarker,
    SyntaxTokenRole::MarkupQuoteMarker,
    SyntaxTokenRole::MarkupListMarker,
    SyntaxTokenRole::MarkupThematicBreak,
    SyntaxTokenRole::MarkupFenceDelimiter,
    SyntaxTokenRole::MarkupFenceInfo,
    SyntaxTokenRole::MarkupCodeBlock,
    SyntaxTokenRole::MarkupCodeSpanDelimiter,
    SyntaxTokenRole::MarkupCodeSpan,
    SyntaxTokenRole::MarkupEmphasisDelimiter,
    SyntaxTokenRole::MarkupStrongDelimiter,
    SyntaxTokenRole::MarkupLinkText,
    SyntaxTokenRole::MarkupLinkDestination,
    SyntaxTokenRole::MarkupImageMarker,
    SyntaxTokenRole::MarkupPunctuation,
    SyntaxTokenRole::MarkupHtml,
    SyntaxTokenRole::Escape,
    SyntaxTokenRole::SyntaxStructuralPunctuation,
    SyntaxTokenRole::SyntaxKey,
    SyntaxTokenRole::SyntaxString,
    SyntaxTokenRole::SyntaxNumber,
    SyntaxTokenRole::SyntaxBoolean,
    SyntaxTokenRole::SyntaxNull,
    SyntaxTokenRole::SyntaxDateTime,
    SyntaxTokenRole::SyntaxComment,
    SyntaxTokenRole::SyntaxSectionHeader,
    SyntaxTokenRole::SyntaxAssignment,
    SyntaxTokenRole::SyntaxEscape,
    SyntaxTokenRole::SyntaxError,
];

fn syntax_token_role_index(role: SyntaxTokenRole) -> usize {
    match role {
        SyntaxTokenRole::MarkupHeadingMarker => 0,
        SyntaxTokenRole::MarkupQuoteMarker => 1,
        SyntaxTokenRole::MarkupListMarker => 2,
        SyntaxTokenRole::MarkupThematicBreak => 3,
        SyntaxTokenRole::MarkupFenceDelimiter => 4,
        SyntaxTokenRole::MarkupFenceInfo => 5,
        SyntaxTokenRole::MarkupCodeBlock => 6,
        SyntaxTokenRole::MarkupCodeSpanDelimiter => 7,
        SyntaxTokenRole::MarkupCodeSpan => 8,
        SyntaxTokenRole::MarkupEmphasisDelimiter => 9,
        SyntaxTokenRole::MarkupStrongDelimiter => 10,
        SyntaxTokenRole::MarkupLinkText => 11,
        SyntaxTokenRole::MarkupLinkDestination => 12,
        SyntaxTokenRole::MarkupImageMarker => 13,
        SyntaxTokenRole::MarkupPunctuation => 14,
        SyntaxTokenRole::MarkupHtml => 15,
        SyntaxTokenRole::Escape => 16,
        SyntaxTokenRole::SyntaxStructuralPunctuation => 17,
        SyntaxTokenRole::SyntaxKey => 18,
        SyntaxTokenRole::SyntaxString => 19,
        SyntaxTokenRole::SyntaxNumber => 20,
        SyntaxTokenRole::SyntaxBoolean => 21,
        SyntaxTokenRole::SyntaxNull => 22,
        SyntaxTokenRole::SyntaxDateTime => 23,
        SyntaxTokenRole::SyntaxComment => 24,
        SyntaxTokenRole::SyntaxSectionHeader => 25,
        SyntaxTokenRole::SyntaxAssignment => 26,
        SyntaxTokenRole::SyntaxEscape => 27,
        SyntaxTokenRole::SyntaxError => 28,
    }
}
