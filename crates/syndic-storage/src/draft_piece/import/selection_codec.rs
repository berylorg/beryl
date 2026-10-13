use super::super::{
    codec::{dec_current_selector, dec_session_head, enc_current_selector, enc_session_head},
    *,
};
use crate::codec::{
    CodecError, Family, HistorySummariesFamily, decode_draft_record, encode_draft_record,
    parts::{Decoder, Encoder},
};

pub(crate) fn enc_import_selection(
    e: &mut Encoder,
    c: &DraftSealedImportSelectionV1,
) -> Result<(), CodecError> {
    enc_current_selector(e, c.prior_selector);
    enc_current_selector(e, c.successor_selector);
    enc_session_head(e, &c.before_session);
    enc_session_head(e, &c.after_session);
    enc_root_reference(e, c.imported_root);
    enc_history_frontier(e, &c.prior_history);
    enc_history_frontier(e, &c.baseline);
    enc_history_frontier(e, &c.before_mutable_history);
    enc_history_frontier(e, &c.after_mutable_history);
    enc_import_basis(e, c.basis);
    e.bytes(&encode_draft_record(&c.prior_draft)?);
    e.bytes(&encode_draft_record(&c.successor_draft)?);
    e.bytes(&HistorySummariesFamily::encode_value(&c.prior_summary)?);
    e.bytes(&HistorySummariesFamily::encode_value(&c.successor_summary)?);
    Ok(())
}
pub(crate) fn dec_import_selection(
    d: &mut Decoder<'_>,
) -> Result<DraftSealedImportSelectionV1, CodecError> {
    Ok(DraftSealedImportSelectionV1 {
        prior_selector: dec_current_selector(d)?,
        successor_selector: dec_current_selector(d)?,
        before_session: Box::new(dec_session_head(d)?),
        after_session: Box::new(dec_session_head(d)?),
        imported_root: dec_root_reference(d)?,
        prior_history: Box::new(dec_history_frontier(d)?),
        baseline: Box::new(dec_history_frontier(d)?),
        before_mutable_history: Box::new(dec_history_frontier(d)?),
        after_mutable_history: Box::new(dec_history_frontier(d)?),
        basis: dec_import_basis(d)?,
        prior_draft: Box::new(decode_draft_record(d.bytes("prior import draft")?)?),
        successor_draft: Box::new(decode_draft_record(d.bytes("successor import draft")?)?),
        prior_summary: Box::new(HistorySummariesFamily::decode_value(
            d.bytes("prior import summary")?,
        )?),
        successor_summary: Box::new(HistorySummariesFamily::decode_value(
            d.bytes("successor import summary")?,
        )?),
    })
}
