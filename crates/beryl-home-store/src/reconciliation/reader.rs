use std::{any::TypeId, marker::PhantomData};

use crate::{
    CodecOperation, ReadError, ReadStage, RecordCodec, StorageDomain,
    command::{MaterializedDomainDescriptor, MaterializedRecordDescriptor},
    domain::{RegisteredDomain, RegisteredFamily},
};

/// One domain hook's exact-side classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DomainReconciliation {
    ExactOld,
    ExactNew,
    Collision,
}

/// Typed old, intended-new, and currently durable values for one exact natural record.
pub struct ReconciliationRecord<K, V> {
    key: K,
    old: Option<V>,
    new: Option<V>,
    current: Option<V>,
}

impl<K, V> ReconciliationRecord<K, V> {
    #[must_use]
    pub fn key(&self) -> &K {
        &self.key
    }
    #[must_use]
    pub fn old(&self) -> Option<&V> {
        self.old.as_ref()
    }
    #[must_use]
    pub fn new(&self) -> Option<&V> {
        self.new.as_ref()
    }
    #[must_use]
    pub fn current(&self) -> Option<&V> {
        self.current.as_ref()
    }
}

/// Descriptor-bound typed reader exposed only while one domain hook executes.
///
/// It can materialize only exact records already named by the ambiguous command descriptor. It
/// has no point-by-key, cursor, raw-keyspace, exhaustive-validation, or cross-domain operation.
pub struct ReconciliationReader<'a, D: StorageDomain> {
    snapshot: &'a fjall::Snapshot,
    domain: &'a RegisteredDomain,
    descriptor: &'a MaterializedDomainDescriptor,
    _typed: PhantomData<fn(D) -> D>,
}

impl<'a, D: StorageDomain> ReconciliationReader<'a, D> {
    pub(crate) fn new(
        snapshot: &'a fjall::Snapshot,
        domain: &'a RegisteredDomain,
        descriptor: &'a MaterializedDomainDescriptor,
    ) -> Self {
        Self {
            snapshot,
            domain,
            descriptor,
            _typed: PhantomData,
        }
    }

    /// Reads and decodes every descriptor-listed record belonging to one exact codec family.
    pub fn records<R: RecordCodec<D>>(
        &self,
    ) -> Result<Vec<ReconciliationRecord<R::Key, R::Value>>, ReadError> {
        let family_slot = self
            .domain
            .family_slot(R::FAMILY)
            .ok_or(ReadError::UnknownFamily {
                domain: D::NAME,
                family: R::FAMILY,
            })?;
        let family = &self.domain.families[family_slot];
        if family.codec_type != TypeId::of::<R>() {
            return Err(ReadError::CodecTypeMismatch {
                domain: D::NAME,
                family: R::FAMILY,
            });
        }
        self.descriptor
            .records
            .iter()
            .filter(|record| record.family_slot == family_slot)
            .map(|record| self.read_record::<R>(family, record))
            .collect()
    }

    fn read_record<R: RecordCodec<D>>(
        &self,
        family: &RegisteredFamily,
        record: &MaterializedRecordDescriptor,
    ) -> Result<ReconciliationRecord<R::Key, R::Value>, ReadError> {
        let key = decode_key::<D, R>(&record.key)?;
        let old = record
            .old
            .as_deref()
            .map(decode_value::<D, R>)
            .transpose()?;
        let new = record
            .new
            .as_deref()
            .map(decode_value::<D, R>)
            .transpose()?;
        let current = read_current::<D, R>(self.snapshot, family, &record.key)?;
        Ok(ReconciliationRecord {
            key,
            old,
            new,
            current,
        })
    }
}

fn decode_key<D: StorageDomain, R: RecordCodec<D>>(encoded: &[u8]) -> Result<R::Key, ReadError> {
    let key = R::decode_key(encoded).map_err(|source| ReadError::Codec {
        domain: D::NAME,
        family: R::FAMILY,
        operation: CodecOperation::DecodeKey,
        source: Box::new(source),
    })?;
    R::validate_stored_key(&key).map_err(|source| ReadError::Codec {
        domain: D::NAME,
        family: R::FAMILY,
        operation: CodecOperation::DecodeKey,
        source: Box::new(source),
    })?;
    Ok(key)
}

pub(crate) fn decode_value<D: StorageDomain, R: RecordCodec<D>>(
    encoded: &[u8],
) -> Result<R::Value, ReadError> {
    let version: [u8; 4] = encoded
        .get(..crate::RECORD_VERSION_BYTES)
        .ok_or(ReadError::MalformedRecord {
            domain: D::NAME,
            family: R::FAMILY,
        })?
        .try_into()
        .expect("four-byte record version");
    let found = u32::from_be_bytes(version);
    if found != R::VERSION.get() {
        return Err(ReadError::UnsupportedRecordVersion {
            domain: D::NAME,
            family: R::FAMILY,
            supported: R::VERSION,
            found,
        });
    }
    let payload = &encoded[crate::RECORD_VERSION_BYTES..];
    if payload.len() > R::MAX_VALUE_BYTES {
        return Err(ReadError::InvalidStoredValueSize {
            domain: D::NAME,
            family: R::FAMILY,
            maximum: R::MAX_VALUE_BYTES.saturating_add(crate::RECORD_VERSION_BYTES),
            actual: encoded.len(),
        });
    }
    R::decode_value(payload).map_err(|source| ReadError::Codec {
        domain: D::NAME,
        family: R::FAMILY,
        operation: CodecOperation::DecodeValue,
        source: Box::new(source),
    })
}

fn read_current<D: StorageDomain, R: RecordCodec<D>>(
    snapshot: &fjall::Snapshot,
    family: &RegisteredFamily,
    key: &[u8],
) -> Result<Option<R::Value>, ReadError> {
    let Some(point) =
        snapshot
            .point(&family.keyspace, key)
            .map_err(|source| ReadError::Storage {
                stage: ReadStage::PointSize,
                source: Box::new(source),
            })?
    else {
        return Ok(None);
    };
    let actual = usize::try_from(point.stored_value_len())
        .expect("u32 always fits usize on supported targets");
    if actual > family.max_stored_value_bytes {
        return Err(ReadError::InvalidStoredValueSize {
            domain: D::NAME,
            family: R::FAMILY,
            maximum: family.max_stored_value_bytes,
            actual,
        });
    }
    let pair = point.acquire().map_err(|source| ReadError::Storage {
        stage: ReadStage::PointValue,
        source: Box::new(source),
    })?;
    (family.validate_envelope)(pair.key(), pair.value())?;
    decode_value::<D, R>(pair.value()).map(Some)
}
