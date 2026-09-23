use super::{point, required};
use crate::{
    DiscussionHandoffGateState, SyndicMutationError, ThreadArchiveState, codec::*,
    domain::SyndicDomain,
};
use beryl_home_store::DomainReader;
use beryl_model::SyndicThreadId;

pub(crate) fn require_editable(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
) -> Result<(), SyndicMutationError> {
    let thread = required::<ThreadsFamily>(reader, &thread_id)?;
    let attributes = required::<ThreadAttributesFamily>(reader, &thread_id)?;
    let gate = point::<DiscussionHandoffGatesFamily>(reader, &thread_id)?;
    if thread.id() != thread_id || attributes.thread_id() != thread_id {
        return Err(SyndicMutationError::DiscussionHandoffConflict);
    }
    match (thread.parent_thread_id(), attributes.archive(), gate) {
        (None, ThreadArchiveState::Ordinary, None) if thread.context_owner_id().is_none() => Ok(()),
        (Some(_), archive, Some(gate))
            if gate.thread_id() == thread_id && thread.context_owner_id().is_some() =>
        {
            match (archive, gate.state()) {
                (ThreadArchiveState::BranchDiscussionOpen, DiscussionHandoffGateState::Open) => {
                    Ok(())
                }
                (
                    ThreadArchiveState::BranchDiscussionOpen,
                    DiscussionHandoffGateState::Pending { .. },
                )
                | (
                    ThreadArchiveState::BranchDiscussionArchived { .. },
                    DiscussionHandoffGateState::Open,
                ) => Err(SyndicMutationError::DiscussionMutationBlocked),
                _ => Err(SyndicMutationError::DiscussionHandoffConflict),
            }
        }
        _ => Err(SyndicMutationError::DiscussionHandoffConflict),
    }
}
