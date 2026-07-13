use std::num::NonZeroU64;

use beryl_home_store::{RecordCodec, RecordVersion};
use beryl_model::{
    AssetId, AssetIdentityVersion, DomainRevision, SyndicAcceptedInputId, SyndicDraftId,
    SyndicDraftMarkerId, SyndicItemId, SyndicProjectionId, SyndicQueuedInputId,
    SyndicRetryRecordId,
};

use crate::{
    UnixMillis,
    encoding::{CodecError, Decoder, Encoder},
};

use super::{
    ASSET_METADATA_LIMIT, ASSET_REFERENCE_LIMIT, AssetDimensions, AssetDomain, AssetMediaType,
    AssetMetadataRecord, AssetReferenceOwner, AssetReferenceRecord, AssetSidecarState,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct AssetReferenceIndexKey {
    pub(super) asset_id: AssetId,
    pub(super) owner: AssetReferenceOwner,
}

impl AssetReferenceIndexKey {
    pub(super) fn minimum(asset_id: AssetId, after: Option<AssetReferenceOwner>) -> Self {
        Self {
            asset_id,
            owner: after.unwrap_or(AssetReferenceOwner::CurrentDraftMarker {
                draft_id: SyndicDraftId::from_bytes([0; 16]),
                marker_id: SyndicDraftMarkerId::from_bytes([0; 16]),
            }),
        }
    }

    pub(super) fn maximum(asset_id: AssetId) -> Self {
        Self {
            asset_id,
            owner: AssetReferenceOwner::TranscriptProjection(SyndicProjectionId::from_bytes(
                [u8::MAX; 16],
            )),
        }
    }
}

pub(super) struct AssetMetadataCodec;

impl RecordCodec<AssetDomain> for AssetMetadataCodec {
    type Key = AssetId;
    type Value = AssetMetadataRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "metadata";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 41;
    const MAX_VALUE_BYTES: usize = ASSET_METADATA_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_asset_id(*key))
    }
    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        decode_asset_id(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encode_asset(&mut encoder, value.asset_id);
        encoder.text(value.media_type.as_str());
        encode_dimensions(&mut encoder, value.dimensions);
        encoder.u64(value.creation_revision.get());
        encoder.u8(match value.sidecar_state {
            AssetSidecarState::Committed => 0,
        });
        encoder.u64(value.reference_count);
        encoder.u64(value.revision.get());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let asset_id = decode_asset(&mut decoder)?;
        let media_type = AssetMediaType::new(decoder.text("asset media type")?)
            .map_err(|source| invalid("asset media type", source))?;
        let dimensions = decode_dimensions(&mut decoder)?;
        let creation_revision = DomainRevision::new(decoder.u64()?)
            .map_err(|source| invalid("asset creation revision", source))?;
        let sidecar_state = match decoder.u8()? {
            0 => AssetSidecarState::Committed,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "asset sidecar state",
                    tag,
                });
            }
        };
        let reference_count = decoder.u64()?;
        let revision = decoder.record_revision()?;
        decoder.finish()?;
        Ok(AssetMetadataRecord {
            asset_id,
            media_type,
            dimensions,
            creation_revision,
            sidecar_state,
            reference_count,
            revision,
        })
    }
}

pub(super) struct AssetReferenceCodec;

impl RecordCodec<AssetDomain> for AssetReferenceCodec {
    type Key = AssetReferenceOwner;
    type Value = AssetReferenceRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "references";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 33;
    const MAX_VALUE_BYTES: usize = ASSET_REFERENCE_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encode_owner(&mut encoder, *key);
        Ok(encoder.finish())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let owner = decode_owner(&mut decoder)?;
        decoder.finish()?;
        Ok(owner)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_reference(value))
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        decode_reference(encoded)
    }
}

pub(super) struct AssetReferenceIndexCodec;

impl RecordCodec<AssetDomain> for AssetReferenceIndexCodec {
    type Key = AssetReferenceIndexKey;
    type Value = AssetReferenceRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "references_by_asset";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 74;
    const MAX_VALUE_BYTES: usize = ASSET_REFERENCE_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut bytes = encode_asset_id(key.asset_id);
        let mut encoder = Encoder::new();
        encode_owner(&mut encoder, key.owner);
        bytes.extend_from_slice(&encoder.finish());
        Ok(bytes)
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        if encoded.len() < 42 {
            return Err(CodecError::Truncated);
        }
        let asset_id = decode_asset_id(&encoded[..41])?;
        let mut decoder = Decoder::new(&encoded[41..]);
        let owner = decode_owner(&mut decoder)?;
        decoder.finish()?;
        Ok(AssetReferenceIndexKey { asset_id, owner })
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(encode_reference(value))
    }
    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        decode_reference(encoded)
    }
}

fn encode_asset_id(asset_id: AssetId) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encode_asset(&mut encoder, asset_id);
    encoder.finish()
}

fn encode_asset(encoder: &mut Encoder, asset_id: AssetId) {
    encoder.u8(match asset_id.version() {
        AssetIdentityVersion::Sha256V1 => 1,
    });
    encoder.fixed_32(&asset_id.digest());
    encoder.u64(asset_id.length().get());
}

fn decode_asset_id(encoded: &[u8]) -> Result<AssetId, CodecError> {
    let mut decoder = Decoder::new(encoded);
    let asset_id = decode_asset(&mut decoder)?;
    decoder.finish()?;
    Ok(asset_id)
}

fn decode_asset(decoder: &mut Decoder<'_>) -> Result<AssetId, CodecError> {
    match decoder.u8()? {
        1 => {
            let digest = decoder.fixed_32()?;
            let length = NonZeroU64::new(decoder.u64()?).ok_or_else(|| {
                invalid(
                    "asset byte length",
                    std::io::Error::other("length must be nonzero"),
                )
            })?;
            Ok(AssetId::sha256_v1(digest, length))
        }
        tag => Err(CodecError::InvalidTag {
            kind: "asset identity version",
            tag,
        }),
    }
}

fn encode_dimensions(encoder: &mut Encoder, dimensions: Option<AssetDimensions>) {
    match dimensions {
        Some(dimensions) => {
            encoder.u8(1);
            encoder.u64(dimensions.width().get());
            encoder.u64(dimensions.height().get());
        }
        None => encoder.u8(0),
    }
}

fn decode_dimensions(decoder: &mut Decoder<'_>) -> Result<Option<AssetDimensions>, CodecError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => {
            let width = NonZeroU64::new(decoder.u64()?).ok_or_else(|| {
                invalid(
                    "asset width",
                    std::io::Error::other("width must be nonzero"),
                )
            })?;
            let height = NonZeroU64::new(decoder.u64()?).ok_or_else(|| {
                invalid(
                    "asset height",
                    std::io::Error::other("height must be nonzero"),
                )
            })?;
            Ok(Some(AssetDimensions::new(width, height)))
        }
        tag => Err(CodecError::InvalidTag {
            kind: "asset dimensions option",
            tag,
        }),
    }
}

fn encode_owner(encoder: &mut Encoder, owner: AssetReferenceOwner) {
    match owner {
        AssetReferenceOwner::CurrentDraftMarker {
            draft_id,
            marker_id,
        } => {
            encoder.u8(0);
            encoder.fixed(draft_id.as_bytes());
            encoder.fixed(marker_id.as_bytes());
        }
        AssetReferenceOwner::AcceptedInput(id) => {
            encoder.u8(1);
            encoder.fixed(id.as_bytes());
        }
        AssetReferenceOwner::SubmittedTurnItem(id) => {
            encoder.u8(2);
            encoder.fixed(id.as_bytes());
        }
        AssetReferenceOwner::QueuedInput(id) => {
            encoder.u8(3);
            encoder.fixed(id.as_bytes());
        }
        AssetReferenceOwner::RetryRecord(id) => {
            encoder.u8(4);
            encoder.fixed(id.as_bytes());
        }
        AssetReferenceOwner::TranscriptProjection(id) => {
            encoder.u8(5);
            encoder.fixed(id.as_bytes());
        }
    }
}

fn decode_owner(decoder: &mut Decoder<'_>) -> Result<AssetReferenceOwner, CodecError> {
    match decoder.u8()? {
        0 => Ok(AssetReferenceOwner::CurrentDraftMarker {
            draft_id: SyndicDraftId::from_bytes(decoder.fixed()?),
            marker_id: SyndicDraftMarkerId::from_bytes(decoder.fixed()?),
        }),
        1 => Ok(AssetReferenceOwner::AcceptedInput(
            SyndicAcceptedInputId::from_bytes(decoder.fixed()?),
        )),
        2 => Ok(AssetReferenceOwner::SubmittedTurnItem(
            SyndicItemId::from_bytes(decoder.fixed()?),
        )),
        3 => Ok(AssetReferenceOwner::QueuedInput(
            SyndicQueuedInputId::from_bytes(decoder.fixed()?),
        )),
        4 => Ok(AssetReferenceOwner::RetryRecord(
            SyndicRetryRecordId::from_bytes(decoder.fixed()?),
        )),
        5 => Ok(AssetReferenceOwner::TranscriptProjection(
            SyndicProjectionId::from_bytes(decoder.fixed()?),
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "asset reference owner",
            tag,
        }),
    }
}

fn encode_reference(value: &AssetReferenceRecord) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encode_owner(&mut encoder, value.owner);
    encode_asset(&mut encoder, value.asset_id);
    encoder.u64(value.created_at.get());
    encoder.finish()
}

fn decode_reference(encoded: &[u8]) -> Result<AssetReferenceRecord, CodecError> {
    let mut decoder = Decoder::new(encoded);
    let owner = decode_owner(&mut decoder)?;
    let asset_id = decode_asset(&mut decoder)?;
    let created_at = UnixMillis::new(decoder.u64()?);
    decoder.finish()?;
    Ok(AssetReferenceRecord {
        owner,
        asset_id,
        created_at,
    })
}

fn invalid(
    kind: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> CodecError {
    CodecError::InvalidValue {
        kind,
        source: Box::new(source),
    }
}
