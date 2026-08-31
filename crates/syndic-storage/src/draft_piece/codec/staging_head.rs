use beryl_model::SyndicDraftId;

use crate::codec::CodecError;
use crate::codec::parts::{Decoder, Encoder};

use super::super::staging::draft_mutation_staging_head_is_locally_exact;
use super::super::*;
use super::{dec_digest, dec_progress_reference, enc_digest, enc_progress_reference};

pub(super) fn enc_staging_identity(e: &mut Encoder, value: DraftMutationStagingIdentityV1) {
    e.fixed16(value.draft_id().as_bytes());
    e.fixed16(value.session_id().as_bytes());
    e.fixed16(value.operation_id().as_bytes());
}

pub(super) fn dec_staging_identity(
    d: &mut Decoder<'_>,
) -> Result<DraftMutationStagingIdentityV1, CodecError> {
    Ok(DraftMutationStagingIdentityV1::new(
        SyndicDraftId::from_bytes(d.fixed16()?),
        DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?),
        DraftMutationOperationIdV1::from_bytes(d.fixed16()?),
    ))
}

pub(super) fn enc_staging_progress_key(
    e: &mut Encoder,
    key: DraftMutationStagingProgressReceiptKeyV1,
) {
    enc_staging_identity(e, key.identity());
    e.u64(key.transition_ordinal());
}
pub(super) fn dec_staging_progress_key(
    d: &mut Decoder<'_>,
) -> Result<DraftMutationStagingProgressReceiptKeyV1, CodecError> {
    DraftMutationStagingProgressReceiptKeyV1::new(dec_staging_identity(d)?, d.u64()?).ok_or(
        CodecError::InvalidLength("draft mutation staging transition ordinal"),
    )
}
pub(super) fn encode_staging_progress_key(
    key: &DraftMutationStagingProgressReceiptKeyV1,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_staging_progress_key(&mut e, *key);
    Ok(e.finish())
}
pub(super) fn decode_staging_progress_key(
    bytes: &[u8],
) -> Result<DraftMutationStagingProgressReceiptKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = dec_staging_progress_key(&mut d)?;
    d.finish()?;
    Ok(value)
}

pub(super) fn enc_staging_frontier(e: &mut Encoder, value: DraftMutationStagingLaneFrontierV1) {
    e.u64(value.next_cursor());
    e.u64(value.next_ordinal());
    e.u64(value.item_total());
    e.u64(value.canonical_byte_total());
    enc_digest(e, value.cumulative_identity());
}

pub(super) fn dec_staging_frontier(
    d: &mut Decoder<'_>,
) -> Result<DraftMutationStagingLaneFrontierV1, CodecError> {
    DraftMutationStagingLaneFrontierV1::new(d.u64()?, d.u64()?, d.u64()?, d.u64()?, dec_digest(d)?)
        .ok_or(CodecError::InvalidLength("draft mutation staging frontier"))
}

fn enc_staging_begin(e: &mut Encoder, value: DraftMutationBeginV1) {
    enc_staging_identity(e, value.identity());
    e.u8(DraftMutationStagingEncodingVersionV1::VALUE);
    e.u64(value.session_generation());
    e.u64(value.predecessor_candidate_generation());
    enc_root_reference(e, value.predecessor_root());
    enc_history_reference(e, value.predecessor_history());
    e.u64(value.predecessor_extent().logical_utf8_bytes());
    e.u64(value.predecessor_extent().logical_line_count());
    enc_position(e, value.predecessor_caret());
    enc_position(e, value.predecessor_selection_anchor());
    enc_position(e, value.predecessor_selection_head());
    enc_position(e, value.replacement_start());
    enc_position(e, value.replacement_end());
    e.u64(value.source_initial_cursor());
    e.u64(value.proposal_initial_cursor());
    enc_writer_admission(e, value.writer_admission());
}

pub(crate) fn canonical_staging_begin_bytes(value: DraftMutationBeginV1) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_staging_begin(&mut e, value);
    e.finish()
}

fn dec_staging_begin(d: &mut Decoder<'_>) -> Result<DraftMutationBeginV1, CodecError> {
    let identity = dec_staging_identity(d)?;
    if d.u8()? != DraftMutationStagingEncodingVersionV1::VALUE {
        return Err(CodecError::InvalidLength(
            "draft mutation staging encoding version",
        ));
    }
    let session_generation = d.u64()?;
    let predecessor_candidate_generation = d.u64()?;
    let predecessor_root = dec_root_reference(d)?;
    let predecessor_history = dec_history_reference(d)?;
    let predecessor_extent = DraftLogicalExtentV1::new(d.u64()?, d.u64()?);
    let begin = DraftMutationBeginV1::new(
        identity,
        session_generation,
        predecessor_candidate_generation,
        predecessor_root,
        predecessor_history,
        predecessor_extent,
        dec_position(d)?,
        dec_position(d)?,
        dec_position(d)?,
        dec_position(d)?,
        dec_position(d)?,
        d.u64()?,
        d.u64()?,
    );
    Ok(match dec_writer_admission(d)? {
        Some(admission) => begin.with_writer_admission(admission),
        None => begin,
    })
}

fn enc_staging_finish(e: &mut Encoder, value: DraftMutationFinishInputV1) {
    enc_staging_frontier(e, value.source());
    enc_staging_frontier(e, value.proposal());
    e.u64(value.intended_extent().logical_utf8_bytes());
    e.u64(value.intended_extent().logical_line_count());
    enc_position(e, value.intended_caret());
    enc_position(e, value.intended_selection_anchor());
    enc_position(e, value.intended_selection_head());
    enc_digest(e, value.proposal_fragment_chain());
}

pub(crate) fn canonical_staging_finish_bytes(value: DraftMutationFinishInputV1) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_staging_finish(&mut e, value);
    e.finish()
}

fn dec_staging_finish(d: &mut Decoder<'_>) -> Result<DraftMutationFinishInputV1, CodecError> {
    let source = dec_staging_frontier(d)?;
    let proposal = dec_staging_frontier(d)?;
    let extent = DraftLogicalExtentV1::new(d.u64()?, d.u64()?);
    Ok(DraftMutationFinishInputV1::new(
        source,
        proposal,
        extent,
        dec_position(d)?,
        dec_position(d)?,
        dec_position(d)?,
        dec_digest(d)?,
    ))
}

pub(super) fn enc_staging_lifecycle(e: &mut Encoder, value: DraftMutationStagingLifecycleV1) {
    match value {
        DraftMutationStagingLifecycleV1::Receiving => e.u8(0),
        DraftMutationStagingLifecycleV1::Finished(finish) => {
            e.u8(1);
            enc_staging_finish(e, finish);
        }
        DraftMutationStagingLifecycleV1::Building(receipt) => {
            e.u8(2);
            enc_progress_reference(e, receipt);
        }
        DraftMutationStagingLifecycleV1::Cancelled => e.u8(3),
        DraftMutationStagingLifecycleV1::Rejected => e.u8(4),
        DraftMutationStagingLifecycleV1::Conflict => e.u8(5),
        DraftMutationStagingLifecycleV1::Error => e.u8(6),
    }
}

pub(super) fn dec_staging_lifecycle(
    d: &mut Decoder<'_>,
) -> Result<DraftMutationStagingLifecycleV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftMutationStagingLifecycleV1::Receiving),
        1 => Ok(DraftMutationStagingLifecycleV1::Finished(
            dec_staging_finish(d)?,
        )),
        2 => Ok(DraftMutationStagingLifecycleV1::Building(
            dec_progress_reference(d)?,
        )),
        3 => Ok(DraftMutationStagingLifecycleV1::Cancelled),
        4 => Ok(DraftMutationStagingLifecycleV1::Rejected),
        5 => Ok(DraftMutationStagingLifecycleV1::Conflict),
        6 => Ok(DraftMutationStagingLifecycleV1::Error),
        tag => Err(CodecError::InvalidTag {
            kind: "draft mutation staging lifecycle",
            tag,
        }),
    }
}

pub(super) fn enc_staging_receipt_reference(
    e: &mut Encoder,
    value: DraftMutationStagingProgressReceiptReferenceV1,
) {
    enc_staging_identity(e, value.identity());
    e.u64(value.transition_ordinal());
    enc_digest(e, value.digest());
}

pub(super) fn dec_staging_receipt_reference(
    d: &mut Decoder<'_>,
) -> Result<DraftMutationStagingProgressReceiptReferenceV1, CodecError> {
    DraftMutationStagingProgressReceiptReferenceV1::new(
        dec_staging_identity(d)?,
        d.u64()?,
        dec_digest(d)?,
    )
    .ok_or(CodecError::InvalidLength(
        "draft mutation staging receipt reference",
    ))
}

pub(super) fn encode_staging_head_key(
    key: &DraftMutationStagingIdentityV1,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_staging_identity(&mut e, *key);
    Ok(e.finish())
}

pub(super) fn decode_staging_head_key(
    bytes: &[u8],
) -> Result<DraftMutationStagingIdentityV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = dec_staging_identity(&mut d)?;
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_staging_head(
    value: &DraftMutationStagingHeadV1,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_staging_identity(&mut e, value.identity());
    enc_staging_begin(&mut e, value.begin());
    enc_digest(&mut e, value.begin_digest());
    enc_staging_frontier(&mut e, value.source());
    enc_staging_frontier(&mut e, value.proposal());
    enc_staging_receipt_reference(&mut e, value.receipt());
    enc_staging_lifecycle(&mut e, value.lifecycle());
    enc_digest(&mut e, value.digest());
    Ok(e.finish())
}

pub(crate) fn canonical_staging_head_digest_bytes(value: &DraftMutationStagingHeadV1) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_staging_identity(&mut e, value.identity());
    enc_staging_begin(&mut e, value.begin());
    enc_digest(&mut e, value.begin_digest());
    enc_staging_frontier(&mut e, value.source());
    enc_staging_frontier(&mut e, value.proposal());
    enc_staging_identity(&mut e, value.receipt().identity());
    e.u64(value.receipt().transition_ordinal());
    enc_staging_lifecycle(&mut e, value.lifecycle());
    e.finish()
}

pub(super) fn decode_staging_head(bytes: &[u8]) -> Result<DraftMutationStagingHeadV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = DraftMutationStagingHeadV1::from_parts(
        dec_staging_identity(&mut d)?,
        dec_staging_begin(&mut d)?,
        dec_digest(&mut d)?,
        dec_staging_frontier(&mut d)?,
        dec_staging_frontier(&mut d)?,
        dec_staging_receipt_reference(&mut d)?,
        dec_staging_lifecycle(&mut d)?,
        dec_digest(&mut d)?,
    );
    d.finish()?;
    if !draft_mutation_staging_head_is_locally_exact(&value) {
        return Err(CodecError::InvalidLength(
            "draft mutation staging head closure",
        ));
    }
    Ok(value)
}
