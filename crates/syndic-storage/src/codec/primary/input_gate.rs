use super::*;

pub(crate) struct InputGatesFamily;
pub(crate) type InputGatesCodec = ExactCodec<InputGatesFamily>;

impl Family for InputGatesFamily {
    type Key = SyndicThreadId;
    type Value = InputGateRecord;
    const NAME: &'static str = "input-gates";
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        key16(encoded, "input-gate key", SyndicThreadId::from_bytes)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_thread(&mut e, value.thread_id());
        enc_input_gate_rev(&mut e, value.revision());
        enc_input_gate_state(&mut e, value.state());
        e.u64(value.accepted_high_water());
        e.u32(value.live_steering_count());
        e.u32(value.live_next_turn_count());
        e.u64(value.live_logical_utf8_bytes());
        Ok(e.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(encoded);
        let value = InputGateRecord::new(
            dec_thread(&mut d)?,
            dec_input_gate_rev(&mut d)?,
            dec_input_gate_state(&mut d)?,
            d.u64()?,
            d.u32()?,
            d.u32()?,
            d.u64()?,
        )
        .map_err(|source| invalid("input gate", source))?;
        d.finish()?;
        Ok(value)
    }
}
