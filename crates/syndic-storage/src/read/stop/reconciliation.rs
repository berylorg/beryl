use beryl_home_store::HomeStore;

use crate::{
    AbandonStopOperation, AdmitStopOperation, BindingState, ClaimStopDispatch, JoinStopCause,
    LiveSourceEvent, SafelyReopenStopOperation, SourceEventPayload, SourceEventRecord,
    StopOperationId, StopOperationState, StopOperationTarget, SyndicReadError, SyndicStorage,
};

use super::{
    StopOperationTransitionStatus,
    authentication::{
        live_successor_matches, observation_authenticates_record, steerable_target_matches,
    },
    observation::{StopObservation, StopTerminalObservation},
};
use crate::read::SyndicPointReadLimit;

impl SyndicStorage {
    /// Reconciles ambiguous atomic stop admission against a stable fixed set of records.
    pub fn stop_admission_status(
        &self,
        store: &HomeStore,
        request: &AdmitStopOperation,
        limit: SyndicPointReadLimit,
    ) -> Result<StopOperationTransitionStatus, SyndicReadError> {
        let observed = self.stable_stop_observation(
            store,
            request.operation_id(),
            request.target(),
            limit,
            "stop admission reconciliation",
        )?;
        if admission_exact(&observed, request) {
            Ok(StopOperationTransitionStatus::Exact)
        } else if admission_prior(&observed, request) {
            Ok(StopOperationTransitionStatus::Prior)
        } else {
            Ok(StopOperationTransitionStatus::Collision)
        }
    }

    /// Reconciles one monotonic cause join.
    pub fn stop_cause_join_status(
        &self,
        store: &HomeStore,
        request: &JoinStopCause,
        limit: SyndicPointReadLimit,
    ) -> Result<StopOperationTransitionStatus, SyndicReadError> {
        let observed = self.stable_stop_observation(
            store,
            request.operation_id(),
            request.target(),
            limit,
            "stop cause-join reconciliation",
        )?;
        let Some(stop) = observed.stop.as_ref().filter(|stop| {
            stop.id() == request.operation_id() && stop.target() == request.target()
        }) else {
            return Ok(StopOperationTransitionStatus::Collision);
        };
        if stop.cause_first_revisions().first_revision(request.cause())
            == request.expected_stop_revision().checked_next().ok()
            && observation_authenticates_record(&observed)
        {
            Ok(StopOperationTransitionStatus::Exact)
        } else if !stop.causes().contains(request.cause())
            && live_prior(
                &observed,
                request.operation_id(),
                request.target(),
                request.expected_gate_revision(),
                request.expected_stop_revision(),
            )
        {
            Ok(StopOperationTransitionStatus::Prior)
        } else {
            Ok(StopOperationTransitionStatus::Collision)
        }
    }

    /// Reconciles the sole dispatch claim for one stop operation.
    pub fn stop_dispatch_claim_status(
        &self,
        store: &HomeStore,
        request: &ClaimStopDispatch,
        limit: SyndicPointReadLimit,
    ) -> Result<StopOperationTransitionStatus, SyndicReadError> {
        let observed = self.stable_stop_observation(
            store,
            request.operation_id(),
            request.target(),
            limit,
            "stop dispatch-claim reconciliation",
        )?;
        let Some(stop) = observed.stop.as_ref().filter(|stop| {
            stop.id() == request.operation_id() && stop.target() == request.target()
        }) else {
            return Ok(StopOperationTransitionStatus::Collision);
        };
        if stop.dispatch_claim().is_some_and(|claim| {
            claim.source_revision() == request.expected_stop_revision()
                && claim.attempt() == request.attempt()
        }) && observation_authenticates_record(&observed)
        {
            Ok(StopOperationTransitionStatus::Exact)
        } else if stop.dispatch_claim().is_none()
            && stop.state() == StopOperationState::Admitted
            && live_prior(
                &observed,
                request.operation_id(),
                request.target(),
                request.expected_gate_revision(),
                request.expected_stop_revision(),
            )
        {
            Ok(StopOperationTransitionStatus::Prior)
        } else {
            Ok(StopOperationTransitionStatus::Collision)
        }
    }

    /// Reconciles a safe local-nondispatch reopening.
    pub fn safe_stop_reopen_status(
        &self,
        store: &HomeStore,
        request: &SafelyReopenStopOperation,
        limit: SyndicPointReadLimit,
    ) -> Result<StopOperationTransitionStatus, SyndicReadError> {
        let observed = self.stable_stop_observation(
            store,
            request.operation_id(),
            request.target(),
            limit,
            "safe stop-reopen reconciliation",
        )?;
        let Some(stop) = observed.stop.as_ref().filter(|stop| {
            stop.id() == request.operation_id() && stop.target() == request.target()
        }) else {
            return Ok(StopOperationTransitionStatus::Collision);
        };
        if matches!(
            stop.state(),
            StopOperationState::SafeReopened(witness)
                if witness.source().gate_revision() == request.expected_gate_revision()
                    && witness.source().stop_revision() == request.expected_stop_revision()
        ) && observation_authenticates_record(&observed)
        {
            Ok(StopOperationTransitionStatus::Exact)
        } else if !stop
            .causes()
            .contains(crate::StopCause::InterruptingApproval)
            && live_prior(
                &observed,
                request.operation_id(),
                request.target(),
                request.expected_gate_revision(),
                request.expected_stop_revision(),
            )
        {
            Ok(StopOperationTransitionStatus::Prior)
        } else {
            Ok(StopOperationTransitionStatus::Collision)
        }
    }

    /// Reconciles classified stop abandonment against its retained successor witness.
    pub fn stop_abandonment_status(
        &self,
        store: &HomeStore,
        request: &AbandonStopOperation,
        limit: SyndicPointReadLimit,
    ) -> Result<StopOperationTransitionStatus, SyndicReadError> {
        let observed = self.stable_stop_observation(
            store,
            request.operation_id(),
            request.target(),
            limit,
            "stop abandonment reconciliation",
        )?;
        if abandonment_exact(&observed, request) {
            Ok(StopOperationTransitionStatus::Exact)
        } else if observed.turn_state.as_ref().is_some_and(|state| {
            state.turn_id() == request.target().turn_id()
                && state.revision() == request.expected_state_revision()
        }) && live_prior(
            &observed,
            request.operation_id(),
            request.target(),
            request.expected_gate_revision(),
            request.expected_stop_revision(),
        ) {
            Ok(StopOperationTransitionStatus::Prior)
        } else {
            Ok(StopOperationTransitionStatus::Collision)
        }
    }

    /// Reconciles matching terminal consumption of one exact live stop operation.
    pub fn stop_matching_terminal_status(
        &self,
        store: &HomeStore,
        operation_id: StopOperationId,
        target: &StopOperationTarget,
        expected_stop_revision: crate::StopOperationRevision,
        event: &LiveSourceEvent,
        limit: SyndicPointReadLimit,
    ) -> Result<StopOperationTransitionStatus, SyndicReadError> {
        if event.thread_id() != target.thread_id()
            || event.turn_id() != target.turn_id()
            || !matches!(event.payload(), SourceEventPayload::TurnEnded(_))
        {
            return Ok(StopOperationTransitionStatus::Collision);
        }
        let observed =
            self.stable_stop_terminal_observation(store, operation_id, target, event, limit)?;
        if matching_terminal_exact(
            &observed,
            operation_id,
            target,
            expected_stop_revision,
            event,
        ) {
            Ok(StopOperationTransitionStatus::Exact)
        } else if observed.event.is_none()
            && observed.stop.turn_state.as_ref().is_some_and(|state| {
                state.revision() == event.expected_state_revision()
                    && state.source_event_count().checked_add(1) == Some(event.sequence().get())
            })
            && live_prior(
                &observed.stop,
                operation_id,
                target,
                event.expected_gate_revision(),
                expected_stop_revision,
            )
        {
            Ok(StopOperationTransitionStatus::Prior)
        } else {
            Ok(StopOperationTransitionStatus::Collision)
        }
    }
}

fn matching_terminal_exact(
    observed: &StopTerminalObservation,
    operation_id: StopOperationId,
    target: &StopOperationTarget,
    expected_stop_revision: crate::StopOperationRevision,
    event: &LiveSourceEvent,
) -> bool {
    let (Some(stop), Some(successor_binding), Some(turn_state), Some(stored_event)) = (
        observed.stop.stop.as_ref(),
        observed.stop.successor_binding.as_ref(),
        observed.stop.turn_state.as_ref(),
        observed.event.as_ref(),
    ) else {
        return false;
    };
    let StopOperationState::MatchingTerminal(witness) = stop.state() else {
        return false;
    };
    let BindingState::Valid(_) = successor_binding.state() else {
        return false;
    };
    let Ok(expected_event) = SourceEventRecord::new(
        event.turn_id(),
        event.sequence(),
        event.source().cloned(),
        event.payload().clone(),
    ) else {
        return false;
    };
    let expected_binding_revision = target.binding_revision().checked_next().ok();
    stop.id() == operation_id
        && stop.target() == target
        && Some(stop.revision()) == expected_stop_revision.checked_next().ok()
        && observation_authenticates_record(&observed.stop)
        && witness.source().gate_revision() == event.expected_gate_revision()
        && witness.source().stop_revision() == expected_stop_revision
        && Some(witness.successor_gate_revision())
            == event.expected_gate_revision().checked_next().ok()
        && Some(witness.successor_turn_state_revision())
            == event.expected_state_revision().checked_next().ok()
        && stored_event == &expected_event
        && successor_binding.thread_id() == target.thread_id()
        && Some(successor_binding.revision()) == expected_binding_revision
        && turn_state.revision() >= witness.successor_turn_state_revision()
        && turn_state.source_event_count() >= event.sequence().get()
        && matches!(
            event.payload(),
            SourceEventPayload::TurnEnded(status)
                if turn_state.end_status() == Some(*status)
        )
}

fn abandonment_exact(observed: &StopObservation, request: &AbandonStopOperation) -> bool {
    let (Some(stop), Some(successor_binding), Some(turn_state)) = (
        observed.stop.as_ref(),
        observed.successor_binding.as_ref(),
        observed.turn_state.as_ref(),
    ) else {
        return false;
    };
    let StopOperationState::Abandoned(witness) = stop.state() else {
        return false;
    };
    let BindingState::Stale(stale) = successor_binding.state() else {
        return false;
    };
    let expected_binding_revision = request.target().binding_revision().checked_next().ok();
    stop.id() == request.operation_id()
        && stop.target() == request.target()
        && Some(stop.revision()) == request.expected_stop_revision().checked_next().ok()
        && observation_authenticates_record(observed)
        && witness.source().gate_revision() == request.expected_gate_revision()
        && witness.source().stop_revision() == request.expected_stop_revision()
        && witness.reason() == request.reason()
        && Some(witness.retired_binding_revision()) == expected_binding_revision
        && successor_binding.thread_id() == request.target().thread_id()
        && Some(successor_binding.revision()) == expected_binding_revision
        && stale == request.stale()
        && turn_state.revision() >= witness.successor_turn_state_revision()
        && turn_state.lifecycle() == crate::TurnLifecycle::Incomplete
        && turn_state.end_status().is_some_and(|status| {
            status.incomplete_reason() == Some(crate::TurnIncompleteReason::AuthorityLost)
        })
}

fn admission_exact(observed: &StopObservation, request: &AdmitStopOperation) -> bool {
    observed.stop.as_ref().is_some_and(|stop| {
        stop.id() == request.operation_id()
            && stop.target() == request.target()
            && stop.admission().source_gate_revision() == request.expected_gate_revision()
            && stop.admission().source_selected_route_option() == request.expected_route_option()
            && stop.admission_causes() == request.causes()
            && observation_authenticates_record(observed)
    })
}

fn admission_prior(observed: &StopObservation, request: &AdmitStopOperation) -> bool {
    if request.expected_route_option().is_none() {
        let (Some(gate), Some(compaction)) = (observed.gate.as_ref(), observed.compaction.as_ref())
        else {
            return false;
        };
        return observed.stop.is_none()
            && request.operation_id().thread_id() == request.target().thread_id()
            && gate.revision() == request.expected_gate_revision()
            && gate.state()
                == &crate::InputGateState::compacting(
                    request.target().turn_id(),
                    compaction.id().nonce(),
                )
            && compaction.state().is_live()
            && compaction.target().snapshot_id() == request.target().snapshot_id()
            && compaction
                .cas_turn()
                .is_some_and(|value| value.cas_turn_id() == request.target().cas_turn_id());
    }
    let (Some(gate), Some(route)) = (observed.gate.as_ref(), observed.route.as_ref()) else {
        return false;
    };
    observed.stop.is_none()
        && request.operation_id().thread_id() == request.target().thread_id()
        && gate.revision() == request.expected_gate_revision()
        && gate.selected_route() == Some(request.expected_route())
        && route.delivering_count() == 0
        && route.delivering_logical_utf8_bytes() == 0
        && steerable_target_matches(observed, request.target())
}

fn live_prior(
    observed: &StopObservation,
    operation_id: StopOperationId,
    target: &StopOperationTarget,
    expected_gate_revision: beryl_model::InputGateRevision,
    expected_stop_revision: crate::StopOperationRevision,
) -> bool {
    if observed
        .stop
        .as_ref()
        .is_some_and(|stop| stop.admission().is_provider_operation())
    {
        let (Some(stop), Some(gate)) = (observed.stop.as_ref(), observed.gate.as_ref()) else {
            return false;
        };
        return stop.id() == operation_id
            && stop.target() == target
            && stop.revision() == expected_stop_revision
            && stop.state().is_live()
            && gate.revision() == expected_gate_revision
            && observation_authenticates_record(observed);
    }
    let (Some(stop), Some(gate), Some(head), Some(route)) = (
        observed.stop.as_ref(),
        observed.gate.as_ref(),
        observed.route_head.as_ref(),
        observed.route.as_ref(),
    ) else {
        return false;
    };
    stop.id() == operation_id
        && stop.target() == target
        && stop.revision() == expected_stop_revision
        && stop.state().is_live()
        && gate.revision() == expected_gate_revision
        && live_successor_matches(observed, stop, gate, head, route)
}
