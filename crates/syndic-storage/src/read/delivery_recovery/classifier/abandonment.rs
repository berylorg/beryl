use crate::{
    AcceptedRouteLostTarget, AcceptedRouteProjectionLostProof, AcceptedRouteTarget,
    ActiveCasBinding, ActiveCasTurnRecord, BindingRecord, BindingState, HistorySummaryRecord,
    InputGateRecord, PendingSteeringTargetProof, StaleCasBinding, SteeringTargetProof,
    SyndicCurrentBinding, TurnStateRecord,
};

use super::{facts, *};

pub(super) fn classify(
    facts: &facts::RecoveryFacts,
    gate: &InputGateRecord,
    state: &TurnStateRecord,
    summary: &HistorySummaryRecord,
    current: &SyndicCurrentBinding,
) -> Result<DeliveryRecoveryCase, DeliveryRecoveryClassificationError> {
    let route = selected_route(facts, gate)?;
    let AcceptedRouteTarget::ProjectionLost(proof) = route.target() else {
        return corruption("pending recovery gate selected a non-loss route");
    };
    let BindingState::Stale(stale) = current.binding().state() else {
        return corruption("post-abandonment recovery has no stale binding successor");
    };
    let prior = required(
        facts.prior_binding.as_ref(),
        "post-abandonment recovery prior active binding is missing",
    )?;
    let BindingState::Active(active) = prior.state() else {
        return corruption("post-abandonment recovery prior binding is not active");
    };
    let snapshot = required(
        facts.snapshot.as_ref(),
        "post-abandonment recovery snapshot is missing",
    )?;

    validate_proof(gate, route, current, prior, proof)?;
    validate_snapshot(gate, summary, prior, active, snapshot)?;
    validate_stale(stale, snapshot)?;
    validate_lost_target(facts.active_turn.as_ref(), proof, active, snapshot)?;

    let mut minimum = minimum_timestamp(state, summary)
        .max(snapshot.started_at())
        .max(stale.observed_at());
    if let Some(active_turn) = &facts.active_turn {
        minimum = minimum.max(active_turn.published_at());
    }
    Ok(DeliveryRecoveryCase::PostAbandonment {
        thread_id: gate.thread_id(),
        turn_id: state.turn_id(),
        minimum_timestamp: minimum,
    })
}

fn validate_proof(
    gate: &InputGateRecord,
    route: &crate::AcceptedRouteGenerationRecord,
    current: &SyndicCurrentBinding,
    prior: &BindingRecord,
    proof: &AcceptedRouteProjectionLostProof,
) -> Result<(), DeliveryRecoveryClassificationError> {
    let pending = proof.prior_target().pending();
    let abandonment = proof.abandonment();
    if pending.active_turn_id()
        != gate
            .state()
            .blocking_turn_id()
            .expect("pending gate has a turn")
        || pending.snapshot_id() != proof.snapshot_id()
        || pending.cas_thread_id() != proof.cas_thread_id()
        || pending.binding_revision() != abandonment.expected_binding_revision()
        || abandonment.expected_binding_revision() != prior.revision()
        || prior.revision().checked_next().ok() != Some(current.binding().revision())
        || proof.retirement_binding_revision() != current.binding().revision()
        || abandonment.expected_gate_revision() >= gate.revision()
        || abandonment.expected_route().generation() != route.generation()
        || abandonment.expected_route().revision().checked_next().ok() != Some(route.revision())
        || current.binding().selected_path() != prior.selected_path()
    {
        return corruption("post-abandonment projection-loss proof disagrees");
    }
    Ok(())
}

fn validate_snapshot(
    gate: &InputGateRecord,
    summary: &HistorySummaryRecord,
    prior: &BindingRecord,
    active: &ActiveCasBinding,
    snapshot: &crate::ExecutionSnapshotRecord,
) -> Result<(), DeliveryRecoveryClassificationError> {
    if snapshot.id() != active.snapshot_id()
        || snapshot.thread_id() != gate.thread_id()
        || snapshot.binding_revision() != prior.revision()
        || snapshot.activation_gate_revision() != active.activation_gate_revision()
        || snapshot.active_turn_id() != active.turn_id()
        || snapshot.selected_path().tail() != Some(active.turn_id())
        || snapshot.selected_path().digest() != summary.selected_path_digest()
        || snapshot.cas_thread_id() != active.usable().cas_thread_id()
        || snapshot.selected_path() != prior.selected_path()
        || snapshot.execution() != active.usable().execution()
        || snapshot.represented_base_prefix() != active.usable().represented_prefix()
        || snapshot.represented_base_native_turn_count() != active.usable().native_turn_count()
        || snapshot.tool_profile() != active.usable().tool_profile()
        || snapshot.lineage() != active.usable().lineage()
        || snapshot.started_at() != active.started_at()
    {
        return corruption("post-abandonment execution snapshot disagrees");
    }
    Ok(())
}

fn validate_stale(
    stale: &StaleCasBinding,
    snapshot: &crate::ExecutionSnapshotRecord,
) -> Result<(), DeliveryRecoveryClassificationError> {
    if stale.execution() != snapshot.execution()
        || stale.cas_thread_id() != snapshot.cas_thread_id()
        || stale.observed_tool_profile() != Some(snapshot.tool_profile())
        || stale.observed_prefix() != Some(snapshot.represented_base_prefix())
        || stale.observed_lineage() != Some(snapshot.lineage())
        || stale.observed_native_turn_count() != Some(snapshot.represented_base_native_turn_count())
        || stale.loaded_generation() != Some(snapshot.loaded_generation())
        || stale.observed_at() < snapshot.started_at()
    {
        return corruption("post-abandonment stale binding disagrees with its snapshot");
    }
    Ok(())
}

fn validate_lost_target(
    active_turn: Option<&ActiveCasTurnRecord>,
    proof: &AcceptedRouteProjectionLostProof,
    active: &ActiveCasBinding,
    snapshot: &crate::ExecutionSnapshotRecord,
) -> Result<(), DeliveryRecoveryClassificationError> {
    match proof.prior_target() {
        AcceptedRouteLostTarget::AwaitingSteering(pending) => {
            validate_pending_target(pending, active, snapshot)?;
            if active_turn.is_some() {
                return corruption(
                    "post-abandonment awaiting target unexpectedly has an active CAS turn",
                );
            }
        }
        AcceptedRouteLostTarget::Steering(target)
        | AcceptedRouteLostTarget::AwaitingTerminal(target) => {
            validate_pending_target(target.pending(), active, snapshot)?;
            let active_turn =
                active_turn.ok_or(DeliveryRecoveryClassificationError::Corruption(
                    "post-abandonment steering target has no active CAS turn",
                ))?;
            validate_active_turn(active_turn, target, snapshot)?;
        }
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
        return corruption("post-abandonment prior target disagrees");
    }
    Ok(())
}

fn validate_active_turn(
    active_turn: &ActiveCasTurnRecord,
    target: &SteeringTargetProof,
    snapshot: &crate::ExecutionSnapshotRecord,
) -> Result<(), DeliveryRecoveryClassificationError> {
    if active_turn.snapshot_id() != snapshot.id()
        || active_turn.thread_id() != snapshot.thread_id()
        || active_turn.turn_id() != snapshot.active_turn_id()
        || active_turn.binding_revision() != snapshot.binding_revision()
        || active_turn.cas_thread_id() != snapshot.cas_thread_id()
        || active_turn.cas_turn_id() != target.cas_turn_id()
        || active_turn.published_at() < snapshot.started_at()
    {
        return corruption("post-abandonment active CAS-turn witness disagrees");
    }
    Ok(())
}
