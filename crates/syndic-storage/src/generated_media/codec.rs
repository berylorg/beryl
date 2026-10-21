use beryl_home_store::RecordVersion;
use beryl_model::{
    GeneratedSourceIncarnation, SyndicExecutionSnapshotId, SyndicItemId, SyndicResourceId,
    SyndicThreadId, SyndicTurnId,
};

use super::{SourceRecord, SourceValue};
use crate::codec::{CodecError, ExactCodec, Family, ScanKey, parts::Decoder};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceKey {
    Allocator,
    Resource(SyndicResourceId),
    Owner(SyndicExecutionSnapshotId),
}

impl ScanKey for SourceKey {
    fn first() -> Self {
        Self::Allocator
    }
    fn last() -> Self {
        Self::Owner(SyndicExecutionSnapshotId::from_bytes([255; 16]))
    }
}

pub(crate) struct SourcesFamily;
pub(crate) type SourcesCodec = ExactCodec<SourcesFamily>;

fn malformed() -> CodecError {
    CodecError::InvalidLength("generated media source")
}

impl Family for SourcesFamily {
    type Key = SourceKey;
    type Value = SourceValue;
    const NAME: &'static str = "generated-media-sources";
    const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 17;
    const MAX_VALUE_BYTES: usize = 155;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        match key {
            SourceKey::Allocator => Ok(vec![0]),
            SourceKey::Resource(id) => {
                let mut bytes = vec![1];
                bytes.extend_from_slice(id.as_bytes());
                Ok(bytes)
            }
            SourceKey::Owner(id) => {
                let mut bytes = vec![2];
                bytes.extend_from_slice(id.as_bytes());
                Ok(bytes)
            }
        }
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        match bytes {
            [0] => Ok(SourceKey::Allocator),
            [1, rest @ ..] if rest.len() == 16 => Ok(SourceKey::Resource(
                SyndicResourceId::from_bytes(rest.try_into().map_err(|_| malformed())?),
            )),
            [2, rest @ ..] if rest.len() == 16 => Ok(SourceKey::Owner(
                SyndicExecutionSnapshotId::from_bytes(rest.try_into().map_err(|_| malformed())?),
            )),
            _ => Err(malformed()),
        }
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        match value {
            SourceValue::Allocator(last) => Ok(last.to_le_bytes().to_vec()),
            SourceValue::Owner(owner) => Ok(owner.as_bytes().to_vec()),
            SourceValue::Source(source) => {
                if source.sequence == 0
                    || source.initial_event == 0
                    || source
                        .completion
                        .is_some_and(|(event, _)| event < source.initial_event)
                {
                    return Err(malformed());
                }
                let mut bytes = Vec::with_capacity(155);
                bytes.extend_from_slice(&source.sequence.to_le_bytes());
                for id in [
                    source.thread.as_bytes(),
                    source.turn.as_bytes(),
                    source.item.as_bytes(),
                    source.snapshot.as_bytes(),
                ] {
                    bytes.extend_from_slice(id);
                }
                bytes.push(1);
                bytes.extend_from_slice(&source.initial_event.to_le_bytes());
                match source.source_incarnation {
                    None => bytes.push(0),
                    Some(owner) => {
                        bytes.push(1);
                        bytes.extend_from_slice(owner.as_bytes());
                    }
                }
                match source.completion {
                    None => bytes.push(0),
                    Some((event, digest)) => {
                        bytes.push(1);
                        bytes.extend_from_slice(&event.to_le_bytes());
                        bytes.extend_from_slice(&digest);
                    }
                }
                Ok(bytes)
            }
        }
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        if bytes.len() == 8 {
            return Ok(SourceValue::Allocator(u64::from_le_bytes(
                bytes.try_into().map_err(|_| malformed())?,
            )));
        }
        if bytes.len() == 32 {
            return Ok(SourceValue::Owner(GeneratedSourceIncarnation::from_bytes(
                bytes.try_into().map_err(|_| malformed())?,
            )));
        }
        let mut d = Decoder::new(bytes);
        let sequence = u64::from_le_bytes(d.take(8)?.try_into().map_err(|_| malformed())?);
        let thread = SyndicThreadId::from_bytes(d.fixed16()?);
        let turn = SyndicTurnId::from_bytes(d.fixed16()?);
        let item = SyndicItemId::from_bytes(d.fixed16()?);
        let snapshot = SyndicExecutionSnapshotId::from_bytes(d.fixed16()?);
        if d.u8()? != 1 {
            return Err(malformed());
        }
        let initial_event = u64::from_le_bytes(d.take(8)?.try_into().map_err(|_| malformed())?);
        let source_incarnation = match d.u8()? {
            0 => None,
            1 => Some(GeneratedSourceIncarnation::from_bytes(d.fixed32()?)),
            _ => return Err(malformed()),
        };
        let completion = match d.u8()? {
            0 => None,
            1 => Some((
                u64::from_le_bytes(d.take(8)?.try_into().map_err(|_| malformed())?),
                d.fixed32()?,
            )),
            _ => return Err(malformed()),
        };
        d.finish()?;
        let value = SourceValue::Source(SourceRecord {
            sequence,
            thread,
            turn,
            item,
            snapshot,
            initial_event,
            source_incarnation,
            completion,
        });
        Self::encode_value(&value)?;
        Ok(value)
    }
}

pub(crate) struct PendingFamily;
pub(crate) type PendingCodec = ExactCodec<PendingFamily>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PendingKey(pub(crate) u64);
impl ScanKey for PendingKey {
    fn first() -> Self {
        Self(1)
    }
    fn last() -> Self {
        Self(u64::MAX)
    }
}

impl Family for PendingFamily {
    type Key = PendingKey;
    type Value = SyndicResourceId;
    const NAME: &'static str = "pending-generated-media-resources";
    const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 8;
    const MAX_VALUE_BYTES: usize = 16;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        if key.0 == 0 {
            return Err(malformed());
        }
        Ok(key.0.to_be_bytes().to_vec())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        let key = PendingKey(u64::from_be_bytes(
            bytes.try_into().map_err(|_| malformed())?,
        ));
        Self::encode_key(&key)?;
        Ok(key)
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        Ok(value.as_bytes().to_vec())
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        Ok(SyndicResourceId::from_bytes(
            bytes.try_into().map_err(|_| malformed())?,
        ))
    }
}
