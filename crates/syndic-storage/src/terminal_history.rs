use beryl_home_store::{DomainReader, ReadError};

use crate::{
    CanonicalItemPresentation, ContentLifecycle, GeneratedMediaResourceDisposition,
    ProjectionLifecycle, ProjectionTextSource, ProviderItemLifecycle, ResourceBacking,
    TranscriptBuildPhase, codec::*, domain::SyndicDomain,
};

#[derive(Clone, Copy)]
pub(crate) struct ExpectedTerminalTranscript {
    generation: crate::TranscriptGeneration,
    revision: beryl_model::ProjectionRevision,
}

impl ExpectedTerminalTranscript {
    pub(crate) const fn new(
        generation: crate::TranscriptGeneration,
        revision: beryl_model::ProjectionRevision,
    ) -> Self {
        Self {
            generation,
            revision,
        }
    }
}

pub(crate) fn is_complete(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    state: &crate::TurnStateRecord,
    expected_transcript: Option<ExpectedTerminalTranscript>,
) -> Result<bool, ReadError> {
    if thread.committed_tail() != Some(state.turn_id()) || !state.lifecycle().is_proven_terminal() {
        return Ok(false);
    }
    let Some(turn) = point::<TurnsFamily>(reader, &state.turn_id())? else {
        return Ok(false);
    };
    let Some(summary) = point::<HistorySummariesFamily>(reader, &thread.id())? else {
        return Ok(false);
    };
    let Some(head) = point::<TranscriptHeadsFamily>(reader, &thread.id())? else {
        return Ok(false);
    };
    if turn.origin_thread_id() != thread.id()
        || expected_transcript.is_some_and(|expected| {
            head.generation() != expected.generation || head.revision() != expected.revision
        })
    {
        return Ok(false);
    }
    let Some(build) = point::<TranscriptBuildsFamily>(
        reader,
        &ThreadTranscriptBuildKey {
            thread: thread.id(),
            generation: head.generation(),
        },
    )?
    else {
        return Ok(false);
    };
    if summary.thread_id() != thread.id()
        || summary.thread_revision() != thread.revision()
        || summary.committed_tail() != thread.committed_tail()
        || summary.selected_path_digest() != thread.selected_path_digest()
        || summary.complete() != build.history_complete()
        || head.thread_id() != thread.id()
        || head.lifecycle() != ProjectionLifecycle::Current
        || head.committed_tail() != thread.committed_tail()
        || head.selected_path_digest() != thread.selected_path_digest()
        || build.thread_id() != thread.id()
        || build.generation() != head.generation()
        || build.revision() != head.revision()
        || build.source_thread_revision() > thread.revision()
        || build.committed_tail() != thread.committed_tail()
        || build.selected_path_digest() != thread.selected_path_digest()
        || build.entry_count() != head.entry_count()
        || !matches!(build.phase(), TranscriptBuildPhase::Complete)
    {
        return Ok(false);
    }
    item_frontier_is_settled(reader, state)
}

fn item_frontier_is_settled(
    reader: &DomainReader<'_, SyndicDomain>,
    state: &crate::TurnStateRecord,
) -> Result<bool, ReadError> {
    if state.finalized_item_count() > state.item_count() {
        return Ok(false);
    }
    if state.finalized_item_count() == state.item_count() {
        return Ok(true);
    }
    let Some(ordinal) = state
        .finalized_item_count()
        .checked_add(1)
        .and_then(|value| crate::TurnItemOrdinal::new(value).ok())
    else {
        return Ok(false);
    };
    let Some(index) = point::<TurnItemsFamily>(
        reader,
        &TurnItemKey {
            owner: state.turn_id(),
            ordinal,
        },
    )?
    else {
        return Ok(false);
    };
    let Some(item) = point::<CanonicalItemsFamily>(reader, &index.item_id())? else {
        return Ok(false);
    };
    if index.turn_id() != state.turn_id()
        || index.ordinal() != ordinal
        || item.id() != index.item_id()
        || item.turn_id() != state.turn_id()
        || item.ordinal() != ordinal
        || item.revision() != index.item_revision()
        || !item_content_is_settled(reader, &item)?
    {
        return Ok(false);
    }
    if item.provider_lifecycle() != ProviderItemLifecycle::Completed {
        return Ok(true);
    }
    let CanonicalItemPresentation::GeneratedMedia { resource_id } = item.presentation() else {
        return Ok(false);
    };
    let Some(resource) = point::<ResourcesFamily>(reader, resource_id)? else {
        return Ok(false);
    };
    if resource.id() != *resource_id || resource.item_id() != item.id() {
        return Ok(false);
    }
    Ok(matches!(
        resource.backing(),
        ResourceBacking::GeneratedMedia(
            GeneratedMediaResourceDisposition::PendingAsset
                | GeneratedMediaResourceDisposition::Unavailable(_)
        )
    ))
}

fn item_content_is_settled(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
) -> Result<bool, ReadError> {
    let provider_manifest = match item.provider_content() {
        Some(content) => {
            let Some(manifest) = point::<ContentManifestsFamily>(reader, &content.id())? else {
                return Ok(false);
            };
            if manifest.owner() != Some(item.id())
                || manifest.current_reference() != Some(content)
                || !matches!(
                    manifest.lifecycle(),
                    ContentLifecycle::Live | ContentLifecycle::Finalized
                )
            {
                return Ok(false);
            }
            Some(manifest)
        }
        None => None,
    };
    let projection_is_settled = match item.projection_source() {
        Some(ProjectionTextSource::Composer(content)) => {
            let Some(manifest) = point::<ContentManifestsFamily>(reader, &content.id())? else {
                return Ok(false);
            };
            manifest.lifecycle() == ContentLifecycle::Sealed
                && manifest.sealed_reference() == Some(content)
        }
        Some(ProjectionTextSource::ProviderNarrative(narrative)) => {
            provider_manifest.as_ref().is_some_and(|manifest| {
                manifest.id() == narrative.content_id()
                    && manifest.current_reference().is_some_and(|content| {
                        content.summary().logical_utf8_bytes() == narrative.logical_utf8_bytes()
                    })
            })
        }
        None => true,
    };
    let completed_content_is_settled = item.provider_lifecycle()
        != ProviderItemLifecycle::Completed
        || match item.presentation() {
            CanonicalItemPresentation::GeneratedMedia { .. } => provider_manifest
                .as_ref()
                .is_none_or(|manifest| manifest.lifecycle() == ContentLifecycle::Finalized),
            _ => provider_manifest
                .as_ref()
                .is_some_and(|manifest| manifest.lifecycle() == ContentLifecycle::Finalized),
        };
    Ok(projection_is_settled && completed_content_is_settled)
}

fn point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, ReadError> {
    reader.point::<ExactCodec<F>>(key, crate::codec::family_point_limit::<F>())
}
