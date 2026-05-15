use std::ops::Range;

use jsonc_parser::{
    CollectOptions, CommentCollectionStrategy, ParseOptions, Scanner, ScannerOptions,
    ast::{ObjectPropName, Value},
    common::{Range as JsonRange, Ranged},
    parse_to_ast,
    tokens::Token,
};

use super::model::{SyntaxHighlight, SyntaxLanguage, SyntaxToken, SyntaxTokenRole};

pub(crate) fn highlight_json(source: &str) -> SyntaxHighlight {
    SyntaxHighlight::new(SyntaxLanguage::Json, highlight_json_tokens(source, 0))
}

pub(crate) fn highlight_jsonl(source: &str) -> SyntaxHighlight {
    let mut tokens = Vec::new();
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

        let line = &source[line_start..content_end];
        if !line.trim().is_empty() {
            tokens.extend(highlight_json_tokens(line, line_start));
        }
        line_start = line_end;
    }

    SyntaxHighlight::new(SyntaxLanguage::Jsonl, tokens)
}

fn highlight_json_tokens(source: &str, base_offset: usize) -> Vec<SyntaxToken> {
    let scanned = scan_json_tokens(source);
    let parse = parse_json_strict(source);
    let mut tokens = JsonTokenBuilder {
        source,
        base_offset,
        tokens: Vec::new(),
    };

    for (index, scanned_token) in scanned.tokens.iter().enumerate() {
        let mut role = role_for_json_token(&scanned_token.token);
        if scanned_token.is_always_invalid_in_strict_json()
            || scanned.any_error_overlaps(&scanned_token.range)
            || parse.any_error_overlaps(&scanned_token.range)
        {
            role = SyntaxTokenRole::SyntaxError;
        }

        if role == SyntaxTokenRole::SyntaxString
            && parse.string_token_is_key(&scanned, index, &scanned_token.range)
        {
            role = SyntaxTokenRole::SyntaxKey;
        }

        tokens.push_json_token(role, scanned_token.range.clone());
    }

    for range in scanned.error_ranges.iter().chain(parse.error_ranges.iter()) {
        if !scanned.any_token_overlaps(range) {
            tokens.push(SyntaxTokenRole::SyntaxError, range.clone());
        }
    }

    tokens.tokens
}

struct ScannedJson<'a> {
    tokens: Vec<ScannedJsonToken<'a>>,
    error_ranges: Vec<Range<usize>>,
}

impl ScannedJson<'_> {
    fn any_error_overlaps(&self, range: &Range<usize>) -> bool {
        self.error_ranges
            .iter()
            .any(|error_range| ranges_overlap(error_range, range))
    }

    fn any_token_overlaps(&self, range: &Range<usize>) -> bool {
        self.tokens
            .iter()
            .any(|token| ranges_overlap(&token.range, range))
    }
}

struct ScannedJsonToken<'a> {
    token: Token<'a>,
    range: Range<usize>,
}

impl ScannedJsonToken<'_> {
    fn is_always_invalid_in_strict_json(&self) -> bool {
        matches!(
            &self.token,
            Token::Word(_) | Token::CommentLine(_) | Token::CommentBlock(_)
        )
    }
}

struct ParsedJson {
    valid: bool,
    key_ranges: Vec<Range<usize>>,
    error_ranges: Vec<Range<usize>>,
}

impl ParsedJson {
    fn any_error_overlaps(&self, range: &Range<usize>) -> bool {
        self.error_ranges
            .iter()
            .any(|error_range| ranges_overlap(error_range, range))
    }

    fn string_token_is_key(
        &self,
        scanned: &ScannedJson<'_>,
        index: usize,
        range: &Range<usize>,
    ) -> bool {
        if self.valid {
            return self.key_ranges.iter().any(|key_range| key_range == range);
        }

        scanned
            .tokens
            .get(index + 1)
            .is_some_and(|token| matches!(&token.token, Token::Colon))
            && scanned.tokens[..index]
                .iter()
                .rev()
                .find(|token| {
                    !matches!(&token.token, Token::CommentLine(_) | Token::CommentBlock(_))
                })
                .is_some_and(|token| matches!(&token.token, Token::OpenBrace | Token::Comma))
    }
}

struct JsonTokenBuilder<'a> {
    source: &'a str,
    base_offset: usize,
    tokens: Vec<SyntaxToken>,
}

impl JsonTokenBuilder<'_> {
    fn push_json_token(&mut self, role: SyntaxTokenRole, range: Range<usize>) {
        if matches!(
            role,
            SyntaxTokenRole::SyntaxKey | SyntaxTokenRole::SyntaxString
        ) {
            self.push_string_like_token(role, range);
        } else {
            self.push(role, range);
        }
    }

    fn push_string_like_token(&mut self, role: SyntaxTokenRole, range: Range<usize>) {
        if self.source.get(range.clone()).is_none() {
            return;
        }

        let bytes = self.source.as_bytes();
        let mut content_start = range.start;
        let mut content_end = range.end;
        if bytes.get(range.start) == Some(&b'"') {
            self.push(
                SyntaxTokenRole::SyntaxStructuralPunctuation,
                range.start..range.start + 1,
            );
            content_start += 1;
        }
        if content_end > content_start && bytes.get(content_end - 1) == Some(&b'"') {
            self.push(
                SyntaxTokenRole::SyntaxStructuralPunctuation,
                content_end - 1..content_end,
            );
            content_end -= 1;
        }

        let mut chunk_start = content_start;
        let mut index = content_start;
        while index < content_end {
            if bytes[index] == b'\\' {
                self.push(role, chunk_start..index);
                let escape_end = json_escape_end(bytes, index, content_end);
                self.push(SyntaxTokenRole::SyntaxEscape, index..escape_end);
                index = escape_end;
                chunk_start = index;
            } else {
                index = next_char_end(self.source, index, content_end);
            }
        }
        self.push(role, chunk_start..content_end);
    }

    fn push(&mut self, role: SyntaxTokenRole, range: Range<usize>) {
        if range.is_empty() || self.source.get(range.clone()).is_none() {
            return;
        }
        self.tokens.push(SyntaxToken::new(
            role,
            self.base_offset + range.start..self.base_offset + range.end,
        ));
    }
}

fn scan_json_tokens(source: &str) -> ScannedJson<'_> {
    let mut scanner = Scanner::new(source, &strict_scanner_options());
    let mut tokens = Vec::new();
    let mut error_ranges = Vec::new();

    loop {
        match scanner.scan() {
            Ok(Some(token)) => {
                let range = scanner.token_start()..scanner.token_end();
                if source.get(range.clone()).is_some() {
                    tokens.push(ScannedJsonToken { token, range });
                }
            }
            Ok(None) => break,
            Err(error) => {
                if let Some(range) = source_range(source, error.range()) {
                    error_ranges.push(range);
                }
                break;
            }
        }
    }

    ScannedJson {
        tokens,
        error_ranges,
    }
}

fn parse_json_strict(source: &str) -> ParsedJson {
    match parse_to_ast(source, &strict_collect_options(), &strict_parse_options()) {
        Ok(result) => {
            let mut key_ranges = Vec::new();
            if let Some(value) = result.value.as_ref() {
                collect_key_ranges(source, value, &mut key_ranges);
            }
            ParsedJson {
                valid: true,
                key_ranges,
                error_ranges: Vec::new(),
            }
        }
        Err(error) => ParsedJson {
            valid: false,
            key_ranges: Vec::new(),
            error_ranges: source_range(source, error.range()).into_iter().collect(),
        },
    }
}

fn collect_key_ranges(source: &str, value: &Value<'_>, key_ranges: &mut Vec<Range<usize>>) {
    match value {
        Value::Object(object) => {
            for property in &object.properties {
                if let ObjectPropName::String(name) = &property.name {
                    if let Some(range) = source_range(source, name.range()) {
                        key_ranges.push(range);
                    }
                }
                collect_key_ranges(source, &property.value, key_ranges);
            }
        }
        Value::Array(array) => {
            for element in &array.elements {
                collect_key_ranges(source, element, key_ranges);
            }
        }
        Value::StringLit(_)
        | Value::NumberLit(_)
        | Value::BooleanLit(_)
        | Value::NullKeyword(_) => {}
    }
}

fn role_for_json_token(token: &Token<'_>) -> SyntaxTokenRole {
    match token {
        Token::OpenBrace
        | Token::CloseBrace
        | Token::OpenBracket
        | Token::CloseBracket
        | Token::Comma
        | Token::Colon => SyntaxTokenRole::SyntaxStructuralPunctuation,
        Token::String(_) => SyntaxTokenRole::SyntaxString,
        Token::Number(_) => SyntaxTokenRole::SyntaxNumber,
        Token::Boolean(_) => SyntaxTokenRole::SyntaxBoolean,
        Token::Null => SyntaxTokenRole::SyntaxNull,
        Token::Word(_) | Token::CommentLine(_) | Token::CommentBlock(_) => {
            SyntaxTokenRole::SyntaxError
        }
    }
}

fn strict_collect_options() -> CollectOptions {
    CollectOptions {
        comments: CommentCollectionStrategy::Off,
        tokens: false,
    }
}

fn strict_parse_options() -> ParseOptions {
    ParseOptions {
        allow_comments: false,
        allow_loose_object_property_names: false,
        allow_trailing_commas: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    }
}

fn strict_scanner_options() -> ScannerOptions {
    ScannerOptions {
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    }
}

fn source_range(source: &str, range: JsonRange) -> Option<Range<usize>> {
    let range = range.start..range.end;
    (!range.is_empty() && source.get(range.clone()).is_some()).then_some(range)
}

fn json_escape_end(bytes: &[u8], start: usize, content_end: usize) -> usize {
    if start + 1 >= content_end {
        return content_end;
    }
    if bytes.get(start + 1) == Some(&b'u') {
        (start + 6).min(content_end)
    } else {
        (start + 2).min(content_end)
    }
}

fn next_char_end(source: &str, start: usize, end: usize) -> usize {
    source[start..end]
        .chars()
        .next()
        .map_or(start, |ch| start + ch.len_utf8())
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}
