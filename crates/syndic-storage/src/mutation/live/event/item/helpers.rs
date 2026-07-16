use beryl_home_store::DomainReader;
use beryl_model::SyndicItemId;

use crate::mutation::{point, required};
use crate::{
    CanonicalItemRecord, CasItemSource, CasTurnSource, ContentChunkOrdinal, ContentChunkRecord,
    ContentEncoding, ContentLifecycle, ContentManifestRecord, ContentSummary,
    ItemSourceEventOrdinal, SourceEventRecord, SyndicMutationError, TurnItemOrdinal,
    advance_content_chain, codec::*, domain::SyndicDomain,
};

pub(super) fn sourced_item(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    item_id: SyndicItemId,
    cas_item_id: &beryl_model::CasItemId,
) -> Result<CanonicalItemRecord, SyndicMutationError> {
    let item = required::<CanonicalItemsFamily>(reader, &item_id)?;
    let source = exact_item_source(event.source(), cas_item_id)?;
    validate_sourced_item(reader, event, &item, &source)?;
    Ok(item)
}

pub(super) fn validate_sourced_item(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    item: &CanonicalItemRecord,
    source: &CasItemSource,
) -> Result<(), SyndicMutationError> {
    if item.turn_id() != event.turn_id() || item.cas_source() != Some(source) {
        return Err(SyndicMutationError::SourceIdentityConflict);
    }
    let index = required::<CasItemIndexFamily>(
        reader,
        &CasItemKey::Record(
            source.turn().thread_id().clone(),
            source.turn().turn_id().clone(),
            source.item_id().clone(),
        ),
    )?;
    if index.item_id() != item.id() || index.item_revision() != item.revision() {
        return Err(SyndicMutationError::SourceIdentityConflict);
    }
    Ok(())
}

pub(super) fn exact_item_source(
    turn: Option<&CasTurnSource>,
    item: &beryl_model::CasItemId,
) -> Result<CasItemSource, SyndicMutationError> {
    let turn = turn.ok_or(SyndicMutationError::SourceIdentityConflict)?;
    Ok(CasItemSource::new(turn.clone(), item.clone()))
}

pub(super) fn ensure_new_cas_item(
    reader: &DomainReader<'_, SyndicDomain>,
    source: &CasItemSource,
) -> Result<(), SyndicMutationError> {
    if point::<CasItemIndexFamily>(
        reader,
        &CasItemKey::Record(
            source.turn().thread_id().clone(),
            source.turn().turn_id().clone(),
            source.item_id().clone(),
        ),
    )?
    .is_some()
    {
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

pub(super) fn merge_assistant_phase(
    current: Option<crate::AssistantMessagePhase>,
    supplied: Option<crate::AssistantMessagePhase>,
) -> Result<Option<crate::AssistantMessagePhase>, SyndicMutationError> {
    match (current, supplied) {
        (None, None) => Ok(None),
        (Some(crate::AssistantMessagePhase::Unknown), Some(observed)) => Ok(Some(observed)),
        (Some(known), Some(crate::AssistantMessagePhase::Unknown)) => Ok(Some(known)),
        (Some(left), Some(right)) if left == right => Ok(Some(left)),
        (Some(known), None) => Ok(Some(known)),
        _ => Err(SyndicMutationError::AssistantPhaseConflict),
    }
}

pub(super) fn append_live_content(
    current: &ContentManifestRecord,
    text: &str,
) -> Result<(ContentManifestRecord, Vec<ContentChunkRecord>), SyndicMutationError> {
    let mut chunks = Vec::new();
    let mut chain = current.chain_digest();
    let mut next_ordinal = current.chunk_count();
    for bytes in crate::content::utf8_chunks(text) {
        next_ordinal = next_ordinal
            .checked_add(1)
            .ok_or(SyndicMutationError::CanonicalItemConflict)?;
        let chunk =
            ContentChunkRecord::new(current.id(), ContentChunkOrdinal::new(next_ordinal)?, bytes)?;
        chain = advance_content_chain(chain, &chunk);
        chunks.push(chunk);
    }
    let text_bytes =
        u64::try_from(text.len()).map_err(|_| crate::SyndicRecordError::LengthOverflow {
            kind: "source-event text",
        })?;
    let encoded_bytes = current
        .encoded_bytes()
        .checked_add(text_bytes)
        .ok_or(SyndicMutationError::CanonicalItemConflict)?;
    let summary = ContentSummary::new(
        next_ordinal,
        next_ordinal,
        encoded_bytes,
        encoded_bytes,
        1,
        0,
        crate::content::input_marker_digest(std::iter::empty()),
        chain,
    );
    Ok((
        ContentManifestRecord::with_owner(
            current.id(),
            current.owner(),
            current.revision().checked_next()?,
            ContentEncoding::Utf8V1,
            ContentLifecycle::Live,
            next_ordinal,
            encoded_bytes,
            chain,
            summary,
        ),
        chunks,
    ))
}
