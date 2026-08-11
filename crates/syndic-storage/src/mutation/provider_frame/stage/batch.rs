use crate::{
    advance_content_chain, ContentByteSpanRecord, ContentChunkRecord, ProviderItemBuildLifecycle,
    ProviderItemBuildRecord, ProviderNarrativeReference, ProviderNarrativeSpanRecord,
    ProviderStorageRecordError, SyndicRecordError, CONTENT_APPEND_MAX_CHUNKS,
};

use super::PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS;

/// Exact durable position of one stage batch during ambiguous-outcome reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderFrameStageBatchState {
    Expected,
    Next,
    Conflict,
}

/// One bounded atomic provider-frame staging contribution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderFrameStageBatch {
    expected: ProviderItemBuildRecord,
    next: ProviderItemBuildRecord,
    chunks: Vec<ContentChunkRecord>,
    byte_spans: Vec<ContentByteSpanRecord>,
    narrative_spans: Vec<ProviderNarrativeSpanRecord>,
}

impl ProviderFrameStageBatch {
    pub(crate) fn new(
        expected: ProviderItemBuildRecord,
        next: ProviderItemBuildRecord,
        chunks: Vec<ContentChunkRecord>,
        byte_spans: Vec<ContentByteSpanRecord>,
        narrative_spans: Vec<ProviderNarrativeSpanRecord>,
    ) -> Result<Self, ProviderFrameStageBatchError> {
        let batch = Self {
            expected,
            next,
            chunks,
            byte_spans,
            narrative_spans,
        };
        batch.validate()?;
        Ok(batch)
    }

    #[must_use]
    pub const fn expected_build(&self) -> &ProviderItemBuildRecord {
        &self.expected
    }

    #[must_use]
    pub const fn next_build(&self) -> &ProviderItemBuildRecord {
        &self.next
    }

    #[must_use]
    pub fn chunks(&self) -> &[ContentChunkRecord] {
        &self.chunks
    }

    #[must_use]
    pub fn byte_spans(&self) -> &[ContentByteSpanRecord] {
        &self.byte_spans
    }

    #[must_use]
    pub fn narrative_spans(&self) -> &[ProviderNarrativeSpanRecord] {
        &self.narrative_spans
    }

    /// Classifies an exact current build without inferring a retry or success.
    #[must_use]
    pub fn classify_current(
        &self,
        current: &ProviderItemBuildRecord,
    ) -> ProviderFrameStageBatchState {
        if current == &self.expected {
            ProviderFrameStageBatchState::Expected
        } else if current == &self.next {
            ProviderFrameStageBatchState::Next
        } else {
            ProviderFrameStageBatchState::Conflict
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ProviderFrameStageBatchError> {
        if self.chunks.is_empty() && self.narrative_spans.is_empty() {
            return Err(ProviderFrameStageBatchError::Empty);
        }
        if self.chunks.len() > CONTENT_APPEND_MAX_CHUNKS {
            return Err(ProviderFrameStageBatchError::TooManyChunks {
                maximum: CONTENT_APPEND_MAX_CHUNKS,
                actual: self.chunks.len(),
            });
        }
        if self.narrative_spans.len() > PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS {
            return Err(ProviderFrameStageBatchError::TooManyNarrativeSpans {
                maximum: PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS,
                actual: self.narrative_spans.len(),
            });
        }
        if self.chunks.len() != self.byte_spans.len() {
            return Err(ProviderFrameStageBatchError::ChunkSpanCountMismatch);
        }
        if self.expected.lifecycle() != ProviderItemBuildLifecycle::Staging {
            return Err(ProviderFrameStageBatchError::ExpectedBuildNotStaging);
        }

        let target = self.expected.target();
        let content_id = target.content().id();
        let mut chunk_count = self.expected.staged_chunk_count();
        let mut encoded_bytes = self.expected.staged_encoded_bytes();
        let mut chain = self.expected.staged_chain_digest();
        for (chunk, span) in self.chunks.iter().zip(&self.byte_spans) {
            let ordinal = chunk_count
                .checked_add(1)
                .ok_or(ProviderFrameStageBatchError::FrontierOverflow)?;
            if chunk.content_id() != content_id || chunk.ordinal().get() != ordinal {
                return Err(ProviderFrameStageBatchError::ChunkContinuity);
            }
            let expected_span = ContentByteSpanRecord::for_chunk(chunk, encoded_bytes)?;
            if span != &expected_span {
                return Err(ProviderFrameStageBatchError::ByteSpanContinuity);
            }
            chunk_count = ordinal;
            encoded_bytes = expected_span.end();
            chain = advance_content_chain(chain, chunk);
        }

        let narrative = advance_narrative_frontier(
            &self.expected,
            &self.narrative_spans,
            target.content().summary().encoded_bytes(),
        )?;

        let complete = chunk_count == target.content().summary().chunk_count()
            && encoded_bytes == target.content().summary().encoded_bytes()
            && narrative == target.narrative();
        let completion_terminal = self
            .expected
            .completion_check()
            .is_none_or(|check| check.state().is_terminal());
        let lifecycle = if complete && completion_terminal {
            ProviderItemBuildLifecycle::Sealed
        } else {
            ProviderItemBuildLifecycle::Staging
        };
        let expected_next =
            self.expected
                .advance(chunk_count, encoded_bytes, chain, narrative, lifecycle)?;
        if self.next != expected_next {
            return Err(ProviderFrameStageBatchError::NextBuildMismatch);
        }
        Ok(())
    }
}

fn advance_narrative_frontier(
    build: &ProviderItemBuildRecord,
    records: &[ProviderNarrativeSpanRecord],
    content_frontier: u64,
) -> Result<Option<ProviderNarrativeReference>, ProviderFrameStageBatchError> {
    let mut frontier = build.staged_narrative();
    for record in records {
        let previous = frontier.ok_or(ProviderFrameStageBatchError::UnexpectedNarrativeSpan)?;
        let target = build
            .target()
            .narrative()
            .ok_or(ProviderFrameStageBatchError::UnexpectedNarrativeSpan)?;
        if record.content_id() != target.content_id()
            || record.generation() != target.generation()
            || record.logical_start() != previous.logical_utf8_bytes()
            || record.frame_ordinal() != build.target().frame().ordinal()
            || record.frame_encoded_digest() != build.target().frame().encoded_digest()
            || record.source_end() > content_frontier
        {
            return Err(ProviderFrameStageBatchError::NarrativeSpanContinuity);
        }
        let expected = ProviderNarrativeSpanRecord::new(
            record.content_id(),
            record.generation(),
            record.logical_start(),
            record.logical_end(),
            record.frame_ordinal(),
            record.frame_encoded_digest(),
            record.source_start(),
            record.source_end(),
            record.source_digest(),
            previous.chain_digest(),
        )?;
        if &expected != record {
            return Err(ProviderFrameStageBatchError::NarrativeChainMismatch);
        }
        let span_count = previous
            .span_count()
            .checked_add(1)
            .ok_or(ProviderFrameStageBatchError::FrontierOverflow)?;
        frontier = Some(ProviderNarrativeReference::new(
            previous.content_id(),
            previous.generation(),
            span_count,
            record.logical_end(),
            record.resulting_chain_digest(),
        )?);
    }
    Ok(frontier)
}

/// Why a staged batch was not one exact bounded frontier advance.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderFrameStageBatchError {
    #[error("provider-frame stage batch carries no new record")]
    Empty,
    #[error("provider-frame stage batch has {actual} chunks; maximum is {maximum}")]
    TooManyChunks { maximum: usize, actual: usize },
    #[error("provider-frame stage batch has {actual} narrative spans; maximum is {maximum}")]
    TooManyNarrativeSpans { maximum: usize, actual: usize },
    #[error("provider-frame stage batch chunk and byte-span counts differ")]
    ChunkSpanCountMismatch,
    #[error("provider-frame stage batch expected build is not staging")]
    ExpectedBuildNotStaging,
    #[error("provider-frame stage batch chunk identity or ordinal is not contiguous")]
    ChunkContinuity,
    #[error("provider-frame stage batch byte-span frontier is not contiguous")]
    ByteSpanContinuity,
    #[error("provider-frame stage batch carries narrative for a nonnarrative build")]
    UnexpectedNarrativeSpan,
    #[error("provider-frame stage batch narrative-span identity or frontier is not contiguous")]
    NarrativeSpanContinuity,
    #[error("provider-frame stage batch narrative-span chain is not canonical")]
    NarrativeChainMismatch,
    #[error("provider-frame stage batch next build is not the exact derived frontier")]
    NextBuildMismatch,
    #[error("provider-frame stage batch frontier overflowed")]
    FrontierOverflow,
    #[error(transparent)]
    Record(#[from] SyndicRecordError),
    #[error(transparent)]
    StorageRecord(#[from] ProviderStorageRecordError),
}
