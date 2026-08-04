use beryl_home_store::DomainReader;
use beryl_model::{ProjectionRevision, SyndicItemId};

use crate::{
    AssistantMessagePhase, CanonicalItemKind, ContentEncoding, ContentLifecycle,
    ItemSourceEventOrdinal, ProjectionTextSource, ProviderFrameObservationSummaryV1,
    ProviderItemKind, ProviderItemLifecycle, ProviderMessagePhaseV1, SealedProviderFrameReference,
    SourceEventPayload, codec::*, domain::SyndicDomain, error::SyndicValidationError,
};

use super::super::{
    provider_frame::{
        ProviderFrameStorageValidationError, validate_published_narrative_completion,
        validate_published_provider_frame,
    },
    scan::{point, require, scan},
};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    validate_index_presence(reader)?;
    validate_replay(reader)
}

fn validate_index_presence(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<CanonicalItemsFamily>(reader, |_, item| {
        if (item.source_event_count() == 0) != item.source_event().is_none() {
            return invariant("canonical item source-event frontier shape disagrees");
        }
        let first = ItemEventKey {
            owner: item.id(),
            ordinal: ItemSourceEventOrdinal::FIRST,
        };
        if (item.source_event_count() == 0)
            == point::<ItemSourceEventsFamily>(reader, &first)?.is_some()
        {
            return invariant("canonical item source-event index presence disagrees");
        }
        if item.source_event_count() == 0
            && (item.provider_kind() != ProviderItemKind::UserMessage
                || item.provider_lifecycle() != ProviderItemLifecycle::AwaitingCorrelation
                || item.revision().get() != 1)
        {
            return invariant("uncorrelated canonical item is not pristine submitted user input");
        }
        Ok(())
    })
}

fn validate_replay(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    let mut replay = None;
    scan::<ItemSourceEventsFamily>(reader, |key, index| {
        if replay
            .as_ref()
            .is_some_and(|state: &Replay| state.item != key.owner)
        {
            finish(reader, replay.take().expect("replay exists"))?;
        }
        let state = replay.get_or_insert_with(|| Replay::new(key.owner));
        state.push(reader, key, index)
    })?;
    if let Some(state) = replay {
        finish(reader, state)?;
    }
    Ok(())
}

struct Replay {
    item: SyndicItemId,
    next_ordinal: u64,
    last_sequence: Option<crate::SourceEventSequence>,
    provider: Option<SealedProviderFrameReference>,
    assistant_phase: Option<AssistantMessagePhase>,
    completion_span: Option<crate::ProviderFrameTextSpanV1>,
}

impl Replay {
    const fn new(item: SyndicItemId) -> Self {
        Self {
            item,
            next_ordinal: 1,
            last_sequence: None,
            provider: None,
            assistant_phase: None,
            completion_span: None,
        }
    }
}

impl Replay {
    fn push(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        key: &ItemEventKey,
        index: &crate::ItemSourceEventIndexRecord,
    ) -> Result<(), SyndicValidationError> {
        if key.owner != self.item
            || key.ordinal != index.ordinal()
            || index.item_id() != self.item
            || index.ordinal().get() != self.next_ordinal
            || self
                .last_sequence
                .is_some_and(|previous| index.source_event() <= previous)
        {
            return invariant("item source-event key, order, or sequence disagrees");
        }
        let item = require::<CanonicalItemsFamily>(
            reader,
            &self.item,
            "item source-event owner is missing",
        )?;
        if item.turn_id() != index.turn_id() {
            return invariant("item source-event turn owner disagrees");
        }
        let event = require::<SourceEventsFamily>(
            reader,
            &TurnEventKey {
                owner: index.turn_id(),
                ordinal: index.source_event(),
            },
            "item source event is missing",
        )?;
        let SourceEventPayload::ItemFrame { item_id, frame } = event.payload() else {
            return invariant("item source-event index selected a non-item event");
        };
        if *item_id != self.item {
            return invariant("item source-event payload names another item");
        }
        validate_external_source(&item, &event, frame)?;
        let validation = validate_published_provider_frame(reader, self.provider.as_ref(), frame)
            .map_err(provider_validation_error)?;
        validate_structural_facts(&item, validation.structural())?;
        self.assistant_phase = merge_assistant_phase(
            frame.frame().item_kind(),
            frame.observation(),
            validation.structural().message_phase(),
            self.assistant_phase,
        )?;
        if frame.stream_state().is_complete() {
            self.completion_span = validation.completion_span();
        } else if validation.completion_span().is_some() {
            return invariant("incomplete provider frame retained completion evidence");
        }
        self.provider = Some(frame.as_ref().clone());
        self.last_sequence = Some(index.source_event());
        self.next_ordinal =
            self.next_ordinal
                .checked_add(1)
                .ok_or(SyndicValidationError::Invariant(
                    "item source-event order exhausted",
                ))?;
        Ok(())
    }
}

fn validate_external_source(
    item: &crate::CanonicalItemRecord,
    event: &crate::SourceEventRecord,
    frame: &SealedProviderFrameReference,
) -> Result<(), SyndicValidationError> {
    match (item.cas_source(), event.source()) {
        (Some(item_source), Some(turn_source))
            if item_source.turn() == turn_source
                && item_source.item_id() == frame.frame().item_id() =>
        {
            Ok(())
        }
        _ => invariant("item source-event external identity disagrees"),
    }
}

fn validate_structural_facts(
    item: &crate::CanonicalItemRecord,
    frame: &crate::ProviderFrameStructuralValidationV1,
) -> Result<(), SyndicValidationError> {
    if frame.reference().item_kind() == ProviderItemKind::UserMessage {
        if frame.submitted_content() != item.presentation_content() {
            return invariant("provider user frame changed submitted composer content");
        }
    } else if frame.submitted_content().is_some() {
        return invariant("non-user provider frame retained submitted composer content");
    }
    if frame.reference().item_kind() != ProviderItemKind::AgentMessage
        && frame.message_phase().is_some()
    {
        return invariant("non-assistant provider frame retained assistant phase");
    }
    Ok(())
}

fn finish(
    reader: &DomainReader<'_, SyndicDomain>,
    replay: Replay,
) -> Result<(), SyndicValidationError> {
    let item = require::<CanonicalItemsFamily>(
        reader,
        &replay.item,
        "item source-event owner is missing",
    )?;
    let observed = replay.next_ordinal - 1;
    let provider = replay.provider.ok_or(SyndicValidationError::Invariant(
        "item source replay omitted its provider frame",
    ))?;
    let expected_lifecycle = if provider.stream_state().is_complete() {
        ProviderItemLifecycle::Completed
    } else {
        ProviderItemLifecycle::Started
    };
    if item.source_event_count() != observed
        || item.source_event() != replay.last_sequence
        || item.provider_kind() != provider.frame().item_kind()
        || item.provider_lifecycle() != expected_lifecycle
        || item.assistant_phase() != replay.assistant_phase
    {
        return invariant("canonical item does not equal replayed provider lifecycle");
    }

    validate_completion(reader, &item, &provider, replay.completion_span)?;
    validate_canonical_provider(reader, &item, provider, observed)
}

fn validate_completion(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
    provider: &SealedProviderFrameReference,
    completion_span: Option<crate::ProviderFrameTextSpanV1>,
) -> Result<(), SyndicValidationError> {
    let is_narrative_completion =
        provider.stream_state().is_complete() && provider.frame().item_kind().requires_narrative();
    match (is_narrative_completion, item.narrative_completion()) {
        (true, Some(disposition)) => {
            validate_published_narrative_completion(reader, provider, completion_span, disposition)
                .map_err(provider_validation_error)
        }
        (false, None) if completion_span.is_none() => Ok(()),
        _ => invariant("canonical narrative completion evidence disagrees"),
    }
}

fn validate_canonical_provider(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
    published: SealedProviderFrameReference,
    source_event_count: u64,
) -> Result<(), SyndicValidationError> {
    let canonical = item.provider().ok_or(SyndicValidationError::Invariant(
        "provider-backed canonical item omitted its sealed frame",
    ))?;
    let local_revision = u64::from(item.provider_kind() == ProviderItemKind::UserMessage);
    let published_revision =
        source_event_count
            .checked_add(local_revision)
            .ok_or(SyndicValidationError::Invariant(
                "canonical item revision frontier overflowed",
            ))?;
    let manifest = require::<ContentManifestsFamily>(
        reader,
        &canonical.content().id(),
        "canonical provider content manifest is missing",
    )?;

    if canonical == &published {
        if item.revision().get() != published_revision
            || manifest.lifecycle() != ContentLifecycle::Live
            || manifest.current_reference() != Some(canonical.content())
        {
            return invariant("live canonical provider frontier disagrees");
        }
    } else {
        let expected_item_revision =
            published_revision
                .checked_add(1)
                .ok_or(SyndicValidationError::Invariant(
                    "canonical finalization revision overflowed",
                ))?;
        let published_content = published.content();
        let canonical_content = canonical.content();
        let finalization_only = canonical.frame() == published.frame()
            && canonical.observation() == published.observation()
            && canonical.stream_state() == published.stream_state()
            && canonical.narrative() == published.narrative()
            && canonical_content.id() == published_content.id()
            && canonical_content.encoding() == published_content.encoding()
            && canonical_content.summary() == published_content.summary()
            && published_content.revision().get().checked_add(1)
                == Some(canonical_content.revision().get());
        let state = require::<TurnStatesFamily>(
            reader,
            &item.turn_id(),
            "finalized canonical provider turn state is missing",
        )?;
        if !finalization_only
            || !canonical.stream_state().is_complete()
            || !state.lifecycle().is_proven_terminal()
            || item.revision().get() != expected_item_revision
            || manifest.lifecycle() != ContentLifecycle::Finalized
            || manifest.current_reference() != Some(canonical_content)
        {
            return invariant("finalized canonical provider frontier disagrees");
        }
    }
    Ok(())
}

fn merge_assistant_phase(
    kind: ProviderItemKind,
    observation: ProviderFrameObservationSummaryV1,
    observed: Option<ProviderMessagePhaseV1>,
    current: Option<AssistantMessagePhase>,
) -> Result<Option<AssistantMessagePhase>, SyndicValidationError> {
    if kind != ProviderItemKind::AgentMessage {
        return if observed.is_none() && current.is_none() {
            Ok(None)
        } else {
            invariant("non-assistant provider frame changed assistant phase")
        };
    }
    if matches!(observation, ProviderFrameObservationSummaryV1::Delta) {
        return current.map(Some).ok_or(SyndicValidationError::Invariant(
            "assistant delta preceded assistant start",
        ));
    }
    let supplied = match observed {
        Some(ProviderMessagePhaseV1::Commentary) => AssistantMessagePhase::Commentary,
        Some(ProviderMessagePhaseV1::FinalAnswer) => AssistantMessagePhase::FinalAnswer,
        None => AssistantMessagePhase::Unknown,
    };
    let merged = match (current, supplied) {
        (None, supplied) | (Some(AssistantMessagePhase::Unknown), supplied) => Some(supplied),
        (Some(known), AssistantMessagePhase::Unknown) => Some(known),
        (Some(left), right) if left == right => Some(left),
        _ => return invariant("assistant source phases disagree"),
    };
    Ok(merged)
}

pub(super) fn validate_projection_snapshot(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
    source_item_revision: ProjectionRevision,
    source: ProjectionTextSource,
) -> Result<bool, SyndicValidationError> {
    if source_item_revision > item.revision() {
        return invariant("projection source revision exceeds its canonical item");
    }
    if matches!(item.kind(), CanonicalItemKind::UserInput) {
        if item.projection_source() != Some(source) {
            return invariant("user projection source changed across canonical revisions");
        }
        let ProjectionTextSource::Composer(content) = source else {
            return invariant("user projection source is not composer content");
        };
        let manifest = require::<ContentManifestsFamily>(
            reader,
            &content.id(),
            "user projection source content is missing",
        )?;
        if content.encoding() != ContentEncoding::ComposerV1
            || manifest.lifecycle() != ContentLifecycle::Sealed
            || manifest.sealed_reference() != Some(content)
        {
            return invariant("user projection source is not exact sealed composer content");
        }
        return Ok(true);
    }

    let event_ordinal = source_event_ordinal_for_revision(item, source_item_revision)?;
    let frame = indexed_frame(reader, item, event_ordinal)?;
    let expected = frame
        .narrative()
        .map(ProjectionTextSource::provider_narrative)
        .ok_or(SyndicValidationError::Invariant(
            "provider projection snapshot omitted narrative authority",
        ))?;
    if source != expected {
        return invariant("provider projection source snapshot disagrees");
    }
    Ok(frame.stream_state().is_complete())
}

fn source_event_ordinal_for_revision(
    item: &crate::CanonicalItemRecord,
    revision: ProjectionRevision,
) -> Result<u64, SyndicValidationError> {
    let count = item.source_event_count();
    if revision.get() <= count {
        return Ok(revision.get());
    }
    if revision == item.revision() && revision.get().checked_sub(1) == Some(count) && count != 0 {
        return Ok(count);
    }
    invariant("provider projection source revision has no source frame")
}

fn indexed_frame(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
    ordinal: u64,
) -> Result<SealedProviderFrameReference, SyndicValidationError> {
    let ordinal = ItemSourceEventOrdinal::new(ordinal).map_err(|_| {
        SyndicValidationError::Invariant("projection source event ordinal is invalid")
    })?;
    let index = require::<ItemSourceEventsFamily>(
        reader,
        &ItemEventKey {
            owner: item.id(),
            ordinal,
        },
        "projection source event index is missing",
    )?;
    if index.item_id() != item.id()
        || index.turn_id() != item.turn_id()
        || index.ordinal() != ordinal
    {
        return invariant("projection source event index disagrees");
    }
    let event = require::<SourceEventsFamily>(
        reader,
        &TurnEventKey {
            owner: item.turn_id(),
            ordinal: index.source_event(),
        },
        "projection source event is missing",
    )?;
    match event.payload() {
        SourceEventPayload::ItemFrame { item_id, frame } if *item_id == item.id() => {
            Ok(frame.as_ref().clone())
        }
        _ => invariant("projection source event does not name the canonical item"),
    }
}

fn provider_validation_error(error: ProviderFrameStorageValidationError) -> SyndicValidationError {
    match error {
        ProviderFrameStorageValidationError::Read(source) => SyndicValidationError::Read(source),
        ProviderFrameStorageValidationError::Invariant(message) => {
            SyndicValidationError::Invariant(message)
        }
    }
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
