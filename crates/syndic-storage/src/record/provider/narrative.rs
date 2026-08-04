use beryl_model::SyndicContentId;
use sha2::{Digest, Sha256};

use crate::{ProviderFrameOrdinalV1, ProviderFrameReferenceV1, ProviderNarrativeGeneration};

use super::ProviderStorageRecordError;

const PROVIDER_NARRATIVE_SEED_V1: &[u8] = b"beryl.syndic.provider-narrative-chain.seed.v1";
const PROVIDER_NARRATIVE_SPAN_V1: &[u8] = b"beryl.syndic.provider-narrative-chain.span.v1";

/// Exact selected narrative frontier over one provider content stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderNarrativeReference {
    content_id: SyndicContentId,
    generation: ProviderNarrativeGeneration,
    span_count: u64,
    logical_utf8_bytes: u64,
    chain_digest: [u8; 32],
}

impl ProviderNarrativeReference {
    pub fn new(
        content_id: SyndicContentId,
        generation: ProviderNarrativeGeneration,
        span_count: u64,
        logical_utf8_bytes: u64,
        chain_digest: [u8; 32],
    ) -> Result<Self, ProviderStorageRecordError> {
        if (span_count == 0) != (logical_utf8_bytes == 0) {
            return Err(ProviderStorageRecordError::InvalidNarrativeSummary);
        }
        Ok(Self {
            content_id,
            generation,
            span_count,
            logical_utf8_bytes,
            chain_digest,
        })
    }

    /// Constructs the canonical empty frontier for one exact selected generation.
    #[must_use]
    pub fn empty(content_id: SyndicContentId, generation: ProviderNarrativeGeneration) -> Self {
        Self {
            content_id,
            generation,
            span_count: 0,
            logical_utf8_bytes: 0,
            chain_digest: provider_narrative_chain_seed(content_id, generation),
        }
    }

    #[must_use]
    pub const fn content_id(self) -> SyndicContentId {
        self.content_id
    }
    #[must_use]
    pub const fn generation(self) -> ProviderNarrativeGeneration {
        self.generation
    }
    #[must_use]
    pub const fn span_count(self) -> u64 {
        self.span_count
    }
    #[must_use]
    pub const fn logical_utf8_bytes(self) -> u64 {
        self.logical_utf8_bytes
    }
    #[must_use]
    pub const fn chain_digest(self) -> [u8; 32] {
        self.chain_digest
    }
}

/// One selected narrative span pointing into immutable ProviderItemV1 frame bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderNarrativeSpanRecord {
    content_id: SyndicContentId,
    generation: ProviderNarrativeGeneration,
    logical_start: u64,
    logical_end: u64,
    frame_ordinal: ProviderFrameOrdinalV1,
    frame_encoded_digest: [u8; 32],
    source_start: u64,
    source_end: u64,
    source_digest: [u8; 32],
    resulting_chain_digest: [u8; 32],
}

impl ProviderNarrativeSpanRecord {
    /// Constructs one span and computes its canonical resulting chain digest.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        content_id: SyndicContentId,
        generation: ProviderNarrativeGeneration,
        logical_start: u64,
        logical_end: u64,
        frame_ordinal: ProviderFrameOrdinalV1,
        frame_encoded_digest: [u8; 32],
        source_start: u64,
        source_end: u64,
        source_digest: [u8; 32],
        previous_chain_digest: [u8; 32],
    ) -> Result<Self, ProviderStorageRecordError> {
        validate_narrative_span(logical_start, logical_end, source_start, source_end)?;
        let resulting_chain_digest = advance_provider_narrative_chain(
            previous_chain_digest,
            content_id,
            generation,
            logical_start,
            logical_end,
            frame_ordinal,
            frame_encoded_digest,
            source_start,
            source_end,
            source_digest,
        );
        Ok(Self {
            content_id,
            generation,
            logical_start,
            logical_end,
            frame_ordinal,
            frame_encoded_digest,
            source_start,
            source_end,
            source_digest,
            resulting_chain_digest,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_stored_parts(
        content_id: SyndicContentId,
        generation: ProviderNarrativeGeneration,
        logical_start: u64,
        logical_end: u64,
        frame_ordinal: ProviderFrameOrdinalV1,
        frame_encoded_digest: [u8; 32],
        source_start: u64,
        source_end: u64,
        source_digest: [u8; 32],
        resulting_chain_digest: [u8; 32],
    ) -> Result<Self, ProviderStorageRecordError> {
        validate_narrative_span(logical_start, logical_end, source_start, source_end)?;
        Ok(Self {
            content_id,
            generation,
            logical_start,
            logical_end,
            frame_ordinal,
            frame_encoded_digest,
            source_start,
            source_end,
            source_digest,
            resulting_chain_digest,
        })
    }

    #[must_use]
    pub const fn content_id(self) -> SyndicContentId {
        self.content_id
    }
    #[must_use]
    pub const fn generation(self) -> ProviderNarrativeGeneration {
        self.generation
    }
    #[must_use]
    pub const fn logical_start(self) -> u64 {
        self.logical_start
    }
    #[must_use]
    pub const fn logical_end(self) -> u64 {
        self.logical_end
    }
    #[must_use]
    pub const fn frame_ordinal(self) -> ProviderFrameOrdinalV1 {
        self.frame_ordinal
    }
    #[must_use]
    pub const fn frame_encoded_digest(self) -> [u8; 32] {
        self.frame_encoded_digest
    }
    #[must_use]
    pub const fn source_start(self) -> u64 {
        self.source_start
    }
    #[must_use]
    pub const fn source_end(self) -> u64 {
        self.source_end
    }
    #[must_use]
    pub const fn source_digest(self) -> [u8; 32] {
        self.source_digest
    }
    #[must_use]
    pub const fn resulting_chain_digest(self) -> [u8; 32] {
        self.resulting_chain_digest
    }
}

/// Canonical empty-chain digest for one exact provider narrative generation.
#[must_use]
pub fn provider_narrative_chain_seed(
    content_id: SyndicContentId,
    generation: ProviderNarrativeGeneration,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(PROVIDER_NARRATIVE_SEED_V1);
    hash.update(content_id.as_bytes());
    hash.update(generation.get().to_be_bytes());
    hash.finalize().into()
}

/// Folds one exact span provenance record into a provider narrative chain.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn advance_provider_narrative_chain(
    previous_chain_digest: [u8; 32],
    content_id: SyndicContentId,
    generation: ProviderNarrativeGeneration,
    logical_start: u64,
    logical_end: u64,
    frame_ordinal: ProviderFrameOrdinalV1,
    frame_encoded_digest: [u8; 32],
    source_start: u64,
    source_end: u64,
    source_digest: [u8; 32],
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(PROVIDER_NARRATIVE_SPAN_V1);
    hash.update(previous_chain_digest);
    hash.update(content_id.as_bytes());
    hash.update(generation.get().to_be_bytes());
    hash.update(logical_start.to_be_bytes());
    hash.update(logical_end.to_be_bytes());
    hash.update(frame_ordinal.get().to_be_bytes());
    hash.update(frame_encoded_digest);
    hash.update(source_start.to_be_bytes());
    hash.update(source_end.to_be_bytes());
    hash.update(source_digest);
    hash.finalize().into()
}

pub(super) fn validate_sealed_narrative(
    content_id: SyndicContentId,
    frame: &ProviderFrameReferenceV1,
    stream_is_complete: bool,
    narrative: Option<ProviderNarrativeReference>,
) -> Result<(), ProviderStorageRecordError> {
    if frame.item_kind().requires_narrative() != narrative.is_some() {
        return Err(ProviderStorageRecordError::NarrativePresenceMismatch);
    }
    let Some(narrative) = narrative else {
        return Ok(());
    };
    if narrative.content_id() != content_id {
        return Err(ProviderStorageRecordError::NarrativeContentMismatch);
    }
    if narrative.span_count() == 0
        && narrative.chain_digest()
            != provider_narrative_chain_seed(content_id, narrative.generation())
    {
        return Err(ProviderStorageRecordError::EmptyNarrativeChainDigestMismatch);
    }
    if !stream_is_complete
        && (narrative.span_count() < frame.text_span_count()
            || narrative.logical_utf8_bytes() < frame.logical_utf8_bytes())
    {
        return Err(ProviderStorageRecordError::NarrativeFrameFrontierMismatch);
    }
    Ok(())
}

pub(super) fn validate_narrative_frame_frontier(
    narrative: ProviderNarrativeReference,
    frame: &ProviderFrameReferenceV1,
    prior_span_count: u64,
    prior_logical_utf8_bytes: u64,
) -> Result<(), ProviderStorageRecordError> {
    let expected_span_count = prior_span_count
        .checked_add(frame.text_span_count())
        .ok_or(ProviderStorageRecordError::NarrativeFrontierOverflow)?;
    let expected_logical_utf8_bytes = prior_logical_utf8_bytes
        .checked_add(frame.logical_utf8_bytes())
        .ok_or(ProviderStorageRecordError::NarrativeFrontierOverflow)?;
    if narrative.span_count() != expected_span_count
        || narrative.logical_utf8_bytes() != expected_logical_utf8_bytes
    {
        return Err(ProviderStorageRecordError::NarrativeFrameFrontierMismatch);
    }
    Ok(())
}

fn validate_narrative_span(
    logical_start: u64,
    logical_end: u64,
    source_start: u64,
    source_end: u64,
) -> Result<(), ProviderStorageRecordError> {
    let logical_length = logical_end.checked_sub(logical_start);
    let source_length = source_end.checked_sub(source_start);
    if logical_length.is_none() || logical_length == Some(0) || logical_length != source_length {
        return Err(ProviderStorageRecordError::InvalidNarrativeSpan);
    }
    Ok(())
}
