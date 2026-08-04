use beryl_model::ProjectionRevision;

use super::*;
use crate::mutation::required;
use crate::{
    CanonicalItemRecord, CasItemIndexRecord, ContentLifecycle, ContentManifestRecord,
    HistorySummaryRecord, ItemProjectionHeadRecord, SealedProviderFrameReference,
    TranscriptViewHeadRecord, TurnItemIndexRecord, TurnStateRecord,
};

pub(super) struct FreezeRecords {
    state: TurnStateRecord,
    summary: Option<HistorySummaryRecord>,
    changed: FrozenItem,
    transcript_head: Option<TranscriptViewHeadRecord>,
    transcript_build: Option<crate::TranscriptBuildRecord>,
    item_head: Option<ItemProjectionHeadRecord>,
    projection_build: Option<crate::ItemProjectionBuildRecord>,
}

struct FrozenItem {
    manifest: ContentManifestRecord,
    item: CanonicalItemRecord,
    index: TurnItemIndexRecord,
    cas_index: Option<CasItemIndexRecord>,
}

impl FreezeNextTurnItemMutation {
    pub(super) fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<FreezeRecords, SyndicMutationError> {
        let request = self.request;
        let thread = required::<ThreadsFamily>(reader, &request.thread_id)?;
        let turn = required::<TurnsFamily>(reader, &request.turn_id)?;
        let current = required::<TurnStatesFamily>(reader, &request.turn_id)?;
        let summary = required::<HistorySummariesFamily>(reader, &request.thread_id)?;
        if turn.origin_thread_id() != thread.id() || current.turn_id() != turn.id() {
            return Err(SyndicMutationError::LiveTurnConflict);
        }
        if current.revision() != request.expected_state_revision {
            return Err(SyndicMutationError::TurnStateRevisionConflict {
                expected: request.expected_state_revision,
                current: current.revision(),
            });
        }
        if !current.lifecycle().is_proven_terminal()
            || current.finalized_item_count() >= current.item_count()
            || request.expected_item_ordinal.get()
                != current.finalized_item_count().saturating_add(1)
            || request.updated_at < current.updated_at()
        {
            return Err(SyndicMutationError::CanonicalFinalizationConflict);
        }
        let index = required::<TurnItemsFamily>(
            reader,
            &TurnItemKey {
                owner: turn.id(),
                ordinal: request.expected_item_ordinal,
            },
        )?;
        if index.item_id() != request.expected_item_id {
            return Err(SyndicMutationError::CanonicalFinalizationConflict);
        }
        let item = required::<CanonicalItemsFamily>(reader, &request.expected_item_id)?;
        if item.turn_id() != turn.id()
            || item.ordinal() != request.expected_item_ordinal
            || item.revision() != index.item_revision()
        {
            return Err(SyndicMutationError::CanonicalFinalizationConflict);
        }
        let content = item
            .provider_content()
            .ok_or(SyndicMutationError::CanonicalFinalizationConflict)?;
        let manifest = required::<ContentManifestsFamily>(reader, &content.id())?;
        if manifest.lifecycle() != ContentLifecycle::Live {
            return Err(SyndicMutationError::CanonicalFinalizationConflict);
        }
        let visible = item.projection_source().is_some();
        let selected = crate::mutation::turn_is_on_selected_path(reader, &thread, &turn)?;
        let (projection_build, item_head) = if visible {
            crate::mutation::projection::invalidate_item_projection(reader, &item)?
        } else {
            (None, None)
        };
        let changed = freeze_item(manifest, item)?;
        let state = TurnStateRecord::with_capture_frontiers_and_issue(
            current.turn_id(),
            current.revision().checked_next()?,
            current.lifecycle(),
            current.source_event_count(),
            current.item_count(),
            current.finalized_item_count(),
            current.open_item_count(),
            current.history_blocking_item_count(),
            current.provider_observation_issue(),
            current.end_status(),
            request.updated_at,
        )?;
        let (transcript_head, transcript_build) = if visible && selected {
            crate::mutation::transcript::invalidate_transcript_projection(reader, &thread)?
        } else {
            (None, None)
        };
        let next_activity = summary.last_activity_at().max(request.updated_at);
        let summary =
            if selected && (summary.complete() || next_activity != summary.last_activity_at()) {
                Some(HistorySummaryRecord::new(
                    summary.thread_id(),
                    summary.revision().checked_next()?,
                    summary.thread_revision(),
                    summary.committed_tail(),
                    summary.selected_path_digest(),
                    false,
                    next_activity,
                ))
            } else {
                None
            };
        Ok(FreezeRecords {
            state,
            summary,
            changed,
            transcript_head,
            transcript_build,
            item_head,
            projection_build,
        })
    }
}

impl FreezeRecords {
    pub(super) fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<TurnStatesCodec>(&self.state.turn_id(), &self.state)?;
        if let Some(summary) = &self.summary {
            mutations.put::<HistorySummariesCodec>(&summary.thread_id(), summary)?;
        }
        if let Some(head) = &self.transcript_head {
            mutations.put::<TranscriptHeadsCodec>(&head.thread_id(), head)?;
        }
        if let Some(build) = &self.transcript_build {
            mutations.put::<TranscriptBuildsCodec>(
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                build,
            )?;
        }
        if let Some(head) = &self.item_head {
            mutations.put::<ItemProjectionHeadsCodec>(&head.item_id(), head)?;
        }
        if let Some(build) = &self.projection_build {
            mutations.put::<ItemProjectionBuildsCodec>(
                &ItemProjectionSetKey {
                    item: build.item_id(),
                    generation: build.generation(),
                },
                build,
            )?;
        }
        mutations
            .put::<ContentManifestsCodec>(&self.changed.manifest.id(), &self.changed.manifest)?;
        mutations.put::<CanonicalItemsCodec>(&self.changed.item.id(), &self.changed.item)?;
        mutations.put::<TurnItemsCodec>(
            &TurnItemKey {
                owner: self.changed.item.turn_id(),
                ordinal: self.changed.item.ordinal(),
            },
            &self.changed.index,
        )?;
        if let Some(index) = &self.changed.cas_index {
            mutations.put::<CasItemIndexCodec>(
                &CasItemKey::Record(
                    index.cas_thread_id().clone(),
                    index.cas_turn_id().clone(),
                    index.cas_item_id().clone(),
                ),
                index,
            )?;
        }
        Ok(())
    }
}

fn freeze_item(
    current_manifest: ContentManifestRecord,
    current_item: CanonicalItemRecord,
) -> Result<FrozenItem, SyndicMutationError> {
    let current_provider = current_item
        .provider()
        .cloned()
        .ok_or(SyndicMutationError::CanonicalFinalizationConflict)?;
    let current_content = current_provider.content();
    if !current_provider.stream_state().is_complete() {
        return Err(SyndicMutationError::CanonicalFinalizationConflict);
    }
    let source_event = current_item
        .source_event()
        .ok_or(SyndicMutationError::CanonicalFinalizationConflict)?;
    let cas_source = current_item
        .cas_source()
        .cloned()
        .ok_or(SyndicMutationError::CanonicalFinalizationConflict)?;
    if current_manifest.owner() != Some(current_item.id())
        || current_manifest.current_reference() != Some(current_content)
    {
        return Err(SyndicMutationError::CanonicalFinalizationConflict);
    }
    let manifest = ContentManifestRecord::with_owner(
        current_manifest.id(),
        current_manifest.owner(),
        current_manifest.revision().checked_next()?,
        current_manifest.encoding(),
        ContentLifecycle::Finalized,
        current_manifest.chunk_count(),
        current_manifest.encoded_bytes(),
        current_manifest.chain_digest(),
        current_manifest.expected(),
    );
    let content = manifest
        .current_reference()
        .ok_or(SyndicMutationError::ContentManifestConflict)?;
    let provider = SealedProviderFrameReference::new(
        content,
        current_provider.frame().clone(),
        current_provider.observation(),
        current_provider.stream_state().clone(),
        current_provider.narrative(),
    )
    .map_err(|_| SyndicMutationError::CanonicalFinalizationConflict)?;
    let revision: ProjectionRevision = current_item.revision().checked_next()?;
    let item = CanonicalItemRecord::with_provider_state(
        current_item.id(),
        current_item.turn_id(),
        current_item.ordinal(),
        revision,
        source_event,
        current_item.source_event_count(),
        cas_source,
        current_item.assistant_phase(),
        provider,
        current_item.narrative_completion(),
        current_item.presentation().clone(),
    )?;
    let index = TurnItemIndexRecord::new(item.turn_id(), item.ordinal(), item.id(), revision);
    let cas_index = item.cas_source().map(|source| {
        CasItemIndexRecord::new(
            source.turn().thread_id().clone(),
            source.turn().turn_id().clone(),
            source.item_id().clone(),
            item.id(),
            revision,
        )
    });
    Ok(FrozenItem {
        manifest,
        item,
        index,
        cas_index,
    })
}
