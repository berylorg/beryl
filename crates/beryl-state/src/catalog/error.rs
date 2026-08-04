use std::{error::Error, fmt};

/// Why a caller-admitted compact catalog fact was rejected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogValueError {
    Empty {
        kind: &'static str,
    },
    TooLong {
        kind: &'static str,
        maximum: usize,
        actual: usize,
    },
    TooManyScalars {
        kind: &'static str,
        maximum: usize,
        actual: usize,
    },
    SurroundingWhitespace {
        kind: &'static str,
    },
    TrailingWhitespace {
        kind: &'static str,
    },
    ControlCharacter {
        kind: &'static str,
        index: usize,
    },
    MissingAlphanumeric {
        kind: &'static str,
    },
    ZeroCatalogRevision,
    CatalogRevisionExhausted,
    InvalidLineage(&'static str),
    ClaimSourceMismatch,
}

impl fmt::Display for CatalogValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { kind } => write!(formatter, "{kind} must not be empty"),
            Self::TooLong {
                kind,
                maximum,
                actual,
            } => write!(
                formatter,
                "{kind} must not exceed {maximum} UTF-8 bytes, got {actual}"
            ),
            Self::TooManyScalars {
                kind,
                maximum,
                actual,
            } => write!(
                formatter,
                "{kind} must not exceed {maximum} Unicode scalar values, got {actual}"
            ),
            Self::SurroundingWhitespace { kind } => {
                write!(formatter, "{kind} must not have surrounding whitespace")
            }
            Self::TrailingWhitespace { kind } => {
                write!(formatter, "{kind} must not have trailing whitespace")
            }
            Self::ControlCharacter { kind, index } => {
                write!(formatter, "{kind} contains a control character at byte {index}")
            }
            Self::MissingAlphanumeric { kind } => {
                write!(formatter, "{kind} must contain an alphanumeric character")
            }
            Self::ZeroCatalogRevision => formatter.write_str("catalog revision must be nonzero"),
            Self::CatalogRevisionExhausted => {
                formatter.write_str("catalog revision is exhausted")
            }
            Self::InvalidLineage(message) => formatter.write_str(message),
            Self::ClaimSourceMismatch => formatter.write_str(
                "claim summary and claim source revision must either both be present or both be absent",
            ),
        }
    }
}

impl Error for CatalogValueError {}

pub(super) fn bounded_text(
    kind: &'static str,
    value: &str,
    maximum: usize,
) -> Result<Box<str>, CatalogValueError> {
    if value.is_empty() {
        return Err(CatalogValueError::Empty { kind });
    }
    if value.len() > maximum {
        return Err(CatalogValueError::TooLong {
            kind,
            maximum,
            actual: value.len(),
        });
    }
    if value.trim() != value {
        return Err(CatalogValueError::SurroundingWhitespace { kind });
    }
    if let Some((index, _)) = value.char_indices().find(|(_, value)| value.is_control()) {
        return Err(CatalogValueError::ControlCharacter { kind, index });
    }
    Ok(value.into())
}
