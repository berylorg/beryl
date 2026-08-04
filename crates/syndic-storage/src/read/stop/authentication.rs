use crate::{
    AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord, AcceptedRouteTarget,
    BindingLifecycle, BindingState, InputGateRecord, InputGateState, NextTurnReason,
    SourceEventPayload, StopOperationRecord, StopOperationState, StopOperationTarget,
};

use super::observation::StopObservation;

mod admission;

pub(in crate::read) fn observation_authenticates_record(observed: &StopObservation) -> bool {
    let Some(stop) = observed.stop.as_ref() else {
        return false;
    };
    if stop.admission().is_provider_operation() {
        return admission::provider_operation_authenticates(observed, stop);
    }
    if !admission::route_matches(observed, stop) {
        return false;
    }
    match stop.state() {
        StopOperationState::Admitted | StopOperationState::DispatchClaimed => {
            let (Some(gate), Some(head), Some(route)) = (
                observed.gate.as_ref(),
                observed.route_head.as_ref(),
                observed.route.as_ref(),
            ) else {
                return false;
            };
            live_successor_matches(observed, stop, gate, head, route)
        }
        StopOperationState::SafeReopened(witness) => {
            safe_reopen_successor_matches(observed, stop, witness)
        }
        StopOperationState::MatchingTerminal(witness) => {
            terminal_successor_matches(observed, stop, witness)
        }
        StopOperationState::Abandoned(witness) => {
            abandonment_successor_matches(observed, stop, witness)
        }
    }
}

pub(super) fn live_successor_matches(
    observed: &StopObservation,
    stop: &StopOperationRecord,
    gate: &InputGateRecord,
    head: &AcceptedRouteGenerationHeadRecord,
    route: &AcceptedRouteGenerationRecord,
) -> bool {
    let Some(selected) = gate.selected_route() else {
        return false;
    };
    gate.revision() >= stop.admission().successor_gate_revision()
        && gate.state() == &InputGateState::stopping(stop.target().turn_id(), stop.id().nonce())
        && gate.live_steering_count() == 0
        && selected == stop.admission().successor_stopped_route()
        && head.thread_id() == stop.target().thread_id()
        && head.proof() == selected
        && route.thread_id() == stop.target().thread_id()
        && route.generation() == selected.generation()
        && route.revision() == selected.revision()
        && route.target() == &AcceptedRouteTarget::NextTurn(NextTurnReason::Stop)
        && route.ready_retryable_count() == 0
        && route.delivering_count() == 0
        && route.delivering_logical_utf8_bytes() == 0
        && route_sources_match(observed, gate, route)
        && target_authority_matches(observed, stop.target())
}

pub(in crate::read) fn route_sources_match(
    observed: &StopObservation,
    gate: &InputGateRecord,
    route: &AcceptedRouteGenerationRecord,
) -> bool {
    let ready_matches = match (
        route.ready_retryable_count(),
        observed.ready_source.as_ref(),
    ) {
        (0, None) => true,
        (count, Some(source)) if count > 0 => {
            source.thread_id() == route.thread_id()
                && source.gate_revision() == gate.revision()
                && source.generation() == route.generation()
                && source.generation_revision() == route.revision()
                && Some(source.first_ordinal()) == route.first_ordinal()
                && Some(source.last_ordinal()) == route.last_ordinal()
        }
        _ => false,
    };
    let next_matches = match (route.next_turn_count(), observed.next_source.as_ref()) {
        (0, None) => true,
        (count, Some(source)) if count > 0 => {
            source.thread_id() == route.thread_id()
                && source.generation() == route.generation()
                && source.generation_revision() == route.revision()
                && Some(source.first_ordinal()) == route.first_ordinal()
                && Some(source.last_ordinal()) == route.last_ordinal()
        }
        _ => false,
    };
    ready_matches && next_matches
}

fn safe_reopen_successor_matches(
    observed: &StopObservation,
    stop: &StopOperationRecord,
    witness: crate::StopSafeReopenWitness,
) -> bool {
    let (Some(gate), Some(head), Some(route)) = (
        observed.gate.as_ref(),
        observed.route_head.as_ref(),
        observed.route.as_ref(),
    ) else {
        return false;
    };
    let Some(selected) = gate.selected_route() else {
        return false;
    };
    let expected_target = steering_target(stop.target());
    let target_matches = match route.target() {
        AcceptedRouteTarget::Steering(target) => target == &expected_target,
        AcceptedRouteTarget::ProjectionLost(lost) => {
            matches!(
                lost.prior_target(),
                crate::AcceptedRouteLostTarget::Steering(target)
                    if target == &expected_target
            ) && lost.abandonment().expected_route().generation()
                == witness.successor_route().generation()
                && lost.abandonment().expected_route().revision()
                    >= witness.successor_route().revision()
        }
        AcceptedRouteTarget::NextTurn(NextTurnReason::Stop) => {
            route.revision() > witness.successor_route().revision()
        }
        _ => false,
    };
    !stop
        .causes()
        .contains(crate::StopCause::InterruptingApproval)
        && gate.revision() >= witness.successor_gate_revision()
        && selected.generation() == witness.successor_route().generation()
        && selected.revision() >= witness.successor_route().revision()
        && head.thread_id() == stop.target().thread_id()
        && head.proof() == selected
        && route.thread_id() == stop.target().thread_id()
        && route.generation() == selected.generation()
        && route.revision() == selected.revision()
        && gate
            .route_generation_high_water()
            .is_some_and(|high_water| high_water >= selected.generation())
        && (gate.revision() != witness.successor_gate_revision()
            || (gate.state() == &InputGateState::Steerable(stop.target().turn_id())
                && selected == witness.successor_route()
                && route.input_count() == 0
                && route.ready_retryable_count() == 0
                && route.delivering_count() == 0
                && route.next_turn_count() == 0
                && route.live_logical_utf8_bytes() == 0
                && route.delivering_logical_utf8_bytes() == 0))
        && target_matches
        && route_sources_match(observed, gate, route)
}

fn terminal_successor_matches(
    observed: &StopObservation,
    stop: &StopOperationRecord,
    witness: crate::StopMatchingTerminalWitness,
) -> bool {
    finalizing_successor_matches(
        observed,
        stop,
        witness.successor_gate_revision(),
        witness.successor_turn_state_revision(),
        false,
    ) && binding_successor_matches(observed, stop, false)
}

fn abandonment_successor_matches(
    observed: &StopObservation,
    stop: &StopOperationRecord,
    witness: crate::StopAbandonmentWitness,
) -> bool {
    finalizing_successor_matches(
        observed,
        stop,
        witness.successor_gate_revision(),
        witness.successor_turn_state_revision(),
        true,
    ) && Some(witness.retired_binding_revision())
        == stop.target().binding_revision().checked_next().ok()
        && binding_successor_matches(observed, stop, true)
        && observed
            .route
            .as_ref()
            .is_some_and(|route| abandoned_route_matches(stop, witness, route))
}

fn finalizing_successor_matches(
    observed: &StopObservation,
    stop: &StopOperationRecord,
    successor_gate: beryl_model::InputGateRevision,
    successor_state: crate::TurnStateRevision,
    authority_lost: bool,
) -> bool {
    let (Some(gate), Some(head), Some(route), Some(state), Some(event)) = (
        observed.gate.as_ref(),
        observed.route_head.as_ref(),
        observed.route.as_ref(),
        observed.turn_state.as_ref(),
        observed.latest_event.as_ref(),
    ) else {
        return false;
    };
    let Some(selected) = gate.selected_route() else {
        return false;
    };
    let lifecycle_matches = if authority_lost {
        state.lifecycle() == crate::TurnLifecycle::Incomplete
            && state.incomplete_reason() == Some(crate::TurnIncompleteReason::AuthorityLost)
            && event.source().is_none()
    } else {
        state.lifecycle().is_proven_terminal()
            && event.source().is_some_and(|source| {
                source.thread_id() == stop.target().cas_thread_id()
                    && source.turn_id() == stop.target().cas_turn_id()
            })
    };
    let route_matches = if authority_lost {
        route.generation() == stop.admission().successor_stopped_route().generation()
    } else {
        selected == stop.admission().successor_stopped_route()
            && route.target() == &AcceptedRouteTarget::NextTurn(NextTurnReason::Stop)
    };
    gate.revision() >= successor_gate
        && (gate.revision() != successor_gate
            || gate.state() == &InputGateState::FinalizingHistory(stop.target().turn_id()))
        && head.thread_id() == stop.target().thread_id()
        && head.proof() == selected
        && route.thread_id() == stop.target().thread_id()
        && route.generation() == selected.generation()
        && route.revision() == selected.revision()
        && route.ready_retryable_count() == 0
        && route.delivering_count() == 0
        && route.delivering_logical_utf8_bytes() == 0
        && route_matches
        && state.turn_id() == stop.target().turn_id()
        && state.revision() >= successor_state
        && state.source_event_count() == event.sequence().get()
        && matches!(
            event.payload(),
            SourceEventPayload::TurnEnded(status) if state.end_status() == Some(*status)
        )
        && lifecycle_matches
        && route_sources_match(observed, gate, route)
}

fn binding_successor_matches(
    observed: &StopObservation,
    stop: &StopOperationRecord,
    retired: bool,
) -> bool {
    let (
        Some(thread),
        Some(old_binding),
        Some(successor),
        Some(head),
        Some(reservation),
        Some(membership),
    ) = (
        observed.thread.as_ref(),
        observed.binding.as_ref(),
        observed.successor_binding.as_ref(),
        observed.binding_head.as_ref(),
        observed.reservation.as_ref(),
        observed.successor_membership.as_ref(),
    )
    else {
        return false;
    };
    let Ok(successor_revision) = stop.target().binding_revision().checked_next() else {
        return false;
    };
    let state_matches = match (retired, successor.state(), old_binding.state()) {
        (false, BindingState::Valid(usable), BindingState::Active(active)) => {
            usable.execution() == active.usable().execution()
                && usable.cas_thread_id() == stop.target().cas_thread_id()
                && usable.represented_prefix().tail() == Some(stop.target().turn_id())
                && usable.represented_prefix().digest() == thread.selected_path_digest()
                && usable.tool_profile() == active.usable().tool_profile()
                && usable.lineage() == active.usable().lineage()
                && active
                    .usable()
                    .native_turn_count()
                    .checked_next()
                    .is_ok_and(|count| usable.native_turn_count() == count)
        }
        (true, BindingState::Stale(stale), BindingState::Active(active)) => {
            stale.execution() == active.usable().execution()
                && stale.cas_thread_id() == stop.target().cas_thread_id()
                && stale.observed_tool_profile() == Some(active.usable().tool_profile())
                && stale.observed_prefix() == Some(active.usable().represented_prefix())
                && stale.observed_lineage() == Some(active.usable().lineage())
                && stale.observed_native_turn_count() == Some(active.usable().native_turn_count())
                && stale.loaded_generation() == Some(stop.target().loaded_generation())
        }
        _ => false,
    };
    let successor_path_matches = if retired {
        successor.selected_path() == thread.selected_path()
    } else {
        successor.selected_path() == old_binding.selected_path()
    };
    successor.thread_id() == stop.target().thread_id()
        && successor.revision() == successor_revision
        && successor_path_matches
        && head.thread_id() == stop.target().thread_id()
        && head.revision() == successor_revision
        && head.lifecycle()
            == if retired {
                BindingLifecycle::Stale
            } else {
                BindingLifecycle::Valid
            }
        && head.selected_path_digest() == thread.selected_path_digest()
        && reservation.cas_thread_id() == stop.target().cas_thread_id()
        && reservation.thread_id() == stop.target().thread_id()
        && reservation.latest_binding_revision() == successor_revision
        && reservation.retired_binding_revision()
            == if retired {
                Some(successor_revision)
            } else {
                None
            }
        && membership.cas_thread_id() == stop.target().cas_thread_id()
        && membership.thread_id() == stop.target().thread_id()
        && membership.binding_revision() == successor_revision
        && state_matches
}

fn abandoned_route_matches(
    stop: &StopOperationRecord,
    witness: crate::StopAbandonmentWitness,
    route: &AcceptedRouteGenerationRecord,
) -> bool {
    let AcceptedRouteTarget::ProjectionLost(lost) = route.target() else {
        return false;
    };
    let stopped = stop.admission().successor_stopped_route();
    let expected_target = steering_target(stop.target());
    matches!(
        lost.prior_target(),
        crate::AcceptedRouteLostTarget::Steering(target) if target == &expected_target
    ) && lost.abandonment().expected_binding_revision() == stop.target().binding_revision()
        && lost.abandonment().expected_gate_revision() == witness.source().gate_revision()
        && lost.abandonment().expected_route() == stopped
        && lost.abandonment().kind() == crate::AcceptedRouteAbandonmentKind::Generic
        && lost.retirement_binding_revision() == witness.retired_binding_revision()
        && lost.snapshot_id() == stop.target().snapshot_id()
        && lost.cas_thread_id() == stop.target().cas_thread_id()
        && route.revision() > stopped.revision()
}

pub(in crate::read) fn target_authority_matches(
    observed: &StopObservation,
    target: &StopOperationTarget,
) -> bool {
    let (
        Some(thread),
        Some(head),
        Some(binding),
        Some(reservation),
        Some(membership),
        Some(cas_turn),
        Some(snapshot),
        Some(active_turn),
        Some(turn),
        Some(turn_state),
    ) = (
        observed.thread.as_ref(),
        observed.binding_head.as_ref(),
        observed.binding.as_ref(),
        observed.reservation.as_ref(),
        observed.membership.as_ref(),
        observed.cas_turn.as_ref(),
        observed.snapshot.as_ref(),
        observed.active_turn.as_ref(),
        observed.turn.as_ref(),
        observed.turn_state.as_ref(),
    )
    else {
        return false;
    };
    let BindingState::Active(active) = binding.state() else {
        return false;
    };
    thread.id() == target.thread_id()
        && thread.committed_tail() == Some(target.turn_id())
        && head.thread_id() == target.thread_id()
        && head.revision() == target.binding_revision()
        && head.lifecycle() == BindingLifecycle::Active
        && head.selected_path_digest() == thread.selected_path_digest()
        && binding.thread_id() == target.thread_id()
        && binding.revision() == target.binding_revision()
        && binding.selected_path().tail() == thread.committed_tail()
        && binding.selected_path().digest() == thread.selected_path_digest()
        && binding.selected_path().thread_revision() <= thread.revision()
        && active.snapshot_id() == target.snapshot_id()
        && active.turn_id() == target.turn_id()
        && active.usable().execution().runtime_id() == target.runtime_id()
        && active.usable().cas_thread_id() == target.cas_thread_id()
        && reservation.cas_thread_id() == target.cas_thread_id()
        && reservation.thread_id() == target.thread_id()
        && reservation.latest_binding_revision() == target.binding_revision()
        && reservation.retired_binding_revision().is_none()
        && membership.cas_thread_id() == target.cas_thread_id()
        && membership.thread_id() == target.thread_id()
        && membership.binding_revision() == target.binding_revision()
        && cas_turn.cas_thread_id() == target.cas_thread_id()
        && cas_turn.cas_turn_id() == target.cas_turn_id()
        && cas_turn.thread_id() == target.thread_id()
        && cas_turn.turn_id() == target.turn_id()
        && cas_turn.binding_revision() == target.binding_revision()
        && cas_turn.snapshot_id() == target.snapshot_id()
        && snapshot.id() == target.snapshot_id()
        && snapshot.thread_id() == target.thread_id()
        && snapshot.binding_revision() == target.binding_revision()
        && snapshot.active_turn_id() == target.turn_id()
        && snapshot.execution() == active.usable().execution()
        && snapshot.loaded_generation() == target.loaded_generation()
        && snapshot.cas_thread_id() == target.cas_thread_id()
        && snapshot.selected_path() == binding.selected_path()
        && active_turn.snapshot_id() == target.snapshot_id()
        && active_turn.thread_id() == target.thread_id()
        && active_turn.turn_id() == target.turn_id()
        && active_turn.binding_revision() == target.binding_revision()
        && active_turn.cas_thread_id() == target.cas_thread_id()
        && active_turn.cas_turn_id() == target.cas_turn_id()
        && turn.id() == target.turn_id()
        && turn.origin_thread_id() == target.thread_id()
        && turn.kind() == target.turn_kind()
        && turn_state.turn_id() == target.turn_id()
        && matches!(
            turn_state.lifecycle(),
            crate::TurnLifecycle::Pending | crate::TurnLifecycle::Active
        )
}

pub(in crate::read) fn steerable_target_matches(
    observed: &StopObservation,
    target: &StopOperationTarget,
) -> bool {
    let (Some(gate), Some(head), Some(route)) = (
        observed.gate.as_ref(),
        observed.route_head.as_ref(),
        observed.route.as_ref(),
    ) else {
        return false;
    };
    let Some(selected) = gate.selected_route() else {
        return false;
    };
    gate.thread_id() == target.thread_id()
        && gate.state() == &InputGateState::Steerable(target.turn_id())
        && head.thread_id() == target.thread_id()
        && head.proof() == selected
        && route.thread_id() == target.thread_id()
        && route.generation() == selected.generation()
        && route.revision() == selected.revision()
        && route.target() == &AcceptedRouteTarget::Steering(steering_target(target))
        && route
            .ready_retryable_count()
            .checked_add(route.delivering_count())
            == Some(gate.live_steering_count())
        && (route.delivering_count() != 0 || route.delivering_logical_utf8_bytes() == 0)
        && route_sources_match(observed, gate, route)
        && target_authority_matches(observed, target)
}

pub(super) fn steering_target(target: &StopOperationTarget) -> crate::SteeringTargetProof {
    crate::SteeringTargetProof::new(
        crate::PendingSteeringTargetProof::new(
            target.binding_revision(),
            target.snapshot_id(),
            target.turn_id(),
            target.cas_thread_id().clone(),
        ),
        target.cas_turn_id().clone(),
    )
}
