use super::*;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
    request: &AdmitDiscussionHandoff,
    thread: &ThreadRecord,
    attributes: &ThreadAttributesRecord,
) -> Result<(), SyndicMutationError> {
    let target = request.resolving_target.pending();
    let turn_id = target.active_turn_id();
    let turn = required::<TurnsFamily>(reader, &turn_id)?;
    let state = required::<TurnStatesFamily>(reader, &turn_id)?;
    let gate = super::super::input_gate::required_input_gate(reader, &request.thread_id)?;
    if thread.revision() != request.thread_revision
        || thread.committed_tail() != Some(turn_id)
        || attributes.revision() != request.attributes_revision
        || thread.parent_thread_id() != Some(request.parent.thread_id)
        || thread.context_owner_id() != Some(request.context_owner)
        || thread.id() == request.parent.thread_id
        || turn.id() != turn_id
        || turn.origin_thread_id() != request.thread_id
        || state.revision() != request.turn_state_revision
        || !matches!(
            state.lifecycle(),
            TurnLifecycle::Pending | TurnLifecycle::Active
        )
        || gate.revision() != request.input_gate_revision
        || !matches!(gate.state(), InputGateState::AwaitingSteering(id) | InputGateState::Steerable(id) if *id == turn_id)
        || gate.live_next_turn_count() != 0
    {
        return Err(SyndicMutationError::DiscussionHandoffConflict);
    }

    let context =
        required::<ContextEnvelopesFamily>(reader, &ContextOwnerKey::from(request.context_owner))?;
    let parent_link = required::<ThreadParentFamily>(
        reader,
        &ThreadPairKey {
            first: request.parent.thread_id,
            second: request.thread_id,
        },
    )?;
    if context.owner() != request.context_owner
        || context.envelope().descriptor().digest() != request.context_digest
        || context.envelope().descriptor().source().thread_id() != request.parent.thread_id
        || parent_link.parent_thread_id() != request.parent.thread_id
        || parent_link.child_thread_id() != request.thread_id
        || parent_link.child_revision() != request.thread_revision
        || parent_link.context_owner_id() != request.context_owner
    {
        return Err(SyndicMutationError::DiscussionHandoffConflict);
    }

    let parent = required::<ThreadsFamily>(reader, &request.parent.thread_id)?;
    let parent_gate =
        super::super::input_gate::required_input_gate(reader, &request.parent.thread_id)?;
    if parent.id() != request.parent.thread_id
        || parent.revision() != request.parent.thread_revision
        || parent_gate.revision() != request.parent.input_gate_revision
        || parent_gate.accepted_high_water() != request.parent.accepted_high_water
    {
        return Err(SyndicMutationError::DiscussionHandoffConflict);
    }

    let head = required::<BindingHeadsFamily>(reader, &request.thread_id)?;
    let binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: request.thread_id,
            revision: target.binding_revision(),
        },
    )?;
    let BindingState::Active(active) = binding.state() else {
        return Err(SyndicMutationError::BindingStateConflict);
    };
    if head.revision() != target.binding_revision()
        || head.lifecycle() != BindingLifecycle::Active
        || head.selected_path_digest() != thread.selected_path_digest()
        || binding.thread_id() != request.thread_id
        || binding.selected_path().tail() != thread.committed_tail()
        || binding.selected_path().digest() != thread.selected_path_digest()
        || binding.selected_path().thread_revision() > thread.revision()
        || active.snapshot_id() != target.snapshot_id()
        || active.turn_id() != turn_id
        || active.usable().cas_thread_id() != target.cas_thread_id()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    let execution = required::<ThreadExecutionsFamily>(reader, &request.thread_id)?;
    let snapshot = required::<ExecutionSnapshotsFamily>(reader, &target.snapshot_id())?;
    if execution.execution() != active.usable().execution()
        || snapshot.thread_id() != request.thread_id
        || snapshot.binding_revision() != target.binding_revision()
        || snapshot.active_turn_id() != turn_id
        || snapshot.cas_thread_id() != target.cas_thread_id()
        || snapshot.execution() != active.usable().execution()
        || snapshot.selected_path() != binding.selected_path()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    let active_turn = required::<ActiveCasTurnsFamily>(reader, &target.snapshot_id())?;
    if active_turn.snapshot_id() != target.snapshot_id()
        || active_turn.thread_id() != request.thread_id
        || active_turn.turn_id() != turn_id
        || active_turn.binding_revision() != target.binding_revision()
        || active_turn.cas_thread_id() != target.cas_thread_id()
        || active_turn.cas_turn_id() != request.resolving_target.cas_turn_id()
    {
        return Err(SyndicMutationError::ActiveCasTurnCollision);
    }
    let reservation = required::<CasThreadIndexFamily>(
        reader,
        &CasThreadKey::Record(target.cas_thread_id().clone()),
    )?;
    if reservation.thread_id() != request.thread_id
        || reservation.latest_binding_revision() != target.binding_revision()
        || reservation.retired_binding_revision().is_some()
    {
        return Err(SyndicMutationError::CasThreadRetired);
    }
    let membership = required::<CasThreadBindingIndexFamily>(
        reader,
        &CasThreadBindingKey::Record(target.cas_thread_id().clone(), target.binding_revision()),
    )?;
    if membership.cas_thread_id() != target.cas_thread_id()
        || membership.thread_id() != request.thread_id
        || membership.binding_revision() != target.binding_revision()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    let reverse = required::<CasTurnIndexFamily>(
        reader,
        &CasTurnKey::Record(
            target.cas_thread_id().clone(),
            request.resolving_target.cas_turn_id().clone(),
        ),
    )?;
    if reverse.cas_thread_id() != target.cas_thread_id()
        || reverse.cas_turn_id() != request.resolving_target.cas_turn_id()
        || reverse.thread_id() != request.thread_id
        || reverse.turn_id() != turn_id
        || reverse.binding_revision() != target.binding_revision()
        || reverse.snapshot_id() != target.snapshot_id()
    {
        return Err(SyndicMutationError::CasTurnOwnershipConflict);
    }
    Ok(())
}
