use crate::{codec::*, *};

pub fn accepted_input_codec_bytes(record: &AcceptedInputRecord) -> Vec<u8> {
    AcceptedInputsFamily::encode_value(record).unwrap()
}

pub fn decode_accepted_input_for_test(bytes: &[u8]) -> Option<AcceptedInputRecord> {
    AcceptedInputsFamily::decode_value(bytes).ok()
}

pub fn accepted_order_codec_bytes(record: &AcceptedOrderIndexRecord) -> Vec<u8> {
    AcceptedOrderFamily::encode_value(record).unwrap()
}

pub fn decode_accepted_order_for_test(bytes: &[u8]) -> Option<AcceptedOrderIndexRecord> {
    AcceptedOrderFamily::decode_value(bytes).ok()
}

pub fn canonical_item_codec_bytes(record: &CanonicalItemRecord) -> Vec<u8> {
    CanonicalItemsFamily::encode_value(record).unwrap()
}

pub fn decode_canonical_item_for_test(bytes: &[u8]) -> Option<CanonicalItemRecord> {
    CanonicalItemsFamily::decode_value(bytes).ok()
}
