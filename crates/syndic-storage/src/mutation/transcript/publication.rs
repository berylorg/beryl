use beryl_home_store::DomainReader;

use crate::{
    ProjectionLifecycle, ProjectionOrdinal, SyndicMutationError, TranscriptBuildPhase,
    TranscriptBuildRecord, TranscriptPosition, TurnDepth, TurnItemOrdinal, codec::*,
    domain::SyndicDomain,
};

use super::advance::AdvanceRecords;
use crate::mutation::{point, required};

const TRANSCRIPT_ENTRY_BATCH: usize = 64;

pub(super) fn publish_entries(
    reader: &DomainReader<'_, SyndicDomain>,
    build: TranscriptBuildRecord,
    depth: TurnDepth,
    item_ordinal: TurnItemOrdinal,
    projection_ordinal: ProjectionOrdinal,
) -> Result<AdvanceRecords, SyndicMutationError> {
    let path = required::<TranscriptPathTurnsFamily>(
        reader,
        &ThreadTranscriptPathKey {
            thread: build.thread_id(),
            generation: build.generation(),
            depth,
        },
    )?;
    let turn = required::<TurnsFamily>(reader, &path.turn_id())?;
    let state = required::<TurnStatesFamily>(reader, &turn.id())?;
    if state.revision() != path.state_revision()
        || state.lifecycle() != path.lifecycle()
        || state.source_event_count() != path.source_event_count()
        || state.item_count() != path.item_count()
        || state.finalized_item_count() != path.finalized_item_count()
        || state.updated_at() != path.updated_at()
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    if item_ordinal.get() > path.finalized_item_count() {
        let phase = phase_after_depth(&build, depth)?;
        let entry_count = build.entry_count();
        let digest = build.entry_digest();
        return finish_publication(reader, build, Vec::new(), phase, entry_count, digest);
    }
    let index = required::<TurnItemsFamily>(
        reader,
        &TurnItemKey {
            owner: turn.id(),
            ordinal: item_ordinal,
        },
    )?;
    let item = required::<CanonicalItemsFamily>(reader, &index.item_id())?;
    if item.turn_id() != turn.id()
        || item.ordinal() != item_ordinal
        || item.revision() != index.item_revision()
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    if !matches!(
        item.kind(),
        crate::CanonicalItemKind::UserInput | crate::CanonicalItemKind::AssistantMessage(_)
    ) {
        let phase = phase_after_item(&build, depth, item_ordinal, path.finalized_item_count())?;
        let entry_count = build.entry_count();
        let digest = build.entry_digest();
        return finish_publication(reader, build, Vec::new(), phase, entry_count, digest);
    }
    let content = item
        .payload()
        .content()
        .ok_or(SyndicMutationError::TranscriptBuildConflict)?;
    let manifest = required::<ContentManifestsFamily>(reader, &content.id())?;
    if !manifest.lifecycle().is_immutable() || manifest.current_reference() != Some(content) {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    let item_head = point::<ItemProjectionHeadsFamily>(reader, &item.id())?
        .ok_or(SyndicMutationError::TranscriptProjectionUnavailable)?;
    if item_head.lifecycle() != ProjectionLifecycle::Current
        || item_head.source_item_revision() != item.revision()
    {
        return Err(SyndicMutationError::TranscriptProjectionUnavailable);
    }
    let set = required::<ItemProjectionSetsFamily>(
        reader,
        &ItemProjectionSetKey {
            item: item.id(),
            generation: item_head.generation(),
        },
    )?;
    if set.source_item_revision() != item.revision()
        || set.source_content() != content
        || projection_ordinal.get() > set.projection_count()
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    let mut projection_indexes = Vec::with_capacity(TRANSCRIPT_ENTRY_BATCH);
    let mut next_projection = projection_ordinal;
    for _ in 0..TRANSCRIPT_ENTRY_BATCH {
        if next_projection.get() > set.projection_count() {
            break;
        }
        projection_indexes.push(
            crate::membership::point(reader, &set, next_projection)?
                .ok_or(SyndicMutationError::TranscriptBuildConflict)?,
        );
        next_projection = next_projection.checked_next()?;
    }
    if projection_indexes.is_empty() {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    let mut entries = Vec::with_capacity(projection_indexes.len());
    let mut expected = projection_ordinal;
    let mut digest = build.entry_digest();
    let mut entry_count = build.entry_count();
    for projection_index in &projection_indexes {
        if projection_index.ordinal() != expected
            || projection_index.item_id() != item.id()
            || projection_index.generation() != item_head.generation()
        {
            return Err(SyndicMutationError::TranscriptBuildConflict);
        }
        let projection = required::<ProjectionsFamily>(reader, &projection_index.projection_id())?;
        if projection.revision() != projection_index.projection_revision() {
            return Err(SyndicMutationError::TranscriptBuildConflict);
        }
        entry_count = entry_count
            .checked_add(1)
            .ok_or(SyndicMutationError::TranscriptBuildConflict)?;
        let position = TranscriptPosition::new(entry_count)?;
        let entry = crate::TranscriptViewEntryRecord::new(
            build.thread_id(),
            build.generation(),
            position,
            item.id(),
            item.revision(),
            item_head.generation(),
            projection.id(),
            projection.revision(),
        );
        digest = crate::projection::advance_transcript_entry_digest(
            digest,
            entry.thread_id(),
            entry.generation(),
            entry.position(),
            entry.item_id(),
            entry.item_revision(),
            entry.item_projection_generation(),
            entry.projection_id(),
            entry.projection_revision(),
        );
        entries.push(entry);
        expected = expected.checked_next()?;
    }
    let last_projection = entries
        .last()
        .ok_or(SyndicMutationError::TranscriptBuildConflict)?
        .position();
    let emitted =
        u64::try_from(entries.len()).map_err(|_| SyndicMutationError::TranscriptBuildConflict)?;
    let last_item_projection = projection_ordinal
        .get()
        .checked_add(emitted.saturating_sub(1))
        .ok_or(SyndicMutationError::TranscriptBuildConflict)?;
    if last_projection.get() != entry_count {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    let phase = if last_item_projection < set.projection_count() {
        Some(TranscriptBuildPhase::Publishing {
            next_depth: depth,
            next_item: item_ordinal,
            next_projection: ProjectionOrdinal::new(
                last_item_projection
                    .checked_add(1)
                    .ok_or(SyndicMutationError::TranscriptBuildConflict)?,
            )?,
        })
    } else if last_item_projection == set.projection_count() {
        phase_after_item(&build, depth, item_ordinal, path.finalized_item_count())?
    } else {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    };
    finish_publication(reader, build, entries, phase, entry_count, digest)
}

fn phase_after_item(
    build: &TranscriptBuildRecord,
    depth: TurnDepth,
    item: TurnItemOrdinal,
    item_count: u64,
) -> Result<Option<TranscriptBuildPhase>, SyndicMutationError> {
    if item.get() < item_count {
        Ok(Some(TranscriptBuildPhase::Publishing {
            next_depth: depth,
            next_item: item.checked_next()?,
            next_projection: ProjectionOrdinal::FIRST,
        }))
    } else if item.get() == item_count {
        phase_after_depth(build, depth)
    } else {
        Err(SyndicMutationError::TranscriptBuildConflict)
    }
}

fn phase_after_depth(
    build: &TranscriptBuildRecord,
    depth: TurnDepth,
) -> Result<Option<TranscriptBuildPhase>, SyndicMutationError> {
    if depth.get() < build.path_turn_count() {
        Ok(Some(TranscriptBuildPhase::Publishing {
            next_depth: depth.checked_next()?,
            next_item: TurnItemOrdinal::FIRST,
            next_projection: ProjectionOrdinal::FIRST,
        }))
    } else if depth.get() == build.path_turn_count() {
        Ok(None)
    } else {
        Err(SyndicMutationError::TranscriptBuildConflict)
    }
}

fn finish_publication(
    reader: &DomainReader<'_, SyndicDomain>,
    previous: TranscriptBuildRecord,
    entries: Vec<crate::TranscriptViewEntryRecord>,
    next_phase: Option<TranscriptBuildPhase>,
    entry_count: u64,
    entry_digest: [u8; 32],
) -> Result<AdvanceRecords, SyndicMutationError> {
    if entry_count
        != previous
            .entry_count()
            .checked_add(entries.len() as u64)
            .ok_or(SyndicMutationError::TranscriptBuildConflict)?
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    for entry in &entries {
        if point::<TranscriptEntriesFamily>(
            reader,
            &ThreadTranscriptKey {
                thread: entry.thread_id(),
                generation: entry.generation(),
                position: entry.position(),
            },
        )?
        .is_some()
        {
            return Err(SyndicMutationError::TranscriptIdentityCollision);
        }
    }
    let complete = next_phase.is_none();
    let phase = next_phase.unwrap_or(TranscriptBuildPhase::Complete);
    let build = TranscriptBuildRecord::new(
        previous.thread_id(),
        previous.generation(),
        previous.revision().checked_next()?,
        previous.source_thread_revision(),
        previous.committed_tail(),
        previous.selected_path_digest(),
        previous.path_turn_count(),
        entry_count,
        entry_digest,
        previous.history_complete(),
        phase,
    );
    let current_head = required::<TranscriptHeadsFamily>(reader, &build.thread_id())?;
    if current_head.generation() != build.generation()
        || current_head.lifecycle() != ProjectionLifecycle::Stale
        || current_head.entry_count() != 0
        || current_head.revision().checked_next()? != build.revision()
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    let head = Some(crate::TranscriptViewHeadRecord::new(
        build.thread_id(),
        build.generation(),
        build.revision(),
        if complete { build.entry_count() } else { 0 },
        build.committed_tail(),
        build.selected_path_digest(),
        if complete {
            ProjectionLifecycle::Current
        } else {
            ProjectionLifecycle::Stale
        },
    ));
    let summary = if complete {
        let current_summary = required::<HistorySummariesFamily>(reader, &build.thread_id())?;
        Some(crate::HistorySummaryRecord::new(
            build.thread_id(),
            build.source_thread_revision(),
            build.committed_tail(),
            build.selected_path_digest(),
            build.history_complete(),
            current_summary.last_activity_at(),
        ))
    } else {
        None
    };
    Ok(AdvanceRecords {
        build,
        path: None,
        entries,
        head,
        summary,
    })
}
