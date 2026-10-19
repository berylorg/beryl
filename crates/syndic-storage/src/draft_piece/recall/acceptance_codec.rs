use super::super::*;
use super::{AcceptanceDisposalV1, AcceptanceResultV1};
use crate::{AcceptedInputOrdinal, AcceptedRouteGeneration};
use crate::{
    FirstAcceptance, ImageLabelAuthorityHeadV1, ImageLabelFrontier,
    codec::{CodecError, parts::*},
};
use sha2::{Digest, Sha256};

pub(crate) fn enc_recall_activation(e: &mut Encoder, a: DraftEditorCandidateActivationBindingV1) {
    enc_draft(e, a.draft_id());
    e.fixed16(a.session_id().as_bytes());
    e.u64(a.session_generation());
    e.u64(a.candidate_generation());
    enc_root_reference(e, a.root());
    enc_history_reference(e, a.history());
    e.u64(a.logical_extent().logical_utf8_bytes());
    e.u64(a.logical_extent().logical_line_count());
}
pub(crate) fn dec_recall_activation(
    d: &mut Decoder<'_>,
) -> Result<DraftEditorCandidateActivationBindingV1, CodecError> {
    let a = DraftEditorCandidateActivationBindingV1::new(
        dec_draft(d)?,
        DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?),
        d.u64()?,
        d.u64()?,
        dec_root_reference(d)?,
        dec_history_reference(d)?,
        DraftLogicalExtentV1::new(d.u64()?, d.u64()?),
    );
    if a.root().key().draft_id() != a.draft_id()
        || a.history().root() != a.root()
        || a.history().candidate_generation() != a.candidate_generation()
        || a.logical_extent() != a.root().summary().logical_extent()
    {
        return Err(CodecError::InvalidLength("recall activation"));
    }
    Ok(a)
}

pub(crate) fn canonical_first_acceptance_bytes(a: &FirstAcceptance) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_thread(&mut e, a.thread_id());
    enc_thread_rev(&mut e, a.expected_thread_revision());
    let authority = a.expected_image_label_authority();
    enc_thread(&mut e, authority.thread_id());
    e.u64(authority.revision());
    e.u64(authority.inherited().get());
    e.u64(authority.permanent().get());
    enc_draft(&mut e, a.draft_id());
    enc_draft_rev(&mut e, a.expected_draft_revision());
    enc_recall_activation(&mut e, a.candidate());
    let mapping = a.materialization();
    enc_root_reference(&mut e, mapping.key().source());
    e.u8(1);
    e.fixed16(&mapping.sealing_operation().as_bytes());
    e.fixed32(mapping.source_digest().as_bytes());
    e.u64(mapping.source_piece_count());
    e.u64(mapping.source_utf8_bytes());
    e.u64(mapping.source_marker_count());
    enc_content_ref(&mut e, mapping.content());
    enc_input_gate_rev(&mut e, a.expected_gate_revision());
    enc_input_gate_state(&mut e, a.expected_gate_state());
    enc_draft(&mut e, a.next_draft_id());
    enc_item(&mut e, a.idle_user_item_id());
    enc_opt(
        &mut e,
        a.asset_reference_set(),
        enc_sealed_asset_reference_set_proof,
    );
    e.fixed16(a.session_disposal_operation_id().as_bytes());
    enc_timestamp(&mut e, a.admitted_at());
    e.finish()
}

fn dec_first_acceptance(bytes: &[u8]) -> Result<FirstAcceptance, CodecError> {
    let mut d = Decoder::new(bytes);
    let thread = dec_thread(&mut d)?;
    let revision = dec_thread_rev(&mut d)?;
    let authority = ImageLabelAuthorityHeadV1::new(
        dec_thread(&mut d)?,
        d.u64()?,
        ImageLabelFrontier::from_raw(d.u64()?),
        ImageLabelFrontier::from_raw(d.u64()?),
    )
    .map_err(|_| CodecError::InvalidLength("acceptance authority"))?;
    let draft = dec_draft(&mut d)?;
    let draft_revision = dec_draft_rev(&mut d)?;
    let candidate = dec_recall_activation(&mut d)?;
    let root = dec_root_reference(&mut d)?;
    if d.u8()? != 1 {
        return Err(CodecError::InvalidLength(
            "acceptance materialization format",
        ));
    }
    let operation = DraftComposerMaterializationOperationIdV1::from_bytes(d.fixed16()?);
    let digest = d.fixed32()?;
    let pieces = d.u64()?;
    let utf8 = d.u64()?;
    let markers = d.u64()?;
    let mapping = DraftComposerMaterializationRecordV1::new(
        DraftComposerMaterializationKeyV1::new(root, DraftComposerFormatV1::ComposerV1),
        operation,
        dec_content_ref(&mut d)?,
    );
    if mapping.source_digest().as_bytes() != &digest
        || mapping.source_piece_count() != pieces
        || mapping.source_utf8_bytes() != utf8
        || mapping.source_marker_count() != markers
    {
        return Err(CodecError::InvalidLength("acceptance materialization"));
    }
    let gate_revision = dec_input_gate_rev(&mut d)?;
    let gate_state = dec_input_gate_state(&mut d)?;
    let successor = dec_draft(&mut d)?;
    let item = dec_item(&mut d)?;
    let assets = dec_opt(
        &mut d,
        "acceptance assets",
        dec_sealed_asset_reference_set_proof,
    )?;
    let disposal = DraftPieceOperationIdV1::from_bytes(d.fixed16()?);
    let timestamp = dec_timestamp(&mut d)?;
    d.finish()?;
    let a = FirstAcceptance::new(
        thread,
        revision,
        authority,
        draft,
        draft_revision,
        candidate,
        mapping,
        gate_revision,
        gate_state,
        successor,
        item,
        assets,
        disposal,
        timestamp,
    );
    if canonical_first_acceptance_bytes(&a) != bytes {
        return Err(CodecError::InvalidLength("acceptance canonical request"));
    }
    Ok(a)
}

fn enc_acceptance_result(e: &mut Encoder, result: AcceptanceResultV1) {
    match result {
        AcceptanceResultV1::Idle { turn, item } => {
            e.u8(0);
            enc_turn(e, turn);
            enc_item(e, item);
        }
        AcceptanceResultV1::Accepted {
            input,
            ordinal,
            generation,
        } => {
            e.u8(1);
            e.fixed16(input.as_bytes());
            e.u64(ordinal.get());
            e.u64(generation.get());
        }
    }
}

pub(crate) fn enc_acceptance_disposal(
    e: &mut Encoder,
    disposal: &DraftEditorCandidateSessionDisposeReceiptV1,
) -> Result<(), CodecError> {
    let acceptance = disposal
        .acceptance()
        .ok_or(CodecError::InvalidLength("acceptance purpose"))?;
    if !acceptance.is_exact(disposal) {
        return Err(CodecError::InvalidLength("acceptance disposal closure"));
    }
    let bytes = canonical_first_acceptance_bytes(&acceptance.request);
    e.bytes(&bytes);
    enc_acceptance_result(e, acceptance.result);
    e.fixed32(&acceptance_disposal_digest(disposal)?);
    Ok(())
}

pub(crate) fn dec_acceptance_disposal(
    d: &mut Decoder<'_>,
    disposal: DraftEditorCandidateSessionDisposeReceiptV1,
) -> Result<DraftEditorCandidateSessionDisposeReceiptV1, CodecError> {
    let request = dec_first_acceptance(d.bytes("acceptance request")?)?;
    let result = match d.u8()? {
        0 => AcceptanceResultV1::Idle {
            turn: dec_turn(d)?,
            item: dec_item(d)?,
        },
        1 => AcceptanceResultV1::Accepted {
            input: beryl_model::SyndicAcceptedInputId::from_bytes(d.fixed16()?),
            ordinal: AcceptedInputOrdinal::new(d.u64()?)
                .map_err(|_| CodecError::InvalidLength("acceptance ordinal"))?,
            generation: AcceptedRouteGeneration::new(d.u64()?)
                .map_err(|_| CodecError::InvalidLength("acceptance route"))?,
        },
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "acceptance result",
                tag,
            });
        }
    };
    let disposal = disposal.with_acceptance(AcceptanceDisposalV1 { request, result });
    let digest = d.fixed32()?;
    if !disposal.acceptance().unwrap().is_exact(&disposal)
        || acceptance_disposal_digest(&disposal)? != digest
    {
        return Err(CodecError::InvalidLength("acceptance disposal digest"));
    }
    Ok(disposal)
}

pub(crate) fn acceptance_disposal_digest(
    disposal: &DraftEditorCandidateSessionDisposeReceiptV1,
) -> Result<[u8; 32], CodecError> {
    let a = disposal
        .acceptance()
        .ok_or(CodecError::InvalidLength("acceptance purpose"))?;
    let mut key = Encoder::new();
    super::super::codec::enc_session_key(
        &mut key,
        DraftEditorCandidateSessionRecordKeyV1::disposal_receipt(
            a.request.draft_id(),
            a.request.candidate().session_id(),
            a.request.session_disposal_operation_id(),
        ),
    );
    let mut body = Encoder::new();
    body.u8(5);
    body.bytes(disposal.request_bytes());
    super::super::codec::enc_session_head(&mut body, disposal.before_head());
    super::super::codec::enc_session_head(&mut body, disposal.after_head());
    enc_history_frontier(&mut body, disposal.frontier());
    body.bytes(&canonical_first_acceptance_bytes(&a.request));
    enc_acceptance_result(&mut body, a.result);
    let mut hash = Sha256::new();
    for part in [
        b"syndic/composer-acceptance-disposal-receipt/v1".as_slice(),
        key.finish().as_slice(),
        body.finish().as_slice(),
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    Ok(hash.finalize().into())
}
