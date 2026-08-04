use beryl_model::{SyndicContentDigest, SyndicItemId, SyndicTurnId};

use crate::{
    CasItemSource, ProviderFrameObservationSummaryV1, ProviderItemBuildRevision,
    ProviderLogicalTextRoleV1, ProviderNarrativeGeneration,
};

use super::{
    ProviderNarrativeCompletionCheck, ProviderNarrativeCompletionState, ProviderNarrativeReference,
    ProviderStorageRecordError, SealedProviderFrameReference,
    narrative::validate_narrative_frame_frontier,
};

mod lifecycle;
mod validation;

pub use lifecycle::ProviderItemBuildLifecycle;
use validation::{reject_regression, validate_frontier};

/// Resumable bounded staging state for one immutable target provider frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderItemBuildRecord {
    item_id: SyndicItemId,
    turn_id: SyndicTurnId,
    source: CasItemSource,
    source_event: crate::SourceEventSequence,
    revision: ProviderItemBuildRevision,
    prior: Option<SealedProviderFrameReference>,
    target: SealedProviderFrameReference,
    staged_chunk_count: u64,
    staged_encoded_bytes: u64,
    staged_chain_digest: SyndicContentDigest,
    staged_narrative: Option<ProviderNarrativeReference>,
    completion_check: Option<ProviderNarrativeCompletionCheck>,
    lifecycle: ProviderItemBuildLifecycle,
}

impl ProviderItemBuildRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        item_id: SyndicItemId,
        turn_id: SyndicTurnId,
        source: CasItemSource,
        source_event: crate::SourceEventSequence,
        revision: ProviderItemBuildRevision,
        prior: Option<SealedProviderFrameReference>,
        target: SealedProviderFrameReference,
        staged_chunk_count: u64,
        staged_encoded_bytes: u64,
        staged_chain_digest: SyndicContentDigest,
        staged_narrative: Option<ProviderNarrativeReference>,
        completion_check: Option<ProviderNarrativeCompletionCheck>,
        lifecycle: ProviderItemBuildLifecycle,
    ) -> Result<Self, ProviderStorageRecordError> {
        let value = Self {
            item_id,
            turn_id,
            source,
            source_event,
            revision,
            prior,
            target,
            staged_chunk_count,
            staged_encoded_bytes,
            staged_chain_digest,
            staged_narrative,
            completion_check,
            lifecycle,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ProviderStorageRecordError> {
        if self.target.frame().item_id() != self.source.item_id() {
            return Err(ProviderStorageRecordError::TargetCasItemMismatch);
        }

        let (minimum_chunks, minimum_encoded) = match &self.prior {
            Some(prior) => {
                self.validate_prior(prior)?;
                let summary = prior.content().summary();
                (summary.chunk_count(), summary.encoded_bytes())
            }
            None => {
                if self.target.frame().ordinal() != crate::ProviderFrameOrdinalV1::FIRST {
                    return Err(ProviderStorageRecordError::InitialFrameOrdinalMismatch);
                }
                if self.target.frame().encoded_start() != 0 {
                    return Err(ProviderStorageRecordError::InitialFrameStartMismatch);
                }
                if self.target.content().revision().get() != 1 {
                    return Err(ProviderStorageRecordError::InitialContentRevisionMismatch);
                }
                let valid_initial_observation = match self.target.observation() {
                    ProviderFrameObservationSummaryV1::Started(_) => true,
                    ProviderFrameObservationSummaryV1::Completed(_) => {
                        self.target.frame().item_kind().permits_completion_only()
                    }
                    ProviderFrameObservationSummaryV1::Delta => false,
                };
                if !valid_initial_observation {
                    return Err(ProviderStorageRecordError::InitialStreamStateMismatch);
                }
                (0, 0)
            }
        };

        let target_summary = self.target.content().summary();
        validate_frontier(
            "chunk-count",
            minimum_chunks,
            target_summary.chunk_count(),
            self.staged_chunk_count,
        )?;
        validate_frontier(
            "encoded-byte",
            minimum_encoded,
            target_summary.encoded_bytes(),
            self.staged_encoded_bytes,
        )?;
        let content_target = self.staged_chunk_count == target_summary.chunk_count()
            && self.staged_encoded_bytes == target_summary.encoded_bytes();
        if content_target && self.staged_chain_digest != target_summary.digest() {
            return Err(ProviderStorageRecordError::StagedChainDigestMismatch);
        }
        let narrative_target = self.validate_narrative()?;
        let completion_target = self.validate_completion_check()?;
        if (self.lifecycle == ProviderItemBuildLifecycle::Sealed)
            != (content_target && narrative_target && completion_target)
        {
            return Err(ProviderStorageRecordError::BuildLifecycleMismatch);
        }
        Ok(())
    }

    fn validate_prior(
        &self,
        prior: &SealedProviderFrameReference,
    ) -> Result<(), ProviderStorageRecordError> {
        if prior.content().id() != self.target.content().id() {
            return Err(ProviderStorageRecordError::PriorContentMismatch);
        }
        if prior.frame().item_id() != self.target.frame().item_id() {
            return Err(ProviderStorageRecordError::PriorCasItemMismatch);
        }
        if prior.frame().item_kind() != self.target.frame().item_kind() {
            return Err(ProviderStorageRecordError::PriorItemKindMismatch);
        }
        if prior.frame().ordinal().get().checked_add(1) != Some(self.target.frame().ordinal().get())
        {
            return Err(ProviderStorageRecordError::PriorFrameOrdinalMismatch);
        }
        if prior.content().summary().encoded_bytes() != self.target.frame().encoded_start() {
            return Err(ProviderStorageRecordError::PriorContentFrontierMismatch);
        }
        if prior.content().revision().checked_next().ok() != Some(self.target.content().revision())
        {
            return Err(ProviderStorageRecordError::PriorContentRevisionMismatch);
        }
        if prior.stream_state().is_complete()
            || prior.stream_state().started_at() != self.target.stream_state().started_at()
            || matches!(
                self.target.observation(),
                ProviderFrameObservationSummaryV1::Started(_)
            )
        {
            return Err(ProviderStorageRecordError::PriorStreamStateMismatch);
        }
        if prior.history_support().merge(self.target.history_support())
            != self.target.history_support()
        {
            return Err(ProviderStorageRecordError::HistorySupportRegression);
        }
        Ok(())
    }

    fn validate_narrative(&self) -> Result<bool, ProviderStorageRecordError> {
        let Some(target) = self.target.narrative() else {
            if self.staged_narrative.is_some() {
                return Err(ProviderStorageRecordError::UnexpectedStagedNarrative);
            }
            return Ok(true);
        };

        let seed = match self.target.observation() {
            ProviderFrameObservationSummaryV1::Started(_) => {
                if target.generation() != ProviderNarrativeGeneration::FIRST {
                    return Err(ProviderStorageRecordError::InitialNarrativeGenerationMismatch);
                }
                validate_narrative_frame_frontier(target, self.target.frame(), 0, 0)?;
                ProviderNarrativeReference::empty(target.content_id(), target.generation())
            }
            ProviderFrameObservationSummaryV1::Delta => {
                let prior = self
                    .prior
                    .as_ref()
                    .and_then(SealedProviderFrameReference::narrative)
                    .ok_or(ProviderStorageRecordError::MissingPriorNarrative)?;
                if prior.generation() != target.generation() {
                    return Err(ProviderStorageRecordError::AppendNarrativeGenerationMismatch);
                }
                validate_narrative_frame_frontier(
                    target,
                    self.target.frame(),
                    prior.span_count(),
                    prior.logical_utf8_bytes(),
                )?;
                prior
            }
            ProviderFrameObservationSummaryV1::Completed(_) => {
                let prior = self
                    .prior
                    .as_ref()
                    .and_then(SealedProviderFrameReference::narrative)
                    .ok_or(ProviderStorageRecordError::MissingPriorNarrative)?;
                if target != prior {
                    return Err(ProviderStorageRecordError::AppendNarrativeGenerationMismatch);
                }
                prior
            }
        };

        let staged = self
            .staged_narrative
            .ok_or(ProviderStorageRecordError::MissingStagedNarrative)?;
        if staged.content_id() != target.content_id() || staged.generation() != target.generation()
        {
            return Err(ProviderStorageRecordError::StagedNarrativeIdentityMismatch);
        }
        validate_frontier(
            "narrative-span-count",
            seed.span_count(),
            target.span_count(),
            staged.span_count(),
        )?;
        validate_frontier(
            "narrative-logical-UTF-8-byte",
            seed.logical_utf8_bytes(),
            target.logical_utf8_bytes(),
            staged.logical_utf8_bytes(),
        )?;
        let staged_span_delta = staged.span_count() - seed.span_count();
        let staged_byte_delta = staged.logical_utf8_bytes() - seed.logical_utf8_bytes();
        if (staged_span_delta == 0) != (staged_byte_delta == 0) {
            return Err(ProviderStorageRecordError::InvalidStagedNarrativeFrontier);
        }
        if staged.span_count() == seed.span_count() && staged != seed {
            return Err(ProviderStorageRecordError::StagedNarrativeSeedMismatch);
        }
        let at_target = staged.span_count() == target.span_count()
            && staged.logical_utf8_bytes() == target.logical_utf8_bytes();
        if at_target && staged != target {
            return Err(ProviderStorageRecordError::StagedNarrativeChainDigestMismatch);
        }
        Ok(at_target)
    }

    fn validate_completion_check(&self) -> Result<bool, ProviderStorageRecordError> {
        let is_narrative_completion = matches!(
            self.target.observation(),
            ProviderFrameObservationSummaryV1::Completed(_)
        ) && self.target.frame().item_kind().requires_narrative();
        if !is_narrative_completion {
            if self.completion_check.is_some() {
                return Err(ProviderStorageRecordError::UnexpectedNarrativeCompletionCheck);
            }
            return Ok(true);
        }

        let check = self
            .completion_check
            .ok_or(ProviderStorageRecordError::MissingNarrativeCompletionCheck)?;
        let narrative = self
            .target
            .narrative()
            .ok_or(ProviderStorageRecordError::MissingPriorNarrative)?;
        let frame = self.target.frame();
        match (
            frame.logical_utf8_bytes(),
            frame.text_span_count(),
            check.source(),
        ) {
            (0, 0, None) => {}
            (logical_bytes, 1, Some(source))
                if source.frame_ordinal() == frame.ordinal()
                    && source.role() == ProviderLogicalTextRoleV1::Narrative
                    && source.logical_start() == 0
                    && source.logical_end() == logical_bytes
                    && source.source_end() <= self.target.content().summary().encoded_bytes() => {}
            _ => return Err(ProviderStorageRecordError::InvalidNarrativeCompletionSource),
        }

        let live_bytes = narrative.logical_utf8_bytes();
        let completion_bytes = frame.logical_utf8_bytes();
        let common_bytes = live_bytes.min(completion_bytes);
        match check.state() {
            ProviderNarrativeCompletionState::Pending(frontier) => {
                if frontier.compared_utf8_bytes() > common_bytes
                    || frontier.verified_span_count() > narrative.span_count()
                    || (frontier.compared_utf8_bytes() == 0
                        && (frontier.verified_span_count() != 0
                            || frontier.verified_chain_digest()
                                != crate::provider_narrative_chain_seed(
                                    narrative.content_id(),
                                    narrative.generation(),
                                )))
                    || (frontier.compared_utf8_bytes() == live_bytes
                        && (frontier.verified_span_count() != narrative.span_count()
                            || frontier.verified_chain_digest() != narrative.chain_digest()))
                {
                    return Err(ProviderStorageRecordError::InvalidNarrativeComparisonFrontier);
                }
                Ok(false)
            }
            ProviderNarrativeCompletionState::Equal => {
                if live_bytes != completion_bytes {
                    return Err(ProviderStorageRecordError::InvalidNarrativeCompletionDisposition);
                }
                Ok(true)
            }
            ProviderNarrativeCompletionState::Mismatch { utf8_byte_offset } => {
                if utf8_byte_offset > common_bytes
                    || (live_bytes == completion_bytes && utf8_byte_offset >= common_bytes)
                {
                    return Err(ProviderStorageRecordError::InvalidNarrativeCompletionDisposition);
                }
                Ok(true)
            }
        }
    }

    /// Returns a new revision after monotonically advancing the staged frontiers.
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &self,
        staged_chunk_count: u64,
        staged_encoded_bytes: u64,
        staged_chain_digest: SyndicContentDigest,
        staged_narrative: Option<ProviderNarrativeReference>,
        lifecycle: ProviderItemBuildLifecycle,
    ) -> Result<Self, ProviderStorageRecordError> {
        if self.lifecycle == ProviderItemBuildLifecycle::Sealed {
            return Err(ProviderStorageRecordError::BuildAlreadySealed);
        }
        reject_regression("chunk-count", self.staged_chunk_count, staged_chunk_count)?;
        reject_regression(
            "encoded-byte",
            self.staged_encoded_bytes,
            staged_encoded_bytes,
        )?;
        match (self.staged_narrative, staged_narrative) {
            (Some(previous), Some(next)) => {
                if previous.content_id() != next.content_id()
                    || previous.generation() != next.generation()
                {
                    return Err(ProviderStorageRecordError::StagedNarrativeIdentityMismatch);
                }
                reject_regression(
                    "narrative-span-count",
                    previous.span_count(),
                    next.span_count(),
                )?;
                reject_regression(
                    "narrative-logical-UTF-8-byte",
                    previous.logical_utf8_bytes(),
                    next.logical_utf8_bytes(),
                )?;
            }
            (None, None) => {}
            _ => return Err(ProviderStorageRecordError::StagedNarrativePresenceChanged),
        }
        let revision = self
            .revision
            .checked_next()
            .map_err(|_| ProviderStorageRecordError::BuildRevisionExhausted)?;
        Self::new(
            self.item_id,
            self.turn_id,
            self.source.clone(),
            self.source_event,
            revision,
            self.prior.clone(),
            self.target.clone(),
            staged_chunk_count,
            staged_encoded_bytes,
            staged_chain_digest,
            staged_narrative,
            self.completion_check,
            lifecycle,
        )
    }

    /// Advances only the bounded completion-equality frontier after frame bytes are staged.
    pub fn advance_completion(
        &self,
        state: ProviderNarrativeCompletionState,
    ) -> Result<Self, ProviderStorageRecordError> {
        if self.lifecycle == ProviderItemBuildLifecycle::Sealed {
            return Err(ProviderStorageRecordError::BuildAlreadySealed);
        }
        let current = self
            .completion_check
            .ok_or(ProviderStorageRecordError::MissingNarrativeCompletionCheck)?;
        if current.state().is_terminal() {
            return Err(ProviderStorageRecordError::NarrativeCompletionAlreadyTerminal);
        }
        if !self.frame_staged() {
            return Err(ProviderStorageRecordError::BuildLifecycleMismatch);
        }
        let lifecycle = if state.is_terminal() {
            ProviderItemBuildLifecycle::Sealed
        } else {
            ProviderItemBuildLifecycle::Staging
        };
        let revision = self
            .revision
            .checked_next()
            .map_err(|_| ProviderStorageRecordError::BuildRevisionExhausted)?;
        Self::new(
            self.item_id,
            self.turn_id,
            self.source.clone(),
            self.source_event,
            revision,
            self.prior.clone(),
            self.target.clone(),
            self.staged_chunk_count,
            self.staged_encoded_bytes,
            self.staged_chain_digest,
            self.staged_narrative,
            Some(current.with_state(state)),
            lifecycle,
        )
    }

    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn source(&self) -> &CasItemSource {
        &self.source
    }
    #[must_use]
    pub const fn source_event(&self) -> crate::SourceEventSequence {
        self.source_event
    }
    #[must_use]
    pub const fn revision(&self) -> ProviderItemBuildRevision {
        self.revision
    }
    #[must_use]
    pub const fn prior(&self) -> Option<&SealedProviderFrameReference> {
        self.prior.as_ref()
    }
    #[must_use]
    pub const fn target(&self) -> &SealedProviderFrameReference {
        &self.target
    }
    #[must_use]
    pub const fn staged_chunk_count(&self) -> u64 {
        self.staged_chunk_count
    }
    #[must_use]
    pub const fn staged_encoded_bytes(&self) -> u64 {
        self.staged_encoded_bytes
    }
    #[must_use]
    pub const fn staged_chain_digest(&self) -> SyndicContentDigest {
        self.staged_chain_digest
    }
    #[must_use]
    pub const fn staged_narrative(&self) -> Option<ProviderNarrativeReference> {
        self.staged_narrative
    }
    #[must_use]
    pub const fn completion_check(&self) -> Option<ProviderNarrativeCompletionCheck> {
        self.completion_check
    }
    #[must_use]
    pub fn frame_staged(&self) -> bool {
        let summary = self.target.content().summary();
        self.staged_chunk_count == summary.chunk_count()
            && self.staged_encoded_bytes == summary.encoded_bytes()
            && self.staged_chain_digest == summary.digest()
            && self.staged_narrative == self.target.narrative()
    }
    #[must_use]
    pub const fn lifecycle(&self) -> ProviderItemBuildLifecycle {
        self.lifecycle
    }
}
