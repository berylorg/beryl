use super::*;

pub(super) fn classify(
    reader: &impl TerminalHistoryReader,
    expected: DiscussionHandoffGateRecord,
) -> Result<Option<DiscussionChildSettlementDisposition>, SyndicMutationError> {
    let DiscussionHandoffGateState::Pending {
        resolving_turn_id, ..
    } = expected.state()
    else {
        return conflict();
    };
    let id = expected.thread_id();
    let actual = required::<DiscussionHandoffGatesFamily>(reader, &id)?;
    let thread = required::<ThreadsFamily>(reader, &id)?;
    let attributes = required::<ThreadAttributesFamily>(reader, &id)?;
    let turn = required::<TurnsFamily>(reader, &resolving_turn_id)?;
    let state = required::<TurnStatesFamily>(reader, &resolving_turn_id)?;
    let gate = required::<InputGatesFamily>(reader, &id)?;
    let head = required::<BindingHeadsFamily>(reader, &id)?;
    let binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: id,
            revision: head.revision(),
        },
    )?;
    let summary = required::<HistorySummariesFamily>(reader, &id)?;
    let transcript = required::<TranscriptHeadsFamily>(reader, &id)?;
    if actual != expected
        || thread.id() != id
        || thread.parent_thread_id().is_none()
        || thread.context_owner_id().is_none()
        || attributes.thread_id() != id
        || attributes.archive() != ThreadArchiveState::BranchDiscussionOpen
        || thread.committed_tail() != Some(resolving_turn_id)
        || turn.id() != resolving_turn_id
        || turn.origin_thread_id() != id
        || state.turn_id() != resolving_turn_id
        || gate.thread_id() != id
        || summary.thread_id() != id
        || transcript.thread_id() != id
        || head.thread_id() != id
        || binding.thread_id() != id
        || binding.revision() != head.revision()
        || head.lifecycle() != binding.state().lifecycle()
        || head.selected_path_digest() != thread.selected_path_digest()
        || binding.selected_path().tail() != thread.committed_tail()
        || binding.selected_path().digest() != thread.selected_path_digest()
        || binding.selected_path().thread_revision() > thread.revision()
    {
        return conflict();
    }
    if transcript.lifecycle() == ProjectionLifecycle::Current {
        let build = required::<TranscriptBuildsFamily>(
            reader,
            &ThreadTranscriptBuildKey {
                thread: id,
                generation: transcript.generation(),
            },
        )?;
        if build.thread_id() != id || build.generation() != transcript.generation() {
            return conflict();
        }
    }
    let nonidle = reader.read::<NonIdleGateSourcesFamily>(&id)?;
    let is_idle = gate.state() == &InputGateState::Idle;
    if nonidle != (!is_idle).then(|| NonIdleGateSourceRecord::new(id, gate.revision())) {
        return conflict();
    }
    if !is_idle && gate.state().blocking_turn_id() != Some(resolving_turn_id) {
        return conflict();
    }
    if is_idle && matches!(binding.state(), BindingState::Active(_)) {
        return conflict();
    }
    if !state.lifecycle().is_proven_terminal()
        || !is_idle
        || gate.live_steering_count() != 0
        || matches!(binding.state(), BindingState::Active(_))
    {
        return Ok(None);
    }
    if !crate::terminal_history::is_complete(reader, &thread, &state, None)? {
        return Ok(None);
    }
    if gate.live_next_turn_count() > 0 {
        return Ok(Some(DiscussionChildSettlementDisposition::QueuedInput));
    }
    if gate.live_count() != 0 || gate.live_logical_utf8_bytes() != 0 {
        return conflict();
    }
    Ok(Some(DiscussionChildSettlementDisposition::Ready))
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
