use super::*;

pub(super) fn classify(
    reader: &impl TerminalHistoryReader,
    request: DiscussionParentRequest,
) -> Result<Option<DiscussionParentDisposition>, SyndicMutationError> {
    let child_id = request.child_gate.thread_id();
    let parent_id = request.parent_thread_id;
    let DiscussionHandoffGateState::Pending {
        resolving_turn_id, ..
    } = request.child_gate.state()
    else {
        return conflict();
    };
    let child = required::<ThreadsFamily>(reader, &child_id)?;
    let gate = required::<DiscussionHandoffGatesFamily>(reader, &child_id)?;
    let attributes = required::<ThreadAttributesFamily>(reader, &child_id)?;
    let turn = required::<TurnsFamily>(reader, &resolving_turn_id)?;
    let context =
        required::<ContextEnvelopesFamily>(reader, &ContextOwnerKey::from(request.context_owner))?;
    let link = required::<ThreadParentFamily>(
        reader,
        &ThreadPairKey {
            first: parent_id,
            second: child_id,
        },
    )?;
    if child_id == parent_id
        || child.id() != child_id
        || gate != request.child_gate
        || child.parent_thread_id() != Some(parent_id)
        || child.committed_tail() != Some(resolving_turn_id)
        || child.context_owner_id() != Some(request.context_owner)
        || attributes.thread_id() != child_id
        || attributes.archive() != ThreadArchiveState::BranchDiscussionOpen
        || turn.id() != resolving_turn_id
        || turn.origin_thread_id() != child_id
        || context.owner() != request.context_owner
        || context.envelope().descriptor().digest() != request.context_digest
        || context.envelope().descriptor().source().thread_id() != parent_id
        || link.parent_thread_id() != parent_id
        || link.child_thread_id() != child_id
        || link.child_revision() != child.revision()
        || link.context_owner_id() != request.context_owner
    {
        return conflict();
    }
    let parent = required::<ThreadsFamily>(reader, &parent_id)?;
    let attributes = required::<ThreadAttributesFamily>(reader, &parent_id)?;
    let handoff = reader.read::<DiscussionHandoffGatesFamily>(&parent_id)?;
    if parent.id() != parent_id || attributes.thread_id() != parent_id {
        return conflict();
    }
    let pending = match attributes.archive() {
        ThreadArchiveState::Ordinary => {
            if parent.parent_thread_id().is_some()
                || parent.context_owner_id().is_some()
                || handoff.is_some()
            {
                return conflict();
            }
            false
        }
        ThreadArchiveState::BranchDiscussionOpen
        | ThreadArchiveState::BranchDiscussionArchived { .. } => {
            let Some(handoff) = handoff else {
                return conflict();
            };
            if parent.parent_thread_id().is_none()
                || parent.context_owner_id().is_none()
                || handoff.thread_id() != parent_id
            {
                return conflict();
            }
            let pending = matches!(handoff.state(), DiscussionHandoffGateState::Pending { .. });
            if attributes.archive().is_archived() {
                if pending {
                    return conflict();
                }
                return Ok(Some(DiscussionParentDisposition::Archived));
            }
            pending
        }
    };
    let gate = required::<InputGatesFamily>(reader, &parent_id)?;
    let head = required::<BindingHeadsFamily>(reader, &parent_id)?;
    let binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: parent_id,
            revision: head.revision(),
        },
    )?;
    let draft = required::<DraftsFamily>(reader, &parent.current_draft_id())?;
    let reverse = required::<DraftByThreadFamily>(reader, &parent_id)?;
    let nonidle = reader.read::<NonIdleGateSourcesFamily>(&parent_id)?;
    let idle = gate.state() == &InputGateState::Idle;
    if gate.thread_id() != parent_id
        || head.thread_id() != parent_id
        || binding.thread_id() != parent_id
        || binding.revision() != head.revision()
        || head.lifecycle() != binding.state().lifecycle()
        || head.selected_path_digest() != parent.selected_path_digest()
        || binding.selected_path().tail() != parent.committed_tail()
        || binding.selected_path().digest() != parent.selected_path_digest()
        || binding.selected_path().thread_revision() > parent.revision()
        || draft.id() != parent.current_draft_id()
        || draft.thread_id() != parent_id
        || reverse
            != DraftByThreadRecord::new(parent_id, draft.id(), draft.revision(), parent.revision())
        || nonidle != (!idle).then(|| NonIdleGateSourceRecord::new(parent_id, gate.revision()))
        || (idle && matches!(binding.state(), BindingState::Active(_)))
        || matches!(binding.state(), BindingState::Active(active) if gate.state().blocking_turn_id() != Some(active.turn_id()))
        || matches!(draft.submission_intent(), DraftSubmissionIntent::DiscussionContext(owner) if parent.context_owner_id() != Some(owner))
    {
        return conflict();
    }
    if pending
        || !idle
        || gate.live_count() != 0
        || draft.submission_intent() != DraftSubmissionIntent::Ordinary
    {
        return Ok(None);
    }
    if gate.live_logical_utf8_bytes() != 0 {
        return conflict();
    }
    Ok(Some(DiscussionParentDisposition::Ready))
}

fn required<F: Family>(
    reader: &impl TerminalHistoryReader,
    key: &F::Key,
) -> Result<F::Value, SyndicMutationError> {
    reader
        .read::<F>(key)?
        .ok_or(SyndicMutationError::DiscussionHandoffConflict)
}
fn conflict<T>() -> Result<T, SyndicMutationError> {
    Err(SyndicMutationError::DiscussionHandoffConflict)
}
