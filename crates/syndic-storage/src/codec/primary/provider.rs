use super::*;

pub(crate) struct ProviderObservationBuildsFamily;
pub(crate) type ProviderObservationBuildsCodec = ExactCodec<ProviderObservationBuildsFamily>;

impl Family for ProviderObservationBuildsFamily {
    type Key = ProviderObservationId;
    type Value = ProviderObservationBuildRecord;
    const NAME: &'static str = "provider-observation-builds";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        key16(
            encoded,
            "provider-observation build key",
            ProviderObservationId::from_bytes,
        )
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        enc_provider_observation_build_record(&mut encoder, value);
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(encoded);
        let value = dec_provider_observation_build_record(&mut decoder)?;
        decoder.finish()?;
        Ok(value)
    }
}

pub(crate) struct ProviderItemBuildsFamily;
pub(crate) type ProviderItemBuildsCodec = ExactCodec<ProviderItemBuildsFamily>;

impl ProviderItemBuildsFamily {
    pub(crate) fn validate_key_value(
        key: &SyndicItemId,
        value: &ProviderItemBuildRecord,
    ) -> Result<(), CodecError> {
        if *key != value.item_id() {
            return Err(invalid(
                "provider-item build key/value agreement",
                ProviderStorageRecordError::BuildKeyMismatch,
            ));
        }
        Ok(())
    }
}

impl Family for ProviderItemBuildsFamily {
    type Key = SyndicItemId;
    type Value = ProviderItemBuildRecord;
    const NAME: &'static str = "provider-item-builds";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.as_bytes().to_vec())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        key16(encoded, "provider-item build key", SyndicItemId::from_bytes)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        enc_provider_item_build_record(&mut encoder, value);
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(encoded);
        let value = dec_provider_item_build_record(&mut decoder)?;
        decoder.finish()?;
        Ok(value)
    }
}
