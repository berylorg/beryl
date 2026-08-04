use std::num::NonZeroU64;

use beryl_model::{
    AssetId, AssetIdentityVersion, AssetReferenceSetDigest, AssetReferenceSetId, ImageLabelOrdinal,
    SealedAssetReferenceSetProof, SealedContentMarkerSummary, SyndicAcceptedInputId,
    SyndicContentDigest, SyndicContentId, SyndicDraftId, SyndicItemId, SyndicProjectionId,
    SyndicRetryRecordId,
};

use crate::encoding::{CodecError, Decoder, Encoder};

use super::super::{AssetDimensions, AssetOwner, AssetReferenceOrdinal};

pub(super) fn encoded_asset(asset_id: AssetId) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encode_asset(&mut encoder, asset_id);
    encoder.finish()
}

pub(super) fn encode_asset(encoder: &mut Encoder, asset_id: AssetId) {
    encoder.u8(match asset_id.version() {
        AssetIdentityVersion::Sha256V1 => 1,
    });
    encoder.fixed_32(&asset_id.digest());
    encoder.u64(asset_id.length().get());
}

pub(super) fn decode_asset(decoder: &mut Decoder<'_>) -> Result<AssetId, CodecError> {
    match decoder.u8()? {
        1 => {
            let digest = decoder.fixed_32()?;
            let length = NonZeroU64::new(decoder.u64()?)
                .ok_or_else(|| invalid_message("asset byte length", "length must be nonzero"))?;
            Ok(AssetId::sha256_v1(digest, length))
        }
        tag => Err(invalid_tag("asset identity version", tag)),
    }
}

pub(super) fn encode_dimensions(encoder: &mut Encoder, dimensions: Option<AssetDimensions>) {
    match dimensions {
        Some(dimensions) => {
            encoder.u8(1);
            encoder.u64(dimensions.width().get());
            encoder.u64(dimensions.height().get());
        }
        None => encoder.u8(0),
    }
}

pub(super) fn decode_dimensions(
    decoder: &mut Decoder<'_>,
) -> Result<Option<AssetDimensions>, CodecError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => {
            let width = NonZeroU64::new(decoder.u64()?)
                .ok_or_else(|| invalid_message("asset width", "width must be nonzero"))?;
            let height = NonZeroU64::new(decoder.u64()?)
                .ok_or_else(|| invalid_message("asset height", "height must be nonzero"))?;
            Ok(Some(AssetDimensions::new(width, height)))
        }
        tag => Err(invalid_tag("asset dimensions option", tag)),
    }
}

pub(super) fn encode_source(encoder: &mut Encoder, source: SealedContentMarkerSummary) {
    encoder.fixed(source.content_id().as_bytes());
    encoder.fixed_32(source.content_digest().as_bytes());
    encoder.fixed_32(&source.marker_digest());
    encoder.u64(source.marker_count());
    encode_image_label_option(encoder, source.maximum_image_label());
}

pub(super) fn decode_source(
    decoder: &mut Decoder<'_>,
) -> Result<SealedContentMarkerSummary, CodecError> {
    SealedContentMarkerSummary::new(
        SyndicContentId::from_bytes(decoder.fixed()?),
        SyndicContentDigest::from_bytes(decoder.fixed_32()?),
        decoder.fixed_32()?,
        decoder.u64()?,
        decode_image_label_option(decoder)?,
    )
    .map_err(|source| invalid("sealed content marker summary", source))
}

pub(super) fn encode_sealed_proof(encoder: &mut Encoder, proof: SealedAssetReferenceSetProof) {
    encoder.fixed(proof.set_id().as_bytes());
    encode_source(encoder, proof.source());
    encoder.u64(proof.entry_frontier());
    encoder.fixed_32(&proof.asset_chain_digest().as_bytes());
}

pub(super) fn decode_sealed_proof(
    decoder: &mut Decoder<'_>,
) -> Result<SealedAssetReferenceSetProof, CodecError> {
    SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes(decoder.fixed()?),
        decode_source(decoder)?,
        decoder.u64()?,
        AssetReferenceSetDigest::from_bytes(decoder.fixed_32()?),
    )
    .map_err(|source| invalid("sealed asset reference-set proof", source))
}

pub(super) fn encode_image_label_option(encoder: &mut Encoder, value: Option<ImageLabelOrdinal>) {
    match value {
        Some(label) => {
            encoder.u8(1);
            encoder.u64(label.get());
        }
        None => encoder.u8(0),
    }
}

pub(super) fn decode_image_label_option(
    decoder: &mut Decoder<'_>,
) -> Result<Option<ImageLabelOrdinal>, CodecError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => ImageLabelOrdinal::new(decoder.u64()?)
            .map(Some)
            .map_err(|source| invalid("maximum image label", source)),
        tag => Err(invalid_tag("maximum image label option", tag)),
    }
}

pub(super) fn encode_owner(encoder: &mut Encoder, owner: AssetOwner) {
    match owner {
        AssetOwner::CurrentDraft(id) => {
            encoder.u8(0);
            encoder.fixed(id.as_bytes());
        }
        AssetOwner::AcceptedInput(id) => {
            encoder.u8(1);
            encoder.fixed(id.as_bytes());
        }
        AssetOwner::SubmittedTurnItem(id) => {
            encoder.u8(2);
            encoder.fixed(id.as_bytes());
        }
        AssetOwner::RetryRecord(id) => {
            encoder.u8(3);
            encoder.fixed(id.as_bytes());
        }
        AssetOwner::TranscriptProjection(id) => {
            encoder.u8(4);
            encoder.fixed(id.as_bytes());
        }
    }
}

pub(super) fn decode_owner(decoder: &mut Decoder<'_>) -> Result<AssetOwner, CodecError> {
    match decoder.u8()? {
        0 => Ok(AssetOwner::CurrentDraft(SyndicDraftId::from_bytes(
            decoder.fixed()?,
        ))),
        1 => Ok(AssetOwner::AcceptedInput(
            SyndicAcceptedInputId::from_bytes(decoder.fixed()?),
        )),
        2 => Ok(AssetOwner::SubmittedTurnItem(SyndicItemId::from_bytes(
            decoder.fixed()?,
        ))),
        3 => Ok(AssetOwner::RetryRecord(SyndicRetryRecordId::from_bytes(
            decoder.fixed()?,
        ))),
        4 => Ok(AssetOwner::TranscriptProjection(
            SyndicProjectionId::from_bytes(decoder.fixed()?),
        )),
        tag => Err(invalid_tag("asset owner", tag)),
    }
}

pub(super) fn decode_ordinal(
    decoder: &mut Decoder<'_>,
) -> Result<AssetReferenceOrdinal, CodecError> {
    AssetReferenceOrdinal::new(decoder.u64()?)
        .map_err(|source| invalid("asset reference ordinal", source))
}

pub(super) fn decode_fixed_16(encoded: &[u8], kind: &'static str) -> Result<[u8; 16], CodecError> {
    encoded
        .try_into()
        .map_err(|_| CodecError::InvalidLength { kind })
}

pub(super) fn invalid_tag(kind: &'static str, tag: u8) -> CodecError {
    CodecError::InvalidTag { kind, tag }
}

fn invalid_message(kind: &'static str, message: &'static str) -> CodecError {
    invalid(kind, std::io::Error::other(message))
}

pub(super) fn invalid(
    kind: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> CodecError {
    CodecError::InvalidValue {
        kind,
        source: Box::new(source),
    }
}
