use super::{sink::Sink, *};

pub(super) fn parse(source: &str, kind: MarkdownBlockKind, sink: &mut Sink) -> Result<(), ()> {
    if kind == MarkdownBlockKind::ThematicBreak {
        return sink.literal(source);
    }
    let mut parser = Parser {
        source,
        sink,
        work: source.len().saturating_mul(32).saturating_add(32),
    };
    if kind == MarkdownBlockKind::Paragraph {
        parser.inline(0..source.len(), NarrativeStyle::default(), 0)?;
        return if parser.sink.map_count <= source.len() + 1 {
            Ok(())
        } else {
            Err(())
        };
    }
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let line_end = offset + line.len();
        let content_end = line_end
            - if line.ends_with("\r\n") {
                2
            } else if line.ends_with('\n') {
                1
            } else {
                0
            };
        let prefix = block_prefix(line, kind);
        let start = offset + prefix;
        let mut end = content_end;
        let mut hard_break = None;
        if content_end < line_end {
            let spaces = source[start..end]
                .bytes()
                .rev()
                .take_while(|byte| *byte == b' ')
                .count();
            if spaces >= 2 {
                hard_break = Some(end - spaces..end);
                end -= spaces;
            } else if source[start..end].ends_with('\\') {
                let slashes = source[start..end]
                    .bytes()
                    .rev()
                    .take_while(|byte| *byte == b'\\')
                    .count();
                if slashes % 2 == 1 {
                    hard_break = Some(end - 1..end);
                    end -= 1;
                }
            }
        }
        if matches!(kind, MarkdownBlockKind::Heading(_)) {
            let bytes = source.as_bytes();
            let mut suffix = end;
            while suffix > start && bytes[suffix - 1] == b' ' {
                suffix -= 1;
            }
            let hashes = suffix;
            while suffix > start && bytes[suffix - 1] == b'#' {
                suffix -= 1;
            }
            if suffix < hashes && suffix > start && bytes[suffix - 1] == b' ' {
                end = suffix - 1;
            }
        }
        let display_start = parser.sink.display_len;
        parser.inline(start..end, NarrativeStyle::default(), 0)?;
        let display_end = parser.sink.display_len;
        parser
            .sink
            .syntax(offset..start, display_start..display_end)?;
        if hard_break.is_none() {
            parser
                .sink
                .syntax(end..content_end, display_start..display_end)?;
        }
        if content_end < line_end {
            let break_start = parser.sink.display_len;
            parser.sink.text(
                source,
                content_end..line_end,
                "\n",
                NarrativeStyle::default(),
                line_end - content_end == 2,
            )?;
            if let Some(raw) = hard_break {
                parser
                    .sink
                    .syntax(raw, break_start..parser.sink.display_len)?;
            }
        }
        offset = line_end;
    }
    if parser.sink.map_count > source.len() + 1 {
        return Err(());
    }
    Ok(())
}

pub(super) fn block_prefix(line: &str, kind: MarkdownBlockKind) -> usize {
    let bytes = line.as_bytes();
    let indent = bytes
        .iter()
        .take_while(|byte| **byte == b' ')
        .count()
        .min(3);
    let mut end = indent;
    match kind {
        MarkdownBlockKind::Heading(level) => {
            if bytes.get(end..end + level as usize) != Some(&[b'#'; 6][..level as usize]) {
                return 0;
            }
            end += level as usize;
            if !matches!(
                bytes.get(end),
                Some(b' ' | b'\t') | None | Some(b'\r' | b'\n')
            ) {
                return 0;
            }
            while matches!(bytes.get(end), Some(b' ' | b'\t')) {
                end += 1;
            }
            end
        }
        MarkdownBlockKind::BlockQuote => {
            if bytes.get(end) != Some(&b'>') {
                return 0;
            }
            end += 1;
            if matches!(bytes.get(end), Some(b' ' | b'\t')) {
                end += 1;
            }
            end
        }
        MarkdownBlockKind::List => {
            if matches!(bytes.get(end), Some(b'-' | b'+' | b'*')) {
                end += 1;
            } else {
                let first = end;
                while bytes.get(end).is_some_and(u8::is_ascii_digit) && end - first < 9 {
                    end += 1;
                }
                if end == first || !matches!(bytes.get(end), Some(b'.' | b')')) {
                    return 0;
                }
                end += 1;
            }
            if !matches!(bytes.get(end), Some(b' ' | b'\t')) {
                return 0;
            }
            while matches!(bytes.get(end), Some(b' ' | b'\t')) {
                end += 1;
            }
            end
        }
        _ => 0,
    }
}

struct Parser<'a, 'b> {
    source: &'a str,
    sink: &'b mut Sink,
    work: usize,
}

impl Parser<'_, '_> {
    fn spend(&mut self, amount: usize) -> Result<(), ()> {
        self.work = self.work.checked_sub(amount).ok_or(())?;
        Ok(())
    }

    fn raw(&mut self, raw: Range<usize>, style: NarrativeStyle) -> Result<(), ()> {
        self.sink
            .text(self.source, raw.clone(), &self.source[raw], style, false)
    }

    fn inline(
        &mut self,
        range: Range<usize>,
        style: NarrativeStyle,
        depth: usize,
    ) -> Result<(), ()> {
        if depth > NARRATIVE_MAX_DEPTH {
            return Err(());
        }
        let bytes = self.source.as_bytes();
        let mut offset = range.start;
        while offset < range.end {
            self.spend(1)?;
            match bytes[offset] {
                b'\r' if offset + 1 < range.end && bytes[offset + 1] == b'\n' => {
                    self.sink
                        .text(self.source, offset..offset + 2, "\n", style, true)?;
                    offset += 2;
                    continue;
                }
                b' ' => {
                    let mut end = offset;
                    while end < range.end && bytes[end] == b' ' {
                        end += 1;
                    }
                    let newline = self.newline(end, range.end);
                    if end - offset >= 2 && newline > 0 {
                        let begin = self.sink.display_len;
                        self.sink.text(
                            self.source,
                            end..end + newline,
                            "\n",
                            style,
                            newline == 2,
                        )?;
                        self.sink
                            .syntax(offset..end, begin..self.sink.display_len)?;
                        offset = end + newline;
                        continue;
                    }
                    self.raw(offset..end, style)?;
                    offset = end;
                    continue;
                }
                b'\\' if self.newline(offset + 1, range.end) > 0 => {
                    let newline = self.newline(offset + 1, range.end);
                    let begin = self.sink.display_len;
                    self.sink.text(
                        self.source,
                        offset + 1..offset + 1 + newline,
                        "\n",
                        style,
                        newline == 2,
                    )?;
                    self.sink
                        .syntax(offset..offset + 1, begin..self.sink.display_len)?;
                    offset += 1 + newline;
                    continue;
                }
                b'\\' if offset + 1 < range.end && bytes[offset + 1].is_ascii_punctuation() => {
                    self.sink.text(
                        self.source,
                        offset..offset + 2,
                        &self.source[offset + 1..offset + 2],
                        style,
                        true,
                    )?;
                    offset += 2;
                    continue;
                }
                b'`' => {
                    let width = self.width(offset, range.end);
                    if let Some(close) = self.close(offset + width, range.end, b'`', width)? {
                        let begin = self.sink.display_len;
                        let mut code = style;
                        code.code = true;
                        self.code(offset + width..close, code)?;
                        let display = begin..self.sink.display_len;
                        self.sink.syntax(offset..offset + width, display.clone())?;
                        self.sink.syntax(close..close + width, display)?;
                        offset = close + width;
                        continue;
                    }
                    self.raw(offset..offset + width, style)?;
                    offset += width;
                    continue;
                }
                b'*' | b'_' | b'~' => {
                    let marker = bytes[offset];
                    let width = self.width(offset, range.end);
                    let supported = match marker {
                        b'~' => width == 2,
                        _ => width <= 3,
                    };
                    let intraword = marker == b'_'
                        && offset > range.start
                        && self.source[..offset]
                            .chars()
                            .next_back()
                            .is_some_and(char::is_alphanumeric);
                    if supported && !intraword {
                        if let Some(close) = self.close(offset + width, range.end, marker, width)? {
                            let inner = offset + width..close;
                            let text = &self.source[inner.clone()];
                            if !text.is_empty()
                                && !text.starts_with(char::is_whitespace)
                                && !text.ends_with(char::is_whitespace)
                            {
                                let mut nested = style;
                                if marker == b'~' {
                                    nested.strikethrough = true;
                                } else {
                                    nested.emphasis |= width == 1 || width == 3;
                                    nested.strong |= width >= 2;
                                }
                                let begin = self.sink.display_len;
                                self.inline(inner, nested, depth + 1)?;
                                let display = begin..self.sink.display_len;
                                self.sink.syntax(offset..offset + width, display.clone())?;
                                self.sink.syntax(close..close + width, display)?;
                                offset = close + width;
                                continue;
                            }
                        }
                    }
                    self.raw(offset..offset + width, style)?;
                    offset += width;
                    continue;
                }
                b'[' if offset == range.start || bytes[offset - 1] != b'!' => {
                    if let Some((label_end, end)) = self.link(offset, range.end)? {
                        let begin = self.sink.display_len;
                        let mut nested = style;
                        nested.link_label = true;
                        self.inline(offset + 1..label_end, nested, depth + 1)?;
                        let display = begin..self.sink.display_len;
                        self.sink.syntax(offset..offset + 1, display.clone())?;
                        self.sink.syntax(label_end..end, display)?;
                        offset = end;
                        continue;
                    }
                }
                _ => {}
            }
            let end = offset
                + self.source[offset..range.end]
                    .chars()
                    .next()
                    .ok_or(())?
                    .len_utf8();
            self.raw(offset..end, style)?;
            offset = end;
        }
        Ok(())
    }

    fn width(&self, offset: usize, end: usize) -> usize {
        let bytes = self.source.as_bytes();
        let marker = bytes[offset];
        bytes[offset..end]
            .iter()
            .take_while(|value| **value == marker)
            .count()
    }

    fn newline(&self, offset: usize, end: usize) -> usize {
        if offset >= end {
            return 0;
        }
        match self.source.as_bytes()[offset] {
            b'\n' => 1,
            b'\r' if offset + 1 < end && self.source.as_bytes()[offset + 1] == b'\n' => 2,
            _ => 0,
        }
    }

    fn code(&mut self, range: Range<usize>, style: NarrativeStyle) -> Result<(), ()> {
        let mut offset = range.start;
        let begin = self.sink.display_len;
        let text = &self.source[range.clone()];
        let trim = text.len() >= 2
            && text.starts_with(' ')
            && text.ends_with(' ')
            && text.bytes().any(|byte| byte != b' ');
        let end = range.end - usize::from(trim);
        offset += usize::from(trim);
        while offset < end {
            self.spend(1)?;
            let newline = self.newline(offset, end);
            if newline > 0 {
                self.sink
                    .text(self.source, offset..offset + newline, " ", style, true)?;
                offset += newline;
            } else {
                let next = offset
                    + self.source[offset..end]
                        .chars()
                        .next()
                        .ok_or(())?
                        .len_utf8();
                self.raw(offset..next, style)?;
                offset = next;
            }
        }
        if trim {
            let display = begin..self.sink.display_len;
            self.sink
                .syntax(range.start..range.start + 1, display.clone())?;
            self.sink.syntax(range.end - 1..range.end, display)?;
        }
        Ok(())
    }

    fn close(
        &mut self,
        mut offset: usize,
        end: usize,
        marker: u8,
        width: usize,
    ) -> Result<Option<usize>, ()> {
        let bytes = self.source.as_bytes();
        while offset < end {
            self.spend(1)?;
            if bytes[offset] == b'\\' && marker != b'`' {
                offset += 2;
                continue;
            }
            if marker != b'`' && bytes[offset] == b'`' {
                let code_width = self.width(offset, end);
                if let Some(close) = self.close(offset + code_width, end, b'`', code_width)? {
                    offset = close + code_width;
                    continue;
                }
            }
            if marker != b'`' && bytes[offset] == b'[' {
                if let Some((_, link_end)) = self.link(offset, end)? {
                    offset = link_end;
                    continue;
                }
            }
            if bytes[offset] == marker {
                let count = self.width(offset, end);
                let intraword = marker == b'_'
                    && offset + count < end
                    && self.source[offset + count..end]
                        .chars()
                        .next()
                        .is_some_and(char::is_alphanumeric)
                    && self.source[..offset]
                        .chars()
                        .next_back()
                        .is_some_and(char::is_alphanumeric);
                if count == width && !intraword {
                    return Ok(Some(offset));
                }
                offset += count;
            } else {
                offset += 1;
            }
        }
        Ok(None)
    }

    fn link(&mut self, open: usize, end: usize) -> Result<Option<(usize, usize)>, ()> {
        let bytes = self.source.as_bytes();
        let mut offset = open + 1;
        while offset < end {
            self.spend(1)?;
            match bytes[offset] {
                b'\\' => {
                    offset += 2;
                    continue;
                }
                b'[' => return Ok(None),
                b']' => break,
                _ => offset += 1,
            }
        }
        if offset == end || bytes.get(offset + 1) != Some(&b'(') {
            return Ok(None);
        }
        let label_end = offset;
        offset += 2;
        let mut nesting = 1;
        while offset < end {
            self.spend(1)?;
            match bytes[offset] {
                b'\\' => {
                    offset += 2;
                    continue;
                }
                b'(' => {
                    nesting += 1;
                    if nesting > NARRATIVE_MAX_DEPTH {
                        return Err(());
                    }
                }
                b')' => {
                    nesting -= 1;
                    if nesting == 0 {
                        return Ok(Some((label_end, offset + 1)));
                    }
                }
                _ => {}
            }
            offset += 1;
        }
        Ok(None)
    }
}
