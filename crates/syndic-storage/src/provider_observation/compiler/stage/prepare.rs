use beryl_home_store::HomeStore;
use beryl_model::ContentRevision;

use crate::{
    ContentEncoding, ContentReference, ContentSummary, ProviderFrameObservationSummaryV1,
    ProviderFrameOrdinalV1, ProviderItemBuildLifecycle, ProviderItemBuildRecord,
    ProviderItemBuildRevision, ProviderItemKind, ProviderItemStreamStateV1,
    ProviderItemValidationError, ProviderLifecycleTimestampMsV1,
    ProviderNarrativeComparisonFrontier, ProviderNarrativeCompletionCheck,
    ProviderNarrativeCompletionState, ProviderNarrativeGeneration, ProviderNarrativeReference,
    SealedProviderFrameReference, SyndicPointReadLimit, SyndicStorage, content_chain_seed,
};

use super::super::{
    ProviderObservationFramePreparationError, ProviderObservationFramePreparationPlan,
    ProviderObservationReplay,
    encode::{ObservationEncodeError, encode_observation},
    replay::{FieldSelector, ObservationReplayReader},
};
use super::staging::{CountingSink, NarrativeCountingSink};
use crate::provider_observation::{
    ProviderField, ProviderObservationBegin, ProviderObservationItemLifecycle, ProviderScalar,
};

pub(super) fn prepare(
    storage: &SyndicStorage,
    store: &HomeStore,
    replay: &ProviderObservationReplay,
    plan: ProviderObservationFramePreparationPlan,
    limit: SyndicPointReadLimit,
) -> Result<ProviderItemBuildRecord, ProviderObservationFramePreparationError> {
    let reader = ObservationReplayReader::new(storage, store, replay, limit);
    let kind = item_kind(reader.begin());
    let observation = observation_summary(&reader)?;
    let ordinal = match plan.prior.as_ref() {
        Some(prior) => prior.frame().ordinal().checked_next()?,
        None => ProviderFrameOrdinalV1::FIRST,
    };
    let stream_state = next_stream_state(
        plan.source.item_id(),
        kind,
        ordinal,
        observation,
        reader.history_support(),
        plan.prior.as_ref(),
    )?;

    let prior_summary = plan.prior.as_ref().map(|prior| prior.content().summary());
    let prior_encoded_bytes = prior_summary.map_or(0, ContentSummary::encoded_bytes);
    let prior_chunk_count = prior_summary.map_or(0, ContentSummary::chunk_count);
    let prior_chain = prior_summary.map_or_else(
        || content_chain_seed(ContentEncoding::ProviderItemV1),
        ContentSummary::digest,
    );
    let content_id = plan.prior.as_ref().map_or_else(
        || {
            plan.first_content_id
                .expect("first plan has fresh content identity")
        },
        |prior| prior.content().id(),
    );
    let content_revision = match plan.prior.as_ref() {
        Some(prior) => prior.content().revision().checked_next()?,
        None => ContentRevision::new(1)?,
    };

    let mut target_sink = CountingSink::new(
        content_id,
        prior_chunk_count,
        prior_encoded_bytes,
        prior_chain,
        ordinal,
    );
    let frame = encode_observation(
        &reader,
        plan.source.item_id(),
        ordinal,
        kind,
        prior_encoded_bytes,
        &mut target_sink,
    )
    .map_err(map_encode)?;
    target_sink.spans.finish(&frame)?;

    let summary = target_sink.content_summary()?;
    let content = ContentReference::new(
        content_id,
        content_revision,
        ContentEncoding::ProviderItemV1,
        summary,
    );
    let narrative_seed = narrative_seed(content_id, kind, observation, plan.prior.as_ref())?;
    let completion = matches!(observation, ProviderFrameObservationSummaryV1::Completed(_))
        && kind.requires_narrative();
    let mut narrative_sink = NarrativeCountingSink::new(
        content_id,
        prior_chunk_count,
        prior_encoded_bytes,
        prior_chain,
        ordinal,
        frame.encoded_digest(),
        narrative_seed,
        !completion,
    );
    let second = encode_observation(
        &reader,
        plan.source.item_id(),
        ordinal,
        kind,
        prior_encoded_bytes,
        &mut narrative_sink,
    )
    .map_err(map_encode)?;
    narrative_sink.spans.finish(&second)?;
    if second != frame || !narrative_sink.agrees(summary) {
        return Err(ProviderObservationFramePreparationError::NarrativeTraversalMismatch);
    }

    let narrative = narrative_sink.narrative;
    let completion_check = if completion {
        let narrative =
            narrative.ok_or(crate::ProviderStorageRecordError::MissingPriorNarrative)?;
        Some(ProviderNarrativeCompletionCheck::new(
            narrative_sink.completion_span,
            ProviderNarrativeCompletionState::Pending(
                ProviderNarrativeComparisonFrontier::initial(narrative),
            ),
        ))
    } else {
        None
    };
    let target =
        SealedProviderFrameReference::new(content, frame, observation, stream_state, narrative)?;
    ProviderItemBuildRecord::new(
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
    )
    .map_err(Into::into)
}

pub(super) fn observation_summary(
    reader: &ObservationReplayReader<'_>,
) -> Result<ProviderFrameObservationSummaryV1, ProviderObservationFramePreparationError> {
    Ok(match reader.begin() {
        ProviderObservationBegin::Item { lifecycle, .. } => {
            let timestamp =
                match reader.scalar(FieldSelector::top(ProviderField::LifecycleObservedAt)) {
                    Ok(Some(ProviderScalar::Unsigned(value))) => value,
                    Ok(_) => {
                        return Err(
                        super::super::super::ProviderObservationFrameSemanticError::ValueMismatch {
                            field: ProviderField::LifecycleObservedAt,
                        }
                        .into(),
                    );
                    }
                    Err(error) => return Err(error.preparation()),
                };
            match lifecycle {
                ProviderObservationItemLifecycle::Started => {
                    ProviderFrameObservationSummaryV1::Started(ProviderLifecycleTimestampMsV1::new(
                        timestamp,
                    ))
                }
                ProviderObservationItemLifecycle::Completed => {
                    ProviderFrameObservationSummaryV1::Completed(
                        ProviderLifecycleTimestampMsV1::new(timestamp),
                    )
                }
            }
        }
        ProviderObservationBegin::Delta { .. } => ProviderFrameObservationSummaryV1::Delta,
    })
}

pub(super) const fn item_kind(begin: ProviderObservationBegin) -> ProviderItemKind {
    crate::provider_observation_item_kind(begin)
}

fn next_stream_state(
    item_id: &beryl_model::CasItemId,
    kind: ProviderItemKind,
    ordinal: ProviderFrameOrdinalV1,
    observation: ProviderFrameObservationSummaryV1,
    history_support: crate::ProviderFrameHistorySupportV1,
    prior: Option<&SealedProviderFrameReference>,
) -> Result<ProviderItemStreamStateV1, ProviderItemValidationError> {
    let expected_ordinal = prior.map_or(1, |value| value.stream_state().next_ordinal().get());
    if ordinal.get() != expected_ordinal {
        return Err(ProviderItemValidationError::FrameOrdinalConflict {
            expected: expected_ordinal,
            actual: ordinal.get(),
        });
    }
    if let Some(prior) = prior {
        let state = prior.stream_state();
        if state.is_complete() {
            return Err(ProviderItemValidationError::EventAfterCompletion);
        }
        if state.item_id() != item_id {
            return Err(ProviderItemValidationError::ItemIdentityMismatch);
        }
        if state.kind() != kind {
            return Err(ProviderItemValidationError::ItemKindMismatch {
                expected: state.kind(),
                actual: kind,
            });
        }
    }
    let started_at = prior.and_then(|value| value.stream_state().started_at());
    let (started_at, complete) = match observation {
        ProviderFrameObservationSummaryV1::Started(observed_at) => {
            if kind.permits_completion_only() {
                return Err(ProviderItemValidationError::CompletionOnlyItemStarted);
            }
            if started_at.is_some() {
                return Err(ProviderItemValidationError::DuplicateItemStart);
            }
            (Some(observed_at), false)
        }
        ProviderFrameObservationSummaryV1::Delta => {
            let Some(started_at) = started_at else {
                return Err(ProviderItemValidationError::MissingItemStart);
            };
            (Some(started_at), false)
        }
        ProviderFrameObservationSummaryV1::Completed(observed_at) => match started_at {
            Some(started_at) if observed_at < started_at => {
                return Err(ProviderItemValidationError::CompletionBeforeStart {
                    started: started_at.get(),
                    completed: observed_at.get(),
                });
            }
            Some(started_at) => (Some(started_at), true),
            None if kind.permits_completion_only() => (None, true),
            None => return Err(ProviderItemValidationError::MissingItemStart),
        },
    };
    let cumulative = prior.map_or(history_support, |value| {
        value.history_support().merge(history_support)
    });
    ProviderItemStreamStateV1::new(
        item_id.clone(),
        kind,
        ordinal
            .get()
            .checked_add(1)
            .ok_or(ProviderItemValidationError::FrameOrdinalExhausted)?,
        started_at,
        complete,
        cumulative,
    )
}

fn narrative_seed(
    content_id: beryl_model::SyndicContentId,
    kind: ProviderItemKind,
    observation: ProviderFrameObservationSummaryV1,
    prior: Option<&SealedProviderFrameReference>,
) -> Result<Option<ProviderNarrativeReference>, ProviderObservationFramePreparationError> {
    if !kind.requires_narrative() {
        return Ok(None);
    }
    match observation {
        ProviderFrameObservationSummaryV1::Started(_) => Ok(Some(
            ProviderNarrativeReference::empty(content_id, ProviderNarrativeGeneration::FIRST),
        )),
        ProviderFrameObservationSummaryV1::Delta
        | ProviderFrameObservationSummaryV1::Completed(_) => prior
            .and_then(SealedProviderFrameReference::narrative)
            .map(Some)
            .ok_or(crate::ProviderStorageRecordError::MissingPriorNarrative.into()),
    }
}

fn map_encode(
    error: ObservationEncodeError<ProviderObservationFramePreparationError>,
) -> ProviderObservationFramePreparationError {
    match error {
        ObservationEncodeError::Replay(error) => error.preparation(),
        ObservationEncodeError::Validation(error) => error.into(),
        ObservationEncodeError::Sink(error) => error,
    }
}
