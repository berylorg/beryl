use beryl_model::{CasLoadedSessionGeneration, CasThreadId};
use syndic_storage::{
    AbandonActiveBinding, AcceptedRouteLostTarget, ActiveCasTurnRecord, BindingState,
    InputGateRecord, InputGateState, PendingSteeringTargetProof, StaleCasBinding,
    SteeringTargetProof, SyndicPointReadLimit, SyndicStorage,
};

use super::{ActiveBindingLossDisposition, ProviderBrokerLossError, STALE_REASON};
use crate::cas_projection::{PendingTurnActivation, publication};

#[allow(clippy::too_many_arguments)]
pub(super) fn abandon_exact_active(
    home: &beryl_home_store::HomeStore,
    home_id: beryl_model::BerylHomeId,
    home_generation: beryl_home_store::HomeGeneration,
    storage: SyndicStorage,
    activation: &PendingTurnActivation,
    cas_thread_id: &CasThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    limit: SyndicPointReadLimit,
    disposition: ActiveBindingLossDisposition,
) -> Result<(), ProviderBrokerLossError> {
    let current = storage
        .current_binding(home, activation.thread_id(), limit)?
        .ok_or(ProviderBrokerLossError::TargetMismatch)?;
    let snapshot = storage
        .execution_snapshot(home, activation.snapshot_id(), limit)?
        .ok_or(ProviderBrokerLossError::TargetMismatch)?;
    let state = storage
        .turn_state(home, activation.turn_id(), limit)?
        .ok_or(ProviderBrokerLossError::TargetMismatch)?;
    let gate = storage
        .input_gate(home, activation.thread_id(), limit)?
        .ok_or(ProviderBrokerLossError::TargetMismatch)?;
    let active_turn = storage.active_cas_turn(home, activation.snapshot_id(), limit)?;
    let confirmed_current = storage.current_binding(home, activation.thread_id(), limit)?;
    let confirmed_snapshot = storage.execution_snapshot(home, activation.snapshot_id(), limit)?;
    let confirmed_state = storage.turn_state(home, activation.turn_id(), limit)?;
    let confirmed_gate = storage.input_gate(home, activation.thread_id(), limit)?;
    let confirmed_active_turn = storage.active_cas_turn(home, activation.snapshot_id(), limit)?;
    if confirmed_current.as_ref() != Some(&current)
        || confirmed_snapshot.as_ref() != Some(&snapshot)
        || confirmed_state.as_ref() != Some(&state)
        || confirmed_gate.as_ref() != Some(&gate)
        || confirmed_active_turn != active_turn
    {
        return Err(ProviderBrokerLossError::ConcurrentChange);
    }
    if state.lifecycle().is_proven_terminal() {
        return Err(ProviderBrokerLossError::TargetMismatch);
    }
    if snapshot.thread_id() != activation.thread_id()
        || snapshot.id() != activation.snapshot_id()
        || snapshot.binding_revision() != activation.binding_revision()
        || snapshot.activation_gate_revision() != activation.gate_revision()
        || snapshot.active_turn_id() != activation.turn_id()
        || snapshot.cas_thread_id() != cas_thread_id
        || snapshot.loaded_generation() != loaded_generation
        || snapshot.started_at() != activation.observed_at()
        || snapshot.selected_path() != current.binding().selected_path()
    {
        return Err(ProviderBrokerLossError::TargetMismatch);
    }
    let expected_stale = StaleCasBinding::new(
        snapshot.execution().clone(),
        cas_thread_id.clone(),
        Some(snapshot.tool_profile()),
        Some(snapshot.represented_base_prefix()),
        Some(snapshot.lineage()),
        Some(snapshot.represented_base_native_turn_count()),
        Some(loaded_generation),
        STALE_REASON,
        snapshot.started_at(),
    )?;
    match current.binding().state() {
        BindingState::Active(active)
            if current.binding().revision() == activation.binding_revision()
                && active.turn_id() == activation.turn_id()
                && active.snapshot_id() == activation.snapshot_id()
                && active.activation_gate_revision() == activation.gate_revision()
                && active.started_at() == activation.observed_at()
                && active.usable().execution() == snapshot.execution()
                && active.usable().cas_thread_id() == cas_thread_id
                && active.usable().represented_prefix() == snapshot.represented_base_prefix()
                && active.usable().native_turn_count()
                    == snapshot.represented_base_native_turn_count()
                && active.usable().tool_profile() == snapshot.tool_profile()
                && active.usable().lineage() == snapshot.lineage() =>
        {
            let route = gate
                .selected_route()
                .ok_or(ProviderBrokerLossError::TargetMismatch)?;
            let pending_target = PendingSteeringTargetProof::new(
                activation.binding_revision(),
                activation.snapshot_id(),
                activation.turn_id(),
                cas_thread_id.clone(),
            );
            let lost_target = durable_lost_target(
                &gate,
                active_turn.as_ref(),
                activation,
                cas_thread_id,
                pending_target,
            )?;
            let request = match disposition {
                ActiveBindingLossDisposition::Generic => AbandonActiveBinding::new(
                    activation.thread_id(),
                    current.binding().revision(),
                    route.generation(),
                    lost_target,
                    current.binding().selected_path(),
                    expected_stale,
                ),
                ActiveBindingLossDisposition::ExactRejected(rejected) => {
                    AbandonActiveBinding::after_exact_rejection(
                        activation.thread_id(),
                        current.binding().revision(),
                        route.generation(),
                        lost_target,
                        current.binding().selected_path(),
                        expected_stale,
                        rejected,
                    )
                }
            };
            #[cfg(test)]
            if let Some(rejected) = request.exact_rejected_delivery() {
                crate::cas_projection::active_steering::pause_delivery_if_requested(
                    rejected.input_id(),
                    crate::cas_projection::active_steering::DeliveryPause::BeforeLossAbandonment,
                );
            }
            publication::abandon_active_reconciled(
                home,
                home_id,
                home_generation,
                storage,
                &request,
                limit,
            )?;
            Ok(())
        }
        BindingState::Stale(stale)
            if stale == &expected_stale && disposition == ActiveBindingLossDisposition::Generic =>
        {
            Ok(())
        }
        BindingState::Stale(_) if disposition != ActiveBindingLossDisposition::Generic => {
            Err(ProviderBrokerLossError::TargetMismatch)
        }
        BindingState::Unbound { .. } | BindingState::Valid(_) | BindingState::Active(_) => {
            Err(ProviderBrokerLossError::TargetMismatch)
        }
        BindingState::Stale(_) => Err(ProviderBrokerLossError::TargetMismatch),
    }
}

fn durable_lost_target(
    gate: &InputGateRecord,
    active_turn: Option<&ActiveCasTurnRecord>,
    activation: &PendingTurnActivation,
    cas_thread_id: &CasThreadId,
    pending: PendingSteeringTargetProof,
) -> Result<AcceptedRouteLostTarget, ProviderBrokerLossError> {
    match (gate.state(), active_turn) {
        (InputGateState::AwaitingSteering(turn), None) if *turn == activation.turn_id() => {
            Ok(AcceptedRouteLostTarget::AwaitingSteering(pending))
        }
        (InputGateState::Steerable(turn), Some(active_turn))
            if *turn == activation.turn_id()
                && active_turn.snapshot_id() == activation.snapshot_id()
                && active_turn.thread_id() == activation.thread_id()
                && active_turn.turn_id() == activation.turn_id()
                && active_turn.binding_revision() == activation.binding_revision()
                && active_turn.cas_thread_id() == cas_thread_id =>
        {
            Ok(AcceptedRouteLostTarget::Steering(SteeringTargetProof::new(
                pending,
                active_turn.cas_turn_id().clone(),
            )))
        }
        (InputGateState::AwaitingTerminal(turn), Some(active_turn))
            if *turn == activation.turn_id()
                && active_turn.snapshot_id() == activation.snapshot_id()
                && active_turn.thread_id() == activation.thread_id()
                && active_turn.turn_id() == activation.turn_id()
                && active_turn.binding_revision() == activation.binding_revision()
                && active_turn.cas_thread_id() == cas_thread_id =>
        {
            Ok(AcceptedRouteLostTarget::AwaitingTerminal(
                SteeringTargetProof::new(pending, active_turn.cas_turn_id().clone()),
            ))
        }
        _ => Err(ProviderBrokerLossError::TargetMismatch),
    }
}
