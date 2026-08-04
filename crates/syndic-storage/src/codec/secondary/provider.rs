use super::*;

pub(crate) struct ProviderObservationChunksFamily;
pub(crate) type ProviderObservationChunksCodec = ExactCodec<ProviderObservationChunksFamily>;

impl Family for ProviderObservationChunksFamily {
    type Key = ProviderObservationChunkKey;
    type Value = ProviderObservationChunkRecord;
    const NAME: &'static str = "provider-observation-chunks";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = ProviderObservationChunkKey::ENCODED_BYTES;
    const MAX_VALUE_BYTES: usize = LARGE_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok((*key).encode())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        ProviderObservationChunkKey::decode(encoded)
    }

    fn validate_stored_key(key: &Self::Key) -> Result<(), CodecError> {
        if key.ordinal() == 0 {
            return Err(invalid(
                "provider-observation chunk key",
                SyndicRecordError::InvalidProviderObservationFrontier,
            ));
        }
        Ok(())
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        enc_provider_observation_chunk_record(&mut encoder, value);
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(encoded);
        let value = dec_provider_observation_chunk_record(&mut decoder)?;
        decoder.finish()?;
        Ok(value)
    }
}

pub(crate) struct ProviderNarrativeSpansFamily;
pub(crate) type ProviderNarrativeSpansCodec = ExactCodec<ProviderNarrativeSpansFamily>;

impl ProviderNarrativeSpansFamily {
    pub(crate) fn validate_key_value(
        key: &ProviderNarrativeSpanKey,
        value: &ProviderNarrativeSpanRecord,
    ) -> Result<(), CodecError> {
        if key.content_id() != value.content_id()
            || key.generation() != value.generation()
            || key.logical_start() != value.logical_start()
        {
            return Err(invalid(
                "provider narrative-span key/value agreement",
                ProviderStorageRecordError::NarrativeSpanKeyMismatch,
            ));
        }
        Ok(())
    }
}

impl Family for ProviderNarrativeSpansFamily {
    type Key = ProviderNarrativeSpanKey;
    type Value = ProviderNarrativeSpanRecord;
    const NAME: &'static str = "provider-narrative-spans";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = ProviderNarrativeSpanKey::ENCODED_BYTES;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok((*key).encode())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        ProviderNarrativeSpanKey::decode(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        enc_provider_narrative_span_record(&mut encoder, value);
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(encoded);
        let value = dec_provider_narrative_span_record(&mut decoder)?;
        decoder.finish()?;
        Ok(value)
    }
}
