use std::ops::Range;

use taplo::{
    parser::parse,
    rowan::{NodeOrToken, TextRange},
    syntax::{SyntaxKind, SyntaxToken},
};

use super::model::{
    SyntaxHighlight, SyntaxLanguage, SyntaxToken as HighlightToken, SyntaxTokenRole,
};

pub(crate) fn highlight_toml(source: &str) -> SyntaxHighlight {
    let parse = parse(source);
    let parse_error_ranges = parse
        .errors
        .iter()
        .filter_map(|error| source_range(source, error.range))
        .collect::<Vec<_>>();
    let root = parse.into_syntax();
    let mut tokens = TomlTokenBuilder {
        source,
        tokens: Vec::new(),
    };

    for element in root.descendants_with_tokens() {
        let NodeOrToken::Token(token) = element else {
            continue;
        };
        let Some(range) = source_range(source, token.text_range()) else {
            continue;
        };
        let role = if token.kind() == SyntaxKind::ERROR
            || (is_string_kind(token.kind())
                && parse_error_ranges
                    .iter()
                    .any(|error_range| ranges_overlap(error_range, &range)))
        {
            Some(SyntaxTokenRole::SyntaxError)
        } else {
            role_for_toml_token(&token)
        };

        match role {
            Some(role) => tokens.push_toml_token(role, token.kind(), range),
            None => {}
        }
    }

    for range in parse_error_ranges {
        if !tokens.any_token_overlaps(&range) {
            tokens.push(SyntaxTokenRole::SyntaxError, range);
        }
    }

    SyntaxHighlight::new(SyntaxLanguage::Toml, tokens.tokens)
}

struct TomlTokenBuilder<'a> {
    source: &'a str,
    tokens: Vec<HighlightToken>,
}

impl TomlTokenBuilder<'_> {
    fn push_toml_token(&mut self, role: SyntaxTokenRole, kind: SyntaxKind, range: Range<usize>) {
        if is_string_kind(kind)
            && matches!(
                role,
                SyntaxTokenRole::SyntaxKey
                    | SyntaxTokenRole::SyntaxSectionHeader
                    | SyntaxTokenRole::SyntaxString
            )
        {
            self.push_string_like_token(role, kind, range);
        } else {
            self.push(role, range);
        }
    }

    fn push_string_like_token(
        &mut self,
        role: SyntaxTokenRole,
        kind: SyntaxKind,
        range: Range<usize>,
    ) {
        let Some(text) = self.source.get(range.clone()) else {
            return;
        };
        let delimiter = delimiter_for_string_kind(kind);
        let mut content_start = range.start;
        let mut content_end = range.end;

        if text.starts_with(delimiter) {
            let delimiter_end = range.start + delimiter.len();
            self.push(
                SyntaxTokenRole::SyntaxStructuralPunctuation,
                range.start..delimiter_end,
            );
            content_start = delimiter_end;
        }

        if content_end >= content_start + delimiter.len()
            && self.source.get(content_end - delimiter.len()..content_end) == Some(delimiter)
        {
            let delimiter_start = content_end - delimiter.len();
            self.push(
                SyntaxTokenRole::SyntaxStructuralPunctuation,
                delimiter_start..content_end,
            );
            content_end = delimiter_start;
        }

        if is_basic_string_kind(kind) {
            self.push_basic_string_content(role, content_start..content_end);
        } else {
            self.push(role, content_start..content_end);
        }
    }

    fn push_basic_string_content(&mut self, role: SyntaxTokenRole, range: Range<usize>) {
        if self.source.get(range.clone()).is_none() {
            return;
        }

        let mut chunk_start = range.start;
        let mut index = range.start;
        while index < range.end {
            if self.source.as_bytes()[index] == b'\\' {
                self.push(role, chunk_start..index);
                let escape_end = toml_escape_end(self.source, index, range.end);
                self.push(SyntaxTokenRole::SyntaxEscape, index..escape_end);
                index = escape_end;
                chunk_start = index;
            } else {
                index = next_char_end(self.source, index, range.end);
            }
        }
        self.push(role, chunk_start..range.end);
    }

    fn push(&mut self, role: SyntaxTokenRole, range: Range<usize>) {
        if range.is_empty() || self.source.get(range.clone()).is_none() {
            return;
        }
        self.tokens.push(HighlightToken::new(role, range));
    }

    fn any_token_overlaps(&self, range: &Range<usize>) -> bool {
        self.tokens
            .iter()
            .any(|token| ranges_overlap(&token.range(), range))
    }
}

fn role_for_toml_token(token: &SyntaxToken) -> Option<SyntaxTokenRole> {
    match token.kind() {
        SyntaxKind::COMMENT => Some(SyntaxTokenRole::SyntaxComment),
        SyntaxKind::IDENT => role_for_ident(token),
        SyntaxKind::EQ => Some(SyntaxTokenRole::SyntaxAssignment),
        SyntaxKind::PERIOD
        | SyntaxKind::COMMA
        | SyntaxKind::BRACKET_START
        | SyntaxKind::BRACKET_END
        | SyntaxKind::BRACE_START
        | SyntaxKind::BRACE_END => Some(SyntaxTokenRole::SyntaxStructuralPunctuation),
        SyntaxKind::STRING
        | SyntaxKind::MULTI_LINE_STRING
        | SyntaxKind::STRING_LITERAL
        | SyntaxKind::MULTI_LINE_STRING_LITERAL => role_for_string(token),
        SyntaxKind::INTEGER
        | SyntaxKind::INTEGER_HEX
        | SyntaxKind::INTEGER_OCT
        | SyntaxKind::INTEGER_BIN
        | SyntaxKind::FLOAT => Some(SyntaxTokenRole::SyntaxNumber),
        SyntaxKind::BOOL => Some(SyntaxTokenRole::SyntaxBoolean),
        SyntaxKind::DATE_TIME_OFFSET
        | SyntaxKind::DATE_TIME_LOCAL
        | SyntaxKind::DATE
        | SyntaxKind::TIME => Some(SyntaxTokenRole::SyntaxDateTime),
        SyntaxKind::ERROR => Some(SyntaxTokenRole::SyntaxError),
        _ => None,
    }
}

fn role_for_ident(token: &SyntaxToken) -> Option<SyntaxTokenRole> {
    if has_ancestor_kind(token, SyntaxKind::TABLE_HEADER)
        || has_ancestor_kind(token, SyntaxKind::TABLE_ARRAY_HEADER)
    {
        Some(SyntaxTokenRole::SyntaxSectionHeader)
    } else if has_ancestor_kind(token, SyntaxKind::KEY) {
        Some(SyntaxTokenRole::SyntaxKey)
    } else {
        None
    }
}

fn role_for_string(token: &SyntaxToken) -> Option<SyntaxTokenRole> {
    if has_ancestor_kind(token, SyntaxKind::TABLE_HEADER)
        || has_ancestor_kind(token, SyntaxKind::TABLE_ARRAY_HEADER)
    {
        Some(SyntaxTokenRole::SyntaxSectionHeader)
    } else if has_ancestor_kind(token, SyntaxKind::KEY) {
        Some(SyntaxTokenRole::SyntaxKey)
    } else {
        Some(SyntaxTokenRole::SyntaxString)
    }
}

fn has_ancestor_kind(token: &SyntaxToken, kind: SyntaxKind) -> bool {
    let mut parent = token.parent();
    while let Some(node) = parent {
        if node.kind() == kind {
            return true;
        }
        parent = node.parent();
    }
    false
}

fn is_string_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::STRING
            | SyntaxKind::MULTI_LINE_STRING
            | SyntaxKind::STRING_LITERAL
            | SyntaxKind::MULTI_LINE_STRING_LITERAL
    )
}

fn is_basic_string_kind(kind: SyntaxKind) -> bool {
    matches!(kind, SyntaxKind::STRING | SyntaxKind::MULTI_LINE_STRING)
}

fn delimiter_for_string_kind(kind: SyntaxKind) -> &'static str {
    match kind {
        SyntaxKind::MULTI_LINE_STRING => "\"\"\"",
        SyntaxKind::MULTI_LINE_STRING_LITERAL => "'''",
        SyntaxKind::STRING_LITERAL => "'",
        _ => "\"",
    }
}

fn source_range(source: &str, range: TextRange) -> Option<Range<usize>> {
    let range = u32::from(range.start()) as usize..u32::from(range.end()) as usize;
    (!range.is_empty() && source.get(range.clone()).is_some()).then_some(range)
}

fn toml_escape_end(source: &str, start: usize, content_end: usize) -> usize {
    if start + 1 >= content_end {
        return content_end;
    }
    let raw_end = match source.as_bytes()[start + 1] {
        b'u' => (start + 6).min(content_end),
        b'U' => (start + 10).min(content_end),
        _ => (start + 2).min(content_end),
    };
    char_boundary_at_or_after(source, raw_end, content_end)
}

fn char_boundary_at_or_after(source: &str, start: usize, content_end: usize) -> usize {
    let mut end = start.min(content_end);
    while end < content_end && !source.is_char_boundary(end) {
        end += 1;
    }
    end
}

fn next_char_end(source: &str, start: usize, end: usize) -> usize {
    if start >= end || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return end;
    }
    source[start..end]
        .chars()
        .next()
        .map_or(start, |ch| start + ch.len_utf8())
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}
