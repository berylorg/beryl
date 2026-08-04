use beryl_home_store::{RecordCodec, RecordVersion};
use beryl_model::{
    AssetId, AssetReferenceSetDigest, AssetReferenceSetId, DomainRevision, ImageLabelOrdinal,
    SyndicDraftMarkerId,
};

use crate::encoding::{CodecError, Decoder, Encoder};

use super::{
    ASSET_ENTRY_LIMIT, ASSET_HEAD_LIMIT, ASSET_INDEX_LIMIT, ASSET_MANIFEST_LIMIT,
    ASSET_METADATA_LIMIT, AssetDomain, AssetEntryKey, AssetLabelDisposition, AssetLabelFirstKey,
    AssetLabelFirstRecord, AssetMarkerKey, AssetMediaType, AssetMetadataRecord, AssetOwner,
    AssetOwnerHeadRecord, AssetReferenceEntryRecord, AssetReferenceOrdinal,
    AssetReferenceSetLifecycle, AssetReferenceSetManifest, AssetSidecarState,
};

mod parts;

use parts::{
    decode_asset, decode_dimensions, decode_fixed_16, decode_image_label_option, decode_ordinal,
    decode_owner, decode_sealed_proof, decode_source, encode_asset, encode_dimensions,
    encode_image_label_option, encode_owner, encode_sealed_proof, encode_source, encoded_asset,
    invalid, invalid_tag,
};

pub(super) struct AssetMetadataCodec;

impl RecordCodec<AssetDomain> for AssetMetadataCodec {
    type Key = AssetId;
    type Value = AssetMetadataRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "metadata";
    const VERSION: RecordVersion = RecordVersion::new(2);
    const MAX_KEY_BYTES: usize = 41;
    const MAX_VALUE_BYTES: usize = ASSET_METADATA_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(encoded_asset(*key))
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let value = decode_asset(&mut decoder)?;
        decoder.finish()?;
        Ok(value)
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
            tag => return Err(invalid_tag("asset sidecar state", tag)),
        };
        let revision = decoder.record_revision()?;
        decoder.finish()?;
        Ok(AssetMetadataRecord {
            asset_id,
            media_type,
            dimensions,
            creation_revision,
            sidecar_state,
            revision,
        })
    }
}

pub(super) struct AssetReferenceManifestCodec;

impl RecordCodec<AssetDomain> for AssetReferenceManifestCodec {
    type Key = AssetReferenceSetId;
    type Value = AssetReferenceSetManifest;
    type Error = CodecError;

    const FAMILY: &'static str = "reference_set_manifests";
    const VERSION: RecordVersion = RecordVersion::new(2);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = ASSET_MANIFEST_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        decode_fixed_16(encoded, "asset reference-set identity")
            .map(AssetReferenceSetId::from_bytes)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(value.set_id.as_bytes());
        encode_source(&mut encoder, value.source);
        encoder.u8(match value.lifecycle {
            AssetReferenceSetLifecycle::Building => 0,
            AssetReferenceSetLifecycle::Sealed => 1,
        });
        encoder.u64(value.marker_count);
        encoder.fixed_32(&value.marker_digest);
        encode_image_label_option(&mut encoder, value.maximum_image_label);
        encoder.u64(value.entry_frontier);
        encoder.fixed_32(&value.asset_chain_digest.as_bytes());
        encoder.u64(value.revision.get());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let set_id = AssetReferenceSetId::from_bytes(decoder.fixed()?);
        let source = decode_source(&mut decoder)?;
        let lifecycle = match decoder.u8()? {
            0 => AssetReferenceSetLifecycle::Building,
            1 => AssetReferenceSetLifecycle::Sealed,
            tag => return Err(invalid_tag("asset reference-set lifecycle", tag)),
        };
        let marker_count = decoder.u64()?;
        let marker_digest = decoder.fixed_32()?;
        let maximum_image_label = decode_image_label_option(&mut decoder)?;
        let entry_frontier = decoder.u64()?;
        let asset_chain_digest = AssetReferenceSetDigest::from_bytes(decoder.fixed_32()?);
        let revision = decoder.record_revision()?;
        decoder.finish()?;
        Ok(AssetReferenceSetManifest {
            set_id,
            source,
            lifecycle,
            marker_count,
            marker_digest,
            maximum_image_label,
            entry_frontier,
            asset_chain_digest,
            revision,
        })
    }
}

pub(super) struct AssetReferenceEntryCodec;

impl RecordCodec<AssetDomain> for AssetReferenceEntryCodec {
    type Key = AssetEntryKey;
    type Value = AssetReferenceEntryRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "reference_set_entries";
    const VERSION: RecordVersion = RecordVersion::new(2);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = ASSET_ENTRY_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(key.set_id.as_bytes());
        encoder.u64(key.ordinal.get());
        Ok(encoder.finish())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let set_id = AssetReferenceSetId::from_bytes(decoder.fixed()?);
        let ordinal = decode_ordinal(&mut decoder)?;
        decoder.finish()?;
        Ok(AssetEntryKey { set_id, ordinal })
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(value.set_id.as_bytes());
        encoder.u64(value.ordinal.get());
        encoder.fixed(value.marker_id.as_bytes());
        encoder.u64(value.label.get());
        encode_asset(&mut encoder, value.asset_id);
        match value.label_disposition {
            AssetLabelDisposition::First => encoder.u8(0),
            AssetLabelDisposition::Repeated { first_ordinal } => {
                encoder.u8(1);
                encoder.u64(first_ordinal.get());
            }
        }
        encoder.fixed_32(&value.chain_digest.as_bytes());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let set_id = AssetReferenceSetId::from_bytes(decoder.fixed()?);
        let ordinal = decode_ordinal(&mut decoder)?;
        let marker_id = SyndicDraftMarkerId::from_bytes(decoder.fixed()?);
        let label = ImageLabelOrdinal::new(decoder.u64()?)
            .map_err(|source| invalid("image-label ordinal", source))?;
        let asset_id = decode_asset(&mut decoder)?;
        let label_disposition = match decoder.u8()? {
            0 => AssetLabelDisposition::First,
            1 => AssetLabelDisposition::Repeated {
                first_ordinal: decode_ordinal(&mut decoder)?,
            },
            tag => return Err(invalid_tag("asset label disposition", tag)),
        };
        let chain_digest = AssetReferenceSetDigest::from_bytes(decoder.fixed_32()?);
        decoder.finish()?;
        Ok(AssetReferenceEntryRecord {
            set_id,
            ordinal,
            marker_id,
            label,
            asset_id,
            label_disposition,
            chain_digest,
        })
    }
}

pub(super) struct AssetReferenceMarkerCodec;

impl RecordCodec<AssetDomain> for AssetReferenceMarkerCodec {
    type Key = AssetMarkerKey;
    type Value = AssetReferenceOrdinal;
    type Error = CodecError;

    const FAMILY: &'static str = "reference_set_markers";
    const VERSION: RecordVersion = RecordVersion::new(2);
    const MAX_KEY_BYTES: usize = 32;
    const MAX_VALUE_BYTES: usize = ASSET_INDEX_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(key.set_id.as_bytes());
        encoder.fixed(key.marker_id.as_bytes());
        Ok(encoder.finish())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let key = AssetMarkerKey {
            set_id: AssetReferenceSetId::from_bytes(decoder.fixed()?),
            marker_id: SyndicDraftMarkerId::from_bytes(decoder.fixed()?),
        };
        decoder.finish()?;
        Ok(key)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(value.get().to_be_bytes().to_vec())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let value = decode_ordinal(&mut decoder)?;
        decoder.finish()?;
        Ok(value)
    }
}

pub(super) struct AssetReferenceLabelFirstCodec;

impl RecordCodec<AssetDomain> for AssetReferenceLabelFirstCodec {
    type Key = AssetLabelFirstKey;
    type Value = AssetLabelFirstRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "reference_set_label_first";
    const VERSION: RecordVersion = RecordVersion::new(2);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = ASSET_INDEX_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.fixed(key.set_id.as_bytes());
        encoder.u64(key.label.get());
        Ok(encoder.finish())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let set_id = AssetReferenceSetId::from_bytes(decoder.fixed()?);
        let label = ImageLabelOrdinal::new(decoder.u64()?)
            .map_err(|source| invalid("image-label ordinal", source))?;
        decoder.finish()?;
        Ok(AssetLabelFirstKey { set_id, label })
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encoder.u64(value.first_ordinal.get());
        encode_asset(&mut encoder, value.asset_id);
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let first_ordinal = decode_ordinal(&mut decoder)?;
        let asset_id = decode_asset(&mut decoder)?;
        decoder.finish()?;
        Ok(AssetLabelFirstRecord {
            first_ordinal,
            asset_id,
        })
    }
}

pub(super) struct AssetOwnerHeadCodec;

impl RecordCodec<AssetDomain> for AssetOwnerHeadCodec {
    type Key = AssetOwner;
    type Value = AssetOwnerHeadRecord;
    type Error = CodecError;

    const FAMILY: &'static str = "owner_heads";
    const VERSION: RecordVersion = RecordVersion::new(2);
    const MAX_KEY_BYTES: usize = 17;
    const MAX_VALUE_BYTES: usize = ASSET_HEAD_LIMIT;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encode_owner(&mut encoder, *key);
        Ok(encoder.finish())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let value = decode_owner(&mut decoder)?;
        decoder.finish()?;
        Ok(value)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        let mut encoder = Encoder::new();
        encode_owner(&mut encoder, value.owner);
        encode_sealed_proof(&mut encoder, value.set);
        encoder.u64(value.owner_revision.get());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        let mut decoder = Decoder::new(encoded);
        let owner = decode_owner(&mut decoder)?;
        let set = decode_sealed_proof(&mut decoder)?;
        let owner_revision = decoder.record_revision()?;
        decoder.finish()?;
        Ok(AssetOwnerHeadRecord {
            owner,
            set,
            owner_revision,
        })
    }
}
