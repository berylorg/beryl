use std::{error::Error, fmt, marker::PhantomData};

use beryl_home_store::{PointReadLimit, RecordCodec, RecordVersion};

use crate::domain::SyndicDomain;

mod keys;
mod parts;
mod primary;
mod secondary;

pub(crate) use keys::*;
pub(crate) use primary::*;
pub(crate) use secondary::*;

#[derive(Debug)]
pub(crate) enum CodecError {
    Truncated,
    TrailingBytes,
    InvalidLength(&'static str),
    InvalidTag {
        kind: &'static str,
        tag: u8,
    },
    InvalidUtf8(&'static str),
    InvalidValue {
        kind: &'static str,
        source: Box<dyn Error + Send + Sync>,
    },
    CursorSentinel,
}

impl fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => formatter.write_str("Syndic record is truncated"),
            Self::TrailingBytes => formatter.write_str("Syndic record has trailing bytes"),
            Self::InvalidLength(kind) => write!(formatter, "invalid {kind} length"),
            Self::InvalidTag { kind, tag } => write!(formatter, "invalid {kind} tag {tag}"),
            Self::InvalidUtf8(kind) => write!(formatter, "{kind} is not valid UTF-8"),
            Self::InvalidValue { kind, source } => write!(formatter, "invalid {kind}: {source}"),
            Self::CursorSentinel => formatter.write_str("cursor sentinel is not a stored key"),
        }
    }
}

impl Error for CodecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidValue { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

pub(crate) fn invalid(
    kind: &'static str,
    source: impl Error + Send + Sync + 'static,
) -> CodecError {
    CodecError::InvalidValue {
        kind,
        source: Box::new(source),
    }
}

pub(crate) trait Family: Send + Sync + 'static {
    type Key: Clone + Send + Sync;
    type Value: Clone + Send + Sync;
    const NAME: &'static str;
    const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize;
    const MAX_VALUE_BYTES: usize;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError>;
    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError>;
    fn validate_stored_key(_key: &Self::Key) -> Result<(), CodecError> {
        Ok(())
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError>;
    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError>;
}

pub(crate) struct ExactCodec<F>(PhantomData<fn() -> F>);

impl<F: Family> RecordCodec<SyndicDomain> for ExactCodec<F> {
    type Key = F::Key;
    type Value = F::Value;
    type Error = CodecError;
    const FAMILY: &'static str = F::NAME;
    const VERSION: RecordVersion = F::RECORD_VERSION;
    const MAX_KEY_BYTES: usize = F::MAX_KEY_BYTES;
    const MAX_VALUE_BYTES: usize = F::MAX_VALUE_BYTES;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        F::encode_key(key)
    }
    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        F::decode_key(encoded)
    }
    fn validate_stored_key(key: &Self::Key) -> Result<(), Self::Error> {
        F::validate_stored_key(key)
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        F::encode_value(value)
    }
    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        F::decode_value(encoded)
    }
}

pub(crate) const SMALL_MAX: usize = 65_536;
pub(crate) const LARGE_MAX: usize = 393_216;

pub(crate) fn family_point_limit<F: Family>() -> PointReadLimit {
    let bytes = F::MAX_KEY_BYTES
        .checked_add(F::MAX_VALUE_BYTES)
        .and_then(|value| value.checked_add(4))
        .expect("codec point-read limit fits usize");
    PointReadLimit::new(bytes).expect("codec point-read limit is nonzero")
}
