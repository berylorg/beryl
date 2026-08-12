use std::{
    error::Error,
    fmt,
    io::{self, BufRead, Write},
    mem::size_of,
    num::{NonZeroU64, NonZeroUsize},
};

use super::{
    InstalledThemeId, InstalledThemeSummary, ThemeHomeIdentity, ThemeManifestCursor,
    ThemeManifestGeneration, ThemeManifestIdentity, ThemeManifestPage, ThemeName, ThemePageError,
    ThemePageLimits,
};

pub const THEME_MANIFEST_SCHEMA_VERSION: u64 = 1;
pub const THEME_MANIFEST_LINE_MAX_BYTES: usize = 4 * 1024;
pub const THEME_MANIFEST_HEADER_MAX_BYTES: usize = 16 * 1024;
pub const THEME_MANIFEST_PAGE_MAX_ENCODED_BYTES: usize = 256 * 1024;

/// Caller-selected bounds for incremental manifest decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeManifestReadLimits {
    max_line_bytes: NonZeroUsize,
    max_header_encoded_bytes: NonZeroUsize,
    max_page_encoded_bytes: NonZeroUsize,
}

impl ThemeManifestReadLimits {
    pub fn new(
        max_line_bytes: NonZeroUsize,
        max_header_encoded_bytes: NonZeroUsize,
        max_page_encoded_bytes: NonZeroUsize,
    ) -> Result<Self, ThemeManifestDecodeError> {
        if max_line_bytes.get() > THEME_MANIFEST_LINE_MAX_BYTES
            || max_header_encoded_bytes.get() > THEME_MANIFEST_HEADER_MAX_BYTES
            || max_page_encoded_bytes.get() > THEME_MANIFEST_PAGE_MAX_ENCODED_BYTES
        {
            return Err(ThemeManifestDecodeError::LimitsTooLarge);
        }
        Ok(Self {
            max_line_bytes,
            max_header_encoded_bytes,
            max_page_encoded_bytes,
        })
    }

    #[must_use]
    pub const fn max_line_bytes(self) -> usize {
        self.max_line_bytes.get()
    }

    #[must_use]
    pub const fn max_header_encoded_bytes(self) -> usize {
        self.max_header_encoded_bytes.get()
    }

    #[must_use]
    pub const fn max_page_encoded_bytes(self) -> usize {
        self.max_page_encoded_bytes.get()
    }
}

/// The validated bounded header of one installed-theme manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeManifestHeader {
    identity: ThemeManifestIdentity,
}

impl ThemeManifestHeader {
    #[must_use]
    pub(crate) const fn new(identity: ThemeManifestIdentity) -> Self {
        Self { identity }
    }

    #[must_use]
    pub const fn identity(self) -> ThemeManifestIdentity {
        self.identity
    }

    #[must_use]
    pub const fn schema_version(self) -> u64 {
        THEME_MANIFEST_SCHEMA_VERSION
    }
}

/// A forward-only decoder which retains at most one manifest row between pages.
pub(crate) struct ThemeManifestDecoder<R> {
    reader: R,
    limits: ThemeManifestReadLimits,
    header: ThemeManifestHeader,
    line_number: u64,
    next_order: u64,
    row_started: bool,
    row_id: Option<InstalledThemeId>,
    row_name: Option<ThemeName>,
    pending: Option<InstalledThemeSummary>,
    eof: bool,
}

impl<R: BufRead> ThemeManifestDecoder<R> {
    pub(crate) fn open(
        mut reader: R,
        home: ThemeHomeIdentity,
        limits: ThemeManifestReadLimits,
    ) -> Result<Self, ThemeManifestDecodeError> {
        let mut line_number = 0_u64;
        let mut header_bytes = 0_usize;
        let mut schema_version = None;
        let mut generation = None;
        let mut first_row = false;

        loop {
            let Some(line) = read_bounded_line(
                &mut reader,
                limits.max_line_bytes(),
                &mut line_number,
                &mut header_bytes,
                limits.max_header_encoded_bytes(),
                ThemeManifestLimit::HeaderEncodedBytes,
            )?
            else {
                break;
            };
            let line = decode_line(&line, line_number)?;
            let line = strip_comment(line)?.trim();
            if line.is_empty() {
                continue;
            }
            if line == "[[theme]]" {
                first_row = true;
                break;
            }
            let (key, value) = assignment(line, line_number)?;
            match key {
                "schema_version" => {
                    if schema_version
                        .replace(parse_u64(value, line_number)?)
                        .is_some()
                    {
                        return Err(ThemeManifestDecodeError::DuplicateHeaderField {
                            field: "schema_version",
                        });
                    }
                }
                "generation" => {
                    if generation.replace(parse_u64(value, line_number)?).is_some() {
                        return Err(ThemeManifestDecodeError::DuplicateHeaderField {
                            field: "generation",
                        });
                    }
                }
                _ => {
                    return Err(ThemeManifestDecodeError::UnknownHeaderField { line: line_number });
                }
            }
        }

        let schema_version =
            schema_version.ok_or(ThemeManifestDecodeError::MissingSchemaVersion)?;
        if schema_version != THEME_MANIFEST_SCHEMA_VERSION {
            return Err(ThemeManifestDecodeError::UnsupportedSchemaVersion(
                schema_version,
            ));
        }
        let generation = generation.ok_or(ThemeManifestDecodeError::MissingGeneration)?;
        let generation = NonZeroU64::new(generation)
            .map(ThemeManifestGeneration::new)
            .ok_or(ThemeManifestDecodeError::InvalidGeneration)?;
        let identity = ThemeManifestIdentity::new(home, generation);

        Ok(Self {
            reader,
            limits,
            header: ThemeManifestHeader { identity },
            line_number,
            next_order: 0,
            row_started: first_row,
            row_id: None,
            row_name: None,
            pending: None,
            eof: !first_row,
        })
    }

    #[must_use]
    pub(crate) const fn header(&self) -> ThemeManifestHeader {
        self.header
    }

    pub(crate) fn bind_identity(
        &mut self,
        identity: ThemeManifestIdentity,
    ) -> Result<(), ThemeManifestDecodeError> {
        if identity.home() != self.header.identity().home()
            || identity.generation() != self.header.identity().generation()
        {
            return Err(ThemeManifestDecodeError::CursorMismatch);
        }
        self.header = ThemeManifestHeader::new(identity);
        Ok(())
    }

    #[must_use]
    pub(crate) const fn first_cursor(&self) -> ThemeManifestCursor {
        ThemeManifestCursor::first(self.header.identity())
    }

    pub(crate) fn read_page(
        &mut self,
        cursor: ThemeManifestCursor,
        page_limits: ThemePageLimits,
    ) -> Result<ThemeManifestPage, ThemeManifestDecodeError> {
        if cursor.manifest() != self.header.identity() || cursor.next_order() != self.next_order {
            return Err(ThemeManifestDecodeError::CursorMismatch);
        }

        let mut records = Vec::with_capacity(page_limits.max_items().min(16));
        let mut decoded_bytes = 0_usize;
        let mut encoded_bytes = 0_usize;
        let has_more;

        loop {
            let record = if let Some(record) = self.pending.take() {
                Some(record)
            } else {
                self.read_record(&mut encoded_bytes)?
            };
            let Some(record) = record else {
                has_more = false;
                break;
            };
            let record_bytes = decoded_record_bytes(&record)?;
            let would_exceed = records.len() == page_limits.max_items()
                || decoded_bytes
                    .checked_add(record_bytes)
                    .is_none_or(|bytes| bytes > page_limits.max_decoded_bytes());
            if would_exceed {
                self.pending = Some(record);
                if records.is_empty() {
                    return Err(ThemeManifestDecodeError::Page(
                        ThemePageError::LimitExceeded,
                    ));
                }
                has_more = true;
                break;
            }
            decoded_bytes += record_bytes;
            records.push(record);
            self.next_order = self
                .next_order
                .checked_add(1)
                .ok_or(ThemeManifestDecodeError::OrderExhausted)?;
        }

        ThemeManifestPage::checked(cursor, records, has_more, page_limits)
            .map_err(ThemeManifestDecodeError::Page)
    }

    fn read_record(
        &mut self,
        encoded_bytes: &mut usize,
    ) -> Result<Option<InstalledThemeSummary>, ThemeManifestDecodeError> {
        if self.eof {
            return Ok(None);
        }
        loop {
            let line = read_bounded_line(
                &mut self.reader,
                self.limits.max_line_bytes(),
                &mut self.line_number,
                encoded_bytes,
                self.limits.max_page_encoded_bytes(),
                ThemeManifestLimit::PageEncodedBytes,
            )?;
            let Some(line) = line else {
                self.eof = true;
                return self.finish_row().map(Some);
            };
            let line = decode_line(&line, self.line_number)?;
            let line = strip_comment(line)?.trim();
            if line.is_empty() {
                continue;
            }
            if line == "[[theme]]" {
                let completed = self.finish_row()?;
                self.row_started = true;
                return Ok(Some(completed));
            }
            if !self.row_started {
                return Err(ThemeManifestDecodeError::ExpectedThemeTable {
                    line: self.line_number,
                });
            }
            let (key, value) = assignment(line, self.line_number)?;
            match key {
                "id" => {
                    if self.row_id.is_some() {
                        return Err(ThemeManifestDecodeError::DuplicateThemeField {
                            order: self.next_order,
                            field: "id",
                        });
                    }
                    let value = parse_toml_string(value, self.line_number)?;
                    self.row_id = Some(InstalledThemeId::new(value).map_err(|_| {
                        ThemeManifestDecodeError::InvalidThemeId {
                            order: self.next_order,
                        }
                    })?);
                }
                "name" => {
                    if self.row_name.is_some() {
                        return Err(ThemeManifestDecodeError::DuplicateThemeField {
                            order: self.next_order,
                            field: "name",
                        });
                    }
                    let value = parse_toml_string(value, self.line_number)?;
                    self.row_name = Some(ThemeName::new(value).map_err(|_| {
                        ThemeManifestDecodeError::InvalidThemeName {
                            order: self.next_order,
                        }
                    })?);
                }
                _ => {
                    return Err(ThemeManifestDecodeError::UnknownThemeField {
                        line: self.line_number,
                    });
                }
            }
        }
    }

    fn finish_row(&mut self) -> Result<InstalledThemeSummary, ThemeManifestDecodeError> {
        let id = self
            .row_id
            .take()
            .ok_or(ThemeManifestDecodeError::MissingThemeField {
                order: self.next_order,
                field: "id",
            })?;
        let name = self
            .row_name
            .take()
            .ok_or(ThemeManifestDecodeError::MissingThemeField {
                order: self.next_order,
                field: "name",
            })?;
        Ok(InstalledThemeSummary::new(id, name, self.next_order))
    }
}

fn decoded_record_bytes(record: &InstalledThemeSummary) -> Result<usize, ThemeManifestDecodeError> {
    record
        .id()
        .as_str()
        .len()
        .checked_add(record.name().as_str().len())
        .and_then(|bytes| bytes.checked_add(size_of::<u64>()))
        .ok_or(ThemeManifestDecodeError::DecodedBytesOverflow)
}

/// Metadata proved while canonically encoding one complete manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThemeManifestEncoding {
    generation: ThemeManifestGeneration,
    row_count: u64,
    encoded_bytes: u64,
}

/// A canonical forward-only manifest encoder.
pub(crate) struct ThemeManifestEncoder<W> {
    writer: W,
    generation: ThemeManifestGeneration,
    next_order: u64,
    encoded_bytes: u64,
}

impl<W: Write> ThemeManifestEncoder<W> {
    pub(crate) fn new(
        mut writer: W,
        generation: ThemeManifestGeneration,
    ) -> Result<Self, ThemeManifestEncodeError> {
        let header = format!(
            "schema_version = {THEME_MANIFEST_SCHEMA_VERSION}\ngeneration = {}\n\n",
            generation.get()
        );
        writer.write_all(header.as_bytes())?;
        Ok(Self {
            writer,
            generation,
            next_order: 0,
            encoded_bytes: u64::try_from(header.len())
                .map_err(|_| ThemeManifestEncodeError::EncodedLengthOverflow)?,
        })
    }

    pub(crate) fn write_theme(
        &mut self,
        theme: &InstalledThemeSummary,
    ) -> Result<(), ThemeManifestEncodeError> {
        if theme.order() != self.next_order {
            return Err(ThemeManifestEncodeError::NonContiguousOrder {
                expected: self.next_order,
                actual: theme.order(),
            });
        }
        self.write_bytes(b"[[theme]]\nid = ")?;
        self.write_basic_string(theme.id().as_str())?;
        self.write_bytes(b"\nname = ")?;
        self.write_basic_string(theme.name().as_str())?;
        self.write_bytes(b"\n\n")?;
        self.next_order = self
            .next_order
            .checked_add(1)
            .ok_or(ThemeManifestEncodeError::OrderExhausted)?;
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<(W, ThemeManifestEncoding), ThemeManifestEncodeError> {
        self.writer.flush()?;
        let encoding = ThemeManifestEncoding {
            generation: self.generation,
            row_count: self.next_order,
            encoded_bytes: self.encoded_bytes,
        };
        Ok((self.writer, encoding))
    }

    fn write_basic_string(&mut self, value: &str) -> Result<(), ThemeManifestEncodeError> {
        self.write_bytes(b"\"")?;
        for character in value.chars() {
            match character {
                '\u{08}' => self.write_bytes(br"\b")?,
                '\t' => self.write_bytes(br"\t")?,
                '\n' => self.write_bytes(br"\n")?,
                '\u{0c}' => self.write_bytes(br"\f")?,
                '\r' => self.write_bytes(br"\r")?,
                '"' => self.write_bytes(br#"\""#)?,
                '\\' => self.write_bytes(br"\\")?,
                character if character.is_control() => {
                    let escape = if u32::from(character) <= 0xffff {
                        format!(r"\u{:04X}", u32::from(character))
                    } else {
                        format!(r"\U{:08X}", u32::from(character))
                    };
                    self.write_bytes(escape.as_bytes())?;
                }
                character => {
                    let mut encoded = [0_u8; 4];
                    self.write_bytes(character.encode_utf8(&mut encoded).as_bytes())?;
                }
            }
        }
        self.write_bytes(b"\"")
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), ThemeManifestEncodeError> {
        self.writer.write_all(bytes)?;
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(
                u64::try_from(bytes.len())
                    .map_err(|_| ThemeManifestEncodeError::EncodedLengthOverflow)?,
            )
            .ok_or(ThemeManifestEncodeError::EncodedLengthOverflow)?;
        Ok(())
    }
}

fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    max_line_bytes: usize,
    line_number: &mut u64,
    encoded_bytes: &mut usize,
    max_encoded_bytes: usize,
    encoded_limit: ThemeManifestLimit,
) -> Result<Option<Vec<u8>>, ThemeManifestDecodeError> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if line.is_empty() {
                return Ok(None);
            }
            *line_number = line_number
                .checked_add(1)
                .ok_or(ThemeManifestDecodeError::LineNumberOverflow)?;
            break;
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let content_len = newline.unwrap_or(available.len());
        let consumed = content_len + usize::from(newline.is_some());
        if line
            .len()
            .checked_add(content_len)
            .is_none_or(|bytes| bytes > max_line_bytes)
        {
            return Err(ThemeManifestDecodeError::LineTooLong {
                line: line_number.saturating_add(1),
            });
        }
        let next_encoded = encoded_bytes
            .checked_add(consumed)
            .ok_or(ThemeManifestDecodeError::EncodedBytesOverflow)?;
        if next_encoded > max_encoded_bytes {
            return Err(ThemeManifestDecodeError::LimitExceeded(encoded_limit));
        }
        line.extend_from_slice(&available[..content_len]);
        reader.consume(consumed);
        *encoded_bytes = next_encoded;
        if newline.is_some() {
            *line_number = line_number
                .checked_add(1)
                .ok_or(ThemeManifestDecodeError::LineNumberOverflow)?;
            break;
        }
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(Some(line))
}

fn decode_line(bytes: &[u8], line: u64) -> Result<&str, ThemeManifestDecodeError> {
    std::str::from_utf8(bytes).map_err(|_| ThemeManifestDecodeError::InvalidUtf8 { line })
}

fn strip_comment(line: &str) -> Result<&str, ThemeManifestDecodeError> {
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
        } else if character == '"' {
            quoted = true;
        } else if character == '#' {
            return Ok(&line[..index]);
        }
    }
    Ok(line)
}

fn assignment(line: &str, line_number: u64) -> Result<(&str, &str), ThemeManifestDecodeError> {
    let (key, value) = line
        .split_once('=')
        .ok_or(ThemeManifestDecodeError::MalformedAssignment { line: line_number })?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        return Err(ThemeManifestDecodeError::MalformedAssignment { line: line_number });
    }
    Ok((key, value))
}

fn parse_u64(value: &str, line: u64) -> Result<u64, ThemeManifestDecodeError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ThemeManifestDecodeError::InvalidUnsignedInteger { line });
    }
    value
        .parse()
        .map_err(|_| ThemeManifestDecodeError::InvalidUnsignedInteger { line })
}

fn parse_toml_string(value: &str, line: u64) -> Result<String, ThemeManifestDecodeError> {
    let bytes = value.as_bytes();
    if bytes.first() != Some(&b'"') {
        return Err(ThemeManifestDecodeError::InvalidBasicString { line });
    }
    let mut decoded = String::with_capacity(value.len().saturating_sub(2));
    let mut index = 1_usize;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                if !value[index + 1..].trim().is_empty() {
                    return Err(ThemeManifestDecodeError::InvalidBasicString { line });
                }
                return Ok(decoded);
            }
            b'\\' => {
                index += 1;
                let escape = *bytes
                    .get(index)
                    .ok_or(ThemeManifestDecodeError::InvalidBasicString { line })?;
                match escape {
                    b'b' => decoded.push('\u{08}'),
                    b't' => decoded.push('\t'),
                    b'n' => decoded.push('\n'),
                    b'f' => decoded.push('\u{0c}'),
                    b'r' => decoded.push('\r'),
                    b'"' => decoded.push('"'),
                    b'\\' => decoded.push('\\'),
                    b'u' | b'U' => {
                        let digits = if escape == b'u' { 4 } else { 8 };
                        let start = index + 1;
                        let end = start + digits;
                        let raw = value
                            .get(start..end)
                            .ok_or(ThemeManifestDecodeError::InvalidBasicString { line })?;
                        if !raw.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                            return Err(ThemeManifestDecodeError::InvalidBasicString { line });
                        }
                        let scalar = u32::from_str_radix(raw, 16)
                            .map_err(|_| ThemeManifestDecodeError::InvalidBasicString { line })?;
                        decoded.push(
                            char::from_u32(scalar)
                                .ok_or(ThemeManifestDecodeError::InvalidBasicString { line })?,
                        );
                        index = end - 1;
                    }
                    _ => return Err(ThemeManifestDecodeError::InvalidBasicString { line }),
                }
                index += 1;
            }
            byte if byte < 0x20 || byte == 0x7f => {
                return Err(ThemeManifestDecodeError::InvalidBasicString { line });
            }
            _ => {
                let character = value[index..]
                    .chars()
                    .next()
                    .ok_or(ThemeManifestDecodeError::InvalidBasicString { line })?;
                decoded.push(character);
                index += character.len_utf8();
            }
        }
    }
    Err(ThemeManifestDecodeError::InvalidBasicString { line })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeManifestLimit {
    HeaderEncodedBytes,
    PageEncodedBytes,
}

#[derive(Debug)]
pub enum ThemeManifestDecodeError {
    Io(io::Error),
    Page(ThemePageError),
    LimitsTooLarge,
    LimitExceeded(ThemeManifestLimit),
    LineTooLong { line: u64 },
    InvalidUtf8 { line: u64 },
    MalformedAssignment { line: u64 },
    InvalidUnsignedInteger { line: u64 },
    InvalidBasicString { line: u64 },
    MissingSchemaVersion,
    UnsupportedSchemaVersion(u64),
    MissingGeneration,
    InvalidGeneration,
    DuplicateHeaderField { field: &'static str },
    UnknownHeaderField { line: u64 },
    ExpectedThemeTable { line: u64 },
    DuplicateThemeField { order: u64, field: &'static str },
    MissingThemeField { order: u64, field: &'static str },
    UnknownThemeField { line: u64 },
    InvalidThemeId { order: u64 },
    InvalidThemeName { order: u64 },
    DuplicateThemeId { id: InstalledThemeId },
    CursorMismatch,
    OrderExhausted,
    LineNumberOverflow,
    EncodedBytesOverflow,
    DecodedBytesOverflow,
}

impl fmt::Display for ThemeManifestDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not read theme manifest: {error}"),
            Self::Page(error) => write!(formatter, "invalid theme manifest page: {error}"),
            Self::LimitsTooLarge => {
                formatter.write_str("theme manifest read limits exceed hard bounds")
            }
            Self::LimitExceeded(limit) => write!(formatter, "theme manifest exceeded {limit}"),
            Self::LineTooLong { line } => {
                write!(formatter, "theme manifest line {line} is too long")
            }
            Self::InvalidUtf8 { line } => {
                write!(formatter, "theme manifest line {line} is not UTF-8")
            }
            Self::MalformedAssignment { line } => {
                write!(
                    formatter,
                    "theme manifest line {line} is not a compact TOML assignment"
                )
            }
            Self::InvalidUnsignedInteger { line } => {
                write!(
                    formatter,
                    "theme manifest line {line} is not an unsigned integer"
                )
            }
            Self::InvalidBasicString { line } => {
                write!(
                    formatter,
                    "theme manifest line {line} is not a supported TOML basic string"
                )
            }
            Self::MissingSchemaVersion => {
                formatter.write_str("theme manifest schema_version is missing")
            }
            Self::UnsupportedSchemaVersion(version) => {
                write!(
                    formatter,
                    "theme manifest schema version {version} is unsupported"
                )
            }
            Self::MissingGeneration => formatter.write_str("theme manifest generation is missing"),
            Self::InvalidGeneration => {
                formatter.write_str("theme manifest generation must be nonzero")
            }
            Self::DuplicateHeaderField { field } => {
                write!(
                    formatter,
                    "theme manifest header field {field} is duplicated"
                )
            }
            Self::UnknownHeaderField { line } => {
                write!(
                    formatter,
                    "theme manifest line {line} has an unknown header field"
                )
            }
            Self::ExpectedThemeTable { line } => {
                write!(
                    formatter,
                    "theme manifest line {line} appears outside [[theme]]"
                )
            }
            Self::DuplicateThemeField { order, field } => {
                write!(formatter, "theme manifest row {order} duplicates {field}")
            }
            Self::MissingThemeField { order, field } => {
                write!(formatter, "theme manifest row {order} is missing {field}")
            }
            Self::UnknownThemeField { line } => {
                write!(
                    formatter,
                    "theme manifest line {line} has an unknown theme field"
                )
            }
            Self::InvalidThemeId { order } => {
                write!(formatter, "theme manifest row {order} has an invalid id")
            }
            Self::InvalidThemeName { order } => {
                write!(
                    formatter,
                    "theme manifest row {order} has an invalid display name"
                )
            }
            Self::DuplicateThemeId { id } => {
                write!(formatter, "theme manifest duplicates installed id {id}")
            }
            Self::CursorMismatch => {
                formatter.write_str("theme manifest cursor does not match decoder position")
            }
            Self::OrderExhausted => formatter.write_str("theme manifest order exhausted u64"),
            Self::LineNumberOverflow => formatter.write_str("theme manifest line count overflowed"),
            Self::EncodedBytesOverflow => {
                formatter.write_str("theme manifest encoded byte count overflowed")
            }
            Self::DecodedBytesOverflow => {
                formatter.write_str("theme manifest decoded byte count overflowed")
            }
        }
    }
}

impl Error for ThemeManifestDecodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Page(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ThemeManifestDecodeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub enum ThemeManifestEncodeError {
    Io(io::Error),
    NonContiguousOrder { expected: u64, actual: u64 },
    OrderExhausted,
    EncodedLengthOverflow,
}

impl fmt::Display for ThemeManifestEncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "could not write theme manifest: {error}"),
            Self::NonContiguousOrder { expected, actual } => write!(
                formatter,
                "theme manifest order is not contiguous: expected {expected}, found {actual}"
            ),
            Self::OrderExhausted => formatter.write_str("theme manifest order exhausted u64"),
            Self::EncodedLengthOverflow => {
                formatter.write_str("theme manifest encoded byte count overflowed")
            }
        }
    }
}

impl Error for ThemeManifestEncodeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ThemeManifestEncodeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl fmt::Display for ThemeManifestLimit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::HeaderEncodedBytes => "the header encoded-byte limit",
            Self::PageEncodedBytes => "the page encoded-byte limit",
        })
    }
}
