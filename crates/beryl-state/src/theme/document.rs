use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    io::{self, BufRead, Cursor},
};

use super::resolver::ThemeValidationDiagnostics;
use super::schema::CANONICAL_THEME_ROLE_COUNT;
use super::{
    InstalledThemeId, ThemeColor, ThemeDefinition, ThemeFontFamily, ThemeFontWeight,
    ThemeLogicalPixels, ThemePropertyId, ThemePropertySource, ThemeRoleDefinition, ThemeRoleId,
    ThemeValue, ThemeValueKind, canonical_theme_schema,
};

pub const THEME_DOCUMENT_SCHEMA_VERSION: u32 = 1;
pub const THEME_DOCUMENT_MAX_BYTES: usize = 256 * 1024;
pub const THEME_DOCUMENT_MAX_LINES: usize = 4096;
pub const THEME_DOCUMENT_MAX_LINE_BYTES: usize = 2048;
pub const THEME_DOCUMENT_MAX_ROLES: usize = CANONICAL_THEME_ROLE_COUNT;
pub const THEME_DOCUMENT_MAX_PROPERTY_ENTRIES: usize = 1024;
pub const THEME_DOCUMENT_MAX_OUTPUT_BYTES: usize = 256 * 1024;
pub const THEME_DOCUMENT_NAME_MAX_BYTES: usize = 128;

const IO_DIAGNOSTIC_MAX_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeParseMode {
    InstalledLoad,
    StrictCandidate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeDocument {
    id: Option<InstalledThemeId>,
    name: Option<Box<str>>,
    definition: ThemeDefinition,
}

impl ThemeDocument {
    pub fn new(
        id: Option<InstalledThemeId>,
        name: Option<&str>,
        definition: ThemeDefinition,
    ) -> Result<Self, ThemeDocumentError> {
        let name = name.map(validate_name).transpose()?;
        check_definition_bounds(&definition, 0)?;
        super::ThemeResolver::new(&definition).map_err(ThemeDocumentError::Validation)?;
        Ok(Self {
            id,
            name,
            definition,
        })
    }

    pub fn parse_reader<R: BufRead>(
        reader: R,
        mode: ThemeParseMode,
    ) -> Result<Self, ThemeDocumentError> {
        Parser::new(mode).parse(reader)
    }

    pub fn parse_bytes(bytes: &[u8], mode: ThemeParseMode) -> Result<Self, ThemeDocumentError> {
        Self::parse_reader(Cursor::new(bytes), mode)
    }

    #[must_use]
    pub const fn id(&self) -> Option<&InstalledThemeId> {
        self.id.as_ref()
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    #[must_use]
    pub const fn definition(&self) -> &ThemeDefinition {
        &self.definition
    }

    #[must_use]
    pub fn into_definition(self) -> ThemeDefinition {
        self.definition
    }

    pub fn to_canonical_toml(&self) -> Result<String, ThemeDocumentError> {
        let definition = serializable_definition(&self.definition)?;
        check_definition_bounds(&definition, 0)?;
        super::ThemeResolver::new(&definition).map_err(ThemeDocumentError::Validation)?;

        let mut output = String::new();
        push_bounded(&mut output, "schema = 1\n")?;
        if let Some(id) = &self.id {
            push_assignment(&mut output, "id", &string_literal(id.as_str()))?;
        }
        if let Some(name) = &self.name {
            push_assignment(&mut output, "name", &string_literal(name))?;
        }

        for role in definition.roles().values() {
            push_bounded(&mut output, "\n[[role]]\n")?;
            push_assignment(&mut output, "id", &string_literal(role.role_id().as_str()))?;
            if let Some(parent) = role.static_parent() {
                push_assignment(
                    &mut output,
                    "static_parent",
                    &string_literal(parent.as_str()),
                )?;
            }
            for (property, source) in role.properties() {
                push_assignment(&mut output, property.as_str(), &serialize_source(source))?;
            }
        }
        Ok(output)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ThemeDocumentError {
    Io(String),
    DocumentTooLarge,
    TooManyLines,
    LineTooLong {
        line: usize,
    },
    InvalidUtf8 {
        line: usize,
    },
    InvalidSyntax {
        line: usize,
    },
    InvalidSchema {
        line: usize,
    },
    DuplicateTopLevelKey {
        line: usize,
    },
    InvalidId {
        line: usize,
    },
    InvalidName {
        line: usize,
    },
    TooManyRoles {
        line: usize,
    },
    TooManyPropertyEntries {
        line: usize,
    },
    MissingRoleId {
        line: usize,
    },
    DuplicateRole {
        line: usize,
        role: Box<str>,
    },
    UnknownRole {
        line: usize,
        role: Box<str>,
    },
    DuplicateRoleKey {
        line: usize,
        key: Box<str>,
    },
    UnknownProperty {
        line: usize,
        role: Box<str>,
        property: Box<str>,
    },
    InvalidStaticParent {
        line: usize,
        role: Box<str>,
    },
    InvalidPropertySource {
        line: usize,
        role: Box<str>,
        property: Box<str>,
    },
    Validation(ThemeValidationDiagnostics),
    OutputTooLarge,
}

impl fmt::Display for ThemeDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for ThemeDocumentError {}

struct Parser {
    mode: ThemeParseMode,
    top_level_keys: BTreeSet<Box<str>>,
    schema_seen: bool,
    id: Option<InstalledThemeId>,
    name: Option<Box<str>>,
    current_role: Option<RoleBuilder>,
    skipping_unknown_table: bool,
    role_count: usize,
    property_count: usize,
    seen_role_ids: BTreeSet<Box<str>>,
    roles: BTreeMap<ThemeRoleId, ThemeRoleDefinition>,
}

impl Parser {
    fn new(mode: ThemeParseMode) -> Self {
        Self {
            mode,
            top_level_keys: BTreeSet::new(),
            schema_seen: false,
            id: None,
            name: None,
            current_role: None,
            skipping_unknown_table: false,
            role_count: 0,
            property_count: 0,
            seen_role_ids: BTreeSet::new(),
            roles: BTreeMap::new(),
        }
    }

    fn parse<R: BufRead>(mut self, reader: R) -> Result<ThemeDocument, ThemeDocumentError> {
        let mut lines = BoundedLines::new(reader);
        while let Some((line_number, line)) = lines.next_line()? {
            let structural = strip_comment(&line, line_number)?;
            let structural = structural.trim();
            if structural.is_empty() {
                continue;
            }
            if structural == "[[role]]" {
                self.finish_role()?;
                self.skipping_unknown_table = false;
                self.role_count = self
                    .role_count
                    .checked_add(1)
                    .ok_or(ThemeDocumentError::TooManyRoles { line: line_number })?;
                if self.role_count > THEME_DOCUMENT_MAX_ROLES {
                    return Err(ThemeDocumentError::TooManyRoles { line: line_number });
                }
                self.current_role = Some(RoleBuilder::new(line_number));
                continue;
            }
            if structural.starts_with('[') {
                if self.mode == ThemeParseMode::InstalledLoad && structural.ends_with(']') {
                    self.finish_role()?;
                    self.skipping_unknown_table = true;
                    continue;
                }
                return Err(ThemeDocumentError::InvalidSyntax { line: line_number });
            }

            let (key, value) = split_assignment(structural, line_number)?;
            if self.skipping_unknown_table {
                continue;
            }
            if let Some(role) = self.current_role.as_mut() {
                if key != "id" && key != "static_parent" {
                    self.property_count = self
                        .property_count
                        .checked_add(1)
                        .ok_or(ThemeDocumentError::TooManyPropertyEntries { line: line_number })?;
                    if self.property_count > THEME_DOCUMENT_MAX_PROPERTY_ENTRIES {
                        return Err(ThemeDocumentError::TooManyPropertyEntries {
                            line: line_number,
                        });
                    }
                }
                role.insert(key, value, line_number)?;
            } else {
                if self.mode == ThemeParseMode::InstalledLoad
                    && !matches!(key, "schema" | "id" | "name")
                {
                    continue;
                }
                self.insert_top_level(key, parse_raw_value(value, line_number)?, line_number)?;
            }
        }

        self.finish_role()?;
        if !self.schema_seen {
            return Err(ThemeDocumentError::InvalidSchema { line: 0 });
        }
        let definition = ThemeDefinition::checked(self.roles)
            .map_err(|_| ThemeDocumentError::InvalidSyntax { line: 0 })?;
        super::ThemeResolver::new(&definition).map_err(ThemeDocumentError::Validation)?;
        Ok(ThemeDocument {
            id: self.id,
            name: self.name,
            definition,
        })
    }

    fn insert_top_level(
        &mut self,
        key: &str,
        value: RawValue,
        line: usize,
    ) -> Result<(), ThemeDocumentError> {
        if !matches!(key, "schema" | "id" | "name") {
            if self.mode == ThemeParseMode::InstalledLoad {
                return Ok(());
            }
            return Err(ThemeDocumentError::InvalidSyntax { line });
        }
        if !self.top_level_keys.insert(key.into()) {
            return Err(ThemeDocumentError::DuplicateTopLevelKey { line });
        }
        match key {
            "schema" => {
                let RawValue::Bare(value) = value else {
                    return Err(ThemeDocumentError::InvalidSchema { line });
                };
                let normalized =
                    normalize_number(&value).ok_or(ThemeDocumentError::InvalidSchema { line })?;
                if normalized.parse::<u32>().ok() != Some(THEME_DOCUMENT_SCHEMA_VERSION) {
                    return Err(ThemeDocumentError::InvalidSchema { line });
                }
                self.schema_seen = true;
            }
            "id" => {
                let RawValue::String(value) = value else {
                    return Err(ThemeDocumentError::InvalidId { line });
                };
                self.id = Some(
                    InstalledThemeId::new(value)
                        .map_err(|_| ThemeDocumentError::InvalidId { line })?,
                );
            }
            "name" => {
                let RawValue::String(value) = value else {
                    return Err(ThemeDocumentError::InvalidName { line });
                };
                self.name = Some(validate_name_at(&value, line)?);
            }
            _ => unreachable!("top-level key was matched above"),
        }
        Ok(())
    }

    fn finish_role(&mut self) -> Result<(), ThemeDocumentError> {
        let Some(role) = self.current_role.take() else {
            return Ok(());
        };
        let (role_id, id_line) = role.id.ok_or(ThemeDocumentError::MissingRoleId {
            line: role.header_line,
        })?;
        let RawValue::String(role_id) = role_id else {
            return Err(ThemeDocumentError::InvalidSyntax { line: id_line });
        };
        let schema = canonical_theme_schema();
        let Some(role_schema) = schema.role(&role_id) else {
            return match self.mode {
                ThemeParseMode::InstalledLoad => Ok(()),
                ThemeParseMode::StrictCandidate => Err(ThemeDocumentError::UnknownRole {
                    line: id_line,
                    role: role_id.into(),
                }),
            };
        };
        if !self.seen_role_ids.insert(role_id.as_str().into()) {
            return Err(ThemeDocumentError::DuplicateRole {
                line: id_line,
                role: role_id.into(),
            });
        }

        let static_parent = match role.static_parent {
            None => None,
            Some((RawValue::String(parent), _)) if schema.role(&parent).is_some() => Some(
                schema
                    .role(&parent)
                    .expect("role existence was checked")
                    .id()
                    .clone(),
            ),
            Some((_, line)) => {
                return Err(ThemeDocumentError::InvalidStaticParent {
                    line,
                    role: role_id.into(),
                });
            }
        };

        let mut properties = BTreeMap::new();
        for entry in role.properties {
            let Some(property) = ThemePropertyId::from_str(&entry.key) else {
                if self.mode == ThemeParseMode::InstalledLoad {
                    continue;
                }
                return Err(ThemeDocumentError::UnknownProperty {
                    line: entry.line,
                    role: role_id.as_str().into(),
                    property: entry.key,
                });
            };
            let Some(property_schema) = role_schema.property(property) else {
                if self.mode == ThemeParseMode::InstalledLoad {
                    continue;
                }
                return Err(ThemeDocumentError::UnknownProperty {
                    line: entry.line,
                    role: role_id.as_str().into(),
                    property: entry.key,
                });
            };
            let parsed = parse_raw_value(&entry.value, entry.line)?;
            let source =
                parse_property_source(parsed, property_schema.kind()).ok_or_else(|| {
                    ThemeDocumentError::InvalidPropertySource {
                        line: entry.line,
                        role: role_id.as_str().into(),
                        property: entry.key.clone(),
                    }
                })?;
            properties.insert(property, source);
        }

        let canonical_id = role_schema.id().clone();
        self.roles.insert(
            canonical_id.clone(),
            ThemeRoleDefinition::new(canonical_id, static_parent, properties),
        );
        Ok(())
    }
}

struct RoleBuilder {
    header_line: usize,
    keys: BTreeSet<Box<str>>,
    id: Option<(RawValue, usize)>,
    static_parent: Option<(RawValue, usize)>,
    properties: Vec<RawProperty>,
}

impl RoleBuilder {
    fn new(header_line: usize) -> Self {
        Self {
            header_line,
            keys: BTreeSet::new(),
            id: None,
            static_parent: None,
            properties: Vec::new(),
        }
    }

    fn insert(&mut self, key: &str, value: &str, line: usize) -> Result<(), ThemeDocumentError> {
        if !self.keys.insert(key.into()) {
            return Err(ThemeDocumentError::DuplicateRoleKey {
                line,
                key: key.into(),
            });
        }
        match key {
            "id" => self.id = Some((parse_raw_value(value, line)?, line)),
            "static_parent" => {
                self.static_parent = Some((parse_raw_value(value, line)?, line));
            }
            _ => self.properties.push(RawProperty {
                key: key.into(),
                value: value.into(),
                line,
            }),
        }
        Ok(())
    }
}

struct RawProperty {
    key: Box<str>,
    value: Box<str>,
    line: usize,
}

enum RawValue {
    String(String),
    Bare(Box<str>),
}

struct BoundedLines<R> {
    reader: R,
    total_bytes: usize,
    lines: usize,
}

impl<R: BufRead> BoundedLines<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            total_bytes: 0,
            lines: 0,
        }
    }

    fn next_line(&mut self) -> Result<Option<(usize, String)>, ThemeDocumentError> {
        let next_line = self.lines.saturating_add(1);
        let mut bytes = Vec::new();
        loop {
            let available = self.reader.fill_buf().map_err(io_error)?;
            if available.is_empty() {
                if bytes.is_empty() {
                    return Ok(None);
                }
                return self.finish_line(bytes, next_line).map(Some);
            }

            let newline = available.iter().position(|byte| *byte == b'\n');
            let consumed = newline.map_or(available.len(), |index| index + 1);
            self.total_bytes = self
                .total_bytes
                .checked_add(consumed)
                .ok_or(ThemeDocumentError::DocumentTooLarge)?;
            if self.total_bytes > THEME_DOCUMENT_MAX_BYTES {
                return Err(ThemeDocumentError::DocumentTooLarge);
            }

            let content_bytes = newline.map_or(consumed, |index| index);
            let new_length = bytes
                .len()
                .checked_add(content_bytes)
                .ok_or(ThemeDocumentError::LineTooLong { line: next_line })?;
            if new_length > THEME_DOCUMENT_MAX_LINE_BYTES + 1 {
                return Err(ThemeDocumentError::LineTooLong { line: next_line });
            }
            bytes.extend_from_slice(&available[..content_bytes]);
            self.reader.consume(consumed);
            if newline.is_some() {
                return self.finish_line(bytes, next_line).map(Some);
            }
        }
    }

    fn finish_line(
        &mut self,
        mut bytes: Vec<u8>,
        line: usize,
    ) -> Result<(usize, String), ThemeDocumentError> {
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        if bytes.len() > THEME_DOCUMENT_MAX_LINE_BYTES {
            return Err(ThemeDocumentError::LineTooLong { line });
        }
        self.lines = self
            .lines
            .checked_add(1)
            .ok_or(ThemeDocumentError::TooManyLines)?;
        if self.lines > THEME_DOCUMENT_MAX_LINES {
            return Err(ThemeDocumentError::TooManyLines);
        }
        let value =
            String::from_utf8(bytes).map_err(|_| ThemeDocumentError::InvalidUtf8 { line })?;
        Ok((line, value))
    }
}

fn io_error(error: io::Error) -> ThemeDocumentError {
    let mut message = error.to_string();
    if message.len() > IO_DIAGNOSTIC_MAX_BYTES {
        let mut end = IO_DIAGNOSTIC_MAX_BYTES;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
    }
    ThemeDocumentError::Io(message)
}

fn strip_comment(line: &str, line_number: usize) -> Result<&str, ThemeDocumentError> {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if character == '\\' => escaped = true,
            Some(active) if character == active => quote = None,
            Some(_) => {}
            None if matches!(character, '"' | '\'') => quote = Some(character),
            None if character == '#' => return Ok(&line[..index]),
            None => {}
        }
    }
    if quote.is_some() || escaped {
        return Err(ThemeDocumentError::InvalidSyntax { line: line_number });
    }
    Ok(line)
}

fn split_assignment(line: &str, line_number: usize) -> Result<(&str, &str), ThemeDocumentError> {
    let Some((key, value)) = line.split_once('=') else {
        return Err(ThemeDocumentError::InvalidSyntax { line: line_number });
    };
    let key = key.trim();
    if key.is_empty()
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(ThemeDocumentError::InvalidSyntax { line: line_number });
    }
    let value = value.trim();
    if value.is_empty() {
        return Err(ThemeDocumentError::InvalidSyntax { line: line_number });
    }
    Ok((key, value))
}

fn parse_raw_value(value: &str, line: usize) -> Result<RawValue, ThemeDocumentError> {
    if value.is_empty() {
        return Err(ThemeDocumentError::InvalidSyntax { line });
    }
    if value.starts_with('"') {
        return parse_basic_string(value, line).map(RawValue::String);
    }
    if value.starts_with('\'') {
        return parse_literal_string(value, line).map(RawValue::String);
    }
    if value.bytes().any(|byte| byte.is_ascii_whitespace()) || !is_bare_value(value) {
        return Err(ThemeDocumentError::InvalidSyntax { line });
    }
    Ok(RawValue::Bare(value.into()))
}

fn parse_basic_string(value: &str, line: usize) -> Result<String, ThemeDocumentError> {
    let mut characters = value.chars();
    if characters.next() != Some('"') {
        return Err(ThemeDocumentError::InvalidSyntax { line });
    }
    let mut output = String::new();
    while let Some(character) = characters.next() {
        match character {
            '"' if characters.as_str().is_empty() => return Ok(output),
            '"' => return Err(ThemeDocumentError::InvalidSyntax { line }),
            '\\' => {
                let escaped = characters
                    .next()
                    .ok_or(ThemeDocumentError::InvalidSyntax { line })?;
                match escaped {
                    'b' => output.push('\u{0008}'),
                    't' => output.push('\t'),
                    'n' => output.push('\n'),
                    'f' => output.push('\u{000c}'),
                    'r' => output.push('\r'),
                    '"' => output.push('"'),
                    '\\' => output.push('\\'),
                    'u' => output.push(parse_unicode_escape(&mut characters, 4, line)?),
                    'U' => output.push(parse_unicode_escape(&mut characters, 8, line)?),
                    _ => return Err(ThemeDocumentError::InvalidSyntax { line }),
                }
            }
            character if character.is_control() => {
                return Err(ThemeDocumentError::InvalidSyntax { line });
            }
            character => output.push(character),
        }
    }
    Err(ThemeDocumentError::InvalidSyntax { line })
}

fn parse_literal_string(value: &str, line: usize) -> Result<String, ThemeDocumentError> {
    let Some(value) = value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    else {
        return Err(ThemeDocumentError::InvalidSyntax { line });
    };
    if value.contains('\'') || value.chars().any(char::is_control) {
        return Err(ThemeDocumentError::InvalidSyntax { line });
    }
    Ok(value.to_owned())
}

fn parse_unicode_escape(
    characters: &mut std::str::Chars<'_>,
    digits: usize,
    line: usize,
) -> Result<char, ThemeDocumentError> {
    let mut value = 0_u32;
    for _ in 0..digits {
        let digit = characters
            .next()
            .and_then(|character| character.to_digit(16))
            .ok_or(ThemeDocumentError::InvalidSyntax { line })?;
        value = value * 16 + digit;
    }
    char::from_u32(value).ok_or(ThemeDocumentError::InvalidSyntax { line })
}

fn is_bare_value(value: &str) -> bool {
    if matches!(
        value,
        "true" | "false" | "inf" | "+inf" | "-inf" | "nan" | "+nan" | "-nan"
    ) {
        return true;
    }
    normalize_number(value).is_some_and(|normalized| {
        decimal_number_is_valid(&normalized) && normalized.parse::<f64>().is_ok()
    })
}

fn normalize_number(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    for (index, byte) in bytes.iter().copied().enumerate() {
        if byte == b'_'
            && !(index > 0
                && index + 1 < bytes.len()
                && bytes[index - 1].is_ascii_digit()
                && bytes[index + 1].is_ascii_digit())
        {
            return None;
        }
    }
    Some(
        value
            .chars()
            .filter(|character| *character != '_')
            .collect(),
    )
}

fn decimal_number_is_valid(value: &str) -> bool {
    let unsigned = value
        .strip_prefix('+')
        .or_else(|| value.strip_prefix('-'))
        .unwrap_or(value);
    let mut exponent_parts = unsigned.split(['e', 'E']);
    let mantissa = exponent_parts.next().unwrap_or_default();
    let exponent = exponent_parts.next();
    if exponent_parts.next().is_some() {
        return false;
    }
    if let Some(exponent) = exponent {
        let exponent = exponent
            .strip_prefix('+')
            .or_else(|| exponent.strip_prefix('-'))
            .unwrap_or(exponent);
        if exponent.is_empty() || !exponent.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
    }

    let mut fraction_parts = mantissa.split('.');
    let integer = fraction_parts.next().unwrap_or_default();
    let fraction = fraction_parts.next();
    if fraction_parts.next().is_some()
        || integer.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || (integer.len() > 1 && integer.starts_with('0'))
    {
        return false;
    }
    match fraction {
        None => true,
        Some(fraction) => {
            !fraction.is_empty() && fraction.bytes().all(|byte| byte.is_ascii_digit())
        }
    }
}

fn parse_property_source(value: RawValue, kind: ThemeValueKind) -> Option<ThemePropertySource> {
    if let RawValue::String(keyword) = &value {
        match keyword.as_str() {
            "static_parent" => return Some(ThemePropertySource::StaticParent),
            "ambient_parent" => return Some(ThemePropertySource::AmbientParent),
            "fallback" => return Some(ThemePropertySource::Fallback),
            _ => {}
        }
    }
    match (kind, value) {
        (ThemeValueKind::Color, RawValue::String(value)) => ThemeColor::parse(&value)
            .ok()
            .map(ThemeValue::Color)
            .map(ThemePropertySource::Concrete),
        (ThemeValueKind::FontFamily, RawValue::String(value)) => ThemeFontFamily::new(value)
            .ok()
            .map(ThemeValue::FontFamily)
            .map(ThemePropertySource::Concrete),
        (ThemeValueKind::LogicalPixels, RawValue::Bare(value)) => normalize_number(&value)
            .and_then(|value| value.parse::<f32>().ok())
            .and_then(|value| ThemeLogicalPixels::new(value).ok())
            .map(ThemeValue::LogicalPixels)
            .map(ThemePropertySource::Concrete),
        (ThemeValueKind::FontWeight, RawValue::Bare(value)) => normalize_number(&value)
            .and_then(|value| value.parse::<u16>().ok())
            .and_then(|value| ThemeFontWeight::new(value).ok())
            .map(ThemeValue::FontWeight)
            .map(ThemePropertySource::Concrete),
        _ => None,
    }
}

fn validate_name(value: &str) -> Result<Box<str>, ThemeDocumentError> {
    validate_name_at(value, 0)
}

fn validate_name_at(value: &str, line: usize) -> Result<Box<str>, ThemeDocumentError> {
    let value = value.trim();
    if value.is_empty() || value.len() > THEME_DOCUMENT_NAME_MAX_BYTES {
        return Err(ThemeDocumentError::InvalidName { line });
    }
    Ok(value.into())
}

fn check_definition_bounds(
    definition: &ThemeDefinition,
    line: usize,
) -> Result<(), ThemeDocumentError> {
    if definition.roles().len() > THEME_DOCUMENT_MAX_ROLES {
        return Err(ThemeDocumentError::TooManyRoles { line });
    }
    let mut properties = 0_usize;
    for role in definition.roles().values() {
        properties = properties
            .checked_add(role.properties().len())
            .ok_or(ThemeDocumentError::TooManyPropertyEntries { line })?;
        if properties > THEME_DOCUMENT_MAX_PROPERTY_ENTRIES {
            return Err(ThemeDocumentError::TooManyPropertyEntries { line });
        }
    }
    Ok(())
}

fn serializable_definition(
    definition: &ThemeDefinition,
) -> Result<ThemeDefinition, ThemeDocumentError> {
    let schema = canonical_theme_schema();
    let mut roles = BTreeMap::new();
    for role in definition.roles().values() {
        let Some(role_schema) = schema.role(role.role_id().as_str()) else {
            continue;
        };
        let properties = role
            .properties()
            .iter()
            .filter_map(|(property, source)| {
                role_schema
                    .property(*property)
                    .map(|_| (*property, source.clone()))
            })
            .collect();
        let id = role_schema.id().clone();
        roles.insert(
            id.clone(),
            ThemeRoleDefinition::new(id, role.static_parent().cloned(), properties),
        );
    }
    ThemeDefinition::checked(roles).map_err(|_| ThemeDocumentError::InvalidSyntax { line: 0 })
}

fn push_assignment(output: &mut String, key: &str, value: &str) -> Result<(), ThemeDocumentError> {
    push_bounded(output, key)?;
    push_bounded(output, " = ")?;
    push_bounded(output, value)?;
    push_bounded(output, "\n")
}

fn push_bounded(output: &mut String, value: &str) -> Result<(), ThemeDocumentError> {
    let length = output
        .len()
        .checked_add(value.len())
        .ok_or(ThemeDocumentError::OutputTooLarge)?;
    if length > THEME_DOCUMENT_MAX_OUTPUT_BYTES {
        return Err(ThemeDocumentError::OutputTooLarge);
    }
    output.push_str(value);
    Ok(())
}

fn serialize_source(source: &ThemePropertySource) -> String {
    match source {
        ThemePropertySource::Concrete(ThemeValue::Color(value)) => {
            string_literal(&value.to_string())
        }
        ThemePropertySource::Concrete(ThemeValue::FontFamily(value)) => {
            string_literal(value.as_str())
        }
        ThemePropertySource::Concrete(ThemeValue::LogicalPixels(value)) => value.get().to_string(),
        ThemePropertySource::Concrete(ThemeValue::FontWeight(value)) => value.get().to_string(),
        ThemePropertySource::StaticParent => string_literal("static_parent"),
        ThemePropertySource::AmbientParent => string_literal("ambient_parent"),
        ThemePropertySource::Fallback => string_literal("fallback"),
    }
}

fn string_literal(value: &str) -> String {
    let mut output = String::with_capacity(value.len().saturating_add(2));
    output.push('"');
    for character in value.chars() {
        match character {
            '\u{0008}' => output.push_str("\\b"),
            '\t' => output.push_str("\\t"),
            '\n' => output.push_str("\\n"),
            '\u{000c}' => output.push_str("\\f"),
            '\r' => output.push_str("\\r"),
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            character if character <= '\u{001f}' || character == '\u{007f}' => {
                use fmt::Write as _;
                let _ = write!(output, "\\u{:04x}", u32::from(character));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}
