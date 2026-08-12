use beryl_model::{SyndicContentDigest, SyndicContentId};

use crate::provider_observation::compiler::ProviderObservationFramePreparationError;
use crate::{
    ContentChunkOrdinal, ContentChunkRecord, ContentSummary, ProviderFrameOrdinalV1,
    ProviderFrameSinkV1, ProviderFrameTextSpanV1, ProviderFrameTextSpanValidatorV1,
    ProviderLogicalTextRoleV1, ProviderNarrativeReference, ProviderNarrativeSpanRecord,
    advance_content_chain,
};

pub(in crate::provider_observation::compiler::stage) struct CountingSink {
    content_id: SyndicContentId,
    chunk_count: u64,
    encoded_bytes: u64,
    chain: SyndicContentDigest,
    pub(in crate::provider_observation::compiler::stage) spans: ProviderFrameTextSpanValidatorV1,
}

impl CountingSink {
    pub(in crate::provider_observation::compiler::stage) const fn new(
        content_id: SyndicContentId,
        chunk_count: u64,
        encoded_bytes: u64,
        chain: SyndicContentDigest,
        ordinal: ProviderFrameOrdinalV1,
    ) -> Self {
        Self {
            content_id,
            chunk_count,
            encoded_bytes,
            chain,
            spans: ProviderFrameTextSpanValidatorV1::new(ordinal),
        }
    }

    pub(in crate::provider_observation::compiler::stage) fn content_summary(
        &self,
    ) -> Result<ContentSummary, ProviderObservationFramePreparationError> {
        ContentSummary::new(
            self.chunk_count,
            0,
            self.encoded_bytes,
            0,
            0,
            0,
            crate::content::input_marker_digest(std::iter::empty()),
            None,
            self.chain,
        )
        .map_err(Into::into)
    }
}

impl ProviderFrameSinkV1 for CountingSink {
    type Error = ProviderObservationFramePreparationError;

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        let next = self
            .chunk_count
            .checked_add(1)
            .ok_or(ProviderObservationFramePreparationError::FrontierOverflow)?;
        let chunk =
            ContentChunkRecord::new(self.content_id, ContentChunkOrdinal::new(next)?, bytes)?;
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(
                u64::try_from(bytes.len())
                    .map_err(|_| ProviderObservationFramePreparationError::FrontierOverflow)?,
            )
            .ok_or(ProviderObservationFramePreparationError::FrontierOverflow)?;
        self.chain = advance_content_chain(self.chain, &chunk);
        self.chunk_count = next;
        Ok(())
    }

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        self.spans.observe(span)?;
        Ok(())
    }
}

pub(in crate::provider_observation::compiler::stage) struct NarrativeCountingSink {
    content_id: SyndicContentId,
    chunk_count: u64,
    encoded_bytes: u64,
    chain: SyndicContentDigest,
    pub(in crate::provider_observation::compiler::stage) spans: ProviderFrameTextSpanValidatorV1,
    frame_digest: [u8; 32],
    narrative_base: u64,
    pub(in crate::provider_observation::compiler::stage) narrative:
        Option<ProviderNarrativeReference>,
    append: bool,
    pub(in crate::provider_observation::compiler::stage) completion_span:
        Option<ProviderFrameTextSpanV1>,
}

impl NarrativeCountingSink {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::provider_observation::compiler::stage) const fn new(
        content_id: SyndicContentId,
        chunk_count: u64,
        encoded_bytes: u64,
        chain: SyndicContentDigest,
        ordinal: ProviderFrameOrdinalV1,
        frame_digest: [u8; 32],
        narrative: Option<ProviderNarrativeReference>,
        append: bool,
    ) -> Self {
        Self {
            content_id,
            chunk_count,
            encoded_bytes,
            chain,
            spans: ProviderFrameTextSpanValidatorV1::new(ordinal),
            frame_digest,
            narrative_base: match narrative {
                Some(value) => value.logical_utf8_bytes(),
                None => 0,
            },
            narrative,
            append,
            completion_span: None,
        }
    }

    pub(in crate::provider_observation::compiler::stage) fn agrees(
        &self,
        summary: ContentSummary,
    ) -> bool {
        self.chunk_count == summary.chunk_count()
            && self.encoded_bytes == summary.encoded_bytes()
            && self.chain == summary.digest()
    }
}

impl ProviderFrameSinkV1 for NarrativeCountingSink {
    type Error = ProviderObservationFramePreparationError;

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        let next = self
            .chunk_count
            .checked_add(1)
            .ok_or(ProviderObservationFramePreparationError::FrontierOverflow)?;
        let chunk =
            ContentChunkRecord::new(self.content_id, ContentChunkOrdinal::new(next)?, bytes)?;
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(
                u64::try_from(bytes.len())
                    .map_err(|_| ProviderObservationFramePreparationError::FrontierOverflow)?,
            )
            .ok_or(ProviderObservationFramePreparationError::FrontierOverflow)?;
        self.chain = advance_content_chain(self.chain, &chunk);
        self.chunk_count = next;
        Ok(())
    }

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        self.spans.observe(span)?;
        let Some(previous) = self.narrative else {
            return Ok(());
        };
        if span.role() != ProviderLogicalTextRoleV1::Narrative {
            return Err(ProviderObservationFramePreparationError::NarrativeRoleMismatch);
        }
        if !self.append {
            if self.completion_span.replace(span).is_some() {
                return Err(ProviderObservationFramePreparationError::NarrativeTraversalMismatch);
            }
            return Ok(());
        }
        let logical_start = self
            .narrative_base
            .checked_add(span.logical_start())
            .ok_or(ProviderObservationFramePreparationError::FrontierOverflow)?;
        let logical_end = self
            .narrative_base
            .checked_add(span.logical_end())
            .ok_or(ProviderObservationFramePreparationError::FrontierOverflow)?;
        if logical_start != previous.logical_utf8_bytes() {
            return Err(ProviderObservationFramePreparationError::NarrativeTraversalMismatch);
        }
        let record = ProviderNarrativeSpanRecord::new(
            self.content_id,
            previous.generation(),
            logical_start,
            logical_end,
            span.frame_ordinal(),
            self.frame_digest,
            span.source_start(),
            span.source_end(),
            span.source_digest(),
            previous.chain_digest(),
        )?;
        self.narrative = Some(ProviderNarrativeReference::new(
            self.content_id,
            previous.generation(),
            previous
                .span_count()
                .checked_add(1)
                .ok_or(ProviderObservationFramePreparationError::FrontierOverflow)?,
            logical_end,
            record.resulting_chain_digest(),
        )?);
        Ok(())
    }
}
