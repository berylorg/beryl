use beryl_model::{InputGateRevision, SyndicThreadId};

use crate::{
    AcceptedInputLifecycle, AcceptedInputRecord, AcceptedRouteGenerationHeadRecord,
    AcceptedRouteGenerationRecord, AcceptedRouteLeafRecord, AcceptedRouteLeafState,
    ActiveCasTurnRecord, BindingHeadRecord, BindingLifecycle, BindingRecord, BindingState,
    ExecutionSnapshotRecord, InputGateRecord, InputGateState, SteeringTargetProof, SyndicReadError,
};

pub(super) fn input_leaf_identity_agrees(
    input: &AcceptedInputRecord,
    leaf: &AcceptedRouteLeafRecord,
) -> bool {
    leaf.input_id() == input.id()
        && leaf.thread_id() == input.thread_id()
        && leaf.generation() == input.route_generation()
        && leaf.ordinal() == input.ordinal()
}

pub(super) fn validate_route(
    input: &AcceptedInputRecord,
    leaf: &AcceptedRouteLeafRecord,
    gate: &InputGateRecord,
    head: &AcceptedRouteGenerationHeadRecord,
    generation: &AcceptedRouteGenerationRecord,
    target: &SteeringTargetProof,
) -> Result<(), SyndicReadError> {
    let InputGateState::Steerable(gate_turn) = gate.state() else {
        return Err(route_disagreement());
    };
    let route = gate.selected_route().ok_or_else(route_disagreement)?;
    let in_interval = generation
        .first_ordinal()
        .zip(generation.last_ordinal())
        .is_some_and(|(first, last)| first <= input.ordinal() && input.ordinal() <= last);
    if gate.thread_id() != input.thread_id()
        || input.admission_gate_revision() >= gate.revision()
        || gate.accepted_high_water() < input.ordinal().get()
        || gate
            .route_generation_high_water()
            .is_none_or(|high_water| high_water < input.route_generation())
        || gate.live_steering_count() == 0
        || head.thread_id() != input.thread_id()
        || head.proof() != route
        || route.generation() != input.route_generation()
        || generation.thread_id() != input.thread_id()
        || generation.generation() != route.generation()
        || generation.revision() != route.revision()
        || generation.input_count() == 0
        || generation.delivering_count() == 0
        || !in_interval
        || leaf.state() != AcceptedRouteLeafState::Routed
        || leaf.lifecycle() != AcceptedInputLifecycle::Delivering
        || target.pending().active_turn_id() != *gate_turn
    {
        return Err(route_disagreement());
    }
    Ok(())
}

pub(super) fn validate_execution(
    thread: SyndicThreadId,
    gate_revision: InputGateRevision,
    target: &SteeringTargetProof,
    head: &BindingHeadRecord,
    binding: &BindingRecord,
    snapshot: &ExecutionSnapshotRecord,
    active_turn: &ActiveCasTurnRecord,
) -> Result<(), SyndicReadError> {
    let BindingState::Active(active) = binding.state() else {
        return Err(execution_disagreement());
    };
    let pending = target.pending();
    let usable = active.usable();
    if head.thread_id() != thread
        || head.revision() != binding.revision()
        || head.lifecycle() != BindingLifecycle::Active
        || head.selected_path_digest() != binding.selected_path().digest()
        || binding.thread_id() != thread
        || binding.selected_path().tail() != Some(active.turn_id())
        || pending.binding_revision() != binding.revision()
        || pending.snapshot_id() != active.snapshot_id()
        || pending.active_turn_id() != active.turn_id()
        || pending.cas_thread_id() != usable.cas_thread_id()
        || snapshot.id() != pending.snapshot_id()
        || snapshot.thread_id() != thread
        || snapshot.binding_revision() != binding.revision()
        || snapshot.activation_gate_revision() != active.activation_gate_revision()
        || gate_revision < active.activation_gate_revision()
        || snapshot.active_turn_id() != active.turn_id()
        || snapshot.cas_thread_id() != usable.cas_thread_id()
        || snapshot.selected_path() != binding.selected_path()
        || snapshot.represented_base_prefix() != usable.represented_prefix()
        || snapshot.represented_base_native_turn_count() != usable.native_turn_count()
        || snapshot.tool_profile() != usable.tool_profile()
        || snapshot.lineage() != usable.lineage()
        || snapshot.execution() != usable.execution()
        || snapshot.started_at() != active.started_at()
        || active_turn.snapshot_id() != snapshot.id()
        || active_turn.thread_id() != thread
        || active_turn.turn_id() != active.turn_id()
        || active_turn.binding_revision() != binding.revision()
        || active_turn.cas_thread_id() != usable.cas_thread_id()
        || active_turn.cas_turn_id() != target.cas_turn_id()
        || active_turn.published_at() < snapshot.started_at()
        || usable
            .lineage()
            .recovered_injection_generation()
            .is_some_and(|generation| {
                snapshot.loaded_generation().process() != generation.process()
            })
        || usable
            .lineage()
            .recovered_completed_at()
            .is_some_and(|completed_at| snapshot.started_at() < completed_at)
    {
        return Err(execution_disagreement());
    }
    Ok(())
}

fn route_disagreement() -> SyndicReadError {
    SyndicReadError::Invariant("delivering accepted-input route relationships disagree")
}

fn execution_disagreement() -> SyndicReadError {
    SyndicReadError::Invariant("delivering steering execution relationships disagree")
}
