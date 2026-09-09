use beryl_home_store::{RecordCodec, RecordVersion};

use crate::{NonIdleGateSourceRecord, codec::*, domain::SyndicDomain};

pub fn non_idle_gate_source_codec_bytes(source: NonIdleGateSourceRecord) -> (Vec<u8>, Vec<u8>) {
    (
        NonIdleGateSourcesFamily::encode_key(&source.thread_id()).expect("fixed key"),
        NonIdleGateSourcesFamily::encode_value(&source).expect("fixed source"),
    )
}

pub fn decode_non_idle_gate_source_for_test(
    key: &[u8],
    payload: &[u8],
) -> Option<NonIdleGateSourceRecord> {
    let thread_id = NonIdleGateSourcesFamily::decode_key(key).ok()?;
    let source = NonIdleGateSourcesFamily::decode_value(payload).ok()?;
    (source.thread_id() == thread_id).then_some(source)
}

pub fn non_idle_gate_source_codec_limits() -> (RecordVersion, usize, usize) {
    (
        <NonIdleGateSourcesCodec as RecordCodec<SyndicDomain>>::VERSION,
        NonIdleGateSourcesFamily::MAX_KEY_BYTES,
        NonIdleGateSourcesFamily::MAX_VALUE_BYTES,
    )
}
