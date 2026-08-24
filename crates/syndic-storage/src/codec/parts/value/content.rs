use std::num::NonZeroU64;

use super::*;

pub(crate) fn enc_content_encoding(e: &mut Encoder, value: crate::ContentEncoding) {
    e.u8(match value {
        crate::ContentEncoding::ComposerV1 => 0,
        crate::ContentEncoding::Utf8V1 => 1,
        crate::ContentEncoding::ProviderItemV1 => 2,
    });
}

pub(crate) fn dec_content_encoding(
    d: &mut Decoder<'_>,
) -> Result<crate::ContentEncoding, CodecError> {
    match d.u8()? {
        0 => Ok(crate::ContentEncoding::ComposerV1),
        1 => Ok(crate::ContentEncoding::Utf8V1),
        2 => Ok(crate::ContentEncoding::ProviderItemV1),
        tag => Err(CodecError::InvalidTag {
            kind: "content encoding",
            tag,
        }),
    }
}

pub(crate) fn enc_content_lifecycle(e: &mut Encoder, value: crate::ContentLifecycle) {
    e.u8(match value {
        crate::ContentLifecycle::Building => 0,
        crate::ContentLifecycle::Sealed => 1,
        crate::ContentLifecycle::Live => 2,
        crate::ContentLifecycle::Finalized => 3,
    });
}

pub(crate) fn dec_content_lifecycle(
    d: &mut Decoder<'_>,
) -> Result<crate::ContentLifecycle, CodecError> {
    match d.u8()? {
        0 => Ok(crate::ContentLifecycle::Building),
        1 => Ok(crate::ContentLifecycle::Sealed),
        2 => Ok(crate::ContentLifecycle::Live),
        3 => Ok(crate::ContentLifecycle::Finalized),
        tag => Err(CodecError::InvalidTag {
            kind: "content lifecycle",
            tag,
        }),
    }
}

pub(crate) fn enc_content_summary(e: &mut Encoder, value: crate::ContentSummary) {
    e.u64(value.chunk_count());
    e.u64(value.piece_count());
    e.u64(value.encoded_bytes());
    e.u64(value.logical_utf8_bytes());
    e.u64(value.atom_count());
    e.u64(value.image_marker_count());
    e.fixed32(&value.marker_digest());
    enc_opt(e, value.maximum_image_label(), enc_image_label);
    e.fixed32(value.digest().as_bytes());
}

pub(crate) fn dec_content_summary(
    d: &mut Decoder<'_>,
) -> Result<crate::ContentSummary, CodecError> {
    let chunk_count = d.u64()?;
    let piece_count = d.u64()?;
    let encoded_bytes = d.u64()?;
    let logical_utf8_bytes = d.u64()?;
    let atom_count = d.u64()?;
    let image_marker_count = d.u64()?;
    let marker_digest = d.fixed32()?;
    let maximum_image_label = dec_opt(d, "maximum image label", dec_image_label)?;
    let digest = SyndicContentDigest::from_bytes(d.fixed32()?);
    crate::ContentSummary::new(
        chunk_count,
        piece_count,
        encoded_bytes,
        logical_utf8_bytes,
        atom_count,
        image_marker_count,
        marker_digest,
        maximum_image_label,
        digest,
    )
    .map_err(|source| invalid("content marker summary", source))
}

pub(crate) fn enc_content_ref(e: &mut Encoder, value: crate::ContentReference) {
    enc_content(e, value.id());
    enc_content_rev(e, value.revision());
    enc_content_encoding(e, value.encoding());
    enc_content_summary(e, value.summary());
}

pub(crate) fn dec_content_ref(d: &mut Decoder<'_>) -> Result<crate::ContentReference, CodecError> {
    Ok(crate::ContentReference::new(
        dec_content(d)?,
        dec_content_rev(d)?,
        dec_content_encoding(d)?,
        dec_content_summary(d)?,
    ))
}

pub(crate) fn enc_asset_id(e: &mut Encoder, asset_id: AssetId) {
    e.u8(match asset_id.version() {
        AssetIdentityVersion::Sha256V1 => 1,
    });
    e.fixed32(&asset_id.digest());
    e.u64(asset_id.length().get());
}

pub(crate) fn dec_asset_id(d: &mut Decoder<'_>) -> Result<AssetId, CodecError> {
    match d.u8()? {
        1 => {
            let digest = d.fixed32()?;
            let length =
                NonZeroU64::new(d.u64()?).ok_or(CodecError::InvalidLength("asset byte length"))?;
            Ok(AssetId::sha256_v1(digest, length))
        }
        tag => Err(CodecError::InvalidTag {
            kind: "asset identity version",
            tag,
        }),
    }
}

pub(crate) fn enc_sealed_asset_reference_set_proof(
    e: &mut Encoder,
    proof: SealedAssetReferenceSetProof,
) {
    e.fixed16(proof.set_id().as_bytes());
    let summary = proof.summary();
    e.fixed32(&summary.marker_digest());
    e.u64(summary.marker_count());
    enc_opt(e, summary.maximum_image_label(), enc_image_label);
    e.u64(proof.entry_frontier());
    e.fixed32(&proof.asset_chain_digest().as_bytes());
}

pub(crate) fn dec_sealed_asset_reference_set_proof(
    d: &mut Decoder<'_>,
) -> Result<SealedAssetReferenceSetProof, CodecError> {
    let set_id = AssetReferenceSetId::from_bytes(d.fixed16()?);
    let summary = SequentialMarkerSummaryV1::new(
        d.fixed32()?,
        d.u64()?,
        dec_opt(d, "maximum image label", dec_image_label)?,
    )
    .map_err(|source| invalid("sequential marker summary", source))?;
    SealedAssetReferenceSetProof::new(
        set_id,
        summary,
        d.u64()?,
        AssetReferenceSetDigest::from_bytes(d.fixed32()?),
    )
    .map_err(|source| invalid("sealed asset reference-set proof", source))
}

pub(crate) fn enc_image_label_origin_owner(e: &mut Encoder, owner: crate::ImageLabelOriginOwner) {
    match owner {
        crate::ImageLabelOriginOwner::AcceptedInput(id) => {
            e.u8(0);
            enc_accepted(e, id);
        }
        crate::ImageLabelOriginOwner::CanonicalItem(id) => {
            e.u8(1);
            enc_item(e, id);
        }
    }
}

pub(crate) fn dec_image_label_origin_owner(
    d: &mut Decoder<'_>,
) -> Result<crate::ImageLabelOriginOwner, CodecError> {
    match d.u8()? {
        0 => dec_accepted(d).map(crate::ImageLabelOriginOwner::AcceptedInput),
        1 => dec_item(d).map(crate::ImageLabelOriginOwner::CanonicalItem),
        tag => Err(CodecError::InvalidTag {
            kind: "image-label origin owner",
            tag,
        }),
    }
}
