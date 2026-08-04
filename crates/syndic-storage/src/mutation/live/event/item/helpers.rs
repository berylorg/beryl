use beryl_home_store::DomainReader;
use beryl_model::{SyndicItemId, SyndicResourceId, SyndicThreadId};
use sha2::{Digest, Sha256};

use crate::mutation::{point, required};
use crate::{
    AssistantMessagePhase, CanonicalItemPresentation, CanonicalItemRecord, CasItemSource,
    ItemSourceEventOrdinal, ProviderFrameObservationSummaryV1, ProviderItemKind,
    ProviderMessagePhaseV1, SealedProviderFrameReference, SourceEventRecord, SyndicMutationError,
    TurnItemOrdinal, codec::*, domain::SyndicDomain,
};

const GENERATED_MEDIA_RESOURCE_V1: &[u8] = b"beryl.syndic.generated-media-resource.v1";

pub(super) fn exact_item_source(
    event: &SourceEventRecord,
    frame: &SealedProviderFrameReference,
) -> Result<CasItemSource, SyndicMutationError> {
    let turn = event
        .source()
        .ok_or(SyndicMutationError::SourceIdentityConflict)?;
    Ok(CasItemSource::new(
        turn.clone(),
        frame.frame().item_id().clone(),
    ))
}

pub(super) fn ensure_new_cas_item(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &CasItemSource,
) -> Result<(), SyndicMutationError> {
    if point::<CasItemIndexFamily>(reader, &cas_item_key(source))?.is_some() {
        return Err(SyndicMutationError::SourceIdentityConflict);
    }
    Ok(())
}

pub(super) fn ensure_new_turn_item(
    reader: &DomainReader<'_, SyndicDomain>,
    turn: beryl_model::SyndicTurnId,
    ordinal: TurnItemOrdinal,
) -> Result<(), SyndicMutationError> {
    if point::<TurnItemsFamily>(
        reader,
        &TurnItemKey {
            owner: turn,
            ordinal,
        },
    )?
    .is_some()
    {
        return Err(SyndicMutationError::CanonicalItemConflict);
    }
    Ok(())
}

pub(super) fn ensure_new_item_source_event(
    reader: &DomainReader<'_, SyndicDomain>,
    item: SyndicItemId,
    ordinal: ItemSourceEventOrdinal,
) -> Result<(), SyndicMutationError> {
    if point::<ItemSourceEventsFamily>(
        reader,
        &ItemEventKey {
            owner: item,
            ordinal,
        },
    )?
    .is_some()
    {
        return Err(SyndicMutationError::CanonicalItemConflict);
    }
    Ok(())
}

pub(super) fn require_turn_item_index(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &CanonicalItemRecord,
) -> Result<(), SyndicMutationError> {
    let index = required::<TurnItemsFamily>(
        reader,
        &TurnItemKey {
            owner: item.turn_id(),
            ordinal: item.ordinal(),
        },
    )?;
    if index.turn_id() != item.turn_id()
        || index.ordinal() != item.ordinal()
        || index.item_id() != item.id()
        || index.item_revision() != item.revision()
    {
        return Err(SyndicMutationError::CanonicalItemConflict);
    }
    Ok(())
}

pub(super) fn require_current_indexes(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &CanonicalItemRecord,
    source: &CasItemSource,
) -> Result<(), SyndicMutationError> {
    require_turn_item_index(reader, item)?;
    let cas = required::<CasItemIndexFamily>(reader, &cas_item_key(source))?;
    if cas.item_id() != item.id() || cas.item_revision() != item.revision() {
        return Err(SyndicMutationError::SourceIdentityConflict);
    }
    let source_ordinal = ItemSourceEventOrdinal::new(item.source_event_count())?;
    let source_index = required::<ItemSourceEventsFamily>(
        reader,
        &ItemEventKey {
            owner: item.id(),
            ordinal: source_ordinal,
        },
    )?;
    if source_index.item_id() != item.id()
        || source_index.turn_id() != item.turn_id()
        || source_index.source_event()
            != item
                .source_event()
                .ok_or(SyndicMutationError::CanonicalItemConflict)?
    {
        return Err(SyndicMutationError::CanonicalItemConflict);
    }
    Ok(())
}

pub(super) fn next_item_source_ordinal(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &CanonicalItemRecord,
) -> Result<ItemSourceEventOrdinal, SyndicMutationError> {
    let count = item
        .source_event_count()
        .checked_add(1)
        .ok_or(SyndicMutationError::SourceEventFrontierExhausted)?;
    let ordinal = ItemSourceEventOrdinal::new(count)?;
    ensure_new_item_source_event(reader, item.id(), ordinal)?;
    Ok(ordinal)
}

pub(super) fn assistant_phase_for_frame(
    kind: ProviderItemKind,
    observation: ProviderFrameObservationSummaryV1,
    observed: Option<ProviderMessagePhaseV1>,
    current: Option<AssistantMessagePhase>,
) -> Result<Option<AssistantMessagePhase>, SyndicMutationError> {
    if kind != ProviderItemKind::AgentMessage {
        return if observed.is_none() && current.is_none() {
            Ok(None)
        } else {
            Err(SyndicMutationError::AssistantPhaseConflict)
        };
    }
    if matches!(observation, ProviderFrameObservationSummaryV1::Delta) {
        return current
            .map(Some)
            .ok_or(SyndicMutationError::AssistantPhaseConflict);
    }
    let supplied = match observed {
        Some(ProviderMessagePhaseV1::Commentary) => AssistantMessagePhase::Commentary,
        Some(ProviderMessagePhaseV1::FinalAnswer) => AssistantMessagePhase::FinalAnswer,
        None => AssistantMessagePhase::Unknown,
    };
    match (current, supplied) {
        (None, supplied) => Ok(Some(supplied)),
        (Some(AssistantMessagePhase::Unknown), supplied) => Ok(Some(supplied)),
        (Some(known), AssistantMessagePhase::Unknown) => Ok(Some(known)),
        (Some(left), right) if left == right => Ok(Some(left)),
        (Some(_), _) => Err(SyndicMutationError::AssistantPhaseConflict),
    }
}

pub(super) const fn is_visible(presentation: &CanonicalItemPresentation) -> bool {
    matches!(
        presentation,
        CanonicalItemPresentation::UserInput { .. }
            | CanonicalItemPresentation::Narrative
            | CanonicalItemPresentation::GeneratedMedia { .. }
    )
}

pub(super) fn generated_media_resource_id(
    thread_id: SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
    item_id: SyndicItemId,
) -> SyndicResourceId {
    let mut hash = Sha256::new();
    hash.update(GENERATED_MEDIA_RESOURCE_V1);
    hash.update(thread_id.as_bytes());
    hash.update(turn_id.as_bytes());
    hash.update(item_id.as_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest[..16]);
    SyndicResourceId::from_bytes(id)
}

pub(super) const fn became_history_blocking(
    prior: Option<&SealedProviderFrameReference>,
    target: &SealedProviderFrameReference,
    completion_mismatch: bool,
) -> bool {
    let was_blocking = match prior {
        Some(prior) => !prior.history_support().is_supported(),
        None => false,
    };
    let is_blocking = !target.history_support().is_supported() || completion_mismatch;
    !was_blocking && is_blocking
}

fn cas_item_key(source: &CasItemSource) -> CasItemKey {
    CasItemKey::Record(
        source.turn().thread_id().clone(),
        source.turn().turn_id().clone(),
        source.item_id().clone(),
    )
}
