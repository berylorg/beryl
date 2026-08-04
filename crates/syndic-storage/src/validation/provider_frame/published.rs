use beryl_home_store::{DomainReader, PointReadLimit};

use crate::{
    ContentEncoding, ProjectionTextSource, ProviderFrameObservationSummaryV1,
    ProviderFrameStructuralValidationV1, ProviderFrameTextSpanSinkV1, ProviderFrameTextSpanV1,
    ProviderItemStreamValidatorV1, ProviderLogicalTextRoleV1,
    ProviderNarrativeCompletionDisposition, ProviderNarrativeGeneration,
    ProviderNarrativeReference, ProviderNarrativeSpanRecord, SealedProviderFrameReference,
    codec::*, domain::SyndicDomain,
};

use super::{
    FrameSpanError, ProviderFrameStorageValidationError, advance_narrative,
    validate_sealed_provider_frame, validate_source_range,
};

const NARRATIVE_SPAN_POINT_BYTES: usize = 1024;

/// Structurally revalidated evidence from one already published source frame.
pub(crate) struct PublishedProviderFrameValidation {
    structural: ProviderFrameStructuralValidationV1,
    completion_span: Option<ProviderFrameTextSpanV1>,
}

impl PublishedProviderFrameValidation {
    pub(crate) const fn structural(&self) -> &ProviderFrameStructuralValidationV1 {
        &self.structural
    }

    pub(crate) const fn completion_span(&self) -> Option<ProviderFrameTextSpanV1> {
        self.completion_span
    }
}

pub(crate) fn validate_published_provider_frame(
    reader: &DomainReader<'_, SyndicDomain>,
    prior: Option<&SealedProviderFrameReference>,
    target: &SealedProviderFrameReference,
) -> Result<PublishedProviderFrameValidation, ProviderFrameStorageValidationError> {
    validate_content_progression(prior, target)?;
    let mut spans = PublishedSpanVerifier::new(reader, prior, target)?;
    let structural = validate_sealed_provider_frame(reader, prior, target, &mut spans)?;
    let completion_span = spans.finish()?;

    let mut stream = prior.map_or_else(ProviderItemStreamValidatorV1::new, |prior| {
        ProviderItemStreamValidatorV1::from_state(prior.stream_state().clone())
    });
    stream.observe_structural(&structural).map_err(|_| {
        ProviderFrameStorageValidationError::Invariant(
            "published provider frame lifecycle progression is invalid",
        )
    })?;
    if stream.state() != Some(target.stream_state()) {
        return invalid("published provider frame stream state disagrees");
    }

    Ok(PublishedProviderFrameValidation {
        structural,
        completion_span,
    })
}

pub(crate) fn validate_published_narrative_completion(
    reader: &DomainReader<'_, SyndicDomain>,
    target: &SealedProviderFrameReference,
    completion_span: Option<ProviderFrameTextSpanV1>,
    disposition: ProviderNarrativeCompletionDisposition,
) -> Result<(), ProviderFrameStorageValidationError> {
    let narrative = target
        .narrative()
        .ok_or(ProviderFrameStorageValidationError::Invariant(
            "published narrative completion omitted its live source",
        ))?;
    if !matches!(
        target.observation(),
        ProviderFrameObservationSummaryV1::Completed(_)
    ) {
        return invalid("published narrative completion span is invalid");
    }
    let completed_len = match completion_span {
        Some(span)
            if span.role() == ProviderLogicalTextRoleV1::Narrative
                && span.logical_start() == 0
                && span.logical_end() == target.frame().logical_utf8_bytes() =>
        {
            span.logical_end()
        }
        None if target.frame().logical_utf8_bytes() == 0
            && target.frame().text_span_count() == 0 =>
        {
            0
        }
        _ => return invalid("published narrative completion span is invalid"),
    };

    const PAGE_BYTES: u64 = 65_536;
    let live_len = narrative.logical_utf8_bytes();
    let shared_len = live_len.min(completed_len);
    let mut first_mismatch = None;
    let mut start = 0_u64;
    while start < shared_len {
        let end = start.saturating_add(PAGE_BYTES).min(shared_len);
        let live = super::super::content::read_projection_text_range(
            reader,
            ProjectionTextSource::provider_narrative(narrative),
            start,
            end,
        )
        .map_err(validation_error)?;
        let completed_start = completion_span
            .expect("nonempty completion has one span")
            .source_start()
            .checked_add(start)
            .ok_or(ProviderFrameStorageValidationError::Invariant(
                "published narrative completion source offset overflowed",
            ))?;
        let completed_end = completed_start.checked_add(end - start).ok_or(
            ProviderFrameStorageValidationError::Invariant(
                "published narrative completion source offset overflowed",
            ),
        )?;
        let completed = super::super::content::read_encoded_range(
            reader,
            target.content().id(),
            target.content().summary().encoded_bytes(),
            completed_start,
            completed_end,
        )
        .map_err(validation_error)?;
        if let Some(offset) = live
            .iter()
            .zip(&completed)
            .position(|(left, right)| left != right)
        {
            first_mismatch = Some(
                start
                    + u64::try_from(offset).map_err(|_| {
                        ProviderFrameStorageValidationError::Invariant(
                            "published narrative mismatch offset overflowed",
                        )
                    })?,
            );
            break;
        }
        start = end;
    }
    if first_mismatch.is_none() && live_len != completed_len {
        first_mismatch = Some(shared_len);
    }
    let expected = first_mismatch.map_or(
        ProviderNarrativeCompletionDisposition::Equal,
        |utf8_byte_offset| ProviderNarrativeCompletionDisposition::Mismatch { utf8_byte_offset },
    );
    if disposition != expected {
        return invalid("published narrative completion disposition disagrees");
    }
    Ok(())
}

fn validate_content_progression(
    prior: Option<&SealedProviderFrameReference>,
    target: &SealedProviderFrameReference,
) -> Result<(), ProviderFrameStorageValidationError> {
    let content = target.content();
    if content.encoding() != ContentEncoding::ProviderItemV1 {
        return invalid("published provider frame content encoding disagrees");
    }
    match prior {
        Some(prior) => {
            let previous = prior.content();
            if content.id() != previous.id()
                || previous.revision().get().checked_add(1) != Some(content.revision().get())
                || target.frame().encoded_start() != previous.summary().encoded_bytes()
                || content.summary().chunk_count() <= previous.summary().chunk_count()
                || content.summary().encoded_bytes() <= previous.summary().encoded_bytes()
            {
                return invalid(
                    "published provider frame content frontier did not advance exactly",
                );
            }
        }
        None => {
            if content.revision().get() != 1 || target.frame().encoded_start() != 0 {
                return invalid("first published provider frame content frontier is invalid");
            }
        }
    }
    Ok(())
}

struct PublishedSpanVerifier<'a, 'r, 't> {
    reader: &'a DomainReader<'r, SyndicDomain>,
    target: &'t SealedProviderFrameReference,
    logical_base: u64,
    narrative: Option<ProviderNarrativeReference>,
    completion_span: Option<ProviderFrameTextSpanV1>,
}

impl<'a, 'r, 't> PublishedSpanVerifier<'a, 'r, 't> {
    fn new(
        reader: &'a DomainReader<'r, SyndicDomain>,
        prior: Option<&SealedProviderFrameReference>,
        target: &'t SealedProviderFrameReference,
    ) -> Result<Self, ProviderFrameStorageValidationError> {
        let narrative = narrative_seed(prior, target)?;
        Ok(Self {
            reader,
            target,
            logical_base: narrative.map_or(0, |value| value.logical_utf8_bytes()),
            narrative,
            completion_span: None,
        })
    }

    fn finish(
        self,
    ) -> Result<Option<ProviderFrameTextSpanV1>, ProviderFrameStorageValidationError> {
        let is_narrative_completion = self.target.frame().item_kind().requires_narrative()
            && matches!(
                self.target.observation(),
                ProviderFrameObservationSummaryV1::Completed(_)
            );
        let completion_shape_matches = if is_narrative_completion {
            self.completion_span.is_some()
                || (self.target.frame().logical_utf8_bytes() == 0
                    && self.target.frame().text_span_count() == 0)
        } else {
            self.completion_span.is_none()
        };
        if self.narrative != self.target.narrative() || !completion_shape_matches {
            return invalid("published provider narrative frontier disagrees");
        }
        Ok(self.completion_span)
    }
}

impl ProviderFrameTextSpanSinkV1 for PublishedSpanVerifier<'_, '_, '_> {
    type Error = FrameSpanError;

    fn write_text_span(&mut self, span: ProviderFrameTextSpanV1) -> Result<(), Self::Error> {
        validate_source_range(
            self.reader,
            self.target.content(),
            self.target.content().summary().encoded_bytes(),
            span.source_start(),
            span.source_end(),
            span.source_digest(),
        )?;
        if !self.target.frame().item_kind().requires_narrative() {
            return Ok(());
        }
        if span.role() != ProviderLogicalTextRoleV1::Narrative {
            return Err(FrameSpanError::Invariant(
                "published narrative frame emitted a nonnarrative span",
            ));
        }
        if matches!(
            self.target.observation(),
            ProviderFrameObservationSummaryV1::Completed(_)
        ) {
            if self.completion_span.replace(span).is_some() {
                return Err(FrameSpanError::Invariant(
                    "published narrative completion emitted multiple spans",
                ));
            }
            return Ok(());
        }
        self.append_narrative_span(span)
    }
}

impl PublishedSpanVerifier<'_, '_, '_> {
    fn append_narrative_span(
        &mut self,
        span: ProviderFrameTextSpanV1,
    ) -> Result<(), FrameSpanError> {
        let previous = self.narrative.ok_or(FrameSpanError::Invariant(
            "published narrative append omitted its generation seed",
        ))?;
        let logical_start = self.logical_base.checked_add(span.logical_start()).ok_or(
            FrameSpanError::Invariant("published narrative logical frontier overflowed"),
        )?;
        let logical_end =
            self.logical_base
                .checked_add(span.logical_end())
                .ok_or(FrameSpanError::Invariant(
                    "published narrative logical frontier overflowed",
                ))?;
        if logical_start != previous.logical_utf8_bytes() {
            return Err(FrameSpanError::Invariant(
                "published narrative logical frontier is not contiguous",
            ));
        }
        let frame = self.target.frame();
        let expected = ProviderNarrativeSpanRecord::new(
            self.target.content().id(),
            previous.generation(),
            logical_start,
            logical_end,
            span.frame_ordinal(),
            frame.encoded_digest(),
            span.source_start(),
            span.source_end(),
            span.source_digest(),
            previous.chain_digest(),
        )
        .map_err(|_| FrameSpanError::Invariant("published provider narrative span is invalid"))?;
        let key = ProviderNarrativeSpanKey::new(
            expected.content_id(),
            expected.generation(),
            expected.logical_start(),
        );
        let stored = self
            .reader
            .point::<ProviderNarrativeSpansCodec>(
                &key,
                PointReadLimit::new(NARRATIVE_SPAN_POINT_BYTES)
                    .expect("narrative span point bound is nonzero"),
            )
            .map_err(FrameSpanError::Read)?
            .ok_or(FrameSpanError::Invariant(
                "published provider narrative span is missing",
            ))?;
        if stored != expected {
            return Err(FrameSpanError::Invariant(
                "published provider narrative span disagrees",
            ));
        }
        self.narrative = Some(advance_narrative(previous, stored)?);
        Ok(())
    }
}

fn narrative_seed(
    prior: Option<&SealedProviderFrameReference>,
    target: &SealedProviderFrameReference,
) -> Result<Option<ProviderNarrativeReference>, ProviderFrameStorageValidationError> {
    let Some(target_narrative) = target.narrative() else {
        return Ok(None);
    };
    match target.observation() {
        ProviderFrameObservationSummaryV1::Started(_) => {
            if target_narrative.generation() != ProviderNarrativeGeneration::FIRST {
                return invalid("first published provider narrative generation is not first");
            }
            Ok(Some(ProviderNarrativeReference::empty(
                target_narrative.content_id(),
                target_narrative.generation(),
            )))
        }
        ProviderFrameObservationSummaryV1::Delta
        | ProviderFrameObservationSummaryV1::Completed(_) => prior
            .and_then(SealedProviderFrameReference::narrative)
            .map(Some)
            .ok_or(ProviderFrameStorageValidationError::Invariant(
                "published provider frame omitted its prior narrative frontier",
            )),
    }
}

fn validation_error(
    error: crate::error::SyndicValidationError,
) -> ProviderFrameStorageValidationError {
    match error {
        crate::error::SyndicValidationError::Read(source) => {
            ProviderFrameStorageValidationError::Read(source)
        }
        crate::error::SyndicValidationError::Invariant(message) => {
            ProviderFrameStorageValidationError::Invariant(message)
        }
    }
}

const fn invalid<T>(message: &'static str) -> Result<T, ProviderFrameStorageValidationError> {
    Err(ProviderFrameStorageValidationError::Invariant(message))
}
