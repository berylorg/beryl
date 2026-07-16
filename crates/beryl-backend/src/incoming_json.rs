use std::io::{Cursor, Read};

use serde::de::DeserializeSeed;
use serde_json::Value;

mod discard;
mod seed;

use discard::{DiscardController, DiscardingReader};
use seed::JsonRpcValueSeed;

const REDACTED_INVALID_JSON: &str = "[redacted incoming JSON]";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DecodeStats {
    pub(crate) discarded_image_result_bytes: usize,
    pub(crate) maximum_buffered_input_bytes: usize,
}

pub(crate) struct DecodedValue {
    pub(crate) value: Value,
    pub(crate) stats: DecodeStats,
}

pub(crate) fn decode_reader<R>(
    reader: R,
    input_buffer_bytes: usize,
) -> Result<DecodedValue, serde_json::Error>
where
    R: Read,
{
    let controller = DiscardController::default();
    let reader = DiscardingReader::new(reader, input_buffer_bytes, controller.clone());
    let mut deserializer = serde_json::Deserializer::from_reader(reader);
    let value = JsonRpcValueSeed::new(controller.clone()).deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(DecodedValue {
        value,
        stats: controller.stats(),
    })
}

pub(crate) fn decode_value(input: &str) -> Result<Value, serde_json::Error> {
    decode_reader(Cursor::new(input.as_bytes()), 8 * 1024).map(|decoded| decoded.value)
}

pub(crate) fn redacted_invalid_json() -> String {
    REDACTED_INVALID_JSON.to_string()
}
