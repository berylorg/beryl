use super::model::{SyntaxHighlight, SyntaxLanguage, SyntaxToken, SyntaxTokenRole};

pub(crate) fn highlight_markdown(source: &str) -> SyntaxHighlight {
    let mut parser = MarkdownHighlighter {
        source,
        tokens: Vec::new(),
        fence: None,
    };
    parser.parse_lines();
    SyntaxHighlight::new(SyntaxLanguage::Markdown, parser.tokens)
}

#[derive(Clone, Copy)]
struct OpenFence {
    marker: u8,
    len: usize,
}

struct FenceLine {
    marker: u8,
    len: usize,
    delimiter_end: usize,
    info_start: usize,
    info_end: usize,
}

struct MarkdownHighlighter<'a> {
    source: &'a str,
    tokens: Vec<SyntaxToken>,
    fence: Option<OpenFence>,
}

impl MarkdownHighlighter<'_> {
    fn parse_lines(&mut self) {
        let mut line_start = 0;
        while line_start < self.source.len() {
            let line_next = self.next_line_start(line_start);
            let content_end = self.line_content_end(line_start, line_next);
            self.parse_line(line_start, content_end);
            line_start = line_next;
        }
    }

    fn parse_line(&mut self, line_start: usize, line_end: usize) {
        let content_start = markdown_content_start(self.source, line_start, line_end);
        if let Some(open) = self.fence {
            if let Some(fence) = content_start
                .and_then(|start| fence_line(self.source, start, line_end))
                .filter(|fence| fence.closes(open, self.source, line_end))
            {
                self.push(
                    SyntaxTokenRole::MarkupFenceDelimiter,
                    content_start.unwrap()..fence.delimiter_end,
                );
                self.fence = None;
            } else if line_end > line_start {
                self.push(SyntaxTokenRole::MarkupCodeBlock, line_start..line_end);
            }
            return;
        }

        let Some(content_start) = content_start else {
            self.parse_inline(line_start, line_end);
            return;
        };

        if let Some(fence) = fence_line(self.source, content_start, line_end) {
            self.push(
                SyntaxTokenRole::MarkupFenceDelimiter,
                content_start..fence.delimiter_end,
            );
            self.push(
                SyntaxTokenRole::MarkupFenceInfo,
                fence.info_start..fence.info_end,
            );
            self.fence = Some(OpenFence {
                marker: fence.marker,
                len: fence.len,
            });
            return;
        }

        if thematic_break(self.source, content_start, line_end) {
            self.push(
                SyntaxTokenRole::MarkupThematicBreak,
                content_start..line_end,
            );
            return;
        }

        let inline_start = self.parse_block_marker(content_start, line_end);
        self.parse_inline(inline_start, line_end);
    }

    fn parse_block_marker(&mut self, start: usize, end: usize) -> usize {
        if let Some(after) = heading_marker(self.source, start, end) {
            self.push(SyntaxTokenRole::MarkupHeadingMarker, start..after);
            return skip_ascii_spaces(self.source, after, end);
        }
        if self.source.as_bytes().get(start) == Some(&b'>') {
            self.push(SyntaxTokenRole::MarkupQuoteMarker, start..start + 1);
            return skip_ascii_spaces(self.source, start + 1, end);
        }
        if let Some(after) = list_marker(self.source, start, end) {
            self.push(SyntaxTokenRole::MarkupListMarker, start..after);
            return skip_ascii_spaces(self.source, after, end);
        }
        start
    }

    fn parse_inline(&mut self, start: usize, end: usize) {
        let mut index = start;
        while index < end {
            let bytes = self.source.as_bytes();
            let next = match bytes[index] {
                b'\\' => self.parse_escape(index, end),
                b'`' => self.parse_code_span(index, end),
                b'!' if bytes.get(index + 1) == Some(&b'[') => self.parse_link(index, end, true),
                b'[' => self.parse_link(index, end, false),
                b'*' | b'_' => self.parse_emphasis(index, end),
                b'<' => self.parse_html(index, end),
                _ => None,
            };
            index = next.unwrap_or(index + 1);
        }
    }

    fn parse_escape(&mut self, index: usize, end: usize) -> Option<usize> {
        let escaped_start = index + 1;
        if escaped_start >= end {
            return None;
        }
        let escaped_end = next_char_end(self.source, escaped_start, end);
        self.push(SyntaxTokenRole::Escape, index..escaped_end);
        Some(escaped_end)
    }

    fn parse_code_span(&mut self, index: usize, end: usize) -> Option<usize> {
        let len = same_byte_run(self.source, index, end, b'`');
        let content_start = index + len;
        if let Some(close) = find_byte_run(self.source, content_start, end, b'`', len) {
            self.push(
                SyntaxTokenRole::MarkupCodeSpanDelimiter,
                index..content_start,
            );
            self.push(SyntaxTokenRole::MarkupCodeSpan, content_start..close);
            self.push(SyntaxTokenRole::MarkupCodeSpanDelimiter, close..close + len);
            Some(close + len)
        } else {
            self.push(
                SyntaxTokenRole::MarkupCodeSpanDelimiter,
                index..content_start,
            );
            Some(content_start)
        }
    }

    fn parse_emphasis(&mut self, index: usize, end: usize) -> Option<usize> {
        let marker = self.source.as_bytes()[index];
        let len = same_byte_run(self.source, index, end, marker).min(2);
        let content_start = index + len;
        let role = if len == 2 {
            SyntaxTokenRole::MarkupStrongDelimiter
        } else {
            SyntaxTokenRole::MarkupEmphasisDelimiter
        };
        self.push(role, index..content_start);
        if let Some(close) = find_byte_run(self.source, content_start, end, marker, len) {
            self.push(role, close..close + len);
            Some(close + len)
        } else {
            Some(content_start)
        }
    }

    fn parse_link(&mut self, index: usize, end: usize, image: bool) -> Option<usize> {
        let label_start = index + if image { 2 } else { 1 };
        let close_label = find_byte(self.source, label_start, end, b']')?;
        let open_destination = close_label + 1;
        if self.source.as_bytes().get(open_destination) != Some(&b'(') {
            return None;
        }
        let close_destination = find_byte(self.source, open_destination + 1, end, b')')?;
        if image {
            self.push(SyntaxTokenRole::MarkupImageMarker, index..index + 1);
        }
        self.push(
            SyntaxTokenRole::MarkupPunctuation,
            label_start - 1..label_start,
        );
        self.push(SyntaxTokenRole::MarkupLinkText, label_start..close_label);
        self.push(
            SyntaxTokenRole::MarkupPunctuation,
            close_label..close_label + 2,
        );
        let destination = trim_ascii_spaces(self.source, open_destination + 1, close_destination);
        self.push(
            SyntaxTokenRole::MarkupLinkDestination,
            destination.0..destination.1,
        );
        self.push(
            SyntaxTokenRole::MarkupPunctuation,
            close_destination..close_destination + 1,
        );
        Some(close_destination + 1)
    }

    fn parse_html(&mut self, index: usize, end: usize) -> Option<usize> {
        let close = find_byte(self.source, index + 1, end, b'>')?;
        self.push(SyntaxTokenRole::MarkupHtml, index..close + 1);
        Some(close + 1)
    }

    fn push(&mut self, role: SyntaxTokenRole, range: std::ops::Range<usize>) {
        if range.is_empty() || self.source.get(range.clone()).is_none() {
            return;
        }
        self.tokens.push(SyntaxToken::new(role, range));
    }

    fn next_line_start(&self, line_start: usize) -> usize {
        self.source[line_start..]
            .find('\n')
            .map_or(self.source.len(), |offset| line_start + offset + 1)
    }

    fn line_content_end(&self, line_start: usize, line_next: usize) -> usize {
        let mut end = line_next;
        if end > line_start && self.source.as_bytes()[end - 1] == b'\n' {
            end -= 1;
        }
        if end > line_start && self.source.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
        end
    }
}

impl FenceLine {
    fn closes(&self, open: OpenFence, source: &str, line_end: usize) -> bool {
        self.marker == open.marker
            && self.len >= open.len
            && source.as_bytes()[self.delimiter_end..line_end]
                .iter()
                .all(|byte| *byte == b' ' || *byte == b'\t')
    }
}

fn markdown_content_start(source: &str, line_start: usize, line_end: usize) -> Option<usize> {
    let mut index = line_start;
    let mut spaces = 0;
    while index < line_end && source.as_bytes()[index] == b' ' {
        index += 1;
        spaces += 1;
    }
    (spaces <= 3).then_some(index)
}

fn heading_marker(source: &str, start: usize, end: usize) -> Option<usize> {
    let count = same_byte_run(source, start, end, b'#');
    if !(1..=6).contains(&count) {
        return None;
    }
    let after = start + count;
    (after == end
        || source
            .as_bytes()
            .get(after)
            .is_some_and(u8::is_ascii_whitespace))
    .then_some(after)
}

fn list_marker(source: &str, start: usize, end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    match bytes.get(start).copied()? {
        b'-' | b'+' | b'*' if bytes.get(start + 1).is_some_and(u8::is_ascii_whitespace) => {
            Some(start + 1)
        }
        b'0'..=b'9' => ordered_list_marker(source, start, end),
        _ => None,
    }
}

fn ordered_list_marker(source: &str, start: usize, end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = start;
    while index < end && bytes[index].is_ascii_digit() && index - start < 9 {
        index += 1;
    }
    if index == start || index >= end || !matches!(bytes[index], b'.' | b')') {
        return None;
    }
    let after = index + 1;
    bytes
        .get(after)
        .is_some_and(u8::is_ascii_whitespace)
        .then_some(after)
}

fn thematic_break(source: &str, start: usize, end: usize) -> bool {
    let bytes = source.as_bytes();
    let Some(marker @ (b'-' | b'*' | b'_')) = bytes.get(start).copied() else {
        return false;
    };
    let mut count = 0;
    let mut index = start;
    while index < end {
        match bytes[index] {
            byte if byte == marker => count += 1,
            b' ' | b'\t' => {}
            _ => return false,
        }
        index += 1;
    }
    count >= 3
}

fn fence_line(source: &str, start: usize, end: usize) -> Option<FenceLine> {
    let marker @ (b'`' | b'~') = source.as_bytes().get(start).copied()? else {
        return None;
    };
    let len = same_byte_run(source, start, end, marker);
    if len < 3 {
        return None;
    }
    let delimiter_end = start + len;
    let (info_start, info_end) = trim_ascii_spaces(source, delimiter_end, end);
    Some(FenceLine {
        marker,
        len,
        delimiter_end,
        info_start,
        info_end,
    })
}

fn same_byte_run(source: &str, start: usize, end: usize, byte: u8) -> usize {
    let mut index = start;
    while index < end && source.as_bytes()[index] == byte {
        index += 1;
    }
    index - start
}

fn find_byte_run(source: &str, start: usize, end: usize, byte: u8, len: usize) -> Option<usize> {
    let mut index = start;
    while index + len <= end {
        if same_byte_run(source, index, end, byte) >= len {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn find_byte(source: &str, start: usize, end: usize, byte: u8) -> Option<usize> {
    let mut index = start;
    while index < end {
        if source.as_bytes()[index] == byte {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn skip_ascii_spaces(source: &str, start: usize, end: usize) -> usize {
    let mut index = start;
    while index < end && source.as_bytes()[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn trim_ascii_spaces(source: &str, start: usize, end: usize) -> (usize, usize) {
    let mut trimmed_start = start;
    let mut trimmed_end = end;
    while trimmed_start < trimmed_end && source.as_bytes()[trimmed_start].is_ascii_whitespace() {
        trimmed_start += 1;
    }
    while trimmed_end > trimmed_start && source.as_bytes()[trimmed_end - 1].is_ascii_whitespace() {
        trimmed_end -= 1;
    }
    (trimmed_start, trimmed_end)
}

fn next_char_end(source: &str, start: usize, end: usize) -> usize {
    source[start..end]
        .chars()
        .next()
        .map_or(start, |ch| start + ch.len_utf8())
}
