use beryl_home_store::DomainReader;
use beryl_model::{ProjectionRevision, SyndicItemId, SyndicThreadId};

use crate::mutation::{point, required};
use crate::{
    CanonicalItemPresentation, CanonicalItemRecord, ItemProjectionBuildRecord,
    ItemProjectionHeadRecord, ItemSourceEventOrdinal, ProviderItemKind, ProviderItemLifecycle,
    ProviderNarrativeCompletionDisposition, ResourceMetadataRecord, SealedProviderFrameReference,
    SourceEventRecord, SyndicMutationError, TurnItemOrdinal, codec::*, domain::SyndicDomain,
};

mod effect;
mod helpers;
mod publication;

pub(super) use effect::ItemEffect;
use helpers::{
    assistant_phase_for_frame, became_history_blocking, ensure_new_cas_item,
    ensure_new_item_source_event, ensure_new_turn_item, exact_item_source,
    generated_media_resource_id, is_visible, next_item_source_ordinal, require_current_indexes,
    require_turn_item_index,
};
use publication::{publication_manifest, validate_build_identity, validate_structural_frame};

pub(super) struct PublishedItemFrame {
    pub(super) effect: ItemEffect,
    pub(super) added_item: bool,
    pub(super) opened_item: bool,
    pub(super) completed_item: bool,
    pub(super) history_became_blocking: bool,
    pub(super) transcript_dirty: bool,
}

struct CanonicalFrameEffect {
    item: CanonicalItemRecord,
    source_ordinal: ItemSourceEventOrdinal,
    resource: Option<ResourceMetadataRecord>,
    projection_build: Option<ItemProjectionBuildRecord>,
    projection_head: Option<ItemProjectionHeadRecord>,
    added_item: bool,
    opened_item: bool,
    completed_item: bool,
    transcript_dirty: bool,
}

pub(super) fn publish_item_frame(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    thread_id: SyndicThreadId,
    current_item_count: u64,
    item_id: SyndicItemId,
    frame: &SealedProviderFrameReference,
) -> Result<PublishedItemFrame, SyndicMutationError> {
    let build = required::<ProviderItemBuildsFamily>(reader, &item_id)?;
    validate_build_identity(event, item_id, frame, &build)?;
    let structural = validate_structural_frame(reader, &build)?;
    let narrative_completion = build
        .completion_check()
        .and_then(|check| check.disposition());
    let manifest = publication_manifest(reader, &build)?;
    let source = exact_item_source(event, frame)?;
    let canonical = match build.prior() {
        Some(prior) => publish_subsequent_frame(
            reader,
            event,
            thread_id,
            item_id,
            prior,
            frame,
            &source,
            &structural,
            narrative_completion,
        )?,
        None => publish_first_frame(
            reader,
            event,
            thread_id,
            current_item_count,
            item_id,
            frame,
            &source,
            &structural,
        )?,
    };
    let completion_mismatch =
        narrative_completion.is_some_and(ProviderNarrativeCompletionDisposition::is_mismatch);
    let history_became_blocking =
        became_history_blocking(build.prior(), frame, completion_mismatch);
    let mut effect = ItemEffect::new(
        canonical.item,
        source,
        canonical.source_ordinal,
        event.sequence(),
        manifest,
        canonical.resource,
    );
    effect.set_projection_invalidation(canonical.projection_build, canonical.projection_head);
    Ok(PublishedItemFrame {
        effect,
        added_item: canonical.added_item,
        opened_item: canonical.opened_item,
        completed_item: canonical.completed_item,
        history_became_blocking,
        transcript_dirty: canonical.transcript_dirty,
    })
}

#[allow(clippy::too_many_arguments)]
fn publish_first_frame(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    thread_id: SyndicThreadId,
    current_item_count: u64,
    item_id: SyndicItemId,
    frame: &SealedProviderFrameReference,
    source: &crate::CasItemSource,
    structural: &crate::ProviderFrameStructuralValidationV1,
) -> Result<CanonicalFrameEffect, SyndicMutationError> {
    ensure_new_cas_item(reader, source)?;
    ensure_new_item_source_event(reader, item_id, ItemSourceEventOrdinal::FIRST)?;
    if frame.frame().item_kind() == ProviderItemKind::UserMessage {
        return correlate_user_input(reader, event, item_id, frame, source, structural);
    }
    if point::<CanonicalItemsFamily>(reader, &item_id)?.is_some() {
        return Err(SyndicMutationError::CanonicalItemConflict);
    }
    let new_ordinal = current_item_count
        .checked_add(1)
        .ok_or(SyndicMutationError::CanonicalItemConflict)
        .and_then(|value| TurnItemOrdinal::new(value).map_err(Into::into))?;
    ensure_new_turn_item(reader, event.turn_id(), new_ordinal)?;
    let revision = ProjectionRevision::new(1).expect("first canonical item revision");
    let (presentation, resource) = first_presentation(
        reader,
        thread_id,
        event.turn_id(),
        item_id,
        revision,
        frame.frame().item_kind(),
    )?;
    validate_structural_publication_facts(frame, structural, &presentation, None)?;
    let assistant_phase = assistant_phase_for_frame(
        frame.frame().item_kind(),
        frame.observation(),
        structural.message_phase(),
        None,
    )?;
    let item = CanonicalItemRecord::with_provider_state(
        item_id,
        event.turn_id(),
        new_ordinal,
        revision,
        event.sequence(),
        1,
        source.clone(),
        assistant_phase,
        frame.clone(),
        None,
        presentation,
    )?;
    let transcript_dirty = is_visible(item.presentation());
    Ok(CanonicalFrameEffect {
        item,
        source_ordinal: ItemSourceEventOrdinal::FIRST,
        resource,
        projection_build: None,
        projection_head: None,
        added_item: true,
        opened_item: !frame.stream_state().is_complete(),
        completed_item: false,
        transcript_dirty,
    })
}

fn correlate_user_input(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    item_id: SyndicItemId,
    frame: &SealedProviderFrameReference,
    source: &crate::CasItemSource,
    structural: &crate::ProviderFrameStructuralValidationV1,
) -> Result<CanonicalFrameEffect, SyndicMutationError> {
    let current = required::<CanonicalItemsFamily>(reader, &item_id)?;
    if current.id() != item_id
        || current.turn_id() != event.turn_id()
        || current.provider_kind() != ProviderItemKind::UserMessage
        || current.provider_lifecycle() != ProviderItemLifecycle::AwaitingCorrelation
        || current.source_event().is_some()
        || current.source_event_count() != 0
        || current.cas_source().is_some()
        || current.provider().is_some()
        || current.assistant_phase().is_some()
    {
        return Err(SyndicMutationError::ProviderItemLifecycleConflict);
    }
    require_turn_item_index(reader, &current)?;
    validate_structural_publication_facts(
        frame,
        structural,
        current.presentation(),
        Some(&current),
    )?;
    let (projection_build, projection_head) = invalidate_text_projection(reader, &current)?;
    let revision = current.revision().checked_next()?;
    let next = CanonicalItemRecord::with_provider_state(
        current.id(),
        current.turn_id(),
        current.ordinal(),
        revision,
        event.sequence(),
        1,
        source.clone(),
        None,
        frame.clone(),
        None,
        current.presentation().clone(),
    )?;
    Ok(CanonicalFrameEffect {
        item: next,
        source_ordinal: ItemSourceEventOrdinal::FIRST,
        resource: None,
        projection_build,
        projection_head,
        added_item: false,
        opened_item: false,
        completed_item: false,
        transcript_dirty: true,
    })
}

#[allow(clippy::too_many_arguments)]
fn publish_subsequent_frame(
    reader: &DomainReader<'_, SyndicDomain>,
    event: &SourceEventRecord,
    thread_id: SyndicThreadId,
    item_id: SyndicItemId,
    prior: &SealedProviderFrameReference,
    frame: &SealedProviderFrameReference,
    source: &crate::CasItemSource,
    structural: &crate::ProviderFrameStructuralValidationV1,
    narrative_completion: Option<ProviderNarrativeCompletionDisposition>,
) -> Result<CanonicalFrameEffect, SyndicMutationError> {
    let current = required::<CanonicalItemsFamily>(reader, &item_id)?;
    if current.id() != item_id
        || current.turn_id() != event.turn_id()
        || current.cas_source() != Some(source)
        || current.provider_kind() != frame.frame().item_kind()
        || current.provider_lifecycle() != ProviderItemLifecycle::Started
        || current.provider() != Some(prior)
    {
        return Err(SyndicMutationError::ProviderFrameBuildConflict);
    }
    require_current_indexes(reader, &current, source)?;
    validate_current_presentation(reader, thread_id, &current)?;
    validate_structural_publication_facts(
        frame,
        structural,
        current.presentation(),
        Some(&current),
    )?;
    let source_ordinal = next_item_source_ordinal(reader, &current)?;
    let assistant_phase = assistant_phase_for_frame(
        frame.frame().item_kind(),
        frame.observation(),
        structural.message_phase(),
        current.assistant_phase(),
    )?;
    let (projection_build, projection_head) = invalidate_text_projection(reader, &current)?;
    let next = CanonicalItemRecord::with_provider_state(
        current.id(),
        current.turn_id(),
        current.ordinal(),
        current.revision().checked_next()?,
        event.sequence(),
        source_ordinal.get(),
        source.clone(),
        assistant_phase,
        frame.clone(),
        narrative_completion,
        current.presentation().clone(),
    )?;
    Ok(CanonicalFrameEffect {
        transcript_dirty: is_visible(next.presentation()),
        item: next,
        source_ordinal,
        resource: None,
        projection_build,
        projection_head,
        added_item: false,
        opened_item: false,
        completed_item: frame.stream_state().is_complete(),
    })
}

fn first_presentation(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
    item_id: SyndicItemId,
    revision: ProjectionRevision,
    kind: ProviderItemKind,
) -> Result<(CanonicalItemPresentation, Option<ResourceMetadataRecord>), SyndicMutationError> {
    let presentation = match kind {
        ProviderItemKind::UserMessage => {
            return Err(SyndicMutationError::ProviderItemLifecycleConflict);
        }
        ProviderItemKind::AgentMessage | ProviderItemKind::Plan => {
            CanonicalItemPresentation::Narrative
        }
        ProviderItemKind::CommandExecution
        | ProviderItemKind::FileChange
        | ProviderItemKind::McpToolCall
        | ProviderItemKind::DynamicToolCall => CanonicalItemPresentation::Operational,
        ProviderItemKind::HookPrompt
        | ProviderItemKind::Reasoning
        | ProviderItemKind::CollabAgentToolCall
        | ProviderItemKind::SubAgentActivity
        | ProviderItemKind::WebSearch
        | ProviderItemKind::ImageView
        | ProviderItemKind::Sleep
        | ProviderItemKind::EnteredReviewMode
        | ProviderItemKind::ExitedReviewMode
        | ProviderItemKind::ContextCompaction => CanonicalItemPresentation::Activity,
        ProviderItemKind::StandaloneImageGeneration => {
            let resource_id = generated_media_resource_id(thread_id, turn_id, item_id);
            if point::<ResourcesFamily>(reader, &resource_id)?.is_some() {
                return Err(SyndicMutationError::GeneratedMediaResourceCollision);
            }
            let resource =
                ResourceMetadataRecord::pending_generated_media(resource_id, revision, item_id);
            return Ok((
                CanonicalItemPresentation::GeneratedMedia { resource_id },
                Some(resource),
            ));
        }
    };
    Ok((presentation, None))
}

fn validate_current_presentation(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    item: &CanonicalItemRecord,
) -> Result<(), SyndicMutationError> {
    let expected = match item.provider_kind() {
        ProviderItemKind::UserMessage => match item.presentation() {
            CanonicalItemPresentation::UserInput { .. } => return Ok(()),
            _ => return Err(SyndicMutationError::CanonicalItemConflict),
        },
        ProviderItemKind::AgentMessage | ProviderItemKind::Plan => {
            CanonicalItemPresentation::Narrative
        }
        ProviderItemKind::CommandExecution
        | ProviderItemKind::FileChange
        | ProviderItemKind::McpToolCall
        | ProviderItemKind::DynamicToolCall => CanonicalItemPresentation::Operational,
        ProviderItemKind::HookPrompt
        | ProviderItemKind::Reasoning
        | ProviderItemKind::CollabAgentToolCall
        | ProviderItemKind::SubAgentActivity
        | ProviderItemKind::WebSearch
        | ProviderItemKind::ImageView
        | ProviderItemKind::Sleep
        | ProviderItemKind::EnteredReviewMode
        | ProviderItemKind::ExitedReviewMode
        | ProviderItemKind::ContextCompaction => CanonicalItemPresentation::Activity,
        ProviderItemKind::StandaloneImageGeneration => {
            let resource_id = generated_media_resource_id(thread_id, item.turn_id(), item.id());
            let expected_resource = ResourceMetadataRecord::pending_generated_media(
                resource_id,
                ProjectionRevision::new(1).expect("first generated resource revision"),
                item.id(),
            );
            if required::<ResourcesFamily>(reader, &resource_id)? != expected_resource {
                return Err(SyndicMutationError::CanonicalItemConflict);
            }
            CanonicalItemPresentation::GeneratedMedia { resource_id }
        }
    };
    if item.presentation() != &expected {
        return Err(SyndicMutationError::CanonicalItemConflict);
    }
    Ok(())
}

fn validate_structural_publication_facts(
    frame: &SealedProviderFrameReference,
    structural: &crate::ProviderFrameStructuralValidationV1,
    presentation: &CanonicalItemPresentation,
    current: Option<&CanonicalItemRecord>,
) -> Result<(), SyndicMutationError> {
    if frame.frame().item_kind() == ProviderItemKind::UserMessage {
        let CanonicalItemPresentation::UserInput { content, .. } = presentation else {
            return Err(SyndicMutationError::CanonicalItemConflict);
        };
        if structural.submitted_content() != Some(*content) {
            return Err(SyndicMutationError::CanonicalItemConflict);
        }
    } else if structural.submitted_content().is_some() {
        return Err(SyndicMutationError::ProviderFrameValidationConflict);
    }
    if frame.frame().item_kind() != ProviderItemKind::AgentMessage
        && (structural.message_phase().is_some()
            || current.is_some_and(|item| item.assistant_phase().is_some()))
    {
        return Err(SyndicMutationError::AssistantPhaseConflict);
    }
    Ok(())
}

fn invalidate_text_projection(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &CanonicalItemRecord,
) -> Result<
    (
        Option<ItemProjectionBuildRecord>,
        Option<ItemProjectionHeadRecord>,
    ),
    SyndicMutationError,
> {
    if item.projection_source().is_some() {
        crate::mutation::projection::invalidate_item_projection(reader, item)
    } else {
        Ok((None, None))
    }
}
