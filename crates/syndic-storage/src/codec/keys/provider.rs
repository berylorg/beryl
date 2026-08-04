use beryl_model::{ProviderObservationId, SyndicContentId};

use crate::ProviderNarrativeGeneration;

use super::{
    CodecError, Decoder, Encoder, ScanKey, dec_content, dec_provider_narrative_generation,
    enc_content, enc_provider_narrative_generation,
};

/// Exact sortable identity of one unpublished provider-observation chunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProviderObservationChunkKey {
    identity: ProviderObservationId,
    ordinal: u64,
}

impl ProviderObservationChunkKey {
    pub(crate) const ENCODED_BYTES: usize = 24;

    pub(crate) const fn new(identity: ProviderObservationId, ordinal: u64) -> Self {
        Self { identity, ordinal }
    }

    pub(crate) const fn identity(self) -> ProviderObservationId {
        self.identity
    }

    pub(crate) const fn ordinal(self) -> u64 {
        self.ordinal
    }

    pub(crate) fn encode(self) -> Vec<u8> {
        let mut encoder = Encoder::new();
        encoder.fixed16(self.identity.as_bytes());
        encoder.u64(self.ordinal);
        encoder.finish()
    }

    pub(crate) fn decode(encoded: &[u8]) -> Result<Self, CodecError> {
        if encoded.len() != Self::ENCODED_BYTES {
            return Err(CodecError::InvalidLength("provider-observation chunk key"));
        }
        let mut decoder = Decoder::new(encoded);
        let value = Self::new(
            ProviderObservationId::from_bytes(decoder.fixed16()?),
            decoder.u64()?,
        );
        decoder.finish()?;
        Ok(value)
    }
}

impl ScanKey for ProviderObservationChunkKey {
    fn first() -> Self {
        Self::new(ProviderObservationId::from_bytes([0; 16]), 0)
    }

    fn last() -> Self {
        Self::new(ProviderObservationId::from_bytes([u8::MAX; 16]), u64::MAX)
    }
}

/// Exact sortable identity of one selected provider narrative span.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProviderNarrativeSpanKey {
    content_id: SyndicContentId,
    generation: ProviderNarrativeGeneration,
    logical_start: u64,
}

impl ProviderNarrativeSpanKey {
    pub(crate) const ENCODED_BYTES: usize = 32;

    pub(crate) const fn new(
        content_id: SyndicContentId,
        generation: ProviderNarrativeGeneration,
        logical_start: u64,
    ) -> Self {
        Self {
            content_id,
            generation,
            logical_start,
        }
    }

    pub(crate) const fn content_id(self) -> SyndicContentId {
        self.content_id
    }
    pub(crate) const fn generation(self) -> ProviderNarrativeGeneration {
        self.generation
    }
    pub(crate) const fn logical_start(self) -> u64 {
        self.logical_start
    }

    pub(crate) fn first_for_generation(
        content_id: SyndicContentId,
        generation: ProviderNarrativeGeneration,
    ) -> Self {
        Self::new(content_id, generation, 0)
    }

    pub(crate) fn last_for_generation(
        content_id: SyndicContentId,
        generation: ProviderNarrativeGeneration,
    ) -> Self {
        Self::new(content_id, generation, u64::MAX)
    }

    pub(crate) fn encode(self) -> Vec<u8> {
        let mut encoder = Encoder::new();
        enc_content(&mut encoder, self.content_id);
        enc_provider_narrative_generation(&mut encoder, self.generation);
        encoder.u64(self.logical_start);
        let encoded = encoder.finish();
        debug_assert_eq!(encoded.len(), Self::ENCODED_BYTES);
        encoded
    }

    pub(crate) fn decode(encoded: &[u8]) -> Result<Self, CodecError> {
        if encoded.len() != Self::ENCODED_BYTES {
            return Err(CodecError::InvalidLength("provider narrative-span key"));
        }
        let mut decoder = Decoder::new(encoded);
        let key = Self::new(
            dec_content(&mut decoder)?,
            dec_provider_narrative_generation(&mut decoder)?,
            decoder.u64()?,
        );
        decoder.finish()?;
        Ok(key)
    }
}

impl ScanKey for ProviderNarrativeSpanKey {
    fn first() -> Self {
        Self::new(
            SyndicContentId::from_bytes([0; 16]),
            ProviderNarrativeGeneration::FIRST,
            0,
        )
    }

    fn last() -> Self {
        Self::new(
            SyndicContentId::from_bytes([u8::MAX; 16]),
            ProviderNarrativeGeneration::new(u64::MAX)
                .expect("maximum narrative generation is nonzero"),
            u64::MAX,
        )
    }
}
