use super::*;

pub(crate) struct InputGatesFamily;
pub(crate) type InputGatesCodec = ExactCodec<InputGatesFamily>;

impl Family for InputGatesFamily {
    type Key = SyndicThreadId;
    type Value = InputGateRecord;
    const NAME: &'static str = "input-gates";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(4);
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
        enc_input_gate_record(&mut e, value);
        Ok(e.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(encoded);
        let value = dec_input_gate_record(&mut d)?;
        d.finish()?;
        Ok(value)
    }
}

pub(super) fn enc_input_gate_record(e: &mut Encoder, value: &InputGateRecord) {
    enc_thread(e, value.thread_id());
    enc_input_gate_rev(e, value.revision());
    enc_input_gate_state(e, value.state());
    e.u64(value.accepted_high_water());
    enc_opt(e, value.route_generation_high_water(), enc_route_generation);
    enc_opt(e, value.selected_route(), |e, proof| {
        enc_route_generation(e, proof.generation());
        enc_route_revision(e, proof.revision());
    });
    e.u64(value.live_steering_count());
    e.u64(value.live_next_turn_count());
    e.u64(value.live_logical_utf8_bytes());
}

pub(super) fn dec_input_gate_record(d: &mut Decoder<'_>) -> Result<InputGateRecord, CodecError> {
    InputGateRecord::new(
        dec_thread(d)?,
        dec_input_gate_rev(d)?,
        dec_input_gate_state(d)?,
        d.u64()?,
        dec_opt(
            d,
            "input-gate route-generation high-water",
            dec_route_generation,
        )?,
        dec_opt(d, "input-gate selected route", |d| {
            Ok(AcceptedRouteHeadProof::new(
                dec_route_generation(d)?,
                dec_route_revision(d)?,
            ))
        })?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
    )
    .map_err(|source| invalid("input gate", source))
}
