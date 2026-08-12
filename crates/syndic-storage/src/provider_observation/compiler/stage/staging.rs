use beryl_home_store::{CommandOutcome, HomeStore};
use beryl_model::SyndicContentDigest;

use crate::{
    CONTENT_APPEND_MAX_CHUNKS, ContentByteSpanRecord, ContentChunkOrdinal, ContentChunkRecord,
    ProviderFrameReferenceV1, ProviderFrameSinkV1, ProviderFrameStageBatch,
    ProviderFrameStageCallback, ProviderFrameTextSpanV1, ProviderItemBuildLifecycle,
    ProviderItemBuildRecord, ProviderLogicalTextRoleV1, ProviderNarrativeReference,
    ProviderNarrativeSpanRecord, SyndicPointReadLimit, SyndicStorage, advance_content_chain,
};

use super::super::{
    PreparedProviderObservationFrame, ProviderObservationFrameStageError,
    ProviderObservationFrameStageOutcome,
    encode::{ObservationEncodeError, encode_observation},
    replay::{ObservationReplayReader, ReplayError},
};

mod count;

pub(super) use count::{CountingSink, NarrativeCountingSink};

pub(super) fn stage<C: ProviderFrameStageCallback>(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedProviderObservationFrame,
    current: ProviderItemBuildRecord,
    limit: SyndicPointReadLimit,
    callback: &mut C,
) -> Result<ProviderObservationFrameStageOutcome, ProviderObservationFrameStageError> {
    if !same_plan(&prepared.initial_build, &current) {
        return Err(ProviderObservationFrameStageError::BuildPlanMismatch);
    }
    if current.lifecycle() == ProviderItemBuildLifecycle::Sealed {
        return Ok(ProviderObservationFrameStageOutcome::Unchanged { value: current });
    }
    let initial = &prepared.initial_build;
    let reader = ObservationReplayReader::new(storage, store, &prepared.replay, limit);
    let mut sink = StagingSink::new(
        initial.target().clone(),
        current,
        initial.staged_chunk_count(),
        initial.staged_encoded_bytes(),
        initial.staged_chain_digest(),
        initial.staged_narrative(),
        callback,
    )?;
    let encoded = match encode_observation(
        &reader,
        initial.source().item_id(),
        initial.target().frame().ordinal(),
        initial.target().frame().item_kind(),
        initial.target().frame().encoded_start(),
        &mut sink,
    ) {
        Ok(encoded) => encoded,
        Err(error) => return map_stage_error(map_stage_encode(error)),
    };
    if &encoded != initial.target().frame() {
        return Err(ProviderObservationFrameStageError::StagingTraversalMismatch);
    }
    match sink.finish(&encoded) {
        Ok(outcome) => Ok(outcome),
        Err(error) => map_stage_error(error),
    }
}

fn map_stage_error(
    error: ProviderObservationFrameStageError,
) -> Result<ProviderObservationFrameStageOutcome, ProviderObservationFrameStageError> {
    match error {
        ProviderObservationFrameStageError::NotCommitted { evidence } => {
            Ok(ProviderObservationFrameStageOutcome::NotCommitted { evidence })
        }
        ProviderObservationFrameStageError::CommittedLaterFailure {
            value,
            receipt,
            later_failure,
        } => Ok(ProviderObservationFrameStageOutcome::Committed {
            value,
            receipt,
            later_failure: Some(later_failure),
        }),
        ProviderObservationFrameStageError::Indeterminate {
            failure,
            reconciliation,
        } => Ok(ProviderObservationFrameStageOutcome::Indeterminate {
            failure,
            reconciliation,
        }),
        error => Err(error),
    }
}

fn same_plan(initial: &ProviderItemBuildRecord, current: &ProviderItemBuildRecord) -> bool {
    initial.item_id() == current.item_id()
        && initial.turn_id() == current.turn_id()
        && initial.source() == current.source()
        && initial.source_event() == current.source_event()
        && initial.prior() == current.prior()
        && initial.target() == current.target()
}

struct StagingSink<'a, C: ProviderFrameStageCallback> {
    target: crate::SealedProviderFrameReference,
    current: ProviderItemBuildRecord,
    resume_chunk_count: u64,
    resume_encoded_bytes: u64,
    resume_chain: SyndicContentDigest,
    resume_narrative: Option<ProviderNarrativeReference>,
    seen_chunk_count: u64,
    seen_encoded_bytes: u64,
    seen_chain: SyndicContentDigest,
    seen_span_count: u64,
    seen_logical_bytes: u64,
    seen_completion_span: Option<ProviderFrameTextSpanV1>,
    narrative_base: u64,
    seen_narrative: Option<ProviderNarrativeReference>,
    chunks: Vec<ContentChunkRecord>,
    byte_spans: Vec<ContentByteSpanRecord>,
    narrative_spans: Vec<ProviderNarrativeSpanRecord>,
    last_receipt: Option<beryl_home_store::CommitReceipt>,
    callback: &'a mut C,
}

impl<'a, C: ProviderFrameStageCallback> StagingSink<'a, C> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        target: crate::SealedProviderFrameReference,
        current: ProviderItemBuildRecord,
        prior_chunks: u64,
        prior_bytes: u64,
        prior_chain: SyndicContentDigest,
        narrative_seed: Option<ProviderNarrativeReference>,
        callback: &'a mut C,
    ) -> Result<Self, ProviderObservationFrameStageError> {
        if current.staged_chunk_count() == prior_chunks
            && (current.staged_encoded_bytes() != prior_bytes
                || current.staged_chain_digest() != prior_chain)
        {
            return Err(ProviderObservationFrameStageError::ResumeChunkFrontierMismatch);
        }
        if current.staged_narrative().is_some() != narrative_seed.is_some() {
            return Err(ProviderObservationFrameStageError::ResumeNarrativeFrontierMismatch);
        }
        Ok(Self {
            target,
            resume_chunk_count: current.staged_chunk_count(),
            resume_encoded_bytes: current.staged_encoded_bytes(),
            resume_chain: current.staged_chain_digest(),
            resume_narrative: current.staged_narrative(),
            current,
            seen_chunk_count: prior_chunks,
            seen_encoded_bytes: prior_bytes,
            seen_chain: prior_chain,
            seen_span_count: 0,
            seen_logical_bytes: 0,
            seen_completion_span: None,
            narrative_base: narrative_seed.map_or(0, |value| value.logical_utf8_bytes()),
            seen_narrative: narrative_seed,
            chunks: Vec::with_capacity(CONTENT_APPEND_MAX_CHUNKS),
            byte_spans: Vec::with_capacity(CONTENT_APPEND_MAX_CHUNKS),
            narrative_spans: Vec::with_capacity(crate::PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS),
            last_receipt: None,
            callback,
        })
    }

    fn finish(
        mut self,
        encoded: &ProviderFrameReferenceV1,
    ) -> Result<ProviderObservationFrameStageOutcome, ProviderObservationFrameStageError> {
        let summary = self.target.content().summary();
        if encoded != self.target.frame()
            || self.seen_chunk_count != summary.chunk_count()
            || self.seen_encoded_bytes != summary.encoded_bytes()
            || self.seen_chain != summary.digest()
            || self.seen_span_count != encoded.text_span_count()
            || self.seen_logical_bytes != encoded.logical_utf8_bytes()
            || self.seen_completion_span
                != self
                    .current
                    .completion_check()
                    .and_then(|check| check.source())
            || self.seen_narrative != self.target.narrative()
        {
            return Err(ProviderObservationFrameStageError::IncompleteStagingTraversal);
        }
        self.verify_resumed_frontiers()?;
        self.flush(true)?;
        let expected = if self.current.completion_check().is_some() {
            ProviderItemBuildLifecycle::Staging
        } else {
            ProviderItemBuildLifecycle::Sealed
        };
        if self.current.lifecycle() != expected || self.current.target() != &self.target {
            return Err(ProviderObservationFrameStageError::IncompleteStagingTraversal);
        }
        match self.last_receipt {
            Some(receipt) => Ok(ProviderObservationFrameStageOutcome::Committed {
                value: self.current,
                receipt,
                later_failure: None,
            }),
            None => Ok(ProviderObservationFrameStageOutcome::Unchanged {
                value: self.current,
            }),
        }
    }

    fn verify_resumed_frontiers(&self) -> Result<(), ProviderObservationFrameStageError> {
        if self.seen_chunk_count < self.resume_chunk_count
            || (self.seen_chunk_count == self.resume_chunk_count
                && (self.seen_encoded_bytes != self.resume_encoded_bytes
                    || self.seen_chain != self.resume_chain))
        {
            return Err(ProviderObservationFrameStageError::ResumeChunkFrontierMismatch);
        }
        match (self.seen_narrative, self.resume_narrative) {
            (None, None) => Ok(()),
            (Some(seen), Some(resume))
                if seen.span_count() > resume.span_count()
                    || (seen.span_count() == resume.span_count() && seen == resume) =>
            {
                Ok(())
            }
            _ => Err(ProviderObservationFrameStageError::ResumeNarrativeFrontierMismatch),
        }
    }

    fn prospective(
        &self,
    ) -> (
        u64,
        u64,
        SyndicContentDigest,
        Option<ProviderNarrativeReference>,
    ) {
        let (chunks, bytes, chain) = if self.chunks.is_empty() {
            (
                self.current.staged_chunk_count(),
                self.current.staged_encoded_bytes(),
                self.current.staged_chain_digest(),
            )
        } else {
            (
                self.seen_chunk_count,
                self.seen_encoded_bytes,
                self.seen_chain,
            )
        };
        let narrative = if self.narrative_spans.is_empty() {
            self.current.staged_narrative()
        } else {
            self.seen_narrative
        };
        (chunks, bytes, chain, narrative)
    }

    fn prospective_complete(&self) -> bool {
        let (chunks, bytes, _, narrative) = self.prospective();
        let summary = self.target.content().summary();
        chunks == summary.chunk_count()
            && bytes == summary.encoded_bytes()
            && narrative == self.target.narrative()
    }

    fn maybe_flush(&mut self) -> Result<(), ProviderObservationFrameStageError> {
        if (self.chunks.len() >= CONTENT_APPEND_MAX_CHUNKS
            || self.narrative_spans.len() >= crate::PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS)
            && !self.prospective_complete()
        {
            self.flush(false)?;
        }
        Ok(())
    }

    fn flush(&mut self, seal: bool) -> Result<(), ProviderObservationFrameStageError> {
        if self.chunks.is_empty() && self.narrative_spans.is_empty() {
            return if seal && !self.prospective_complete() {
                Err(ProviderObservationFrameStageError::IncompleteStagingTraversal)
            } else {
                Ok(())
            };
        }
        let complete = self.prospective_complete();
        if complete && !seal {
            return Ok(());
        }
        if complete != seal {
            return Err(ProviderObservationFrameStageError::IncompleteStagingTraversal);
        }
        let (chunks, bytes, chain, narrative) = self.prospective();
        let completion_terminal = self
            .current
            .completion_check()
            .is_none_or(|check| check.state().is_terminal());
        let lifecycle = if seal && completion_terminal {
            ProviderItemBuildLifecycle::Sealed
        } else {
            ProviderItemBuildLifecycle::Staging
        };
        let next = self
            .current
            .advance(chunks, bytes, chain, narrative, lifecycle)?;
        let batch = ProviderFrameStageBatch::new(
            self.current.clone(),
            next.clone(),
            std::mem::take(&mut self.chunks),
            std::mem::take(&mut self.byte_spans),
            std::mem::take(&mut self.narrative_spans),
        )?;
        match self.callback.stage_batch(&batch) {
            CommandOutcome::NotCommitted { evidence } => {
                Err(ProviderObservationFrameStageError::NotCommitted { evidence })
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
            } => {
                self.current = next;
                self.last_receipt = Some(receipt.clone());
                match later_failure {
                    Some(later_failure) => {
                        Err(ProviderObservationFrameStageError::CommittedLaterFailure {
                            value: self.current.clone(),
                            receipt,
                            later_failure,
                        })
                    }
                    None => Ok(()),
                }
            }
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => Err(ProviderObservationFrameStageError::Indeterminate {
                failure,
                reconciliation,
            }),
        }
    }
}

impl<C: ProviderFrameStageCallback> ProviderFrameSinkV1 for StagingSink<'_, C> {
    type Error = ProviderObservationFrameStageError;

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        let next = self
            .seen_span_count
            .checked_add(1)
            .ok_or(ProviderObservationFrameStageError::FrontierOverflow)?;
        if span.frame_ordinal() != self.target.frame().ordinal()
            || span.logical_start() != self.seen_logical_bytes
            || span.source_end() > self.target.content().summary().encoded_bytes()
        {
            return Err(ProviderObservationFrameStageError::StagingTraversalMismatch);
        }
        self.seen_span_count = next;
        self.seen_logical_bytes = span.logical_end();

        if let Some(check) = self.current.completion_check() {
            if check.source() != Some(span) || self.seen_completion_span.replace(span).is_some() {
                return Err(ProviderObservationFrameStageError::StagingTraversalMismatch);
            }
            return Ok(());
        }
        let Some(previous) = self.seen_narrative else {
            return Ok(());
        };
        if span.role() != ProviderLogicalTextRoleV1::Narrative {
            return Err(ProviderObservationFrameStageError::NarrativeRoleMismatch);
        }
        let logical_start = self
            .narrative_base
            .checked_add(span.logical_start())
            .ok_or(ProviderObservationFrameStageError::FrontierOverflow)?;
        let logical_end = self
            .narrative_base
            .checked_add(span.logical_end())
            .ok_or(ProviderObservationFrameStageError::FrontierOverflow)?;
        if logical_start != previous.logical_utf8_bytes() {
            return Err(ProviderObservationFrameStageError::StagingTraversalMismatch);
        }
        let record = ProviderNarrativeSpanRecord::new(
            self.target.content().id(),
            previous.generation(),
            logical_start,
            logical_end,
            span.frame_ordinal(),
            self.target.frame().encoded_digest(),
            span.source_start(),
            span.source_end(),
            span.source_digest(),
            previous.chain_digest(),
        )?;
        let next_narrative = ProviderNarrativeReference::new(
            previous.content_id(),
            previous.generation(),
            previous
                .span_count()
                .checked_add(1)
                .ok_or(ProviderObservationFrameStageError::FrontierOverflow)?,
            logical_end,
            record.resulting_chain_digest(),
        )?;
        self.seen_narrative = Some(next_narrative);
        let resume = self
            .resume_narrative
            .ok_or(ProviderObservationFrameStageError::ResumeNarrativeFrontierMismatch)?;
        if next_narrative.span_count() <= resume.span_count() {
            if next_narrative.logical_utf8_bytes() > resume.logical_utf8_bytes()
                || (next_narrative.span_count() == resume.span_count() && next_narrative != resume)
            {
                return Err(ProviderObservationFrameStageError::ResumeNarrativeFrontierMismatch);
            }
        } else {
            self.narrative_spans.push(record);
            self.maybe_flush()?;
        }
        Ok(())
    }

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        let next = self
            .seen_chunk_count
            .checked_add(1)
            .ok_or(ProviderObservationFrameStageError::FrontierOverflow)?;
        let chunk = ContentChunkRecord::new(
            self.target.content().id(),
            ContentChunkOrdinal::new(next)?,
            bytes,
        )?;
        let span = ContentByteSpanRecord::for_chunk(&chunk, self.seen_encoded_bytes)?;
        self.seen_chunk_count = next;
        self.seen_encoded_bytes = span.end();
        self.seen_chain = advance_content_chain(self.seen_chain, &chunk);
        if next <= self.resume_chunk_count {
            if self.seen_encoded_bytes > self.resume_encoded_bytes
                || (next == self.resume_chunk_count
                    && (self.seen_encoded_bytes != self.resume_encoded_bytes
                        || self.seen_chain != self.resume_chain))
            {
                return Err(ProviderObservationFrameStageError::ResumeChunkFrontierMismatch);
            }
        } else {
            self.chunks.push(chunk);
            self.byte_spans.push(span);
            self.maybe_flush()?;
        }
        Ok(())
    }
}

fn map_stage_encode(
    error: ObservationEncodeError<ProviderObservationFrameStageError>,
) -> ProviderObservationFrameStageError {
    match error {
        ObservationEncodeError::Replay(error) => match error {
            ReplayError::Cursor(error) => error.into(),
            ReplayError::Validation(error) => error.into(),
            ReplayError::Semantic(error) => error.into(),
            ReplayError::FrontierOverflow => ProviderObservationFrameStageError::FrontierOverflow,
        },
        ObservationEncodeError::Validation(error) => error.into(),
        ObservationEncodeError::Sink(error) => error,
    }
}
