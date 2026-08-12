use beryl_home_store::DomainReader;

use crate::mutation::{point, required};
use crate::{
    ProjectionLifecycle, SyndicMutationError, TranscriptPathTurnRecord, TurnRecord,
    TurnStateRecord, codec::*, domain::SyndicDomain,
};

/// Refreshes one selected current transcript path row after a presentation-neutral state advance.
pub(in crate::mutation) fn refresh_current_path_state(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    turn: &TurnRecord,
    state: &TurnStateRecord,
) -> Result<Option<TranscriptPathTurnRecord>, SyndicMutationError> {
    if !crate::mutation::turn_is_on_selected_path(reader, thread, turn)? {
        return Ok(None);
    }
    let head = required::<TranscriptHeadsFamily>(reader, &thread.id())?;
    if head.lifecycle() != ProjectionLifecycle::Current {
        return Ok(None);
    }
    let key = ThreadTranscriptPathKey {
        thread: thread.id(),
        generation: head.generation(),
        depth: turn.depth(),
    };
    let Some(path) = point::<TranscriptPathTurnsFamily>(reader, &key)? else {
        return Ok(None);
    };
    if path.turn_id() != turn.id() || path.turn_path_digest() != turn.chain_digest() {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    Ok(Some(TranscriptPathTurnRecord::new(
        path.thread_id(),
        path.generation(),
        path.depth(),
        path.turn_id(),
        path.turn_path_digest(),
        state.revision(),
        state.lifecycle(),
        state.source_event_count(),
        state.item_count(),
        state.finalized_item_count(),
        state.updated_at(),
    )))
}
