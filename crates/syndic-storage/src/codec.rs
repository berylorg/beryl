use std::{error::Error, fmt, marker::PhantomData};

use beryl_home_store::{PointReadLimit, RECORD_VERSION_BYTES, RecordCodec, RecordVersion};

use crate::domain::SyndicDomain;

mod keys;
mod parts;
mod primary;
mod secondary;

pub(crate) use keys::*;
pub(crate) use primary::*;
pub(crate) use secondary::*;

#[cfg(feature = "test-faults")]
pub(crate) fn awaiting_terminal_scalar_codec_tags() -> Option<(u8, u8)> {
    let mut reason_encoder = parts::Encoder::new();
    parts::enc_next_turn_reason(&mut reason_encoder, crate::NextTurnReason::UnknownTerminal);
    let reason_encoded = reason_encoder.finish();
    let mut reason_decoder = parts::Decoder::new(&reason_encoded);
    if parts::dec_next_turn_reason(&mut reason_decoder).ok()?
        != crate::NextTurnReason::UnknownTerminal
        || reason_decoder.finish().is_err()
    {
        return None;
    }

    let state =
        crate::InputGateState::AwaitingTerminal(beryl_model::SyndicTurnId::from_bytes([0xA7; 16]));
    let mut gate_encoder = parts::Encoder::new();
    parts::enc_input_gate_state(&mut gate_encoder, &state);
    let gate_encoded = gate_encoder.finish();
    let mut gate_decoder = parts::Decoder::new(&gate_encoded);
    if parts::dec_input_gate_state(&mut gate_decoder).ok()? != state
        || gate_decoder.finish().is_err()
    {
        return None;
    }

    Some((*reason_encoded.first()?, *gate_encoded.first()?))
}

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
    const RECORD_VERSION: RecordVersion;
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
    let bytes = F::MAX_VALUE_BYTES
        .checked_add(RECORD_VERSION_BYTES)
        .expect("codec point-read limit fits usize");
    PointReadLimit::new(bytes).expect("codec point-read limit is nonzero")
}

pub(crate) fn family_cursor_max_bytes<F: Family>() -> usize {
    F::MAX_KEY_BYTES
        .checked_add(F::MAX_VALUE_BYTES)
        .and_then(|value| value.checked_add(RECORD_VERSION_BYTES))
        .expect("codec cursor-read limit fits usize")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version<C: RecordCodec<SyndicDomain>>() -> RecordVersion {
        C::VERSION
    }

    #[test]
    fn every_changed_record_family_declares_its_current_version_explicitly() {
        let v2 = RecordVersion::new(2);
        assert_eq!(version::<primary::ThreadsCodec>(), v2);
        assert_eq!(version::<secondary::AcceptedOrderCodec>(), v2);
        assert_eq!(version::<primary::ContentManifestsCodec>(), v2);
        assert_eq!(version::<primary::CanonicalItemsCodec>(), v2);
        assert_eq!(version::<primary::TurnStatesCodec>(), v2);

        let v3 = RecordVersion::new(3);
        assert_eq!(version::<primary::AcceptedInputsCodec>(), v3);
        assert_eq!(version::<primary::SourceEventsCodec>(), v3);
        assert_eq!(version::<secondary::AcceptedRouteGenerationsCodec>(), v3);

        let v4 = RecordVersion::new(4);
        assert_eq!(version::<primary::InputGatesCodec>(), v4);
        assert_eq!(version::<primary::AcceptedRouteLeavesCodec>(), v4);

        assert_eq!(
            version::<primary::StopOperationsCodec>(),
            RecordVersion::new(1)
        );
        assert_eq!(
            version::<primary::CompactionOperationsCodec>(),
            RecordVersion::new(1)
        );
        assert_eq!(
            version::<primary::CompactionSettlementReceiptsCodec>(),
            RecordVersion::new(1)
        );
    }
}
