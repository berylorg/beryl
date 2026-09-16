use crate::{
    RepairRequiredTarget,
    codec::parts::{Decoder, Encoder, repair::*},
};

pub fn encode_repair_target_for_test(target: &RepairRequiredTarget) -> Vec<u8> {
    let mut encoder = Encoder::new();
    enc_repair_target(&mut encoder, target);
    encoder.finish()
}

pub fn decode_repair_target_for_test(bytes: &[u8]) -> Result<RepairRequiredTarget, String> {
    let mut decoder = Decoder::new(bytes);
    let target = dec_repair_target(&mut decoder).map_err(|error| error.to_string())?;
    decoder.finish().map_err(|error| error.to_string())?;
    Ok(target)
}
