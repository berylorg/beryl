pub(super) const MAX_COPY_SOURCE_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CopyTextError {
    Capacity,
    Allocation,
}

pub(super) fn stripped_span(source: &str, ordinal: u64) -> Result<String, CopyTextError> {
    if ordinal == 1 {
        return stripped_source(source);
    }
    if source.len() > MAX_COPY_SOURCE_BYTES {
        return Err(CopyTextError::Capacity);
    }
    // A continuation can start inside a literal or delimiter opened in an unloaded span.
    let mut output = String::new();
    output
        .try_reserve_exact(source.len())
        .map_err(|_| CopyTextError::Allocation)?;
    output.push_str(source);
    Ok(output)
}

pub(super) fn stripped_source(source: &str) -> Result<String, CopyTextError> {
    if source.len() > MAX_COPY_SOURCE_BYTES {
        return Err(CopyTextError::Capacity);
    }
    let mut output = String::new();
    output
        .try_reserve_exact(source.len())
        .map_err(|_| CopyTextError::Allocation)?;
    let mut remaining_work = source.len().saturating_mul(8);
    let mut rest = source;
    while !rest.is_empty() {
        let line_end = rest.find('\n').map_or(rest.len(), |offset| offset + 1);
        let line = &rest[..line_end];
        if line.contains('<') || opaque_container_code(line) {
            output.push_str(rest);
            break;
        }
        if line.starts_with("    ") || line.starts_with('\t') {
            output.push_str(line);
            rest = &rest[line_end..];
            continue;
        }
        if let Some((marker, width)) = fence(line) {
            let mut offset = line_end;
            let mut closing = None;
            while offset < rest.len() && remaining_work > 0 {
                let end = rest[offset..]
                    .find('\n')
                    .map_or(rest.len(), |end| offset + end + 1);
                let candidate = &rest[offset..end];
                remaining_work = remaining_work.saturating_sub(candidate.len());
                if closing_fence(candidate, marker, width) {
                    closing = Some((offset, end));
                    break;
                }
                offset = end;
            }
            if let Some((start, end)) = closing {
                output.push_str(&rest[line_end..start]);
                rest = &rest[end..];
                continue;
            }
            output.push_str(rest);
            break;
        }
        let content = block_content(line);
        if line.contains("```") || line.contains("~~~") {
            output.push_str(rest);
            break;
        }
        if inline_content(content, &mut output, &mut remaining_work) {
            output.push_str(&rest[line_end..]);
            break;
        }
        rest = &rest[line_end..];
    }
    Ok(output)
}

fn fence(line: &str) -> Option<(u8, usize)> {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let marker = *trimmed.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let width = trimmed.bytes().take_while(|byte| *byte == marker).count();
    if width < 3 || (marker == b'`' && trimmed[width..].contains('`')) {
        return None;
    }
    Some((marker, width))
}

fn opaque_container_code(mut line: &str) -> bool {
    let mut container = false;
    loop {
        let trimmed = line.trim_start_matches(' ');
        let indentation = line.len() - trimmed.len();
        if container && (indentation >= 4 || trimmed.starts_with('\t')) {
            return true;
        }
        if indentation > 3 {
            return false;
        }
        let prefix = if trimmed.starts_with('>') {
            Some(1)
        } else if matches!(trimmed.as_bytes().first(), Some(b'-' | b'+' | b'*'))
            && trimmed
                .as_bytes()
                .get(1)
                .is_some_and(u8::is_ascii_whitespace)
        {
            Some(1)
        } else {
            let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
            if (1..=9).contains(&digits)
                && matches!(trimmed.as_bytes().get(digits), Some(b'.' | b')'))
                && trimmed
                    .as_bytes()
                    .get(digits + 1)
                    .is_some_and(u8::is_ascii_whitespace)
            {
                Some(digits + 1)
            } else {
                None
            }
        };
        let Some(prefix) = prefix else {
            return container && (trimmed.contains('`') || trimmed.contains("~~~"));
        };
        container = true;
        line = &trimmed[prefix..];
        if line.starts_with("    ") || line.starts_with('\t') {
            return true;
        }
        if let Some(rest) = line.strip_prefix(' ') {
            line = rest;
        }
    }
}

fn closing_fence(line: &str, marker: u8, width: usize) -> bool {
    let trimmed = line.trim_start_matches(' ');
    let count = trimmed.bytes().take_while(|byte| *byte == marker).count();
    line.len() - trimmed.len() <= 3
        && count >= width
        && trimmed[count..]
            .bytes()
            .all(|byte| byte.is_ascii_whitespace())
}

fn block_content(line: &str) -> &str {
    let mut content = line.trim_start_matches(' ');
    let indent = line.len() - content.len();
    if indent > 3 {
        return line;
    }
    if let Some(rest) = content.strip_prefix("> ") {
        content = rest;
    }
    let heading = content.bytes().take_while(|byte| *byte == b'#').count();
    if (1..=6).contains(&heading) && content.as_bytes().get(heading) == Some(&b' ') {
        return &content[heading + 1..];
    }
    if matches!(content.as_bytes().first(), Some(b'-' | b'+' | b'*'))
        && content.as_bytes().get(1) == Some(&b' ')
        && !content[2..].trim().is_empty()
        && !content[2..]
            .trim()
            .bytes()
            .all(|byte| matches!(byte, b'-' | b'*' | b' '))
    {
        return &content[2..];
    }
    let digits = content.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits)
        && matches!(content.as_bytes().get(digits), Some(b'.' | b')'))
        && content.as_bytes().get(digits + 1) == Some(&b' ')
    {
        return &content[digits + 2..];
    }
    if content.len() != line.len() - indent {
        content
    } else {
        line
    }
}

fn inline_content(mut source: &str, output: &mut String, work: &mut usize) -> bool {
    while !source.is_empty() {
        if *work == 0 {
            output.push_str(source);
            return true;
        }
        *work = work.saturating_sub(1);
        let byte = source.as_bytes()[0];
        if byte == b'\\'
            && source
                .as_bytes()
                .get(1)
                .is_some_and(u8::is_ascii_punctuation)
        {
            output.push(char::from(source.as_bytes()[1]));
            source = &source[2..];
            continue;
        }
        let image = source.starts_with("![");
        if byte == b'[' || image {
            let start = if image { 2 } else { 1 };
            if let Some(end) = find_bounded(source, "](", start, work) {
                let label = &source[start..end];
                if label.contains('`') {
                    output.push_str(source);
                    return true;
                }
                if !label.contains(['[', ']']) {
                    if let Some(close) = find_bounded(source, ")", end + 2, work) {
                        let destination = &source[end + 2..close];
                        if !destination.contains(['(', '\\'])
                            && !destination.bytes().any(|byte| byte.is_ascii_whitespace())
                        {
                            output.push_str(label);
                            source = &source[close + 1..];
                            continue;
                        }
                        output.push_str(&source[..close + 1]);
                        source = &source[close + 1..];
                        continue;
                    }
                }
            }
        }
        if matches!(byte, b'`' | b'*' | b'_') {
            let width = source.bytes().take_while(|value| *value == byte).count();
            if byte == b'`' || width <= 2 {
                let delimiter = &source[..width];
                if let Some(end) = find_bounded(source, delimiter, width, work) {
                    let inner = &source[width..end];
                    if byte != b'`' && inner.contains('`') {
                        output.push_str(source);
                        return true;
                    }
                    let exact_end = source.as_bytes().get(end + width) != Some(&byte);
                    let readable = !inner.is_empty()
                        && (byte == b'`'
                            || (!inner.starts_with(char::is_whitespace)
                                && !inner.ends_with(char::is_whitespace)
                                && !inner.contains('\\')));
                    let word_suffix = byte == b'_'
                        && (source[end + width..].starts_with(char::is_alphanumeric)
                            || output.ends_with(char::is_alphanumeric));
                    if exact_end && readable && !word_suffix {
                        output.push_str(inner);
                        source = &source[end + width..];
                        continue;
                    }
                }
            }
            if byte == b'`' {
                output.push_str(source);
                return true;
            }
            output.push_str(&source[..width]);
            source = &source[width..];
            continue;
        }
        let character = source.chars().next().unwrap();
        output.push(character);
        source = &source[character.len_utf8()..];
    }
    false
}

fn find_bounded(source: &str, needle: &str, start: usize, work: &mut usize) -> Option<usize> {
    let bytes = source.as_bytes();
    for offset in start..source.len() {
        if *work == 0 || matches!(bytes[offset], b'\r' | b'\n') {
            return None;
        }
        *work -= 1;
        if bytes[offset..].starts_with(needle.as_bytes()) {
            return Some(offset);
        }
    }
    None
}
