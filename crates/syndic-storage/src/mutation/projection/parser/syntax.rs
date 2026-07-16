use crate::{MarkdownBlockKind, MarkdownFenceMarker};

pub(super) fn opening_fence(line: &str) -> Option<(MarkdownFenceMarker, Option<Box<str>>)> {
    let source = trim_line_ending(line);
    let indent = source.bytes().take_while(|byte| *byte == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &source[indent..];
    let byte = *rest.as_bytes().first()?;
    if !matches!(byte, b'`' | b'~') {
        return None;
    }
    let length = rest.bytes().take_while(|value| *value == byte).count();
    if length < 3 || length > u8::MAX as usize {
        return None;
    }
    let info = rest[length..].trim();
    if byte == b'`' && info.as_bytes().contains(&b'`') {
        return None;
    }
    let language = info
        .split_ascii_whitespace()
        .next()
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .map(Into::into);
    Some((MarkdownFenceMarker::new(byte, length as u8), language))
}

pub(super) fn is_closing_fence(line: &str, fence: MarkdownFenceMarker) -> bool {
    let source = trim_line_ending(line);
    let indent = source.bytes().take_while(|byte| *byte == b' ').count();
    if indent > 3 {
        return false;
    }
    let rest = &source[indent..];
    let length = rest
        .bytes()
        .take_while(|value| *value == fence.byte())
        .count();
    length >= fence.length() as usize
        && rest[length..]
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t'))
}

pub(super) fn one_line_block_kind(line: &str) -> Option<MarkdownBlockKind> {
    let source = trim_line_ending(line);
    let trimmed = source.trim_start_matches(' ');
    let indent = source.len() - trimmed.len();
    if indent > 3 {
        return None;
    }
    let heading = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    if (1..=6).contains(&heading)
        && trimmed
            .as_bytes()
            .get(heading)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        return Some(MarkdownBlockKind::Heading(heading as u8));
    }
    if trimmed.starts_with('>') {
        return Some(MarkdownBlockKind::BlockQuote);
    }
    if is_list_line(trimmed) {
        return Some(MarkdownBlockKind::List);
    }
    if is_thematic_break(trimmed) {
        return Some(MarkdownBlockKind::ThematicBreak);
    }
    None
}

pub(super) fn starts_distinct_block(line: &str) -> bool {
    opening_fence(line).is_some() || one_line_block_kind(line).is_some()
}

fn is_list_line(source: &str) -> bool {
    if source
        .as_bytes()
        .first()
        .is_some_and(|byte| matches!(byte, b'-' | b'+' | b'*'))
    {
        return source
            .as_bytes()
            .get(1)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'));
    }
    let digits = source.bytes().take_while(u8::is_ascii_digit).count();
    digits != 0
        && source
            .as_bytes()
            .get(digits)
            .is_some_and(|byte| matches!(byte, b'.' | b')'))
        && source
            .as_bytes()
            .get(digits + 1)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
}

fn is_thematic_break(source: &str) -> bool {
    let compact: Vec<u8> = source
        .bytes()
        .filter(|byte| !matches!(byte, b' ' | b'\t'))
        .collect();
    compact.len() >= 3
        && compact
            .first()
            .is_some_and(|first| matches!(first, b'-' | b'_' | b'*'))
        && compact.iter().all(|byte| Some(byte) == compact.first())
}

pub(super) fn table_columns(line: &str) -> Option<u64> {
    let source = trim_line_ending(line).trim();
    let source = source.strip_prefix('|').unwrap_or(source);
    let source = source.strip_suffix('|').unwrap_or(source);
    let mut separators = 0_u64;
    let mut escaped = false;
    let mut in_code = false;
    for byte in source.bytes() {
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'`' {
            in_code = !in_code;
        } else if byte == b'|' && !in_code {
            separators = separators.checked_add(1)?;
        }
    }
    (separators != 0).then_some(separators + 1)
}

pub(super) fn table_delimiter_columns(line: &str) -> Option<u64> {
    let source = trim_line_ending(line).trim();
    let source = source.strip_prefix('|').unwrap_or(source);
    let source = source.strip_suffix('|').unwrap_or(source);
    let cells: Vec<&str> = source.split('|').collect();
    if cells.is_empty() {
        return None;
    }
    for cell in &cells {
        let cell = cell.trim();
        let cell = cell.strip_prefix(':').unwrap_or(cell);
        let cell = cell.strip_suffix(':').unwrap_or(cell);
        if cell.len() < 3 || !cell.bytes().all(|byte| byte == b'-') {
            return None;
        }
    }
    u64::try_from(cells.len()).ok()
}

pub(super) fn looks_like_table_row(line: &str) -> bool {
    table_columns(line).is_some()
}

pub(super) fn single_logical_line(source: &str) -> bool {
    let bytes = source.as_bytes();
    match bytes.iter().position(|byte| *byte == b'\n') {
        None => true,
        Some(index) => index + 1 == bytes.len(),
    }
}

pub(super) fn is_blank(line: &str) -> bool {
    trim_line_ending(line)
        .bytes()
        .all(|byte| matches!(byte, b' ' | b'\t'))
}

fn trim_line_ending(line: &str) -> &str {
    line.strip_suffix("\r\n")
        .or_else(|| line.strip_suffix('\n'))
        .unwrap_or(line)
}
