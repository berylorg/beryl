use beryl_home_store::DomainReader;
use beryl_model::SyndicTurnId;

use crate::mutation::{point, required};
use crate::{
    AcceptedNextSourceRecord, AcceptedRouteGenerationHeadRecord, AcceptedRouteGenerationRecord,
    AcceptedRouteHeadProof, AcceptedRouteTarget, InputGateRecord, InputGateState, NextTurnReason,
    SyndicMutationError, SyndicRecordError, TurnLifecycle, codec::*, domain::SyndicDomain,
};

pub(in crate::mutation::live) struct LiveGateEffect {
    gate: InputGateRecord,
    route: Option<AcceptedRouteGenerationRecord>,
    route_head: Option<AcceptedRouteGenerationHeadRecord>,
    delete_ready: Option<ThreadRouteKey>,
    next_source: Option<NextSourceEffect>,
}

enum NextSourceEffect {
    Put(AcceptedNextSourceRecord),
    Delete(ThreadRouteKey),
}

impl LiveGateEffect {
    #[must_use]
    pub(in crate::mutation::live) const fn gate(&self) -> &InputGateRecord {
        &self.gate
    }

    pub(in crate::mutation::live) fn contribute(
        self,
        mutations: &mut beryl_home_store::MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        if let Some(route) = &self.route {
            mutations.put::<AcceptedRouteGenerationsCodec>(
                &ThreadRouteKey {
                    thread: route.thread_id(),
                    generation: route.generation(),
                },
                route,
            )?;
        }
        if let Some(head) = &self.route_head {
            mutations.put::<AcceptedRouteGenerationHeadsCodec>(&head.thread_id(), head)?;
        }
        if let Some(key) = &self.delete_ready {
            mutations.delete::<AcceptedReadySourcesCodec>(key)?;
        }
        match &self.next_source {
            Some(NextSourceEffect::Put(source)) => {
                mutations.put::<AcceptedNextSourcesCodec>(
                    &ThreadRouteKey {
                        thread: source.thread_id(),
                        generation: source.generation(),
                    },
                    source,
                )?;
            }
            Some(NextSourceEffect::Delete(key)) => {
                mutations.delete::<AcceptedNextSourcesCodec>(key)?;
            }
            None => {}
        }
        mutations.put::<InputGatesCodec>(&self.gate.thread_id(), &self.gate)?;
        Ok(())
    }
}

pub(in crate::mutation::live) fn terminal_gate_effect(
    reader: &DomainReader<'_, SyndicDomain>,
    current: &InputGateRecord,
    turn: SyndicTurnId,
    lifecycle: TurnLifecycle,
) -> Result<LiveGateEffect, SyndicMutationError> {
    if !gate_targets_turn(current.state(), turn) {
        return Err(SyndicMutationError::InputGateStateConflict);
    }

    if matches!(current.state(), InputGateState::Steerable(_)) {
        let (target, state) = if lifecycle.is_proven_terminal() {
            (
                ReclassifiedTarget::NextTurn(NextTurnReason::TerminalHistory),
                InputGateState::FinalizingHistory(turn),
            )
        } else {
            (
                ReclassifiedTarget::AwaitingTerminal,
                InputGateState::AwaitingTerminal(turn),
            )
        };
        return reclassify_steering_route(reader, current, turn, target, state);
    }

    if current.live_steering_count() != 0 {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    let state = match (lifecycle.is_proven_terminal(), current.state()) {
        (true, _) => InputGateState::FinalizingHistory(turn),
        (false, InputGateState::PendingTurn(_)) => InputGateState::PendingTurn(turn),
        (false, InputGateState::Compacting { .. }) => {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        (
            false,
            InputGateState::Stopping {
                turn_id,
                operation_nonce,
            },
        ) => InputGateState::stopping(*turn_id, *operation_nonce),
        (false, InputGateState::AwaitingTerminal(_)) => InputGateState::AwaitingTerminal(turn),
        (
            false,
            InputGateState::AwaitingSteering(_)
            | InputGateState::FinalizingHistory(_)
            | InputGateState::Idle,
        ) => return Err(SyndicMutationError::InputGateStateConflict),
        (false, InputGateState::Steerable(_)) => unreachable!("handled above"),
    };
    let gate = InputGateRecord::new(
        current.thread_id(),
        current.revision().checked_next()?,
        state,
        current.accepted_high_water(),
        current.route_generation_high_water(),
        current.selected_route(),
        current.live_steering_count(),
        current.live_next_turn_count(),
        current.live_logical_utf8_bytes(),
    )?;
    Ok(LiveGateEffect {
        gate,
        route: None,
        route_head: None,
        delete_ready: None,
        next_source: None,
    })
}

pub(in crate::mutation::live) fn activation_gate_effect(
    reader: &DomainReader<'_, SyndicDomain>,
    current: &InputGateRecord,
    turn: SyndicTurnId,
) -> Result<Option<LiveGateEffect>, SyndicMutationError> {
    let InputGateState::AwaitingTerminal(gate_turn) = current.state() else {
        return Ok(None);
    };
    if *gate_turn != turn || current.live_steering_count() != 0 {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    let selected = current
        .selected_route()
        .ok_or(SyndicMutationError::ActiveSteeringRouteConflict)?;
    let selected_key = ThreadRouteKey {
        thread: current.thread_id(),
        generation: selected.generation(),
    };
    let head = required::<AcceptedRouteGenerationHeadsFamily>(reader, &current.thread_id())?;
    let prior = required::<AcceptedRouteGenerationsFamily>(reader, &selected_key)?;
    let AcceptedRouteTarget::AwaitingTerminal(target) = prior.target() else {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    };
    if head.proof() != selected
        || prior.revision() != selected.revision()
        || target.pending().active_turn_id() != turn
        || prior.ready_retryable_count() != 0
        || prior.delivering_count() != 0
        || prior.delivering_logical_utf8_bytes() != 0
    {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    }
    validate_route_sources(reader, current, &prior)?;

    let generation = current.next_route_generation()?;
    let key = ThreadRouteKey {
        thread: current.thread_id(),
        generation,
    };
    if point::<AcceptedRouteGenerationsFamily>(reader, &key)?.is_some()
        || point::<AcceptedReadySourcesFamily>(reader, &key)?.is_some()
        || point::<AcceptedNextSourcesFamily>(reader, &key)?.is_some()
    {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    }
    let route = AcceptedRouteGenerationRecord::new(
        current.thread_id(),
        generation,
        crate::AcceptedRouteRevision::FIRST,
        AcceptedRouteTarget::Steering(target.clone()),
        None,
        None,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    )?;
    let proof = AcceptedRouteHeadProof::new(generation, route.revision());
    let route_head = AcceptedRouteGenerationHeadRecord::new(current.thread_id(), proof);
    let gate = InputGateRecord::new(
        current.thread_id(),
        current.revision().checked_next()?,
        InputGateState::Steerable(turn),
        current.accepted_high_water(),
        Some(generation),
        Some(proof),
        0,
        current.live_next_turn_count(),
        current.live_logical_utf8_bytes(),
    )?;
    Ok(Some(LiveGateEffect {
        gate,
        route: Some(route),
        route_head: Some(route_head),
        delete_ready: None,
        next_source: None,
    }))
}

enum ReclassifiedTarget {
    AwaitingTerminal,
    NextTurn(NextTurnReason),
}

fn reclassify_steering_route(
    reader: &DomainReader<'_, SyndicDomain>,
    current: &InputGateRecord,
    turn: SyndicTurnId,
    target: ReclassifiedTarget,
    state: InputGateState,
) -> Result<LiveGateEffect, SyndicMutationError> {
    let selected = current
        .selected_route()
        .ok_or(SyndicMutationError::ActiveSteeringRouteConflict)?;
    let key = ThreadRouteKey {
        thread: current.thread_id(),
        generation: selected.generation(),
    };
    let head = required::<AcceptedRouteGenerationHeadsFamily>(reader, &current.thread_id())?;
    let prior = required::<AcceptedRouteGenerationsFamily>(reader, &key)?;
    let AcceptedRouteTarget::Steering(steering) = prior.target() else {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    };
    if head.proof() != selected
        || prior.revision() != selected.revision()
        || steering.pending().active_turn_id() != turn
        || prior.delivering_count() != 0
        || prior.delivering_logical_utf8_bytes() != 0
        || current.live_steering_count() != prior.ready_retryable_count()
    {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    }
    validate_route_sources(reader, current, &prior)?;

    let next_turn_count = prior
        .next_turn_count()
        .checked_add(prior.ready_retryable_count())
        .ok_or(SyndicRecordError::LengthOverflow {
            kind: "accepted-route next-turn count",
        })?;
    let revision = prior.revision().checked_next()?;
    let route_target = match target {
        ReclassifiedTarget::AwaitingTerminal => {
            AcceptedRouteTarget::AwaitingTerminal(steering.clone())
        }
        ReclassifiedTarget::NextTurn(reason) => AcceptedRouteTarget::NextTurn(reason),
    };
    let route = AcceptedRouteGenerationRecord::new(
        prior.thread_id(),
        prior.generation(),
        revision,
        route_target,
        prior.first_ordinal(),
        prior.last_ordinal(),
        prior.input_count(),
        0,
        0,
        next_turn_count,
        prior.terminal_count(),
        prior.live_logical_utf8_bytes(),
        0,
    )?;
    let proof = AcceptedRouteHeadProof::new(route.generation(), route.revision());
    let route_head = AcceptedRouteGenerationHeadRecord::new(route.thread_id(), proof);
    let live_next_turn_count = current
        .live_next_turn_count()
        .checked_add(current.live_steering_count())
        .ok_or(SyndicRecordError::LengthOverflow {
            kind: "live next-turn count",
        })?;
    let gate = InputGateRecord::new(
        current.thread_id(),
        current.revision().checked_next()?,
        state,
        current.accepted_high_water(),
        current.route_generation_high_water(),
        Some(proof),
        0,
        live_next_turn_count,
        current.live_logical_utf8_bytes(),
    )?;
    let next_source = match route.first_ordinal().zip(route.last_ordinal()) {
        Some((first, last)) if route.next_turn_count() > 0 => {
            NextSourceEffect::Put(AcceptedNextSourceRecord::new(
                route.thread_id(),
                route.generation(),
                route.revision(),
                first,
                last,
            ))
        }
        _ => NextSourceEffect::Delete(key),
    };
    Ok(LiveGateEffect {
        gate,
        route: Some(route),
        route_head: Some(route_head),
        delete_ready: Some(key),
        next_source: Some(next_source),
    })
}

fn validate_route_sources(
    reader: &DomainReader<'_, SyndicDomain>,
    gate: &InputGateRecord,
    route: &AcceptedRouteGenerationRecord,
) -> Result<(), SyndicMutationError> {
    let key = ThreadRouteKey {
        thread: route.thread_id(),
        generation: route.generation(),
    };
    let ready = point::<AcceptedReadySourcesFamily>(reader, &key)?;
    match (route.ready_retryable_count(), ready) {
        (0, None) => {}
        (count, Some(source))
            if count > 0
                && source.thread_id() == route.thread_id()
                && source.gate_revision() == gate.revision()
                && source.generation() == route.generation()
                && source.generation_revision() == route.revision()
                && Some(source.first_ordinal()) == route.first_ordinal()
                && Some(source.last_ordinal()) == route.last_ordinal() => {}
        _ => return Err(SyndicMutationError::ActiveSteeringRouteConflict),
    }
    let next = point::<AcceptedNextSourcesFamily>(reader, &key)?;
    match (route.next_turn_count(), next) {
        (0, None) => {}
        (count, Some(source))
            if count > 0
                && source.thread_id() == route.thread_id()
                && source.generation() == route.generation()
                && source.generation_revision() == route.revision()
                && Some(source.first_ordinal()) == route.first_ordinal()
                && Some(source.last_ordinal()) == route.last_ordinal() => {}
        _ => return Err(SyndicMutationError::ActiveSteeringRouteConflict),
    }
    Ok(())
}

fn gate_targets_turn(state: &InputGateState, turn: SyndicTurnId) -> bool {
    match state {
        InputGateState::PendingTurn(current)
        | InputGateState::AwaitingTerminal(current)
        | InputGateState::FinalizingHistory(current) => *current == turn,
        InputGateState::Compacting { turn_id, .. } => *turn_id == turn,
        InputGateState::AwaitingSteering(target) | InputGateState::Steerable(target) => {
            *target == turn
        }
        InputGateState::Stopping { turn_id, .. } => *turn_id == turn,
        InputGateState::Idle => false,
    }
}
