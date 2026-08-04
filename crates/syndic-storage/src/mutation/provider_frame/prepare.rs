use beryl_model::{ContentRevision, RevisionError, SyndicContentId, SyndicItemId, SyndicTurnId};

use crate::{
    CasItemSource, ContentChunkOrdinal, ContentChunkRecord, ContentEncoding, ContentReference,
    ContentSummary, ProviderFrameEncodeError, ProviderFrameObservationSummaryV1,
    ProviderFrameSinkV1, ProviderFrameTextSpanV1, ProviderFrameTextSpanValidatorV1,
    ProviderItemBuildLifecycle, ProviderItemBuildRecord, ProviderItemBuildRevision,
    ProviderItemFrameV1, ProviderItemObservationV1, ProviderItemStreamValidatorV1,
    ProviderItemValidationError, ProviderLogicalTextRoleV1, ProviderNarrativeComparisonFrontier,
    ProviderNarrativeCompletionCheck, ProviderNarrativeCompletionState,
    ProviderNarrativeGeneration, ProviderNarrativeReference, ProviderNarrativeSpanRecord,
    ProviderStorageRecordError, SealedProviderFrameReference, SourceEventSequence,
    SyndicRecordError, SyndicValueError, advance_content_chain, content_chain_seed,
    encode_provider_item_frame_v1,
};

/// Immutable inputs for preparing one provider frame without retaining its encoded payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderFramePreparationPlan {
    item_id: SyndicItemId,
    turn_id: SyndicTurnId,
    source: CasItemSource,
    source_event: SourceEventSequence,
    first_content_id: Option<SyndicContentId>,
    prior: Option<SealedProviderFrameReference>,
    frame: ProviderItemFrameV1,
}

impl ProviderFramePreparationPlan {
    /// Plans the first frame under a caller-owned fresh content identity.
    #[must_use]
    pub const fn first(
        item_id: SyndicItemId,
        turn_id: SyndicTurnId,
        source: CasItemSource,
        source_event: SourceEventSequence,
        fresh_content_id: SyndicContentId,
        frame: ProviderItemFrameV1,
    ) -> Self {
        Self {
            item_id,
            turn_id,
            source,
            source_event,
            first_content_id: Some(fresh_content_id),
            prior: None,
            frame,
        }
    }

    /// Plans the frame immediately following `prior` in the same provider content stream.
    #[must_use]
    pub const fn subsequent(
        item_id: SyndicItemId,
        turn_id: SyndicTurnId,
        source: CasItemSource,
        source_event: SourceEventSequence,
        prior: SealedProviderFrameReference,
        frame: ProviderItemFrameV1,
    ) -> Self {
        Self {
            item_id,
            turn_id,
            source,
            source_event,
            first_content_id: None,
            prior: Some(prior),
            frame,
        }
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
    pub const fn source_event(&self) -> SourceEventSequence {
        self.source_event
    }

    #[must_use]
    pub const fn prior(&self) -> Option<&SealedProviderFrameReference> {
        self.prior.as_ref()
    }

    #[must_use]
    pub const fn frame(&self) -> &ProviderItemFrameV1 {
        &self.frame
    }
}

/// Completed constant-resident preparation result plus its source frame for durable staging.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedProviderFrame {
    frame: ProviderItemFrameV1,
    initial_build: ProviderItemBuildRecord,
}

impl PreparedProviderFrame {
    #[must_use]
    pub const fn frame(&self) -> &ProviderItemFrameV1 {
        &self.frame
    }

    #[must_use]
    pub const fn initial_build(&self) -> &ProviderItemBuildRecord {
        &self.initial_build
    }

    #[must_use]
    pub const fn target(&self) -> &SealedProviderFrameReference {
        self.initial_build.target()
    }
}

/// Why a provider frame could not be prepared in constant resident memory.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderFramePreparationError {
    #[error(transparent)]
    Validation(#[from] ProviderItemValidationError),
    #[error(transparent)]
    Record(#[from] SyndicRecordError),
    #[error(transparent)]
    Value(#[from] SyndicValueError),
    #[error(transparent)]
    Revision(#[from] RevisionError),
    #[error(transparent)]
    StorageRecord(#[from] ProviderStorageRecordError),
    #[error("provider-frame cumulative count or byte frontier overflowed")]
    FrontierOverflow,
    #[error("provider narrative kind emitted a non-narrative logical span")]
    NarrativeRoleMismatch,
    #[error("the provider-frame narrative-target traversal disagreed with frame preparation")]
    NarrativeTraversalMismatch,
}

/// Traverses the encoding twice through constant-resident read-only sinks to derive one target.
pub fn prepare_provider_frame(
    plan: ProviderFramePreparationPlan,
) -> Result<PreparedProviderFrame, ProviderFramePreparationError> {
    let mut stream_validator = plan
        .prior
        .as_ref()
        .map_or_else(ProviderItemStreamValidatorV1::new, |prior| {
            ProviderItemStreamValidatorV1::from_state(prior.stream_state().clone())
        });
    stream_validator.observe(&plan.frame)?;
    let stream_state = stream_validator
        .state()
        .expect("observing one provider frame produces resumable stream state")
        .clone();

    let prior_summary = plan.prior.as_ref().map(|prior| prior.content().summary());
    let prior_encoded_bytes = prior_summary.map_or(0, ContentSummary::encoded_bytes);
    let prior_chunk_count = prior_summary.map_or(0, ContentSummary::chunk_count);
    let prior_chain = prior_summary.map_or_else(
        || content_chain_seed(ContentEncoding::ProviderItemV1),
        ContentSummary::digest,
    );
    let content_id = plan.prior.as_ref().map_or_else(
        || plan.first_content_id.expect("first plan has content id"),
        |prior| prior.content().id(),
    );
    let content_revision = match &plan.prior {
        Some(prior) => prior.content().revision().checked_next()?,
        None => ContentRevision::new(1)?,
    };

    let mut frame_target = CountingSink::new(
        content_id,
        prior_chunk_count,
        prior_encoded_bytes,
        prior_chain,
        plan.frame.ordinal(),
    );
    let frame_reference =
        match encode_provider_item_frame_v1(&plan.frame, prior_encoded_bytes, &mut frame_target) {
            Ok(reference) => reference,
            Err(ProviderFrameEncodeError::Validation(source)) => return Err(source.into()),
            Err(ProviderFrameEncodeError::Sink(source)) => return Err(source),
        };
    frame_target.spans.finish(&frame_reference)?;

    let summary = ContentSummary::new(
        frame_target.chunk_count,
        0,
        frame_target.encoded_bytes,
        0,
        0,
        0,
        crate::content::input_marker_digest(std::iter::empty()),
        None,
        frame_target.chain,
    )?;
    let content = ContentReference::new(
        content_id,
        content_revision,
        ContentEncoding::ProviderItemV1,
        summary,
    );
    let narrative_seed = narrative_seed(content_id, plan.frame.observation(), plan.prior.as_ref())?;
    let is_narrative_completion = matches!(
        plan.frame.observation(),
        ProviderItemObservationV1::Completed { .. }
    ) && plan.frame.observation().kind().requires_narrative();
    let mut narrative_target = NarrativeCountingSink::new(
        content_id,
        prior_chunk_count,
        prior_encoded_bytes,
        prior_chain,
        plan.frame.ordinal(),
        frame_reference.encoded_digest(),
        narrative_seed,
        !is_narrative_completion,
    );
    let second_reference = match encode_provider_item_frame_v1(
        &plan.frame,
        prior_encoded_bytes,
        &mut narrative_target,
    ) {
        Ok(reference) => reference,
        Err(ProviderFrameEncodeError::Validation(source)) => return Err(source.into()),
        Err(ProviderFrameEncodeError::Sink(source)) => return Err(source),
    };
    narrative_target.spans.finish(&second_reference)?;
    if second_reference != frame_reference
        || narrative_target.chunk_count != summary.chunk_count()
        || narrative_target.encoded_bytes != summary.encoded_bytes()
        || narrative_target.chain != summary.digest()
    {
        return Err(ProviderFramePreparationError::NarrativeTraversalMismatch);
    }
    let narrative = narrative_target.narrative;
    let completion_check = if is_narrative_completion {
        let narrative = narrative.ok_or(ProviderStorageRecordError::MissingPriorNarrative)?;
        Some(ProviderNarrativeCompletionCheck::new(
            narrative_target.completion_span,
            ProviderNarrativeCompletionState::Pending(
                ProviderNarrativeComparisonFrontier::initial(narrative),
            ),
        ))
    } else {
        None
    };
    let observation = observation_summary(plan.frame.observation());
    let target = SealedProviderFrameReference::new(
        content,
        frame_reference,
        observation,
        stream_state,
        narrative,
    )?;
    let initial_build = ProviderItemBuildRecord::new(
        plan.item_id,
        plan.turn_id,
        plan.source,
        plan.source_event,
        ProviderItemBuildRevision::FIRST,
        plan.prior,
        target,
        prior_chunk_count,
        prior_encoded_bytes,
        prior_chain,
        narrative_seed,
        completion_check,
        ProviderItemBuildLifecycle::Staging,
    )?;
    Ok(PreparedProviderFrame {
        frame: plan.frame,
        initial_build,
    })
}

fn observation_summary(
    observation: &ProviderItemObservationV1,
) -> ProviderFrameObservationSummaryV1 {
    match observation {
        ProviderItemObservationV1::Started { observed_at, .. } => {
            ProviderFrameObservationSummaryV1::Started(*observed_at)
        }
        ProviderItemObservationV1::Delta(_) => ProviderFrameObservationSummaryV1::Delta,
        ProviderItemObservationV1::Completed { observed_at, .. } => {
            ProviderFrameObservationSummaryV1::Completed(*observed_at)
        }
    }
}

fn narrative_seed(
    content_id: SyndicContentId,
    observation: &ProviderItemObservationV1,
    prior: Option<&SealedProviderFrameReference>,
) -> Result<Option<ProviderNarrativeReference>, ProviderFramePreparationError> {
    if !observation.kind().requires_narrative() {
        return Ok(None);
    }
    let generation = match observation {
        ProviderItemObservationV1::Started { .. } => ProviderNarrativeGeneration::FIRST,
        ProviderItemObservationV1::Delta(_) => {
            let prior = prior
                .and_then(SealedProviderFrameReference::narrative)
                .ok_or(ProviderStorageRecordError::MissingPriorNarrative)?;
            return Ok(Some(prior));
        }
        ProviderItemObservationV1::Completed { .. } => {
            return prior
                .and_then(SealedProviderFrameReference::narrative)
                .map(Some)
                .ok_or(ProviderStorageRecordError::MissingPriorNarrative.into());
        }
    };
    Ok(Some(ProviderNarrativeReference::empty(
        content_id, generation,
    )))
}

struct CountingSink {
    content_id: SyndicContentId,
    chunk_count: u64,
    encoded_bytes: u64,
    chain: beryl_model::SyndicContentDigest,
    spans: ProviderFrameTextSpanValidatorV1,
}

struct NarrativeCountingSink {
    content_id: SyndicContentId,
    chunk_count: u64,
    encoded_bytes: u64,
    chain: beryl_model::SyndicContentDigest,
    spans: ProviderFrameTextSpanValidatorV1,
    frame_encoded_digest: [u8; 32],
    narrative_logical_base: u64,
    narrative: Option<ProviderNarrativeReference>,
    append_narrative: bool,
    completion_span: Option<ProviderFrameTextSpanV1>,
}

impl NarrativeCountingSink {
    #[allow(clippy::too_many_arguments)]
    const fn new(
        content_id: SyndicContentId,
        chunk_count: u64,
        encoded_bytes: u64,
        chain: beryl_model::SyndicContentDigest,
        frame_ordinal: crate::ProviderFrameOrdinalV1,
        frame_encoded_digest: [u8; 32],
        narrative: Option<ProviderNarrativeReference>,
        append_narrative: bool,
    ) -> Self {
        let narrative_logical_base = match narrative {
            Some(value) => value.logical_utf8_bytes(),
            None => 0,
        };
        Self {
            content_id,
            chunk_count,
            encoded_bytes,
            chain,
            spans: ProviderFrameTextSpanValidatorV1::new(frame_ordinal),
            frame_encoded_digest,
            narrative_logical_base,
            narrative,
            append_narrative,
            completion_span: None,
        }
    }
}

impl CountingSink {
    const fn new(
        content_id: SyndicContentId,
        chunk_count: u64,
        encoded_bytes: u64,
        chain: beryl_model::SyndicContentDigest,
        frame_ordinal: crate::ProviderFrameOrdinalV1,
    ) -> Self {
        Self {
            content_id,
            chunk_count,
            encoded_bytes,
            chain,
            spans: ProviderFrameTextSpanValidatorV1::new(frame_ordinal),
        }
    }
}

impl ProviderFrameSinkV1 for CountingSink {
    type Error = ProviderFramePreparationError;

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        let next_count = self
            .chunk_count
            .checked_add(1)
            .ok_or(ProviderFramePreparationError::FrontierOverflow)?;
        let ordinal = ContentChunkOrdinal::new(next_count)?;
        let chunk = ContentChunkRecord::new(self.content_id, ordinal, bytes)?;
        let encoded_bytes = self
            .encoded_bytes
            .checked_add(
                u64::try_from(bytes.len())
                    .map_err(|_| ProviderFramePreparationError::FrontierOverflow)?,
            )
            .ok_or(ProviderFramePreparationError::FrontierOverflow)?;
        self.chain = advance_content_chain(self.chain, &chunk);
        self.chunk_count = next_count;
        self.encoded_bytes = encoded_bytes;
        Ok(())
    }

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        self.spans.observe(span)?;
        Ok(())
    }
}

impl ProviderFrameSinkV1 for NarrativeCountingSink {
    type Error = ProviderFramePreparationError;

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        let next_count = self
            .chunk_count
            .checked_add(1)
            .ok_or(ProviderFramePreparationError::FrontierOverflow)?;
        let ordinal = ContentChunkOrdinal::new(next_count)?;
        let chunk = ContentChunkRecord::new(self.content_id, ordinal, bytes)?;
        self.encoded_bytes = self
            .encoded_bytes
            .checked_add(
                u64::try_from(bytes.len())
                    .map_err(|_| ProviderFramePreparationError::FrontierOverflow)?,
            )
            .ok_or(ProviderFramePreparationError::FrontierOverflow)?;
        self.chain = advance_content_chain(self.chain, &chunk);
        self.chunk_count = next_count;
        Ok(())
    }

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        self.spans.observe(span)?;
        let Some(previous) = self.narrative else {
            return Ok(());
        };
        if span.role() != ProviderLogicalTextRoleV1::Narrative {
            return Err(ProviderFramePreparationError::NarrativeRoleMismatch);
        }
        if !self.append_narrative {
            if self.completion_span.replace(span).is_some() {
                return Err(ProviderFramePreparationError::NarrativeTraversalMismatch);
            }
            return Ok(());
        }
        let logical_start = self
            .narrative_logical_base
            .checked_add(span.logical_start())
            .ok_or(ProviderFramePreparationError::FrontierOverflow)?;
        let logical_end = self
            .narrative_logical_base
            .checked_add(span.logical_end())
            .ok_or(ProviderFramePreparationError::FrontierOverflow)?;
        if logical_start != previous.logical_utf8_bytes() {
            return Err(ProviderFramePreparationError::NarrativeTraversalMismatch);
        }
        let record = ProviderNarrativeSpanRecord::new(
            self.content_id,
            previous.generation(),
            logical_start,
            logical_end,
            span.frame_ordinal(),
            self.frame_encoded_digest,
            span.source_start(),
            span.source_end(),
            span.source_digest(),
            previous.chain_digest(),
        )?;
        let span_count = previous
            .span_count()
            .checked_add(1)
            .ok_or(ProviderFramePreparationError::FrontierOverflow)?;
        self.narrative = Some(ProviderNarrativeReference::new(
            self.content_id,
            previous.generation(),
            span_count,
            logical_end,
            record.resulting_chain_digest(),
        )?);
        Ok(())
    }
}
