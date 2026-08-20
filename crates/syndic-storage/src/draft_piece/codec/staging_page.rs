use crate::codec::CodecError;
use crate::codec::parts::{Decoder, Encoder};

use super::super::staging::draft_mutation_staging_page_is_locally_exact;
use super::super::*;
use super::staging_head::{dec_staging_identity, enc_staging_identity};
use super::{dec_digest, dec_replacement, enc_digest, enc_replacement};

pub(super) fn enc_staging_page_key(e: &mut Encoder, key: DraftMutationStagingPageKeyV1) {
    enc_staging_identity(e, key.identity());
    e.u8(match key.lane() {
        DraftMutationStagingLaneV1::Source => 0,
        DraftMutationStagingLaneV1::Proposal => 1,
    });
    e.u64(key.ordinal());
}

pub(super) fn dec_staging_page_key(
    d: &mut Decoder<'_>,
) -> Result<DraftMutationStagingPageKeyV1, CodecError> {
    let identity = dec_staging_identity(d)?;
    let lane = match d.u8()? {
        0 => DraftMutationStagingLaneV1::Source,
        1 => DraftMutationStagingLaneV1::Proposal,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft mutation staging lane",
                tag,
            });
        }
    };
    DraftMutationStagingPageKeyV1::new(identity, lane, d.u64()?).ok_or(CodecError::InvalidLength(
        "draft mutation staging page ordinal",
    ))
}

pub(super) fn encode_staging_page_key(
    key: &DraftMutationStagingPageKeyV1,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_staging_page_key(&mut e, *key);
    Ok(e.finish())
}

pub(super) fn decode_staging_page_key(
    bytes: &[u8],
) -> Result<DraftMutationStagingPageKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = dec_staging_page_key(&mut d)?;
    d.finish()?;
    Ok(value)
}

fn enc_staging_item(e: &mut Encoder, item: &DraftMutationStagingPageItemV1) {
    match item {
        DraftMutationStagingPageItemV1::SourcePosition(position) => {
            e.u8(0);
            enc_position(e, *position);
        }
        DraftMutationStagingPageItemV1::Proposal(replacement) => {
            e.u8(1);
            enc_replacement(e, replacement);
        }
    }
}

pub(crate) fn canonical_staging_items_bytes(
    items: &[DraftMutationStagingPageItemV1],
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    e.u32(
        u32::try_from(items.len())
            .map_err(|_| CodecError::InvalidLength("draft mutation staging page items"))?,
    );
    for item in items {
        enc_staging_item(&mut e, item);
    }
    Ok(e.finish())
}

fn dec_staging_item(d: &mut Decoder<'_>) -> Result<DraftMutationStagingPageItemV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftMutationStagingPageItemV1::SourcePosition(
            dec_position(d)?,
        )),
        1 => Ok(DraftMutationStagingPageItemV1::Proposal(dec_replacement(
            d,
        )?)),
        tag => Err(CodecError::InvalidTag {
            kind: "draft mutation staging page item",
            tag,
        }),
    }
}

pub(super) fn encode_staging_page(
    value: &DraftMutationStagingPageV1,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_staging_page_key(&mut e, value.key());
    e.u64(value.transition_ordinal());
    e.u64(value.input_cursor());
    e.u64(value.successor_cursor());
    e.u32(u32::from(value.item_ceiling()));
    e.u32(value.byte_ceiling());
    enc_digest(&mut e, value.prior_cumulative_identity());
    enc_digest(&mut e, value.successor_cumulative_identity());
    e.u64(value.cumulative_item_total());
    e.u64(value.cumulative_byte_total());
    e.u32(
        u32::try_from(value.items().len())
            .map_err(|_| CodecError::InvalidLength("draft mutation staging page items"))?,
    );
    for item in value.items() {
        enc_staging_item(&mut e, item);
    }
    enc_digest(&mut e, value.digest());
    let bytes = e.finish();
    if bytes.len() > DRAFT_PIECE_PAGE_MAX_BYTES {
        return Err(CodecError::InvalidLength(
            "draft mutation staging page bytes",
        ));
    }
    Ok(bytes)
}

pub(crate) fn canonical_staging_page_bytes(
    value: &DraftMutationStagingPageV1,
) -> Result<Vec<u8>, CodecError> {
    encode_staging_page(value)
}

pub(super) fn decode_staging_page(bytes: &[u8]) -> Result<DraftMutationStagingPageV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_staging_page_key(&mut d)?;
    let transition_ordinal = d.u64()?;
    let input_cursor = d.u64()?;
    let successor_cursor = d.u64()?;
    let item_ceiling = u16::try_from(d.u32()?)
        .map_err(|_| CodecError::InvalidLength("draft mutation staging item ceiling"))?;
    let byte_ceiling = d.u32()?;
    let prior = dec_digest(&mut d)?;
    let successor = dec_digest(&mut d)?;
    let item_total = d.u64()?;
    let byte_total = d.u64()?;
    let count = usize::try_from(d.u32()?)
        .map_err(|_| CodecError::InvalidLength("draft mutation staging page items"))?;
    if count == 0 || count > DRAFT_PIECE_PAGE_MAX_RECORDS || count > usize::from(item_ceiling) {
        return Err(CodecError::InvalidLength(
            "draft mutation staging page items",
        ));
    }
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(dec_staging_item(&mut d)?);
    }
    let digest = dec_digest(&mut d)?;
    d.finish()?;
    let value = DraftMutationStagingPageV1::from_parts(
        key,
        transition_ordinal,
        input_cursor,
        successor_cursor,
        item_ceiling,
        byte_ceiling,
        prior,
        successor,
        item_total,
        byte_total,
        items.into_boxed_slice(),
        digest,
    );
    if !draft_mutation_staging_page_is_locally_exact(&value) {
        return Err(CodecError::InvalidLength(
            "draft mutation staging page closure",
        ));
    }
    Ok(value)
}
