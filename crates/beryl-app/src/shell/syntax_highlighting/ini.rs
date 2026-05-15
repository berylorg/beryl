use std::ops::Range;

use super::model::{SyntaxHighlight, SyntaxLanguage, SyntaxToken, SyntaxTokenRole};

// This is intentionally a conservative Windows INI highlighter: semicolon-only
// comments and first-`=` assignments. Hash comments and colon assignments are
// INI-like dialect extensions, so the `ini` label marks them malformed.
pub(crate) fn highlight_windows_ini(source: &str) -> SyntaxHighlight {
    let mut tokens = WindowsIniTokenBuilder {
        source,
        tokens: Vec::new(),
    };

    let mut line_start = 0;
    while line_start < source.len() {
        let line_end = source[line_start..]
            .find('\n')
            .map_or(source.len(), |index| line_start + index + 1);
        let mut content_end = line_end;
        if content_end > line_start && source.as_bytes().get(content_end - 1) == Some(&b'\n') {
            content_end -= 1;
        }
        if content_end > line_start && source.as_bytes().get(content_end - 1) == Some(&b'\r') {
            content_end -= 1;
        }

        tokens.highlight_line(line_start..content_end);
        line_start = line_end;
    }

    SyntaxHighlight::new(SyntaxLanguage::WindowsIni, tokens.tokens)
}

struct WindowsIniTokenBuilder<'a> {
    source: &'a str,
    tokens: Vec<SyntaxToken>,
}

impl WindowsIniTokenBuilder<'_> {
    fn highlight_line(&mut self, range: Range<usize>) {
        let trimmed = trim_ascii_whitespace_range(self.source, range);
        if trimmed.is_empty() {
            return;
        }

        match self.source.as_bytes().get(trimmed.start) {
            Some(b';') => self.push(SyntaxTokenRole::SyntaxComment, trimmed.start..trimmed.end),
            Some(b'[') => self.highlight_section_header(trimmed),
            _ => self.highlight_assignment(trimmed),
        }
    }

    fn highlight_section_header(&mut self, range: Range<usize>) {
        let content_start = range.start + 1;
        let Some(close_bracket) = self.source[content_start..range.end].find(']') else {
            self.push(SyntaxTokenRole::SyntaxError, range);
            return;
        };
        let close_bracket = content_start + close_bracket;
        let section_range = trim_ascii_whitespace_range(self.source, content_start..close_bracket);
        let trailing_range = trim_ascii_whitespace_range(self.source, close_bracket + 1..range.end);

        self.push(
            SyntaxTokenRole::SyntaxStructuralPunctuation,
            range.start..range.start + 1,
        );
        self.push(SyntaxTokenRole::SyntaxSectionHeader, section_range);
        self.push(
            SyntaxTokenRole::SyntaxStructuralPunctuation,
            close_bracket..close_bracket + 1,
        );
        self.push(SyntaxTokenRole::SyntaxError, trailing_range);
    }

    fn highlight_assignment(&mut self, range: Range<usize>) {
        let Some(separator) = self.source[range.clone()].find('=') else {
            self.push(SyntaxTokenRole::SyntaxError, range);
            return;
        };
        let separator = range.start + separator;
        let key_range = trim_ascii_whitespace_range(self.source, range.start..separator);
        let value_range = trim_ascii_whitespace_range(self.source, separator + 1..range.end);

        if key_range.is_empty() {
            self.push(SyntaxTokenRole::SyntaxError, range);
            return;
        }

        self.push(SyntaxTokenRole::SyntaxKey, key_range);
        self.push(SyntaxTokenRole::SyntaxAssignment, separator..separator + 1);
        self.push(SyntaxTokenRole::SyntaxString, value_range);
    }

    fn push(&mut self, role: SyntaxTokenRole, range: Range<usize>) {
        if range.is_empty() || self.source.get(range.clone()).is_none() {
            return;
        }
        self.tokens.push(SyntaxToken::new(role, range));
    }
}

fn trim_ascii_whitespace_range(source: &str, mut range: Range<usize>) -> Range<usize> {
    let bytes = source.as_bytes();
    while range.start < range.end && bytes[range.start].is_ascii_whitespace() {
        range.start += 1;
    }
    while range.end > range.start && bytes[range.end - 1].is_ascii_whitespace() {
        range.end -= 1;
    }
    range
}
