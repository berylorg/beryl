use crate::{
    AcceptedRouteLostTarget, AcceptedRouteTarget, ActiveCasBinding, ActiveCasTurnRecord,
    BindingState, HistorySummaryRecord, InputGateRecord, InputGateState,
    PendingSteeringTargetProof, SteeringTargetProof, SyndicCurrentBinding, TurnLifecycle,
};

use super::{facts, *};

pub(super) fn classify(
    facts: &facts::RecoveryFacts,
    gate: &InputGateRecord,
    summary: &HistorySummaryRecord,
    binding: &SyndicCurrentBinding,
) -> Result<ActiveDeliveryRecovery, DeliveryRecoveryClassificationError> {
    let BindingState::Active(active) = binding.binding().state() else {
        return corruption("delivery-recovery active classifier received a non-active binding");
    };
    let turn_id =
        gate.state()
            .blocking_turn_id()
            .ok_or(DeliveryRecoveryClassificationError::Corruption(
                "active binding has an idle delivery-recovery gate",
            ))?;
    let state = validate_blocking_turn(facts, turn_id)?;
    validate_current_tail(state, summary)?;
    if !valid_active_lifecycle(state.lifecycle(), state.source_event_count()) {
        return corruption("active delivery-recovery turn lifecycle is incoherent");
    }
    let route = selected_route(facts, gate)?;
    let snapshot = required(
        facts.snapshot.as_ref(),
        "active delivery-recovery binding snapshot is missing",
    )?;
    validate_snapshot(gate, summary, binding, active, snapshot)?;

    let lost_target = match route.target() {
        AcceptedRouteTarget::AwaitingSteering(target) => {
            if !matches!(
                gate.state(),
                InputGateState::AwaitingSteering(turn) if *turn == active.turn_id()
            ) || facts.active_turn.is_some()
                || state.lifecycle() != TurnLifecycle::Pending
            {
                return corruption("awaiting delivery-recovery authority disagrees");
            }
            validate_pending_target(target, active, snapshot)?;
            AcceptedRouteLostTarget::AwaitingSteering(target.clone())
        }
        AcceptedRouteTarget::Steering(target) => {
            if !matches!(
                gate.state(),
                InputGateState::Steerable(turn) if *turn == active.turn_id()
            ) || state.lifecycle() == TurnLifecycle::UnknownTerminal
            {
                return corruption("steerable delivery-recovery gate disagrees");
            }
            validate_pending_target(target.pending(), active, snapshot)?;
            let active_turn = required(
                facts.active_turn.as_ref(),
                "steering delivery-recovery authority has no active CAS turn",
            )?;
            validate_active_turn(active_turn, target, active, snapshot)?;
            AcceptedRouteLostTarget::Steering(target.clone())
        }
        AcceptedRouteTarget::AwaitingTerminal(target) => {
            if !matches!(
                gate.state(),
                InputGateState::AwaitingTerminal(turn) if *turn == active.turn_id()
            ) || state.lifecycle() != TurnLifecycle::UnknownTerminal
                || gate.live_steering_count() != 0
                || route.ready_retryable_count() != 0
                || route.delivering_count() != 0
                || route.delivering_logical_utf8_bytes() != 0
            {
                return corruption("awaiting-terminal delivery-recovery authority disagrees");
            }
            validate_pending_target(target.pending(), active, snapshot)?;
            let active_turn = required(
                facts.active_turn.as_ref(),
                "awaiting-terminal recovery authority has no active CAS turn",
            )?;
            validate_active_turn(active_turn, target, active, snapshot)?;
            AcceptedRouteLostTarget::AwaitingTerminal(target.clone())
        }
        AcceptedRouteTarget::NextTurn(_) | AcceptedRouteTarget::ProjectionLost(_) => {
            return corruption("active binding selected a non-active recovery route");
        }
    };
    let mut minimum = minimum_timestamp(state, summary).max(snapshot.started_at());
    if let Some(active_turn) = &facts.active_turn {
        minimum = minimum.max(active_turn.published_at());
    }
    Ok(ActiveDeliveryRecovery {
        snapshot: snapshot.clone(),
        current_gate_revision: gate.revision(),
        current_state_revision: state.revision(),
        route_generation: route.generation(),
        lost_target,
        minimum_timestamp: minimum,
    })
}

fn valid_active_lifecycle(lifecycle: TurnLifecycle, source_event_count: u64) -> bool {
    match lifecycle {
        TurnLifecycle::Pending => source_event_count == 0,
        TurnLifecycle::Active | TurnLifecycle::UnknownTerminal => source_event_count > 0,
        TurnLifecycle::Complete
        | TurnLifecycle::Interrupted
        | TurnLifecycle::Failed
        | TurnLifecycle::Incomplete => false,
    }
}

fn validate_snapshot(
    gate: &InputGateRecord,
    summary: &HistorySummaryRecord,
    binding: &SyndicCurrentBinding,
    active: &ActiveCasBinding,
    snapshot: &crate::ExecutionSnapshotRecord,
) -> Result<(), DeliveryRecoveryClassificationError> {
    if snapshot.id() != active.snapshot_id()
        || snapshot.thread_id() != gate.thread_id()
        || snapshot.binding_revision() != binding.binding().revision()
        || snapshot.activation_gate_revision() != active.activation_gate_revision()
        || snapshot.activation_gate_revision() > gate.revision()
        || snapshot.active_turn_id() != active.turn_id()
        || snapshot.selected_path().tail() != Some(active.turn_id())
        || snapshot.selected_path().digest() != summary.selected_path_digest()
        || snapshot.cas_thread_id() != active.usable().cas_thread_id()
        || snapshot.selected_path() != binding.binding().selected_path()
        || snapshot.execution() != active.usable().execution()
        || snapshot.represented_base_prefix() != active.usable().represented_prefix()
        || snapshot.represented_base_native_turn_count() != active.usable().native_turn_count()
        || snapshot.tool_profile() != active.usable().tool_profile()
        || snapshot.lineage() != active.usable().lineage()
        || snapshot.started_at() != active.started_at()
    {
        return corruption("active delivery-recovery snapshot authority disagrees");
    }
    Ok(())
}

fn validate_pending_target(
    pending: &PendingSteeringTargetProof,
    active: &ActiveCasBinding,
    snapshot: &crate::ExecutionSnapshotRecord,
) -> Result<(), DeliveryRecoveryClassificationError> {
    if pending.binding_revision() != snapshot.binding_revision()
        || pending.snapshot_id() != snapshot.id()
        || pending.active_turn_id() != active.turn_id()
        || pending.cas_thread_id() != active.usable().cas_thread_id()
    {
        return corruption("active delivery-recovery route target disagrees");
    }
    Ok(())
}

fn validate_active_turn(
    active_turn: &ActiveCasTurnRecord,
    target: &SteeringTargetProof,
    active: &ActiveCasBinding,
    snapshot: &crate::ExecutionSnapshotRecord,
) -> Result<(), DeliveryRecoveryClassificationError> {
    if active_turn.snapshot_id() != snapshot.id()
        || active_turn.thread_id() != snapshot.thread_id()
        || active_turn.turn_id() != active.turn_id()
        || active_turn.binding_revision() != snapshot.binding_revision()
        || active_turn.cas_thread_id() != snapshot.cas_thread_id()
        || active_turn.cas_turn_id() != target.cas_turn_id()
        || active_turn.published_at() < snapshot.started_at()
    {
        return corruption("active delivery-recovery CAS-turn authority disagrees");
    }
    Ok(())
}
