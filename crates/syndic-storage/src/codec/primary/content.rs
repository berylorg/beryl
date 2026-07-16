use super::*;

pub(crate) struct ContentManifestsFamily;
pub(crate) type ContentManifestsCodec = ExactCodec<ContentManifestsFamily>;

impl Family for ContentManifestsFamily {
    type Key = SyndicContentId;
    type Value = ContentManifestRecord;
    const NAME: &'static str = "content-manifests";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(2);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        key16(encoded, "content key", SyndicContentId::from_bytes)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_content(&mut e, value.id());
        enc_opt(&mut e, value.owner(), enc_item);
        enc_content_rev(&mut e, value.revision());
        enc_content_encoding(&mut e, value.encoding());
        enc_content_lifecycle(&mut e, value.lifecycle());
        e.u64(value.chunk_count());
        e.u64(value.encoded_bytes());
        e.fixed32(value.chain_digest().as_bytes());
        enc_content_summary(&mut e, value.expected());
        Ok(e.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(encoded);
        let value = ContentManifestRecord::with_owner(
            dec_content(&mut d)?,
            dec_opt(&mut d, "content owner", dec_item)?,
            dec_content_rev(&mut d)?,
            dec_content_encoding(&mut d)?,
            dec_content_lifecycle(&mut d)?,
            d.u64()?,
            d.u64()?,
            SyndicContentDigest::from_bytes(d.fixed32()?),
            dec_content_summary(&mut d)?,
        );
        d.finish()?;
        Ok(value)
    }
}

pub(crate) struct ContentChunksFamily;
pub(crate) type ContentChunksCodec = ExactCodec<ContentChunksFamily>;

impl Family for ContentChunksFamily {
    type Key = ContentChunkKey;
    type Value = ContentChunkRecord;
    const NAME: &'static str = "content-chunks";
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = CONTENT_CHUNK_MAX_BYTES + 128;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        ContentChunkKey::decode(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_content(&mut e, value.content_id());
        enc_content_chunk_ord(&mut e, value.ordinal());
        e.fixed32(value.digest());
        e.bytes(value.bytes());
        Ok(e.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(encoded);
        let content_id = dec_content(&mut d)?;
        let ordinal = dec_content_chunk_ord(&mut d)?;
        let digest = d.fixed32()?;
        let value = ContentChunkRecord::new(content_id, ordinal, d.bytes("content chunk")?)
            .map_err(|source| invalid("content chunk", source))?;
        if value.digest() != &digest {
            return Err(CodecError::InvalidLength("content chunk digest"));
        }
        d.finish()?;
        Ok(value)
    }
}

pub(crate) struct InputMarkerResolutionsFamily;
pub(crate) type InputMarkerResolutionsCodec = ExactCodec<InputMarkerResolutionsFamily>;

impl Family for InputMarkerResolutionsFamily {
    type Key = InputMarkerKey;
    type Value = InputMarkerResolutionRecord;
    const NAME: &'static str = "input-marker-resolutions";
    const MAX_KEY_BYTES: usize = 25;
    const MAX_VALUE_BYTES: usize = 128;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        InputMarkerKey::decode(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_input_marker_owner(&mut e, value.owner());
        enc_input_marker_ord(&mut e, value.ordinal());
        let marker = value.marker();
        enc_marker(&mut e, marker.marker_id());
        enc_image_label(&mut e, marker.label());
        enc_asset_id(&mut e, marker.asset_id());
        Ok(e.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(encoded);
        let value = InputMarkerResolutionRecord::new(
            dec_input_marker_owner(&mut d)?,
            dec_input_marker_ord(&mut d)?,
            ResolvedImageMarker::new(
                dec_marker(&mut d)?,
                dec_image_label(&mut d)?,
                dec_asset_id(&mut d)?,
            ),
        );
        d.finish()?;
        Ok(value)
    }
}
