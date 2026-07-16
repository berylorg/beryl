use crate::{
    CasItemIndexRecord, CasThreadBindingIndexRecord, CasThreadIndexRecord, CasTurnIndexRecord,
};

use super::super::{CodecError, parts::*};

pub(super) fn encode_cas_item_index(v: &CasItemIndexRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_external(&mut e, v.cas_thread_id.as_str());
    enc_external(&mut e, v.cas_turn_id.as_str());
    enc_external(&mut e, v.cas_item_id.as_str());
    enc_item(&mut e, v.item_id);
    enc_projection_rev(&mut e, v.item_revision);
    Ok(e.finish())
}

pub(super) fn decode_cas_item_index(b: &[u8]) -> Result<CasItemIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = CasItemIndexRecord::new(
        dec_cas_thread(&mut d)?,
        dec_cas_turn(&mut d)?,
        dec_cas_item(&mut d)?,
        dec_item(&mut d)?,
        dec_projection_rev(&mut d)?,
    );
    d.finish()?;
    Ok(v)
}

pub(super) fn encode_cas_thread_index(v: &CasThreadIndexRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_external(&mut e, v.cas_thread_id.as_str());
    enc_thread(&mut e, v.thread_id);
    enc_binding_rev(&mut e, v.first_binding_revision);
    enc_binding_rev(&mut e, v.latest_binding_revision);
    enc_opt(&mut e, v.retired_binding_revision, enc_binding_rev);
    Ok(e.finish())
}

pub(super) fn decode_cas_thread_index(b: &[u8]) -> Result<CasThreadIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let cas_thread = dec_cas_thread(&mut d)?;
    let thread = dec_thread(&mut d)?;
    let first = dec_binding_rev(&mut d)?;
    let latest = dec_binding_rev(&mut d)?;
    let retired = dec_opt(&mut d, "CAS thread retirement revision", dec_binding_rev)?;
    let v = match retired {
        Some(retired) => {
            CasThreadIndexRecord::retired_with_latest(cas_thread, thread, first, latest, retired)
        }
        None => CasThreadIndexRecord::with_latest(cas_thread, thread, first, latest),
    };
    d.finish()?;
    Ok(v)
}

pub(super) fn encode_cas_thread_binding_index(
    v: &CasThreadBindingIndexRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_external(&mut e, v.cas_thread_id.as_str());
    enc_thread(&mut e, v.thread_id);
    enc_binding_rev(&mut e, v.binding_revision);
    Ok(e.finish())
}

pub(super) fn decode_cas_thread_binding_index(
    b: &[u8],
) -> Result<CasThreadBindingIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let value = CasThreadBindingIndexRecord::new(
        dec_cas_thread(&mut d)?,
        dec_thread(&mut d)?,
        dec_binding_rev(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

pub(super) fn encode_cas_turn_index(v: &CasTurnIndexRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_external(&mut e, v.cas_thread_id.as_str());
    enc_external(&mut e, v.cas_turn_id.as_str());
    enc_thread(&mut e, v.thread_id);
    enc_turn(&mut e, v.turn_id);
    enc_binding_rev(&mut e, v.binding_revision);
    enc_snapshot(&mut e, v.snapshot_id);
    enc_native_turn_count(&mut e, v.post_turn_native_count);
    Ok(e.finish())
}

pub(super) fn decode_cas_turn_index(b: &[u8]) -> Result<CasTurnIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = CasTurnIndexRecord::new(
        dec_cas_thread(&mut d)?,
        dec_cas_turn(&mut d)?,
        dec_thread(&mut d)?,
        dec_turn(&mut d)?,
        dec_binding_rev(&mut d)?,
        dec_snapshot(&mut d)?,
        dec_native_turn_count(&mut d)?,
    );
    d.finish()?;
    Ok(v)
}
