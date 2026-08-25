use std::{any::TypeId, collections::HashSet, marker::PhantomData};

use crate::{
    MutationBuildError, ReadError, ReadStage, RecordCodec, StorageDomain, domain::RegisteredDomain,
    read::encode_stored_key,
};

use super::{
    DerivedReadFact, SUCCESSOR_READ_FIXED_BYTES, SuccessorPointRead, SuccessorProtocol, digest,
};

pub struct SuccessorReadReservation<'a, D, P>
where
    D: StorageDomain,
    P: SuccessorProtocol,
{
    pub(super) reads: Vec<ReservedSuccessorRead>,
    derivations: HashSet<TypeId>,
    pub(super) descriptor_bytes: usize,
    _typed: PhantomData<&'a mut fn(D, P)>,
}

impl<'a, D, P> SuccessorReadReservation<'a, D, P>
where
    D: StorageDomain,
    P: SuccessorProtocol,
{
    pub(super) fn new() -> Self {
        Self {
            reads: Vec::new(),
            derivations: HashSet::new(),
            descriptor_bytes: 0,
            _typed: PhantomData,
        }
    }

    pub fn reserve<Q>(&mut self, count: usize) -> Result<(), MutationBuildError>
    where
        Q: SuccessorPointRead<D, P>,
    {
        type_for_read::<D, P, Q>();
        let family = <Q::Record as RecordCodec<D>>::FAMILY;
        if count == 0 {
            return Err(MutationBuildError::ZeroSuccessorReadReservation {
                domain: D::NAME,
                family,
            });
        }
        if !self.derivations.insert(TypeId::of::<Q>()) {
            return Err(MutationBuildError::DuplicateSuccessorReadReservation {
                domain: D::NAME,
                family,
            });
        }
        let max_stored_value_bytes = <Q::Record as RecordCodec<D>>::MAX_VALUE_BYTES
            .checked_add(crate::RECORD_VERSION_BYTES)
            .ok_or(MutationBuildError::SuccessorReservationOverflow { domain: D::NAME })?;
        let per_read = SUCCESSOR_READ_FIXED_BYTES
            .checked_add(<Q::Record as RecordCodec<D>>::MAX_KEY_BYTES)
            .and_then(|bytes| bytes.checked_add(max_stored_value_bytes))
            .and_then(|bytes| bytes.checked_add(Q::MAX_DECODED_BYTES))
            .ok_or(MutationBuildError::SuccessorReservationOverflow { domain: D::NAME })?;
        let total = per_read
            .checked_mul(count)
            .ok_or(MutationBuildError::SuccessorReservationOverflow { domain: D::NAME })?;
        self.descriptor_bytes = self
            .descriptor_bytes
            .checked_add(total)
            .ok_or(MutationBuildError::SuccessorReservationOverflow { domain: D::NAME })?;
        self.reads.push(ReservedSuccessorRead {
            derivation_type: TypeId::of::<Q>(),
            family,
            codec_type: TypeId::of::<Q::Record>(),
            count,
            max_key_bytes: <Q::Record as RecordCodec<D>>::MAX_KEY_BYTES,
            max_stored_value_bytes,
            max_decoded_bytes: Q::MAX_DECODED_BYTES,
        });
        Ok(())
    }
}

fn type_for_read<D, P, Q>()
where
    D: StorageDomain,
    P: SuccessorProtocol,
    Q: SuccessorPointRead<D, P>,
{
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuccessorReadRejection {
    Undeclared,
    QuotaExhausted,
    InvalidKey,
    InvalidExpected,
    KeyLimit,
    DecodedLimit,
}

pub enum SuccessorPointRecord<V> {
    Present(V),
    Absent,
    Rejected(SuccessorReadRejection),
}

pub struct SuccessorPointReader<'a, D, P>
where
    D: StorageDomain,
    P: SuccessorProtocol,
{
    pub(super) snapshot: &'a fjall::Snapshot,
    pub(super) domain: &'a RegisteredDomain,
    pub(super) correlation: &'a P::Correlation,
    pub(super) reads: &'a [ReservedSuccessorRead],
    pub(super) used: Vec<usize>,
    pub(super) rejected: bool,
    pub(super) facts: Vec<DerivedReadFact>,
    pub(super) _typed: PhantomData<fn(D, P)>,
}

impl<'a, D, P> SuccessorPointReader<'a, D, P>
where
    D: StorageDomain,
    P: SuccessorProtocol,
{
    pub fn correlation(&self) -> &P::Correlation {
        self.correlation
    }

    pub fn read<Q>(
        &mut self,
    ) -> Result<SuccessorPointRecord<<Q::Record as RecordCodec<D>>::Value>, ReadError>
    where
        Q: SuccessorPointRead<D, P>,
    {
        let Some(read_index) = self
            .reads
            .iter()
            .position(|read| read.derivation_type == TypeId::of::<Q>())
        else {
            return Ok(self.reject(SuccessorReadRejection::Undeclared));
        };
        let read = &self.reads[read_index];
        if self.used[read_index] >= read.count {
            return Ok(self.reject(SuccessorReadRejection::QuotaExhausted));
        }
        let ordinal = self.used[read_index];
        self.used[read_index] += 1;

        let family_slot = self
            .domain
            .family_slot(read.family)
            .ok_or(ReadError::UnknownFamily {
                domain: D::NAME,
                family: read.family,
            })?;
        let family = &self.domain.families[family_slot];
        if family.codec_type != read.codec_type || read.codec_type != TypeId::of::<Q::Record>() {
            return Err(ReadError::CodecTypeMismatch {
                domain: D::NAME,
                family: read.family,
            });
        }
        let typed_key = Q::derive_key(self.correlation, ordinal);
        let key = match encode_stored_key::<D, Q::Record>(&typed_key) {
            Ok(key) => key,
            Err(
                ReadError::Codec { .. }
                | ReadError::InvalidKeySize { .. }
                | ReadError::BoundExceeded { .. },
            ) => return Ok(self.reject(SuccessorReadRejection::InvalidKey)),
            Err(error) => return Err(error),
        };
        if key.len() > read.max_key_bytes {
            return Ok(self.reject(SuccessorReadRejection::KeyLimit));
        }
        let key_decoded_bytes = <Q::Record as RecordCodec<D>>::decoded_key_bytes(&key, &typed_key);
        drop(typed_key);
        let expected_value = Q::expected_value(self.correlation, ordinal);
        let expected_encoded = match crate::read::encode_value::<D, Q::Record>(&expected_value) {
            Ok(expected) => expected,
            Err(ReadError::Codec { .. } | ReadError::BoundExceeded { .. }) => {
                return Ok(self.reject(SuccessorReadRejection::InvalidExpected));
            }
            Err(error) => return Err(error),
        };
        let expected_payload = &expected_encoded[crate::RECORD_VERSION_BYTES..];
        let expected_decoded_bytes = key_decoded_bytes
            .checked_add(<Q::Record as RecordCodec<D>>::decoded_value_bytes(
                expected_payload,
                &expected_value,
            ))
            .unwrap_or(usize::MAX);
        if expected_decoded_bytes > read.max_decoded_bytes {
            drop(expected_value);
            drop(expected_encoded);
            return Ok(self.reject(SuccessorReadRejection::DecodedLimit));
        }
        let expected_digest = digest(&expected_encoded);
        drop(expected_value);
        drop(expected_encoded);
        let Some(point) = self
            .snapshot
            .point(&family.keyspace, &key)
            .map_err(|source| ReadError::Storage {
                stage: ReadStage::PointSize,
                source: Box::new(source),
            })?
        else {
            if key_decoded_bytes > read.max_decoded_bytes {
                return Ok(self.reject(SuccessorReadRejection::DecodedLimit));
            }
            self.facts.push(DerivedReadFact {
                _family_slot: family_slot,
                _key_digest: digest(&key),
                _current_digest: None,
                _expected_digest: expected_digest,
            });
            return Ok(SuccessorPointRecord::Absent);
        };
        let actual = usize::try_from(point.stored_value_len())
            .expect("u32 always fits usize on supported targets");
        if actual > read.max_stored_value_bytes {
            return Err(ReadError::InvalidStoredValueSize {
                domain: D::NAME,
                family: read.family,
                maximum: read.max_stored_value_bytes,
                actual,
            });
        }
        let pair = point.acquire().map_err(|source| ReadError::Storage {
            stage: ReadStage::PointValue,
            source: Box::new(source),
        })?;
        (family.validate_envelope)(pair.key(), pair.value())?;
        let value = crate::reconciliation::reader::decode_value::<D, Q::Record>(pair.value())?;
        let payload = &pair.value()[crate::RECORD_VERSION_BYTES..];
        let decoded_bytes = key_decoded_bytes
            .checked_add(<Q::Record as RecordCodec<D>>::decoded_value_bytes(
                payload, &value,
            ))
            .unwrap_or(usize::MAX);
        if decoded_bytes > read.max_decoded_bytes {
            return Ok(self.reject(SuccessorReadRejection::DecodedLimit));
        }
        self.facts.push(DerivedReadFact {
            _family_slot: family_slot,
            _key_digest: digest(&key),
            _current_digest: Some(digest(pair.value())),
            _expected_digest: expected_digest,
        });
        Ok(SuccessorPointRecord::Present(value))
    }

    fn reject<V>(&mut self, rejection: SuccessorReadRejection) -> SuccessorPointRecord<V> {
        self.rejected = true;
        SuccessorPointRecord::Rejected(rejection)
    }
}

pub(super) struct ReservedSuccessorRead {
    derivation_type: TypeId,
    family: &'static str,
    codec_type: TypeId,
    count: usize,
    max_key_bytes: usize,
    max_stored_value_bytes: usize,
    max_decoded_bytes: usize,
}
