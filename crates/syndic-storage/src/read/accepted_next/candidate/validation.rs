use super::*;

pub(super) fn validate_candidate_basis(
    basis: &AcceptedNextCandidateBasis,
    reason: NextTurnReason,
) -> Result<(), SyndicReadError> {
    let source = basis.source();
    let gate = basis.gate();
    let thread = basis.thread();
    let reverse = basis.draft_by_thread();
    let generation = basis.generation();
    let leaf = basis.leaf();
    let input = basis.input();
    let order = basis.order();
    let binding_head = basis.binding_head();
    let binding = basis.binding();
    let transcript = basis.transcript_head();
    let summary = basis.summary();
    let activity = basis.activity_head();
    let selected = thread.selected_path();

    if gate.state() != &InputGateState::Idle
        || thread.id() != source.thread_id()
        || thread.committed_tail().is_none()
        || gate.thread_id() != source.thread_id()
        || reverse.thread_id() != source.thread_id()
        || reverse.draft_id() != thread.current_draft_id()
        || reverse.thread_revision() != thread.revision()
        || generation.thread_id() != source.thread_id()
        || generation.generation() != source.generation()
        || generation.revision() != source.generation_revision()
        || generation.first_ordinal() != Some(source.first_ordinal())
        || generation.last_ordinal() != Some(source.last_ordinal())
        || generation.next_turn_count() == 0
        || gate.live_steering_count() != 0
        || gate.live_next_turn_count() < generation.next_turn_count()
        || gate.live_logical_utf8_bytes() < generation.live_logical_utf8_bytes()
    {
        return Err(SyndicReadError::Invariant(
            "accepted-next candidate source authority is incoherent",
        ));
    }
    if !route_head_is_coherent(source.thread_id(), gate, basis.route_head()) {
        return Err(SyndicReadError::Invariant(
            "accepted-next candidate route head is incoherent",
        ));
    }
    if input.id() != order.input_id()
        || input.thread_id() != source.thread_id()
        || input.ordinal() != order.ordinal()
        || input.route_generation() != source.generation()
        || leaf.input_id() != input.id()
        || leaf.thread_id() != input.thread_id()
        || leaf.ordinal() != input.ordinal()
        || leaf.generation() != input.route_generation()
        || effective_next_turn_reason(generation, leaf) != Some(reason)
        || input.content().summary().logical_utf8_bytes() > generation.live_logical_utf8_bytes()
        || input.content().summary().logical_utf8_bytes() > gate.live_logical_utf8_bytes()
    {
        return Err(SyndicReadError::Invariant(
            "accepted-next candidate identity is incoherent",
        ));
    }
    if binding_head.thread_id() != source.thread_id()
        || binding_head.revision() != binding.revision()
        || binding_head.lifecycle() != binding.state().lifecycle()
        || binding_head.selected_path_digest() != binding.selected_path().digest()
        || binding.thread_id() != source.thread_id()
        || binding.selected_path().tail() != selected.tail()
        || binding.selected_path().digest() != selected.digest()
        || binding.selected_path().thread_revision() > selected.thread_revision()
    {
        return Err(SyndicReadError::Invariant(
            "accepted-next candidate selected binding is incoherent",
        ));
    }
    // Transcript lifecycle is rebuildable derivative state. Both an exact current head and an
    // exact stale head are valid promotion bases; promotion supersedes any active build and
    // advances the head to the new pending tail atomically.
    if transcript.thread_id() != source.thread_id()
        || transcript.committed_tail() != thread.committed_tail()
        || transcript.selected_path_digest() != thread.selected_path_digest()
        || summary.thread_id() != source.thread_id()
        || summary.thread_revision() != thread.revision()
        || summary.committed_tail() != thread.committed_tail()
        || summary.selected_path_digest() != thread.selected_path_digest()
        || activity.thread_id() != source.thread_id()
        || activity.lifecycle() != ProjectionLifecycle::Current
    {
        return Err(SyndicReadError::Invariant(
            "accepted-next candidate thread projections are incoherent",
        ));
    }
    Ok(())
}
