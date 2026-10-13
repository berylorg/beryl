use super::super::{
    codec::{dec_current_selector, dec_session_head, enc_current_selector, enc_session_head},
    *,
};
use super::cancellation_model::*;
use crate::codec::{CodecError, decode_draft_record, encode_draft_record, parts::*};
use crate::{CurrentTranscriptEntryProof, ReplacementEditIntent};
const DOMAIN: &[u8] = b"syndic/replacement-intent-cancellation/v1";
pub(crate) fn canonical_cancellation_request(r: DraftReplacementCancellationRequestV1) -> Vec<u8> {
    let mut e = Encoder::new();
    e.bytes(DOMAIN);
    enc_current_selector(&mut e, r.selector);
    e.fixed16(r.session.as_bytes());
    e.u64(r.generation);
    e.fixed16(r.operation.as_bytes());
    enc_turn(&mut e, r.intent.target_turn_id());
    enc_selected_path(&mut e, r.intent.selected_path());
    enc_transcript_generation(&mut e, r.intent.transcript_entry().generation());
    enc_transcript_pos(&mut e, r.intent.transcript_entry().position());
    enc_timestamp(&mut e, r.at);
    e.finish()
}
fn decode_request(bytes: &[u8]) -> Result<DraftReplacementCancellationRequestV1, CodecError> {
    let mut d = Decoder::new(bytes);
    if d.bytes("replacement cancellation domain")? != DOMAIN {
        return Err(CodecError::InvalidLength("replacement cancellation domain"));
    }
    let selector = dec_current_selector(&mut d)?;
    let session = DraftEditorCandidateSessionIdV1::from_bytes(d.fixed16()?);
    let generation = d.u64()?;
    let operation = DraftPieceOperationIdV1::from_bytes(d.fixed16()?);
    let turn = dec_turn(&mut d)?;
    let path = dec_selected_path(&mut d)?;
    let entry = CurrentTranscriptEntryProof::new(
        dec_transcript_generation(&mut d)?,
        dec_transcript_pos(&mut d)?,
    );
    let at = dec_timestamp(&mut d)?;
    d.finish()?;
    Ok(DraftReplacementCancellationRequestV1::new(
        selector,
        session,
        generation,
        operation,
        ReplacementEditIntent::new(turn, path, entry),
        at,
    ))
}
pub(crate) fn enc_cancellation_receipt(
    e: &mut Encoder,
    r: &DraftReplacementCancellationReceiptV1,
) -> Result<(), CodecError> {
    if !r.is_locally_exact() {
        return Err(CodecError::InvalidLength(
            "replacement cancellation closure",
        ));
    }
    e.bytes(&r.request_bytes);
    enc_current_selector(e, r.prior_selector);
    enc_current_selector(e, r.successor_selector);
    enc_session_head(e, &r.before);
    enc_session_head(e, &r.after);
    e.bytes(&encode_draft_record(&r.prior_draft)?);
    e.bytes(&encode_draft_record(&r.successor_draft)?);
    enc_history_frontier(e, &r.newest_history);
    enc_history_frontier(e, &r.mutable_history);
    Ok(())
}
pub(crate) fn dec_cancellation_receipt(
    d: &mut Decoder<'_>,
) -> Result<DraftReplacementCancellationReceiptV1, CodecError> {
    let request_bytes = d.bytes("replacement cancellation request")?.to_vec();
    let request = decode_request(&request_bytes)?;
    let receipt = DraftReplacementCancellationReceiptV1 {
        request,
        request_bytes,
        prior_selector: dec_current_selector(d)?,
        successor_selector: dec_current_selector(d)?,
        before: Box::new(dec_session_head(d)?),
        after: Box::new(dec_session_head(d)?),
        prior_draft: Box::new(decode_draft_record(d.bytes("prior cancellation draft")?)?),
        successor_draft: Box::new(decode_draft_record(
            d.bytes("successor cancellation draft")?,
        )?),
        newest_history: Box::new(dec_history_frontier(d)?),
        mutable_history: Box::new(dec_history_frontier(d)?),
    };
    if !receipt.is_locally_exact() {
        return Err(CodecError::InvalidLength(
            "replacement cancellation closure",
        ));
    }
    Ok(receipt)
}
