use beryl_home_store::DomainReader;

use crate::{
    ProjectionLifecycle, ProjectionOrdinal, SyndicMutationError, TranscriptBuildPhase,
    TranscriptBuildRecord, TurnDepth, TurnItemOrdinal, codec::*, domain::SyndicDomain,
};

use super::advance::AdvanceRecords;
use crate::mutation::{point, required};

pub(super) fn collect_path_turn(
    reader: &DomainReader<'_, SyndicDomain>,
    build: TranscriptBuildRecord,
    next_turn: Option<beryl_model::SyndicTurnId>,
) -> Result<AdvanceRecords, SyndicMutationError> {
    let turn_id = next_turn.ok_or(SyndicMutationError::TranscriptBuildConflict)?;
    let turn = required::<TurnsFamily>(reader, &turn_id)?;
    let state = required::<TurnStatesFamily>(reader, &turn_id)?;
    let path_turn_count = build
        .path_turn_count()
        .checked_add(1)
        .ok_or(SyndicMutationError::TranscriptBuildConflict)?;
    let path = crate::TranscriptPathTurnRecord::new(
        build.thread_id(),
        build.generation(),
        turn.depth(),
        turn.id(),
        turn.chain_digest(),
        state.revision(),
        state.lifecycle(),
        state.source_event_count(),
        state.item_count(),
        state.finalized_item_count(),
        state.updated_at(),
    );
    let path_key = ThreadTranscriptPathKey {
        thread: path.thread_id(),
        generation: path.generation(),
        depth: path.depth(),
    };
    if point::<TranscriptPathTurnsFamily>(reader, &path_key)?.is_some() {
        return Err(SyndicMutationError::TranscriptIdentityCollision);
    }
    let history_complete = build.history_complete()
        && crate::selected_path::turn_history_is_complete(
            state.lifecycle(),
            state.item_count(),
            state.finalized_item_count(),
            state.open_item_count(),
            state.history_blocking_item_count(),
            state.incomplete_reason(),
        );
    let parent = turn.parent().turn();
    let phase = match parent {
        Some(parent) => TranscriptBuildPhase::Collecting {
            next_turn: Some(parent),
        },
        None => {
            let tail = required::<TurnsFamily>(
                reader,
                &build
                    .committed_tail()
                    .ok_or(SyndicMutationError::TranscriptBuildConflict)?,
            )?;
            if turn.depth() != TurnDepth::FIRST || path_turn_count != tail.depth().get() {
                return Err(SyndicMutationError::TranscriptBuildConflict);
            }
            TranscriptBuildPhase::Publishing {
                next_depth: TurnDepth::FIRST,
                next_item: TurnItemOrdinal::FIRST,
                next_projection: ProjectionOrdinal::FIRST,
            }
        }
    };
    let build = TranscriptBuildRecord::new(
        build.thread_id(),
        build.generation(),
        build.revision().checked_next()?,
        build.source_thread_revision(),
        build.committed_tail(),
        build.selected_path_digest(),
        path_turn_count,
        build.entry_count(),
        build.entry_digest(),
        history_complete,
        phase,
    );
    let current_head = required::<TranscriptHeadsFamily>(reader, &build.thread_id())?;
    if current_head.generation() != build.generation()
        || current_head.lifecycle() != ProjectionLifecycle::Stale
        || current_head.revision().checked_next()? != build.revision()
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    let next_head = crate::TranscriptViewHeadRecord::new(
        current_head.thread_id(),
        current_head.generation(),
        build.revision(),
        0,
        build.committed_tail(),
        build.selected_path_digest(),
        ProjectionLifecycle::Stale,
    );
    Ok(AdvanceRecords {
        build,
        path: Some(path),
        entries: Vec::new(),
        head: Some(next_head),
        summary: None,
    })
}
