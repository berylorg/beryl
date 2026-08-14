use std::{hash::Hash, ops::Range};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SyntaxLanguage {
    Markdown,
    Json,
    Jsonl,
    Toml,
    WindowsIni,
}

impl SyntaxLanguage {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Toml => "toml",
            Self::WindowsIni => "ini",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SyntaxTokenRole {
    MarkupHeadingMarker,
    MarkupQuoteMarker,
    MarkupListMarker,
    MarkupThematicBreak,
    MarkupFenceDelimiter,
    MarkupFenceInfo,
    MarkupCodeBlock,
    MarkupCodeSpanDelimiter,
    MarkupCodeSpan,
    MarkupEmphasisDelimiter,
    MarkupStrongDelimiter,
    MarkupLinkText,
    MarkupLinkDestination,
    MarkupImageMarker,
    MarkupPunctuation,
    MarkupHtml,
    Escape,
    SyntaxStructuralPunctuation,
    SyntaxKey,
    SyntaxString,
    SyntaxNumber,
    SyntaxBoolean,
    SyntaxNull,
    SyntaxDateTime,
    SyntaxComment,
    SyntaxSectionHeader,
    SyntaxAssignment,
    SyntaxEscape,
    SyntaxError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntaxToken {
    role: SyntaxTokenRole,
    range: Range<usize>,
}

impl SyntaxToken {
    pub(crate) fn new(role: SyntaxTokenRole, range: Range<usize>) -> Self {
        Self { role, range }
    }

    pub(crate) fn role(&self) -> SyntaxTokenRole {
        self.role
    }

    pub(crate) fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    pub(crate) fn source_text<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.range.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntaxHighlight {
    language: Option<SyntaxLanguage>,
    tokens: Vec<SyntaxToken>,
}

impl SyntaxHighlight {
    pub(crate) fn new(language: SyntaxLanguage, mut tokens: Vec<SyntaxToken>) -> Self {
        tokens.retain(|token| !token.range.is_empty());
        tokens.sort_by_key(|token| (token.range.start, token.range.end));
        Self {
            language: Some(language),
            tokens,
        }
    }

    pub(crate) fn plain() -> Self {
        Self {
            language: None,
            tokens: Vec::new(),
        }
    }

    pub(crate) fn language(&self) -> Option<SyntaxLanguage> {
        self.language
    }

    pub(crate) fn tokens(&self) -> &[SyntaxToken] {
        &self.tokens
    }

    pub(crate) fn is_plain(&self) -> bool {
        self.language.is_none() && self.tokens.is_empty()
    }
}
