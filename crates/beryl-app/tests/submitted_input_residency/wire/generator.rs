use std::fmt::{self, Write as _};

use beryl_model::ImageLabelOrdinal;

use super::{InputSpec, TEXT_PATTERN};

pub(super) enum Part {
    Empty,
    Static(StaticCursor),
    Fixed(FixedCursor),
    Escaped(EscapedCursor),
}

impl Part {
    pub(super) const fn empty() -> Self {
        Self::Empty
    }

    pub(super) const fn bytes(bytes: &'static [u8]) -> Self {
        Self::Static(StaticCursor::new(bytes))
    }

    pub(super) fn decimal(value: u64) -> Self {
        Self::Fixed(FixedCursor::decimal(value))
    }

    pub(super) fn image_label(value: u64) -> Self {
        Self::Fixed(FixedCursor::image_label(value))
    }

    pub(super) fn escaped_pattern(repetitions: u64) -> Self {
        Self::Escaped(EscapedCursor::pattern(repetitions))
    }

    pub(super) fn escaped_path() -> Self {
        Self::Escaped(EscapedCursor::path())
    }

    pub(super) fn next(&mut self, spec: &InputSpec) -> Option<u8> {
        match self {
            Self::Empty => None,
            Self::Static(cursor) => cursor.next(),
            Self::Fixed(cursor) => cursor.next(),
            Self::Escaped(cursor) => cursor.next(spec),
        }
    }

    pub(super) fn next_simple(&mut self) -> Option<u8> {
        match self {
            Self::Empty => None,
            Self::Static(cursor) => cursor.next(),
            Self::Fixed(cursor) => cursor.next(),
            Self::Escaped(_) => panic!("escaped output requires an input specification"),
        }
    }
}

pub(super) struct StaticCursor {
    bytes: &'static [u8],
    index: usize,
}

impl StaticCursor {
    pub(super) const fn new(bytes: &'static [u8]) -> Self {
        Self { bytes, index: 0 }
    }
}

impl Iterator for StaticCursor {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        let byte = self.bytes.get(self.index).copied()?;
        self.index += 1;
        Some(byte)
    }
}

pub(super) struct FixedCursor {
    bytes: [u8; 20],
    len: u8,
    index: u8,
}

impl FixedCursor {
    fn decimal(value: u64) -> Self {
        let mut cursor = Self::empty();
        write!(&mut cursor, "{value}").expect("u64 decimal fits fixed cursor");
        cursor
    }

    fn image_label(value: u64) -> Self {
        let label = ImageLabelOrdinal::new(value).expect("image label ordinal is nonzero");
        let mut cursor = Self::empty();
        write!(&mut cursor, "{label}").expect("image label fits fixed cursor");
        cursor
    }

    const fn empty() -> Self {
        Self {
            bytes: [0; 20],
            len: 0,
            index: 0,
        }
    }
}

impl fmt::Write for FixedCursor {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let start = usize::from(self.len);
        let end = start.checked_add(value.len()).ok_or(fmt::Error)?;
        if end > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[start..end].copy_from_slice(value.as_bytes());
        self.len = u8::try_from(end).map_err(|_| fmt::Error)?;
        Ok(())
    }
}

impl Iterator for FixedCursor {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.len {
            return None;
        }
        let byte = self.bytes[usize::from(self.index)];
        self.index += 1;
        Some(byte)
    }
}

pub(super) struct EscapedCursor {
    source: EscapedSource,
    escape: [u8; 6],
    escape_len: u8,
    escape_index: u8,
}

enum EscapedSource {
    Pattern { repetitions: u64, index: usize },
    Path { index: usize },
}

impl EscapedCursor {
    fn pattern(repetitions: u64) -> Self {
        Self::new(EscapedSource::Pattern {
            repetitions,
            index: 0,
        })
    }

    fn path() -> Self {
        Self::new(EscapedSource::Path { index: 0 })
    }

    const fn new(source: EscapedSource) -> Self {
        Self {
            source,
            escape: [0; 6],
            escape_len: 0,
            escape_index: 0,
        }
    }

    fn next(&mut self, spec: &InputSpec) -> Option<u8> {
        loop {
            if self.escape_index < self.escape_len {
                let byte = self.escape[usize::from(self.escape_index)];
                self.escape_index += 1;
                return Some(byte);
            }
            let raw = self.source.next(spec)?;
            if let Some(byte) = self.prepare_escape(raw) {
                return Some(byte);
            }
        }
    }

    fn prepare_escape(&mut self, byte: u8) -> Option<u8> {
        let escaped = match byte {
            b'"' => Some(*b"\\\""),
            b'\\' => Some(*b"\\\\"),
            b'\x08' => Some(*b"\\b"),
            b'\x0c' => Some(*b"\\f"),
            b'\n' => Some(*b"\\n"),
            b'\r' => Some(*b"\\r"),
            b'\t' => Some(*b"\\t"),
            0x00..=0x1f => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                self.escape = [
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    HEX[usize::from(byte >> 4)],
                    HEX[usize::from(byte & 0x0f)],
                ];
                self.escape_len = 6;
                self.escape_index = 0;
                return None;
            }
            _ => return Some(byte),
        };
        let escaped = escaped.expect("short escape is present");
        self.escape[..2].copy_from_slice(&escaped);
        self.escape_len = 2;
        self.escape_index = 0;
        None
    }
}

impl EscapedSource {
    fn next(&mut self, spec: &InputSpec) -> Option<u8> {
        match self {
            Self::Pattern { repetitions, index } => {
                if *repetitions == 0 {
                    return None;
                }
                let byte = TEXT_PATTERN.as_bytes()[*index];
                *index += 1;
                if *index == TEXT_PATTERN.len() {
                    *index = 0;
                    *repetitions -= 1;
                }
                Some(byte)
            }
            Self::Path { index } => {
                let byte = spec.runtime_path_byte(*index)?;
                *index += 1;
                Some(byte)
            }
        }
    }
}
