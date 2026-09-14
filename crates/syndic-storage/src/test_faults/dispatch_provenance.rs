use crate::{
    TurnStateRecord,
    codec::{Family, TurnStatesFamily},
};

pub fn turn_state_codec_bytes(state: &TurnStateRecord) -> Vec<u8> {
    TurnStatesFamily::encode_value(state).expect("bounded turn-state fixture")
}

pub fn decode_turn_state_for_test(bytes: &[u8]) -> Option<TurnStateRecord> {
    TurnStatesFamily::decode_value(bytes).ok()
}
