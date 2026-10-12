use std::ops::Range;

pub(crate) fn content(line: &str) -> &str {
    line.strip_suffix('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .unwrap_or(line)
}

pub(crate) fn cell_bounds(line: &str) -> Range<usize> {
    let source = content(line);
    let trimmed = source.trim();
    let leading = source.len() - source.trim_start().len();
    let trailing = leading + trimmed.len();
    let start = if trimmed.starts_with('|') {
        leading + 1
    } else {
        0
    };
    let end = if trimmed.len() > usize::from(trimmed.starts_with('|')) && trimmed.ends_with('|') {
        trailing - 1
    } else if trimmed == "|" {
        start
    } else {
        source.len()
    };
    start..end
}

pub(crate) fn separator(byte: u8, escaped: &mut bool, in_code: &mut bool) -> bool {
    if *escaped {
        *escaped = false;
    } else if byte == b'\\' {
        *escaped = true;
    } else if byte == b'`' {
        *in_code = !*in_code;
    } else if byte == b'|' && !*in_code {
        return true;
    }
    false
}

pub(crate) fn columns(line: &str) -> Option<u64> {
    let bounds = cell_bounds(line);
    let mut escaped = false;
    let mut in_code = false;
    let mut count = 1_u64;
    for byte in line[bounds].bytes() {
        if separator(byte, &mut escaped, &mut in_code) {
            count = count.checked_add(1)?;
        }
    }
    (count > 1).then_some(count)
}

pub(crate) fn cells(line: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let bounds = cell_bounds(line);
    let mut position = bounds.start;
    let mut done = false;
    let mut escaped = false;
    let mut in_code = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        let start = position;
        while position < bounds.end {
            if separator(line.as_bytes()[position], &mut escaped, &mut in_code) {
                break;
            }
            position += 1;
        }
        let range = start..position;
        if position == bounds.end {
            done = true;
        } else {
            position += 1;
        }
        Some(range)
    })
}

pub(crate) fn trimmed_cell(source: &str, raw: Range<usize>) -> Range<usize> {
    let cell = &source[raw.clone()];
    let leading = cell.len() - cell.trim_start_matches([' ', '\t']).len();
    let trimmed = cell[leading..].trim_end_matches([' ', '\t']);
    raw.start + leading..raw.start + leading + trimmed.len()
}

pub(crate) fn literal_fallback(trimmed: &str) -> bool {
    trimmed.bytes().any(|byte| {
        matches!(
            byte,
            b'\\' | b'`' | b'*' | b'_' | b'[' | b']' | b'<' | b'>' | b'&' | b'~'
        )
    })
}

pub(crate) fn delimiter_cells(line: &str) -> impl Iterator<Item = &str> {
    let source = content(line).trim();
    let source = source.strip_prefix('|').unwrap_or(source);
    let source = source.strip_suffix('|').unwrap_or(source);
    source.split('|')
}

pub(crate) fn delimiter_alignment(cell: &str) -> Option<(bool, bool)> {
    let cell = cell.trim();
    let left = cell.starts_with(':');
    let right = cell.ends_with(':');
    let source = cell.strip_prefix(':').unwrap_or(cell);
    let source = source.strip_suffix(':').unwrap_or(source);
    (source.len() >= 3 && source.bytes().all(|byte| byte == b'-')).then_some((left, right))
}

pub(crate) fn delimiter_columns(line: &str) -> Option<u64> {
    let mut count = 0_u64;
    for cell in delimiter_cells(line) {
        delimiter_alignment(cell)?;
        count = count.checked_add(1)?;
    }
    Some(count)
}
