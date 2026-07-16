use beryl_home_store::DomainReader;
use beryl_model::{ContentRevision, ProjectionRevision, SyndicItemId};

use crate::mutation::{point, required};
use crate::{
    CanonicalItemKind, CanonicalItemPayload, CanonicalItemRecord, ContentLifecycle,
    ContentManifestRecord, ItemProjectionBuildRecord, ItemProjectionHeadRecord,
    ItemSourceEventOrdinal, ProviderItemDisposition, ProviderItemKind, ProviderItemLifecycle,
    ResourceMetadataRecord, SourceEventRecord, SourceEventText, SourceItemDescriptor,
    SyndicMutationError, TurnItemOrdinal, codec::*, domain::SyndicDomain,
};

mod effect;
mod helpers;

pub(super) use effect::ItemEffect;
use helpers::{
    append_live_content, ensure_new_cas_item, ensure_new_item_source_event, ensure_new_turn_item,
    exact_item_source, merge_assistant_phase, sourced_item, validate_sourced_item,
};

pub(super) struct StartedItem {
    pub(super) effect: ItemEffect,
    pub(super) added_item: bool,
    pub(super) transcript_dirty: bool,
}

pub(super) struct CompletedItem {
    pub(super) effect: ItemEffect,
    pub(super) added_item: bool,
    pub(super) transcript_dirty: bool,
}

pub(super) fn start_item(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    new_ordinal: u64,
    descriptor: &SourceItemDescriptor,
    assistant_phase: Option<crate::AssistantMessagePhase>,
) -> Result<StartedItem, SyndicMutationError> {
    if descriptor.kind() == ProviderItemKind::UserMessage {
        return correlate_user_input(reader, event, descriptor);
    }
    if point::<CanonicalItemsFamily>(reader, &descriptor.item_id())?.is_some() {
        return Err(SyndicMutationError::CanonicalItemConflict);
    }
    let ordinal = TurnItemOrdinal::new(new_ordinal)?;
    ensure_new_turn_item(reader, event.turn_id(), ordinal)?;
    let source = exact_item_source(event.source(), descriptor.cas_item_id())?;
    ensure_new_cas_item(reader, &source)?;
    ensure_new_item_source_event(reader, descriptor.item_id(), ItemSourceEventOrdinal::FIRST)?;

    let revision = ProjectionRevision::new(1).expect("first canonical item revision");
    let (manifest, resource, payload) = initial_payload(reader, descriptor, revision)?;
    let item = CanonicalItemRecord::with_source_state(
        descriptor.item_id(),
        event.turn_id(),
        ordinal,
        revision,
        Some(event.sequence()),
        1,
        Some(source.clone()),
        descriptor.kind(),
        ProviderItemLifecycle::Started,
        descriptor.disposition(),
        assistant_phase,
        payload,
    )?;
    let transcript_dirty = is_transcript_visible(&item);
    Ok(StartedItem {
        transcript_dirty,
        added_item: true,
        effect: ItemEffect::new(
            item,
            source,
            ItemSourceEventOrdinal::FIRST,
            event.sequence(),
            manifest,
            resource,
        ),
    })
}

pub(super) fn append_item(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    item_id: SyndicItemId,
    cas_item_id: &beryl_model::CasItemId,
    expected_kind: ProviderItemKind,
    text: &SourceEventText,
) -> Result<(ItemEffect, bool), SyndicMutationError> {
    let item = sourced_item(reader, event, item_id, cas_item_id)?;
    if item.provider_kind() != expected_kind {
        return Err(SyndicMutationError::ProviderItemKindConflict);
    }
    if item.provider_lifecycle() != ProviderItemLifecycle::Started
        || item.disposition() != ProviderItemDisposition::CanonicalText
    {
        return Err(SyndicMutationError::ProviderItemLifecycleConflict);
    }
    let content = item
        .payload()
        .content()
        .ok_or(SyndicMutationError::CanonicalItemConflict)?;
    let (projection_build, projection_head) = invalidate_visible_projection(reader, &item)?;
    let current = required::<ContentManifestsFamily>(reader, &content.id())?;
    if current.owner() != Some(item.id())
        || current.lifecycle() != ContentLifecycle::Live
        || current.current_reference() != Some(content)
    {
        return Err(SyndicMutationError::CanonicalItemConflict);
    }
    let span_start = current.encoded_bytes();
    let (manifest, chunks) = append_live_content(&current, text.as_str())?;
    let spans = crate::content_byte_spans(&chunks, span_start)?;
    let text_spans = crate::utf8_content_text_spans(&chunks, span_start)?;
    let pieces = text_spans
        .iter()
        .copied()
        .map(crate::ContentPieceRecord::text)
        .collect();
    let content = manifest
        .current_reference()
        .ok_or(SyndicMutationError::ContentManifestConflict)?;
    let (next, source_ordinal) = advance_item(
        reader,
        event,
        &item,
        ProviderItemLifecycle::Started,
        item.assistant_phase(),
        CanonicalItemPayload::text(content),
    )?;
    let visible = is_transcript_visible(&next);
    let mut effect = ItemEffect::new(
        next,
        item.cas_source()
            .cloned()
            .ok_or(SyndicMutationError::SourceIdentityConflict)?,
        source_ordinal,
        event.sequence(),
        Some(manifest),
        None,
    );
    effect.set_content_parts(chunks, spans, text_spans, pieces);
    effect.set_projection_invalidation(projection_build, projection_head);
    Ok((effect, visible))
}

pub(super) fn complete_item(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    new_ordinal: u64,
    descriptor: &SourceItemDescriptor,
    assistant_phase: Option<crate::AssistantMessagePhase>,
) -> Result<CompletedItem, SyndicMutationError> {
    let Some(item) = point::<CanonicalItemsFamily>(reader, &descriptor.item_id())? else {
        return complete_instantaneous_item(
            reader,
            event,
            new_ordinal,
            descriptor,
            assistant_phase,
        );
    };
    let source = exact_item_source(event.source(), descriptor.cas_item_id())?;
    validate_sourced_item(reader, event, &item, &source)?;
    if item.provider_kind() != descriptor.kind() {
        return Err(SyndicMutationError::ProviderItemKindConflict);
    }
    if item.provider_lifecycle() != ProviderItemLifecycle::Started
        || item.disposition() != descriptor.disposition()
    {
        return Err(SyndicMutationError::ProviderItemLifecycleConflict);
    }
    let phase = merge_assistant_phase(item.assistant_phase(), assistant_phase)?;
    let (projection_build, projection_head) = invalidate_visible_projection(reader, &item)?;
    let (manifest, payload) = complete_payload(reader, &item)?;
    let (next, source_ordinal) = advance_item(
        reader,
        event,
        &item,
        ProviderItemLifecycle::Completed,
        phase,
        payload,
    )?;
    let visible = is_transcript_visible(&next);
    let mut effect = ItemEffect::new(
        next,
        source,
        source_ordinal,
        event.sequence(),
        manifest,
        None,
    );
    effect.set_projection_invalidation(projection_build, projection_head);
    Ok(CompletedItem {
        effect,
        added_item: false,
        transcript_dirty: visible,
    })
}

fn correlate_user_input(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    descriptor: &SourceItemDescriptor,
) -> Result<StartedItem, SyndicMutationError> {
    let item = required::<CanonicalItemsFamily>(reader, &descriptor.item_id())?;
    if item.turn_id() != event.turn_id()
        || item.provider_kind() != ProviderItemKind::UserMessage
        || item.provider_lifecycle() != ProviderItemLifecycle::AwaitingCorrelation
        || item.disposition() != descriptor.disposition()
        || item.source_event_count() != 0
        || item.cas_source().is_some()
    {
        return Err(SyndicMutationError::ProviderItemLifecycleConflict);
    }
    let source = exact_item_source(event.source(), descriptor.cas_item_id())?;
    ensure_new_cas_item(reader, &source)?;
    ensure_new_item_source_event(reader, item.id(), ItemSourceEventOrdinal::FIRST)?;
    let (projection_build, projection_head) = invalidate_visible_projection(reader, &item)?;
    let revision = item.revision().checked_next()?;
    let next = CanonicalItemRecord::with_source_state(
        item.id(),
        item.turn_id(),
        item.ordinal(),
        revision,
        Some(event.sequence()),
        1,
        Some(source.clone()),
        item.provider_kind(),
        ProviderItemLifecycle::Started,
        item.disposition(),
        None,
        item.payload().clone(),
    )?;
    let mut effect = ItemEffect::new(
        next,
        source,
        ItemSourceEventOrdinal::FIRST,
        event.sequence(),
        None,
        None,
    );
    effect.set_projection_invalidation(projection_build, projection_head);
    Ok(StartedItem {
        effect,
        added_item: false,
        transcript_dirty: true,
    })
}

fn complete_instantaneous_item(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    new_ordinal: u64,
    descriptor: &SourceItemDescriptor,
    assistant_phase: Option<crate::AssistantMessagePhase>,
) -> Result<CompletedItem, SyndicMutationError> {
    if !descriptor.kind().permits_completion_only()
        || descriptor.disposition() != ProviderItemDisposition::ActivityOnly
        || assistant_phase.is_some()
    {
        return Err(SyndicMutationError::ProviderItemLifecycleConflict);
    }
    let ordinal = TurnItemOrdinal::new(new_ordinal)?;
    ensure_new_turn_item(reader, event.turn_id(), ordinal)?;
    let source = exact_item_source(event.source(), descriptor.cas_item_id())?;
    ensure_new_cas_item(reader, &source)?;
    ensure_new_item_source_event(reader, descriptor.item_id(), ItemSourceEventOrdinal::FIRST)?;
    let revision = ProjectionRevision::new(1).expect("first canonical item revision");
    let item = CanonicalItemRecord::with_source_state(
        descriptor.item_id(),
        event.turn_id(),
        ordinal,
        revision,
        Some(event.sequence()),
        1,
        Some(source.clone()),
        descriptor.kind(),
        ProviderItemLifecycle::Completed,
        descriptor.disposition(),
        None,
        CanonicalItemPayload::activity(),
    )?;
    Ok(CompletedItem {
        effect: ItemEffect::new(
            item,
            source,
            ItemSourceEventOrdinal::FIRST,
            event.sequence(),
            None,
            None,
        ),
        added_item: true,
        transcript_dirty: false,
    })
}

fn initial_payload(
    reader: &DomainReader<'_, SyndicDomain>,
    descriptor: &SourceItemDescriptor,
    revision: ProjectionRevision,
) -> Result<
    (
        Option<ContentManifestRecord>,
        Option<ResourceMetadataRecord>,
        CanonicalItemPayload,
    ),
    SyndicMutationError,
> {
    match descriptor.disposition() {
        ProviderItemDisposition::CanonicalText => {
            let content_id = crate::content::live_item_content_id(descriptor.item_id());
            if point::<ContentManifestsFamily>(reader, &content_id)?.is_some() {
                return Err(SyndicMutationError::CanonicalItemConflict);
            }
            let manifest = ContentManifestRecord::live(
                content_id,
                descriptor.item_id(),
                ContentRevision::new(1).expect("first live content revision"),
            );
            let content = manifest
                .current_reference()
                .ok_or(SyndicMutationError::ContentManifestConflict)?;
            Ok((Some(manifest), None, CanonicalItemPayload::text(content)))
        }
        ProviderItemDisposition::ActivityOnly => Ok((None, None, CanonicalItemPayload::activity())),
        ProviderItemDisposition::GeneratedMedia { resource_id } => {
            if point::<ResourcesFamily>(reader, &resource_id)?.is_some() {
                return Err(SyndicMutationError::CanonicalItemConflict);
            }
            let resource = ResourceMetadataRecord::pending_generated_media(
                resource_id,
                revision,
                descriptor.item_id(),
            );
            Ok((
                None,
                Some(resource),
                CanonicalItemPayload::generated_media(resource_id),
            ))
        }
        ProviderItemDisposition::Unsupported(reason) => {
            Ok((None, None, CanonicalItemPayload::unsupported(reason)))
        }
        ProviderItemDisposition::CorrelatedUserInput { .. } => {
            Err(SyndicMutationError::ProviderItemLifecycleConflict)
        }
    }
}

fn complete_payload(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &CanonicalItemRecord,
) -> Result<(Option<ContentManifestRecord>, CanonicalItemPayload), SyndicMutationError> {
    let Some(content) = item.payload().content() else {
        return Ok((None, item.payload().clone()));
    };
    if matches!(
        item.disposition(),
        ProviderItemDisposition::CorrelatedUserInput { .. }
    ) {
        return Ok((None, item.payload().clone()));
    }
    let current = required::<ContentManifestsFamily>(reader, &content.id())?;
    if current.owner() != Some(item.id())
        || current.lifecycle() != ContentLifecycle::Live
        || current.current_reference() != Some(content)
    {
        return Err(SyndicMutationError::CanonicalItemConflict);
    }
    let manifest = ContentManifestRecord::with_owner(
        current.id(),
        current.owner(),
        current.revision().checked_next()?,
        current.encoding(),
        ContentLifecycle::Finalized,
        current.chunk_count(),
        current.encoded_bytes(),
        current.chain_digest(),
        current.expected(),
    );
    let content = manifest
        .current_reference()
        .ok_or(SyndicMutationError::ContentManifestConflict)?;
    Ok((Some(manifest), CanonicalItemPayload::text(content)))
}

fn advance_item(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    current: &CanonicalItemRecord,
    lifecycle: ProviderItemLifecycle,
    assistant_phase: Option<crate::AssistantMessagePhase>,
    payload: CanonicalItemPayload,
) -> Result<(CanonicalItemRecord, ItemSourceEventOrdinal), SyndicMutationError> {
    let revision = current.revision().checked_next()?;
    let source_count = current
        .source_event_count()
        .checked_add(1)
        .ok_or(SyndicMutationError::SourceEventFrontierExhausted)?;
    let source_ordinal = ItemSourceEventOrdinal::new(source_count)?;
    ensure_new_item_source_event(reader, current.id(), source_ordinal)?;
    let item = CanonicalItemRecord::with_source_state(
        current.id(),
        current.turn_id(),
        current.ordinal(),
        revision,
        Some(event.sequence()),
        source_count,
        current.cas_source().cloned(),
        current.provider_kind(),
        lifecycle,
        current.disposition(),
        assistant_phase,
        payload,
    )?;
    Ok((item, source_ordinal))
}

fn invalidate_visible_projection(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &CanonicalItemRecord,
) -> Result<
    (
        Option<ItemProjectionBuildRecord>,
        Option<ItemProjectionHeadRecord>,
    ),
    SyndicMutationError,
> {
    if is_transcript_visible(item) {
        crate::mutation::projection::invalidate_item_projection(reader, item)
    } else {
        Ok((None, None))
    }
}

fn is_transcript_visible(item: &CanonicalItemRecord) -> bool {
    matches!(
        item.kind(),
        CanonicalItemKind::UserInput | CanonicalItemKind::AssistantMessage(_)
    )
}
