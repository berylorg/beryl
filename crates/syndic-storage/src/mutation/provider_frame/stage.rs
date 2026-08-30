mod batch;

use beryl_home_store::{CommandError, CommandOutcome, CommitReceipt, ReconciliationCustody};

use crate::{
    CONTENT_APPEND_MAX_CHUNKS, ContentByteSpanRecord, ContentChunkOrdinal, ContentChunkRecord,
    ProviderFrameEncodeError, ProviderFrameReferenceV1, ProviderFrameSinkV1,
    ProviderFrameTextSpanV1, ProviderItemBuildLifecycle, ProviderItemBuildRecord,
    ProviderItemValidationError, ProviderLogicalTextRoleV1, ProviderNarrativeReference,
    ProviderNarrativeSpanRecord, ProviderStorageRecordError, SyndicRecordError, SyndicValueError,
    advance_content_chain, encode_provider_item_frame_v1,
};

use super::PreparedProviderFrame;

pub use batch::{
    ProviderFrameStageBatch, ProviderFrameStageBatchError, ProviderFrameStageBatchState,
};

/// Maximum selected narrative-span records carried by one staging command.
///
/// This matches Syndic's 256-record bounded page policy. Together with at most 16 content chunks,
/// 16 byte spans, and one next-build record, it gives every command a fixed record-work ceiling.
pub const PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS: usize = 256;

/// Synchronous durability/reconciliation boundary invoked before encoding may continue.
pub trait ProviderFrameStageCallback {
    fn stage_batch(&mut self, batch: &ProviderFrameStageBatch) -> CommandOutcome;
}

impl<F> ProviderFrameStageCallback for F
where
    F: FnMut(&ProviderFrameStageBatch) -> CommandOutcome,
{
    fn stage_batch(&mut self, batch: &ProviderFrameStageBatch) -> CommandOutcome {
        self(batch)
    }
}

/// Exact result of one provider-frame staging traversal.
#[derive(Debug)]
pub enum ProviderFrameStageOutcome {
    /// The supplied build was already sealed, so no command was issued.
    Unchanged { value: ProviderItemBuildRecord },
    /// The offered batch definitely did not commit.
    NotCommitted { evidence: CommandError },
    /// The returned build is the exact durable successor of the last committed batch.
    Committed {
        value: ProviderItemBuildRecord,
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
    },
    /// The offered batch may have committed; no local successor is inferred.
    Indeterminate {
        failure: CommandError,
        reconciliation: ReconciliationCustody,
    },
}

/// Why the single staging encode could not reach the exact sealed target.
#[derive(Debug, thiserror::Error)]
pub enum ProviderFrameStageError {
    #[error("the current provider build belongs to another prepared frame")]
    BuildPlanMismatch,
    #[error("the resumed provider chunk frontier does not match deterministic encoding")]
    ResumeChunkFrontierMismatch,
    #[error("the resumed provider narrative frontier does not match deterministic encoding")]
    ResumeNarrativeFrontierMismatch,
    #[error("provider narrative kind emitted a non-narrative logical span")]
    NarrativeRoleMismatch,
    #[error("the durable provider-frame staging traversal did not equal the prepared target")]
    StagingTraversalMismatch,
    #[error("the durable provider-frame staging traversal ended before all target frontiers")]
    IncompleteStagingTraversal,
    #[error("provider-frame staging frontier overflowed")]
    FrontierOverflow,
    #[error(transparent)]
    Validation(#[from] ProviderItemValidationError),
    #[error(transparent)]
    Record(#[from] SyndicRecordError),
    #[error(transparent)]
    Value(#[from] SyndicValueError),
    #[error(transparent)]
    StorageRecord(#[from] ProviderStorageRecordError),
    #[error(transparent)]
    Batch(#[from] ProviderFrameStageBatchError),
    #[error("provider-frame staging reached a committed batch with a later failure")]
    CommittedLaterFailure {
        value: ProviderItemBuildRecord,
        receipt: CommitReceipt,
        later_failure: CommandError,
    },
    #[error("provider-frame staging batch definitely did not commit")]
    NotCommitted { evidence: CommandError },
    #[error("provider-frame staging batch has an indeterminate durable outcome")]
    Indeterminate {
        failure: CommandError,
        reconciliation: ReconciliationCustody,
    },
}

/// Performs the third overall encoding traversal and offers each bounded batch for durability.
///
/// Preparation already completed two constant-resident read-only traversals. `current` may be the
/// initial build or an exact partially staged build after restart. This durable traversal still runs
/// once for the whole frame and discards the already staged prefix; it is never rerun per batch. A
/// callback returns every exact command outcome. The staging result advances only after a
/// `Committed` callback outcome and forwards indeterminate custody without retrying.
pub fn stage_provider_frame<C: ProviderFrameStageCallback>(
    prepared: &PreparedProviderFrame,
    current: ProviderItemBuildRecord,
    callback: &mut C,
) -> Result<ProviderFrameStageOutcome, ProviderFrameStageError> {
    if !same_prepared_plan(prepared.initial_build(), &current) {
        return Err(ProviderFrameStageError::BuildPlanMismatch);
    }
    if current.lifecycle() == ProviderItemBuildLifecycle::Sealed {
        return Ok(ProviderFrameStageOutcome::Unchanged { value: current });
    }

    let prior_chunk_count = prepared.initial_build().staged_chunk_count();
    let prior_encoded_bytes = prepared.initial_build().staged_encoded_bytes();
    let prior_chain = prepared.initial_build().staged_chain_digest();
    let mut sink = StagingSink::new(
        prepared.initial_build().target().clone(),
        current,
        prior_chunk_count,
        prior_encoded_bytes,
        prior_chain,
        prepared.initial_build().staged_narrative(),
        callback,
    )?;
    let encoded =
        match encode_provider_item_frame_v1(prepared.frame(), prior_encoded_bytes, &mut sink) {
            Ok(reference) => reference,
            Err(ProviderFrameEncodeError::Validation(source)) => return Err(source.into()),
            Err(ProviderFrameEncodeError::Sink(source)) => return map_stage_error(source),
        };
    if &encoded != prepared.target().frame() {
        return Err(ProviderFrameStageError::StagingTraversalMismatch);
    }
    match sink.finish(&encoded) {
        Ok(outcome) => Ok(outcome),
        Err(error) => map_stage_error(error),
    }
}

fn map_stage_error(
    error: ProviderFrameStageError,
) -> Result<ProviderFrameStageOutcome, ProviderFrameStageError> {
    match error {
        ProviderFrameStageError::NotCommitted { evidence } => {
            Ok(ProviderFrameStageOutcome::NotCommitted { evidence })
        }
        ProviderFrameStageError::CommittedLaterFailure {
            value,
            receipt,
            later_failure,
        } => Ok(ProviderFrameStageOutcome::Committed {
            value,
            receipt,
            later_failure: Some(later_failure),
        }),
        ProviderFrameStageError::Indeterminate {
            failure,
            reconciliation,
        } => Ok(ProviderFrameStageOutcome::Indeterminate {
            failure,
            reconciliation,
        }),
        error => Err(error),
    }
}

fn same_prepared_plan(
    initial: &ProviderItemBuildRecord,
    current: &ProviderItemBuildRecord,
) -> bool {
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
    resume_chain: beryl_model::SyndicContentDigest,
    resume_narrative: Option<ProviderNarrativeReference>,
    seen_chunk_count: u64,
    seen_encoded_bytes: u64,
    seen_chain: beryl_model::SyndicContentDigest,
    seen_frame_span_count: u64,
    seen_frame_logical_utf8_bytes: u64,
    seen_completion_span: Option<ProviderFrameTextSpanV1>,
    narrative_logical_base: u64,
    seen_narrative: Option<ProviderNarrativeReference>,
    chunks: Vec<ContentChunkRecord>,
    byte_spans: Vec<ContentByteSpanRecord>,
    narrative_spans: Vec<ProviderNarrativeSpanRecord>,
    last_receipt: Option<CommitReceipt>,
    callback: &'a mut C,
}

impl<'a, C: ProviderFrameStageCallback> StagingSink<'a, C> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        target: crate::SealedProviderFrameReference,
        current: ProviderItemBuildRecord,
        prior_chunk_count: u64,
        prior_encoded_bytes: u64,
        prior_chain: beryl_model::SyndicContentDigest,
        narrative_seed: Option<ProviderNarrativeReference>,
        callback: &'a mut C,
    ) -> Result<Self, ProviderFrameStageError> {
        if current.staged_chunk_count() == prior_chunk_count
            && (current.staged_encoded_bytes() != prior_encoded_bytes
                || current.staged_chain_digest() != prior_chain)
        {
            return Err(ProviderFrameStageError::ResumeChunkFrontierMismatch);
        }
        if current.staged_narrative().is_some() != narrative_seed.is_some() {
            return Err(ProviderFrameStageError::ResumeNarrativeFrontierMismatch);
        }
        let narrative_logical_base = narrative_seed.map_or(0, |value| value.logical_utf8_bytes());
        Ok(Self {
            target,
            resume_chunk_count: current.staged_chunk_count(),
            resume_encoded_bytes: current.staged_encoded_bytes(),
            resume_chain: current.staged_chain_digest(),
            resume_narrative: current.staged_narrative(),
            current,
            seen_chunk_count: prior_chunk_count,
            seen_encoded_bytes: prior_encoded_bytes,
            seen_chain: prior_chain,
            seen_frame_span_count: 0,
            seen_frame_logical_utf8_bytes: 0,
            seen_completion_span: None,
            narrative_logical_base,
            seen_narrative: narrative_seed,
            chunks: Vec::with_capacity(CONTENT_APPEND_MAX_CHUNKS),
            byte_spans: Vec::with_capacity(CONTENT_APPEND_MAX_CHUNKS),
            narrative_spans: Vec::with_capacity(PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS),
            last_receipt: None,
            callback,
        })
    }

    fn finish(
        mut self,
        encoded: &ProviderFrameReferenceV1,
    ) -> Result<ProviderFrameStageOutcome, ProviderFrameStageError> {
        let summary = self.target.content().summary();
        if encoded != self.target.frame()
            || self.seen_chunk_count != summary.chunk_count()
            || self.seen_encoded_bytes != summary.encoded_bytes()
            || self.seen_chain != summary.digest()
            || self.seen_frame_span_count != encoded.text_span_count()
            || self.seen_frame_logical_utf8_bytes != encoded.logical_utf8_bytes()
            || self.seen_completion_span
                != self
                    .current
                    .completion_check()
                    .and_then(|check| check.source())
            || self.seen_narrative != self.target.narrative()
        {
            return Err(ProviderFrameStageError::IncompleteStagingTraversal);
        }
        self.verify_resumed_chunk_frontier()?;
        self.verify_resumed_narrative_frontier()?;
        self.flush(true)?;
        let expected_lifecycle = if self.current.completion_check().is_some() {
            ProviderItemBuildLifecycle::Staging
        } else {
            ProviderItemBuildLifecycle::Sealed
        };
        if self.current.lifecycle() != expected_lifecycle || self.current.target() != &self.target {
            return Err(ProviderFrameStageError::IncompleteStagingTraversal);
        }
        match self.last_receipt {
            Some(receipt) => Ok(ProviderFrameStageOutcome::Committed {
                value: self.current,
                receipt,
                later_failure: None,
            }),
            None => Ok(ProviderFrameStageOutcome::Unchanged {
                value: self.current,
            }),
        }
    }

    fn verify_resumed_chunk_frontier(&self) -> Result<(), ProviderFrameStageError> {
        if self.seen_chunk_count < self.resume_chunk_count
            || (self.seen_chunk_count == self.resume_chunk_count
                && (self.seen_encoded_bytes != self.resume_encoded_bytes
                    || self.seen_chain != self.resume_chain))
        {
            return Err(ProviderFrameStageError::ResumeChunkFrontierMismatch);
        }
        Ok(())
    }

    fn verify_resumed_narrative_frontier(&self) -> Result<(), ProviderFrameStageError> {
        match (self.seen_narrative, self.resume_narrative) {
            (None, None) => Ok(()),
            (Some(seen), Some(resume))
                if seen.span_count() > resume.span_count()
                    || (seen.span_count() == resume.span_count() && seen == resume) =>
            {
                Ok(())
            }
            _ => Err(ProviderFrameStageError::ResumeNarrativeFrontierMismatch),
        }
    }

    fn prospective_frontiers(
        &self,
    ) -> (
        u64,
        u64,
        beryl_model::SyndicContentDigest,
        Option<ProviderNarrativeReference>,
    ) {
        let (chunk_count, encoded_bytes, chain) = if self.chunks.is_empty() {
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
        (chunk_count, encoded_bytes, chain, narrative)
    }

    fn prospective_complete(&self) -> bool {
        let (chunks, bytes, _, narrative) = self.prospective_frontiers();
        let summary = self.target.content().summary();
        chunks == summary.chunk_count()
            && bytes == summary.encoded_bytes()
            && narrative == self.target.narrative()
    }

    fn flush(&mut self, seal: bool) -> Result<(), ProviderFrameStageError> {
        if self.chunks.is_empty() && self.narrative_spans.is_empty() {
            return if seal && !self.prospective_complete() {
                Err(ProviderFrameStageError::IncompleteStagingTraversal)
            } else {
                Ok(())
            };
        }
        let complete = self.prospective_complete();
        if complete && !seal {
            return Ok(());
        }
        if seal != complete {
            return Err(ProviderFrameStageError::IncompleteStagingTraversal);
        }
        let (chunks, bytes, chain, narrative) = self.prospective_frontiers();
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
                Err(ProviderFrameStageError::NotCommitted { evidence })
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization: _,
            } => {
                self.current = next;
                self.last_receipt = Some(receipt.clone());
                match later_failure {
                    Some(later_failure) => Err(ProviderFrameStageError::CommittedLaterFailure {
                        value: self.current.clone(),
                        receipt,
                        later_failure,
                    }),
                    None => Ok(()),
                }
            }
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => Err(ProviderFrameStageError::Indeterminate {
                failure,
                reconciliation,
            }),
        }
    }

    fn maybe_flush(&mut self) -> Result<(), ProviderFrameStageError> {
        if (self.chunks.len() >= CONTENT_APPEND_MAX_CHUNKS
            || self.narrative_spans.len() >= PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS)
            && !self.prospective_complete()
        {
            self.flush(false)?;
        }
        Ok(())
    }
}

impl<C: ProviderFrameStageCallback> ProviderFrameSinkV1 for StagingSink<'_, C> {
    type Error = ProviderFrameStageError;

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        let next_count = self
            .seen_chunk_count
            .checked_add(1)
            .ok_or(ProviderFrameStageError::FrontierOverflow)?;
        let ordinal = ContentChunkOrdinal::new(next_count)?;
        let chunk = ContentChunkRecord::new(self.target.content().id(), ordinal, bytes)?;
        let byte_span = ContentByteSpanRecord::for_chunk(&chunk, self.seen_encoded_bytes)?;
        self.seen_chunk_count = next_count;
        self.seen_encoded_bytes = byte_span.end();
        self.seen_chain = advance_content_chain(self.seen_chain, &chunk);

        if next_count <= self.resume_chunk_count {
            if self.seen_encoded_bytes > self.resume_encoded_bytes
                || (next_count == self.resume_chunk_count
                    && (self.seen_encoded_bytes != self.resume_encoded_bytes
                        || self.seen_chain != self.resume_chain))
            {
                return Err(ProviderFrameStageError::ResumeChunkFrontierMismatch);
            }
        } else {
            self.chunks.push(chunk);
            self.byte_spans.push(byte_span);
            self.maybe_flush()?;
        }
        Ok(())
    }

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        let next_count = self
            .seen_frame_span_count
            .checked_add(1)
            .ok_or(ProviderFrameStageError::FrontierOverflow)?;
        if span.frame_ordinal() != self.target.frame().ordinal()
            || span.logical_start() != self.seen_frame_logical_utf8_bytes
            || span.source_end() > self.target.content().summary().encoded_bytes()
        {
            return Err(ProviderFrameStageError::StagingTraversalMismatch);
        }
        self.seen_frame_span_count = next_count;
        self.seen_frame_logical_utf8_bytes = span.logical_end();

        if let Some(check) = self.current.completion_check() {
            if check.source() != Some(span) || self.seen_completion_span.replace(span).is_some() {
                return Err(ProviderFrameStageError::StagingTraversalMismatch);
            }
            return Ok(());
        }

        let Some(previous) = self.seen_narrative else {
            return Ok(());
        };
        if span.role() != ProviderLogicalTextRoleV1::Narrative {
            return Err(ProviderFrameStageError::NarrativeRoleMismatch);
        }
        let logical_start = self
            .narrative_logical_base
            .checked_add(span.logical_start())
            .ok_or(ProviderFrameStageError::FrontierOverflow)?;
        let logical_end = self
            .narrative_logical_base
            .checked_add(span.logical_end())
            .ok_or(ProviderFrameStageError::FrontierOverflow)?;
        if logical_start != previous.logical_utf8_bytes() {
            return Err(ProviderFrameStageError::StagingTraversalMismatch);
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
        let span_count = previous
            .span_count()
            .checked_add(1)
            .ok_or(ProviderFrameStageError::FrontierOverflow)?;
        let next = ProviderNarrativeReference::new(
            previous.content_id(),
            previous.generation(),
            span_count,
            logical_end,
            record.resulting_chain_digest(),
        )?;
        self.seen_narrative = Some(next);
        let resume = self
            .resume_narrative
            .ok_or(ProviderFrameStageError::ResumeNarrativeFrontierMismatch)?;
        if next.span_count() <= resume.span_count() {
            if next.logical_utf8_bytes() > resume.logical_utf8_bytes()
                || (next.span_count() == resume.span_count() && next != resume)
            {
                return Err(ProviderFrameStageError::ResumeNarrativeFrontierMismatch);
            }
        } else {
            self.narrative_spans.push(record);
            self.maybe_flush()?;
        }
        Ok(())
    }
}
