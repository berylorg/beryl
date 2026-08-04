mod abandonment;
pub(super) mod active;

use beryl_home_store::HomeStore;

use crate::{
    AcceptedRouteGenerationRecord, BindingState, HistorySummaryRecord, InputGateRecord,
    InputGateState, StopAdmissionRead, StopOperationId, SyndicCurrentBinding, SyndicPointReadLimit,
    SyndicStorage, SyndicTimestamp, TurnStateRecord,
};

use super::{facts, *};

impl SyndicStorage {
    /// Stabilizes and classifies one startup source with fixed bounded point work.
    ///
    /// The exact source gate and every dependent fact are observed twice. A stale source or any
    /// disagreement between those passes returns
    /// [`DeliveryRecoveryClassificationError::SourceDrift`]; a stable unsupported combination is
    /// reported separately as corruption.
    pub fn classify_delivery_recovery(
        &self,
        store: &HomeStore,
        source: &DeliveryRecoverySource,
        limit: SyndicPointReadLimit,
    ) -> Result<DeliveryRecoveryCase, DeliveryRecoveryClassificationError> {
        if source.home_id != store.home_id() {
            return Err(DeliveryRecoveryClassificationError::SourceDrift);
        }
        let first = facts::read(self, store, source.thread_id(), limit)?;
        let second = facts::read(self, store, source.thread_id(), limit)?;
        if first != second {
            return Err(DeliveryRecoveryClassificationError::SourceDrift);
        }
        if first.gate.as_ref() != Some(&source.gate)
            && !first
                .gate
                .as_ref()
                .is_some_and(|gate| matches!(gate.state(), InputGateState::Idle))
        {
            return Err(DeliveryRecoveryClassificationError::SourceDrift);
        }
        if first
            .stop
            .as_ref()
            .is_some_and(|stop| stop.admission().is_provider_operation())
        {
            let InputGateState::Stopping {
                turn_id,
                operation_nonce,
            } = source.gate.state()
            else {
                return corruption("provider stop is not selected by a stopping recovery source");
            };
            return match self.stop_admission_read(store, source.thread_id(), limit)? {
                StopAdmissionRead::Stopping(live)
                    if live.operation_id()
                        == StopOperationId::new(source.thread_id(), *operation_nonce)
                        && live.target().turn_id() == *turn_id
                        && live.current_gate_revision() == source.gate.revision() =>
                {
                    Ok(DeliveryRecoveryCase::Stopping(live))
                }
                StopAdmissionRead::Stopping(_)
                | StopAdmissionRead::Admissible(_)
                | StopAdmissionRead::Ineligible(_) => {
                    Err(DeliveryRecoveryClassificationError::SourceDrift)
                }
            };
        }
        classify(source.thread_id(), &first)
    }
}

pub(in crate::read) fn classify(
    thread_id: SyndicThreadId,
    facts: &facts::RecoveryFacts,
) -> Result<DeliveryRecoveryCase, DeliveryRecoveryClassificationError> {
    let gate = required(
        facts.gate.as_ref(),
        "delivery-recovery source gate is missing",
    )?;
    if gate.thread_id() != thread_id {
        return corruption("delivery-recovery gate belongs to another thread");
    }
    let summary = required(
        facts.summary.as_ref(),
        "delivery-recovery thread history summary is missing",
    )?;
    let binding = required(
        facts.binding.as_ref(),
        "delivery-recovery thread has no current binding",
    )?;
    validate_thread_facts(thread_id, summary, binding)?;

    if matches!(gate.state(), InputGateState::Stopping { .. }) {
        return super::stopping::classify(facts, gate, summary, binding)
            .map(Box::new)
            .map(DeliveryRecoveryCase::Stopping);
    }

    if matches!(binding.binding().state(), BindingState::Active(_)) {
        return active::classify(facts, gate, summary, binding)
            .map(Box::new)
            .map(DeliveryRecoveryCase::Active);
    }

    match gate.state() {
        InputGateState::Idle => Ok(DeliveryRecoveryCase::Settled { thread_id }),
        InputGateState::Compacting { turn_id, .. } => {
            let _ = validate_blocking_turn(facts, *turn_id)?;
            Ok(DeliveryRecoveryCase::DeferredCompaction {
                thread_id,
                turn_id: *turn_id,
            })
        }
        InputGateState::PendingTurn(turn_id) => {
            let state = validate_blocking_turn(facts, *turn_id)?;
            validate_current_tail(state, summary)?;
            classify_pending(facts, gate, state, summary, binding)
        }
        InputGateState::FinalizingHistory(turn_id) => {
            let state = required(
                facts.state.as_ref(),
                "finalizing-history gate turn state is missing",
            )?;
            if state.turn_id() != *turn_id
                || !state.lifecycle().is_proven_terminal()
                || summary.committed_tail() != Some(*turn_id)
            {
                return corruption(
                    "finalizing-history gate does not own the proven-terminal committed tail",
                );
            }
            Ok(DeliveryRecoveryCase::FinalizingHistory {
                thread_id,
                turn_id: *turn_id,
                minimum_timestamp: minimum_timestamp(state, summary),
            })
        }
        InputGateState::AwaitingSteering(_)
        | InputGateState::Steerable(_)
        | InputGateState::AwaitingTerminal(_) => {
            corruption("active delivery-recovery gate has no current active binding")
        }
        InputGateState::Stopping { .. } => {
            corruption("stopping recovery gate escaped stop classification")
        }
    }
}

fn classify_pending(
    facts: &facts::RecoveryFacts,
    gate: &InputGateRecord,
    state: &TurnStateRecord,
    summary: &HistorySummaryRecord,
    binding: &SyndicCurrentBinding,
) -> Result<DeliveryRecoveryCase, DeliveryRecoveryClassificationError> {
    if gate.live_steering_count() != 0 {
        return corruption("pending delivery-recovery gate retains live steering work");
    }
    if gate.selected_route().is_some() {
        return abandonment::classify(facts, gate, state, summary, binding);
    }
    if state.lifecycle() != crate::TurnLifecycle::Pending || state.source_event_count() != 0 {
        return corruption("safe pending delivery-recovery turn is not pending and source-free");
    }
    Ok(DeliveryRecoveryCase::Pending {
        thread_id: gate.thread_id(),
        turn_id: state.turn_id(),
        minimum_timestamp: minimum_timestamp(state, summary),
    })
}

fn validate_thread_facts(
    thread_id: SyndicThreadId,
    summary: &HistorySummaryRecord,
    binding: &SyndicCurrentBinding,
) -> Result<(), DeliveryRecoveryClassificationError> {
    if summary.thread_id() != thread_id
        || binding.head().thread_id() != thread_id
        || binding.binding().thread_id() != thread_id
    {
        return corruption("delivery-recovery current thread facts disagree");
    }
    Ok(())
}

pub(super) fn validate_blocking_turn(
    facts: &facts::RecoveryFacts,
    turn_id: SyndicTurnId,
) -> Result<&TurnStateRecord, DeliveryRecoveryClassificationError> {
    let gate = required(
        facts.gate.as_ref(),
        "delivery-recovery blocking turn gate is missing",
    )?;
    let turn = required(
        facts.turn.as_ref(),
        "delivery-recovery blocking turn header is missing",
    )?;
    let state = required(
        facts.state.as_ref(),
        "delivery-recovery blocking turn state is missing",
    )?;
    if turn.id() != turn_id
        || turn.origin_thread_id() != gate.thread_id()
        || state.turn_id() != turn_id
        || !state.lifecycle().blocks_same_thread_start()
    {
        return corruption("delivery-recovery gate turn does not block its thread");
    }
    Ok(state)
}

pub(super) fn selected_route<'a>(
    facts: &'a facts::RecoveryFacts,
    gate: &InputGateRecord,
) -> Result<&'a AcceptedRouteGenerationRecord, DeliveryRecoveryClassificationError> {
    let proof = gate
        .selected_route()
        .ok_or(DeliveryRecoveryClassificationError::Corruption(
            "active delivery-recovery gate has no selected route",
        ))?;
    let head = required(
        facts.route_head.as_ref(),
        "selected delivery-recovery route head is missing",
    )?;
    let route = required(
        facts.route.as_ref(),
        "selected delivery-recovery route generation is missing",
    )?;
    if head.thread_id() != gate.thread_id()
        || head.proof() != proof
        || route.thread_id() != gate.thread_id()
        || route.generation() != proof.generation()
        || route.revision() != proof.revision()
    {
        return corruption("selected delivery-recovery route authority disagrees");
    }
    Ok(route)
}

pub(super) fn minimum_timestamp(
    state: &TurnStateRecord,
    summary: &HistorySummaryRecord,
) -> SyndicTimestamp {
    state.updated_at().max(summary.last_activity_at())
}

pub(super) fn validate_current_tail(
    state: &TurnStateRecord,
    summary: &HistorySummaryRecord,
) -> Result<(), DeliveryRecoveryClassificationError> {
    if summary.committed_tail() != Some(state.turn_id()) || summary.complete() {
        return corruption("delivery-recovery blocking turn is not the current incomplete tail");
    }
    Ok(())
}

pub(super) fn required<'a, T>(
    value: Option<&'a T>,
    message: &'static str,
) -> Result<&'a T, DeliveryRecoveryClassificationError> {
    value.ok_or(DeliveryRecoveryClassificationError::Corruption(message))
}

pub(super) fn corruption<T>(
    message: &'static str,
) -> Result<T, DeliveryRecoveryClassificationError> {
    Err(DeliveryRecoveryClassificationError::Corruption(message))
}
