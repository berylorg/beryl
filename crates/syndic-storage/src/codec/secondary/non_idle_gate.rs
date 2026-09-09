use super::*;

pub(crate) struct NonIdleGateSourcesFamily;
pub(crate) type NonIdleGateSourcesCodec = ExactCodec<NonIdleGateSourcesFamily>;

impl Family for NonIdleGateSourcesFamily {
    type Key = SyndicThreadId;
    type Value = NonIdleGateSourceRecord;
    const NAME: &'static str = "non-idle-gate-sources";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = 24;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        thread_key(bytes)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        enc_thread(&mut encoder, value.thread_id());
        enc_input_gate_rev(&mut encoder, value.gate_revision());
        Ok(encoder.finish())
    }

    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(bytes);
        let value = NonIdleGateSourceRecord::new(
            dec_thread(&mut decoder)?,
            dec_input_gate_rev(&mut decoder)?,
        );
        decoder.finish()?;
        Ok(value)
    }
}
