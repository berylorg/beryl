use std::ops::Bound;

use super::*;

pub(super) fn read_point<D: StorageDomain, R: RecordCodec<D>>(
    snapshot: &Snapshot,
    domain: &RegisteredDomain,
    key: &R::Key,
    limit: PointReadLimit,
) -> Result<Option<R::Value>, ReadError> {
    validate_codec::<D, R>()?;
    let family = resolve_family::<D, R>(domain)?;
    let encoded_key = encode_stored_key::<D, R>(key)?;
    let Some(size) = snapshot
        .size_of(&family.keyspace, &encoded_key)
        .map_err(|source| storage(ReadStage::PointSize, source))?
    else {
        return Ok(None);
    };
    let size = usize::try_from(size).expect("u32 always fits usize on supported targets");
    ensure_stored_value_size::<D, R>(size)?;
    ensure_caller_byte_bound::<D, R>(size, limit.max_bytes())?;
    let encoded = snapshot
        .get(&family.keyspace, &encoded_key)
        .map_err(|source| storage(ReadStage::PointValue, source))?
        .ok_or(ReadError::MalformedRecord {
            domain: D::NAME,
            family: R::FAMILY,
        })?;
    decode_value::<D, R>(&encoded).map(Some)
}

pub(super) fn read_cursor<D: StorageDomain, R: RecordCodec<D>>(
    snapshot: &Snapshot,
    domain: &RegisteredDomain,
    range: &CursorRange<R::Key>,
    direction: CursorDirection,
    limits: CursorReadLimits,
) -> Result<CursorPage<R::Key, R::Value>, ReadError> {
    validate_codec::<D, R>()?;
    let family = resolve_family::<D, R>(domain)?;
    let (start, end) = range.bounds();
    let start = encode_bound::<D, R>(start)?;
    let end = encode_bound::<D, R>(end)?;
    if bound_bytes(&start) > bound_bytes(&end) {
        return Err(ReadError::ReversedRange {
            domain: D::NAME,
            family: R::FAMILY,
        });
    }

    let mut cursor = snapshot.range(&family.keyspace, (start, end));
    let mut records = Vec::with_capacity(limits.max_items());
    let mut stored_bytes = 0usize;
    let mut has_more = false;

    loop {
        let guard = match direction {
            CursorDirection::Forward => cursor.next(),
            CursorDirection::Reverse => cursor.next_back(),
        };
        let Some(guard) = guard else {
            break;
        };
        if records.len() == limits.max_items() {
            has_more = true;
            break;
        }

        let encoded_key = guard
            .key()
            .map_err(|source| storage(ReadStage::CursorKey, source))?;
        ensure_stored_key_size::<D, R>(&encoded_key)?;
        let value_size = snapshot
            .size_of(&family.keyspace, &encoded_key)
            .map_err(|source| storage(ReadStage::CursorValueSize, source))?
            .ok_or(ReadError::MalformedRecord {
                domain: D::NAME,
                family: R::FAMILY,
            })?;
        let value_size =
            usize::try_from(value_size).expect("u32 always fits usize on supported targets");
        ensure_stored_value_size::<D, R>(value_size)?;
        ensure_caller_byte_bound::<D, R>(value_size, limits.max_bytes())?;
        let record_bytes =
            encoded_key
                .len()
                .checked_add(value_size)
                .ok_or(ReadError::BoundExceeded {
                    domain: D::NAME,
                    family: R::FAMILY,
                    maximum: limits.max_bytes(),
                    actual: usize::MAX,
                })?;
        let next_total =
            stored_bytes
                .checked_add(record_bytes)
                .ok_or(ReadError::BoundExceeded {
                    domain: D::NAME,
                    family: R::FAMILY,
                    maximum: limits.max_bytes(),
                    actual: usize::MAX,
                })?;
        if next_total > limits.max_bytes() {
            if records.is_empty() {
                return Err(ReadError::BoundExceeded {
                    domain: D::NAME,
                    family: R::FAMILY,
                    maximum: limits.max_bytes(),
                    actual: next_total,
                });
            }
            has_more = true;
            break;
        }

        let encoded_value = snapshot
            .get(&family.keyspace, &encoded_key)
            .map_err(|source| storage(ReadStage::CursorValue, source))?
            .ok_or(ReadError::MalformedRecord {
                domain: D::NAME,
                family: R::FAMILY,
            })?;
        let key = decode_stored_key::<D, R>(&encoded_key)?;
        let value = decode_value::<D, R>(&encoded_value)?;
        records.push(CursorRecord::new(key, value));
        stored_bytes = next_total;
    }

    Ok(CursorPage::new(records, stored_bytes, has_more))
}

fn encode_cursor_key<D: StorageDomain, R: RecordCodec<D>>(
    key: &R::Key,
) -> Result<Vec<u8>, ReadError> {
    let encoded = R::encode_key(key).map_err(|source| ReadError::Codec {
        domain: D::NAME,
        family: R::FAMILY,
        operation: CodecOperation::EncodeKey,
        source: Box::new(source),
    })?;
    ensure_caller_key_size::<D, R>(&encoded)?;
    Ok(encoded)
}

pub(crate) fn encode_stored_key<D: StorageDomain, R: RecordCodec<D>>(
    key: &R::Key,
) -> Result<Vec<u8>, ReadError> {
    R::validate_stored_key(key).map_err(|source| ReadError::Codec {
        domain: D::NAME,
        family: R::FAMILY,
        operation: CodecOperation::EncodeKey,
        source: Box::new(source),
    })?;
    encode_cursor_key::<D, R>(key)
}

fn decode_stored_key<D: StorageDomain, R: RecordCodec<D>>(
    encoded: &[u8],
) -> Result<R::Key, ReadError> {
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

pub(crate) fn validate_record_envelope<D: StorageDomain, R: RecordCodec<D>>(
    encoded_key: &[u8],
    encoded_value: &[u8],
) -> Result<(), ReadError> {
    validate_codec::<D, R>()?;
    ensure_stored_key_size::<D, R>(encoded_key)?;
    decode_stored_key::<D, R>(encoded_key)?;
    ensure_stored_value_size::<D, R>(encoded_value.len())?;
    decode_value::<D, R>(encoded_value)?;
    Ok(())
}

pub(crate) fn encode_value<D: StorageDomain, R: RecordCodec<D>>(
    value: &R::Value,
) -> Result<Vec<u8>, ReadError> {
    validate_codec::<D, R>()?;
    let payload = R::encode_value(value).map_err(|source| ReadError::Codec {
        domain: D::NAME,
        family: R::FAMILY,
        operation: CodecOperation::EncodeValue,
        source: Box::new(source),
    })?;
    if payload.len() > R::MAX_VALUE_BYTES {
        return Err(ReadError::BoundExceeded {
            domain: D::NAME,
            family: R::FAMILY,
            maximum: R::MAX_VALUE_BYTES,
            actual: payload.len(),
        });
    }
    let mut encoded = Vec::with_capacity(RECORD_VERSION_BYTES + payload.len());
    encoded.extend_from_slice(&R::VERSION.get().to_be_bytes());
    encoded.extend_from_slice(&payload);
    Ok(encoded)
}

fn decode_value<D: StorageDomain, R: RecordCodec<D>>(
    encoded: &[u8],
) -> Result<R::Value, ReadError> {
    let version_bytes: [u8; 4] = encoded
        .get(..RECORD_VERSION_BYTES)
        .ok_or(ReadError::MalformedRecord {
            domain: D::NAME,
            family: R::FAMILY,
        })?
        .try_into()
        .expect("validated four-byte version prefix");
    let found = u32::from_be_bytes(version_bytes);
    if found != R::VERSION.get() {
        return Err(ReadError::UnsupportedRecordVersion {
            domain: D::NAME,
            family: R::FAMILY,
            supported: R::VERSION,
            found,
        });
    }
    let payload = &encoded[RECORD_VERSION_BYTES..];
    if payload.len() > R::MAX_VALUE_BYTES {
        return Err(ReadError::InvalidStoredValueSize {
            domain: D::NAME,
            family: R::FAMILY,
            maximum: R::MAX_VALUE_BYTES.saturating_add(RECORD_VERSION_BYTES),
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

fn encode_bound<D: StorageDomain, R: RecordCodec<D>>(
    bound: &Bound<R::Key>,
) -> Result<Bound<Vec<u8>>, ReadError> {
    match bound {
        Bound::Included(key) => encode_cursor_key::<D, R>(key).map(Bound::Included),
        Bound::Excluded(key) => encode_cursor_key::<D, R>(key).map(Bound::Excluded),
        Bound::Unbounded => unreachable!("CursorRange never contains an unbounded endpoint"),
    }
}

fn bound_bytes(bound: &Bound<Vec<u8>>) -> &[u8] {
    match bound {
        Bound::Included(bytes) | Bound::Excluded(bytes) => bytes,
        Bound::Unbounded => unreachable!("encoded cursor endpoints are bounded"),
    }
}

fn resolve_family<D: StorageDomain, R: RecordCodec<D>>(
    domain: &RegisteredDomain,
) -> Result<&RegisteredFamily, ReadError> {
    let family = domain.family(R::FAMILY).ok_or(ReadError::UnknownFamily {
        domain: D::NAME,
        family: R::FAMILY,
    })?;
    if family.codec_type != std::any::TypeId::of::<R>() {
        return Err(ReadError::CodecTypeMismatch {
            domain: D::NAME,
            family: R::FAMILY,
        });
    }
    Ok(family)
}

fn validate_codec<D: StorageDomain, R: RecordCodec<D>>() -> Result<(), ReadError> {
    if R::MAX_KEY_BYTES == 0
        || R::MAX_KEY_BYTES > u16::MAX.into()
        || R::MAX_VALUE_BYTES > (u32::MAX as usize).saturating_sub(RECORD_VERSION_BYTES)
    {
        return Err(ReadError::InvalidCodecContract {
            domain: D::NAME,
            family: R::FAMILY,
        });
    }
    Ok(())
}

fn ensure_caller_key_size<D: StorageDomain, R: RecordCodec<D>>(
    encoded: &[u8],
) -> Result<(), ReadError> {
    validate_caller_key_size(D::NAME, R::FAMILY, R::MAX_KEY_BYTES, encoded)
}

fn validate_caller_key_size(
    domain: &'static str,
    family: &'static str,
    maximum: usize,
    encoded: &[u8],
) -> Result<(), ReadError> {
    if encoded.is_empty() || encoded.len() > maximum {
        return Err(ReadError::InvalidKeySize {
            domain,
            family,
            maximum,
            actual: encoded.len(),
        });
    }
    Ok(())
}

fn ensure_stored_key_size<D: StorageDomain, R: RecordCodec<D>>(
    encoded: &[u8],
) -> Result<(), ReadError> {
    validate_stored_key_size(D::NAME, R::FAMILY, R::MAX_KEY_BYTES, encoded)
}

pub(crate) fn validate_stored_key_size(
    domain: &'static str,
    family: &'static str,
    maximum: usize,
    encoded: &[u8],
) -> Result<(), ReadError> {
    if encoded.is_empty() || encoded.len() > maximum {
        return Err(ReadError::InvalidStoredKeySize {
            domain,
            family,
            maximum,
            actual: encoded.len(),
        });
    }
    Ok(())
}

fn ensure_stored_value_size<D: StorageDomain, R: RecordCodec<D>>(
    actual: usize,
) -> Result<(), ReadError> {
    let maximum = R::MAX_VALUE_BYTES.saturating_add(RECORD_VERSION_BYTES);
    if actual > maximum {
        return Err(ReadError::InvalidStoredValueSize {
            domain: D::NAME,
            family: R::FAMILY,
            maximum,
            actual,
        });
    }
    Ok(())
}

fn ensure_caller_byte_bound<D: StorageDomain, R: RecordCodec<D>>(
    actual: usize,
    caller_maximum: usize,
) -> Result<(), ReadError> {
    if actual > caller_maximum {
        return Err(ReadError::BoundExceeded {
            domain: D::NAME,
            family: R::FAMILY,
            maximum: caller_maximum,
            actual,
        });
    }
    Ok(())
}
