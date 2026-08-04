use super::*;

pub(super) fn provider_operation_authenticates(
    observed: &StopObservation,
    stop: &StopOperationRecord,
) -> bool {
    let (
        Some(gate),
        Some(binding_head),
        Some(binding),
        Some(reservation),
        Some(membership),
        Some(cas_turn),
        Some(snapshot),
        Some(active_turn),
        Some(turn),
        Some(state),
        Some(compaction),
    ) = (
        observed.gate.as_ref(),
        observed.binding_head.as_ref(),
        observed.binding.as_ref(),
        observed.reservation.as_ref(),
        observed.membership.as_ref(),
        observed.cas_turn.as_ref(),
        observed.snapshot.as_ref(),
        observed.active_turn.as_ref(),
        observed.turn.as_ref(),
        observed.turn_state.as_ref(),
        observed.compaction.as_ref(),
    )
    else {
        return false;
    };
    let target = stop.target();
    let BindingState::Valid(usable) = binding.state() else {
        return false;
    };
    let (Some(source_compaction_revision), Some(successor_compaction_revision)) = (
        stop.admission().source_compaction_revision(),
        stop.admission().successor_compaction_revision(),
    ) else {
        return false;
    };
    let immutable = stop.admission().successor_stopped_route_option().is_none()
        && source_compaction_revision.checked_next().ok() == Some(successor_compaction_revision)
        && stop.id().thread_id() == target.thread_id()
        && target.turn_kind()
            == crate::TurnKind::ProviderOperation(crate::ProviderOperationKind::ContextCompaction)
        && turn.parent() == crate::ConversationParent::Root
        && turn.id() == target.turn_id()
        && turn.origin_thread_id() == target.thread_id()
        && turn.kind() == target.turn_kind()
        && state.turn_id() == target.turn_id()
        && gate.thread_id() == target.thread_id()
        && binding_head.thread_id() == target.thread_id()
        && binding.thread_id() == target.thread_id()
        && binding.revision() == target.binding_revision()
        && binding.selected_path() == snapshot.selected_path()
        && usable.execution().runtime_id() == target.runtime_id()
        && usable.execution() == snapshot.execution()
        && usable.cas_thread_id() == target.cas_thread_id()
        && usable.represented_prefix() == snapshot.represented_base_prefix()
        && usable.native_turn_count() == snapshot.represented_base_native_turn_count()
        && usable.tool_profile() == snapshot.tool_profile()
        && usable.lineage() == snapshot.lineage()
        && reservation.cas_thread_id() == target.cas_thread_id()
        && reservation.thread_id() == target.thread_id()
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
        && snapshot.kind()
            == crate::ExecutionSnapshotKind::ProviderOperation(
                crate::ProviderOperationKind::ContextCompaction,
            )
        && snapshot.active_turn_id() == target.turn_id()
        && snapshot.cas_thread_id() == target.cas_thread_id()
        && snapshot.execution().runtime_id() == target.runtime_id()
        && snapshot.loaded_generation() == target.loaded_generation()
        && active_turn.snapshot_id() == target.snapshot_id()
        && active_turn.thread_id() == target.thread_id()
        && active_turn.turn_id() == target.turn_id()
        && active_turn.binding_revision() == target.binding_revision()
        && active_turn.cas_thread_id() == target.cas_thread_id()
        && active_turn.cas_turn_id() == target.cas_turn_id()
        && compaction.id().thread_id() == target.thread_id()
        && compaction.id().provider_turn_id() == target.turn_id()
        && compaction.revision() >= successor_compaction_revision
        && compaction.target().thread_id() == target.thread_id()
        && compaction.target().turn_id() == target.turn_id()
        && compaction.target().snapshot_id() == target.snapshot_id()
        && compaction.target().binding_revision() == target.binding_revision()
        && compaction.target().runtime_id() == target.runtime_id()
        && compaction.target().loaded_generation() == target.loaded_generation()
        && compaction.target().cas_thread_id() == target.cas_thread_id()
        && compaction
            .cas_turn()
            .is_some_and(|value| value.cas_turn_id() == target.cas_turn_id());
    if !immutable {
        return false;
    }
    match stop.state() {
        StopOperationState::Admitted | StopOperationState::DispatchClaimed => {
            binding_head.revision() == target.binding_revision()
                && binding_head.lifecycle() == BindingLifecycle::Valid
                && binding_head.selected_path_digest() == binding.selected_path().digest()
                && gate.revision() >= stop.admission().successor_gate_revision()
                && gate.state() == &InputGateState::stopping(target.turn_id(), stop.id().nonce())
                && compaction.stopping_descendant_is_exact(
                    stop.id().nonce(),
                    source_compaction_revision,
                    successor_compaction_revision,
                )
                && reservation.latest_binding_revision() == target.binding_revision()
                && reservation.retired_binding_revision().is_none()
                && matches!(
                    state.lifecycle(),
                    crate::TurnLifecycle::Pending | crate::TurnLifecycle::Active
                )
        }
        StopOperationState::SafeReopened(witness) => {
            let crate::StopSafeReopenWitness::ProviderOperation {
                source_compaction_revision: source,
                successor_compaction_revision: successor,
                ..
            } = witness
            else {
                return false;
            };
            compaction.safe_reopen_descendant_is_exact(
                source_compaction_revision,
                successor_compaction_revision,
                source,
                successor,
                observed.compaction_receipt.as_ref(),
            ) && gate.revision() >= witness.successor_gate_revision()
                && (gate.revision() != witness.successor_gate_revision()
                    || gate.state()
                        == &InputGateState::compacting(target.turn_id(), compaction.id().nonce()))
        }
        StopOperationState::MatchingTerminal(witness) => {
            let crate::StopMatchingTerminalWitness::ProviderOperation {
                source_compaction_revision: source,
                successor_compaction_revision: successor,
                ..
            } = witness
            else {
                return false;
            };
            compaction.matching_terminal_descendant_is_exact(
                source_compaction_revision,
                successor_compaction_revision,
                source,
                successor,
                witness.successor_turn_state_revision(),
                observed.compaction_receipt.as_ref(),
            ) && gate.revision() >= witness.successor_gate_revision()
                && state.revision() >= witness.successor_turn_state_revision()
                && state.lifecycle().is_proven_terminal()
        }
        StopOperationState::Abandoned(witness) => {
            let crate::StopAbandonmentWitness::ProviderOperation {
                source_compaction_revision: source,
                successor_compaction_revision: successor_compaction,
                ..
            } = witness
            else {
                return false;
            };
            let (Some(successor), Some(receipt)) = (
                observed.successor_binding.as_ref(),
                observed.compaction_receipt.as_ref(),
            ) else {
                return false;
            };
            gate.revision() >= witness.successor_gate_revision()
                && state.revision() >= witness.successor_turn_state_revision()
                && state.lifecycle() == crate::TurnLifecycle::Incomplete
                && state.incomplete_reason() == Some(crate::TurnIncompleteReason::AuthorityLost)
                && matches!(successor.state(), BindingState::Stale(_))
                && reservation.retired_binding_revision() == Some(successor.revision())
                && matches!(
                    compaction.state(),
                    crate::CompactionOperationState::Consumed(_)
                )
                && compaction.stop_abandonment_successor_is_exact(
                    source_compaction_revision,
                    successor_compaction_revision,
                    source,
                    successor_compaction,
                    receipt,
                )
        }
    }
}

pub(super) fn route_matches(observed: &StopObservation, stop: &StopOperationRecord) -> bool {
    let Some(route) = observed.admission_route.as_ref() else {
        return false;
    };
    let stopped = stop.admission().successor_stopped_route();
    if route.thread_id() != stop.target().thread_id()
        || route.generation() != stopped.generation()
        || route.revision() < stopped.revision()
        || route.ready_retryable_count() != 0
        || route.delivering_count() != 0
        || route.delivering_logical_utf8_bytes() != 0
    {
        return false;
    }
    match stop.state() {
        StopOperationState::Abandoned(witness) => {
            super::abandoned_route_matches(stop, witness, route)
        }
        _ => {
            route.target() == &AcceptedRouteTarget::NextTurn(NextTurnReason::Stop)
                && (!stop.state().is_live() || route.revision() == stopped.revision())
        }
    }
}
