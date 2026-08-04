use super::*;

pub(crate) struct CasItemIndexFamily;
pub(crate) type CasItemIndexCodec = ExactCodec<CasItemIndexFamily>;
impl Family for CasItemIndexFamily {
    type Key = CasItemKey;
    type Value = CasItemIndexRecord;
    const NAME: &'static str = "cas-item-index";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 782;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        CasItemKey::decode(bytes)
    }
    fn validate_stored_key(key: &Self::Key) -> Result<(), CodecError> {
        key.stored()
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_cas_item_index(value)
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        decode_cas_item_index(bytes)
    }
}

pub(crate) struct CasThreadIndexFamily;
pub(crate) type CasThreadIndexCodec = ExactCodec<CasThreadIndexFamily>;
impl Family for CasThreadIndexFamily {
    type Key = CasThreadKey;
    type Value = CasThreadIndexRecord;
    const NAME: &'static str = "cas-thread-index";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 261;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        CasThreadKey::decode(bytes)
    }
    fn validate_stored_key(key: &Self::Key) -> Result<(), CodecError> {
        key.stored()
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_cas_thread_index(value)
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        decode_cas_thread_index(bytes)
    }
}

pub(crate) struct CasThreadBindingIndexFamily;
pub(crate) type CasThreadBindingIndexCodec = ExactCodec<CasThreadBindingIndexFamily>;
impl Family for CasThreadBindingIndexFamily {
    type Key = CasThreadBindingKey;
    type Value = CasThreadBindingIndexRecord;
    const NAME: &'static str = "cas-thread-bindings";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 269;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        CasThreadBindingKey::decode(bytes)
    }
    fn validate_stored_key(key: &Self::Key) -> Result<(), CodecError> {
        key.stored()
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_cas_thread_binding_index(value)
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        decode_cas_thread_binding_index(bytes)
    }
}

pub(crate) struct CasTurnIndexFamily;
pub(crate) type CasTurnIndexCodec = ExactCodec<CasTurnIndexFamily>;
impl Family for CasTurnIndexFamily {
    type Key = CasTurnKey;
    type Value = CasTurnIndexRecord;
    const NAME: &'static str = "cas-turn-index";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 521;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        CasTurnKey::decode(bytes)
    }
    fn validate_stored_key(key: &Self::Key) -> Result<(), CodecError> {
        key.stored()
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_cas_turn_index(value)
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        decode_cas_turn_index(bytes)
    }
}
