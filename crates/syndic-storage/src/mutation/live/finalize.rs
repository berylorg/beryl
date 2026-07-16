use super::*;
use crate::mutation::required;
use crate::{
    CanonicalItemKind, CanonicalItemPayload, GeneratedMediaResourceDisposition,
    HistorySummaryRecord, ResourceBacking, TurnStateRecord,
};

pub(super) struct FinalizationRecords {
    state: TurnStateRecord,
    summary: Option<HistorySummaryRecord>,
    transcript_head: Option<crate::TranscriptViewHeadRecord>,
    transcript_build: Option<crate::TranscriptBuildRecord>,
}

impl FinalizeNextTurnItemMutation {
    pub(super) fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<FinalizationRecords, SyndicMutationError> {
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
        if let Some(content) = item.payload().content() {
            let manifest = required::<ContentManifestsFamily>(reader, &content.id())?;
            if !manifest.lifecycle().is_immutable() || manifest.current_reference() != Some(content)
            {
                return Err(SyndicMutationError::CanonicalFinalizationConflict);
            }
        }
        if let CanonicalItemPayload::GeneratedMedia(resource_id) = item.payload() {
            let resource = required::<ResourcesFamily>(reader, resource_id)?;
            if resource.item_id() != item.id()
                || !matches!(
                    resource.backing(),
                    ResourceBacking::GeneratedMedia(GeneratedMediaResourceDisposition::Asset(_))
                )
            {
                return Err(SyndicMutationError::CanonicalFinalizationConflict);
            }
        }
        if matches!(
            item.kind(),
            CanonicalItemKind::UserInput | CanonicalItemKind::AssistantMessage(_)
        ) {
            require_current_projection(reader, &item)?;
        }
        let selected = crate::mutation::turn_is_on_selected_path(reader, &thread, &turn)?;
        let state = TurnStateRecord::with_capture_frontiers(
            current.turn_id(),
            current.revision().checked_next()?,
            current.lifecycle(),
            current.source_event_count(),
            current.item_count(),
            request.expected_item_ordinal.get(),
            current.open_item_count(),
            current.history_blocking_item_count(),
            current.end_status(),
            request.updated_at,
        )?;
        let summary = selected.then(|| {
            HistorySummaryRecord::new(
                summary.thread_id(),
                summary.thread_revision(),
                summary.committed_tail(),
                summary.selected_path_digest(),
                false,
                summary.last_activity_at().max(request.updated_at),
            )
        });
        let (transcript_head, transcript_build) = if selected {
            crate::mutation::transcript::invalidate_transcript_projection(reader, &thread)?
        } else {
            (None, None)
        };
        Ok(FinalizationRecords {
            state,
            summary,
            transcript_head,
            transcript_build,
        })
    }
}

impl FinalizationRecords {
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
        Ok(())
    }
}

fn require_current_projection(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
) -> Result<(), SyndicMutationError> {
    let content = item
        .payload()
        .content()
        .ok_or(SyndicMutationError::CanonicalFinalizationConflict)?;
    let head = required::<ItemProjectionHeadsFamily>(reader, &item.id())?;
    if head.lifecycle() != crate::ProjectionLifecycle::Current
        || head.source_item_revision() != item.revision()
    {
        return Err(SyndicMutationError::CanonicalFinalizationConflict);
    }
    let set = required::<ItemProjectionSetsFamily>(
        reader,
        &ItemProjectionSetKey {
            item: item.id(),
            generation: head.generation(),
        },
    )?;
    if set.source_item_revision() != item.revision()
        || set.source_content() != content
        || set.projection_count() == 0
    {
        return Err(SyndicMutationError::CanonicalFinalizationConflict);
    }
    Ok(())
}
