use beryl_home_store::DomainReader;
use beryl_model::{SyndicPathDigest, SyndicTurnId};

use crate::codec::*;
use crate::domain::SyndicDomain;
use crate::{
    ConversationParent, SyndicMutationError, ThreadParentIndexRecord, ThreadRecord, TurnDepth,
    child_turn_chain_digest, root_turn_chain_digest,
};

use super::required;

pub(super) fn turn_shape(
    reader: &DomainReader<'_, SyndicDomain>,
    turn_id: SyndicTurnId,
    parent: ConversationParent,
) -> Result<(TurnDepth, SyndicPathDigest, Option<SyndicTurnId>), SyndicMutationError> {
    match parent {
        ConversationParent::Root => Ok((TurnDepth::FIRST, root_turn_chain_digest(turn_id), None)),
        ConversationParent::Turn(parent_id) => {
            let parent = required::<TurnsFamily>(reader, &parent_id)?;
            let depth = parent.depth().checked_next()?;
            let ancestor_skip = crate::selected_path::child_ancestor_skip(
                parent.clone(),
                depth,
                |turn_id| required::<TurnsFamily>(reader, &turn_id),
                |_| SyndicMutationError::SourceTailConflict,
            )?;
            Ok((
                depth,
                child_turn_chain_digest(turn_id, parent_id, parent.chain_digest()),
                Some(ancestor_skip),
            ))
        }
    }
}

pub(super) fn thread_parent_index(thread: &ThreadRecord) -> Option<ThreadParentIndexRecord> {
    match (thread.parent_thread_id(), thread.context_owner_id()) {
        (Some(parent), Some(owner)) => Some(ThreadParentIndexRecord::new(
            parent,
            thread.id(),
            thread.revision(),
            owner,
        )),
        _ => None,
    }
}
