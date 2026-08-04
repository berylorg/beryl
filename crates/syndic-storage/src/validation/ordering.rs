use beryl_home_store::DomainReader;

use crate::{
    AcceptedInputLifecycle, AcceptedOrderIndexRecord, AcceptedReadySourceRecord,
    AcceptedRouteGenerationRecord, AcceptedRouteHeadProof, AcceptedRouteLeafState,
    AcceptedRouteTarget, InputGateState, NextTurnReason, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::scan::{point, require, scan, scan_range};

mod abandonment;
mod promotion;
mod transition_witness;
mod util;

use abandonment::validate_lost_proof;
use util::{add, invariant};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    validate_inputs(reader)?;
    validate_order(reader)?;
    validate_leaves(reader)?;
    validate_generations(reader)?;
    validate_heads(reader)?;
    validate_ready_sources(reader)?;
    validate_next_sources(reader)?;
    validate_gates(reader)
}

fn validate_inputs(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<AcceptedInputsFamily>(reader, |key, input| {
        let proof = input.admission();
        if *key != input.id()
            || input.id() != proof.source_draft_id().accepted_input_id()
            || proof.source_draft_id() == proof.replacement_draft_id()
        {
            return invariant("accepted-input key, identity, and admission proof disagree");
        }
        if point::<DraftsFamily>(reader, &proof.source_draft_id())?.is_some()
            || point::<TurnsFamily>(reader, &proof.source_draft_id().submitted_turn_id())?.is_some()
        {
            return invariant("accepted-input source draft was not consumed exclusively");
        }
        let thread = require::<ThreadsFamily>(
            reader,
            &input.thread_id(),
            "accepted input references a missing thread",
        )?;
        let gate = require::<InputGatesFamily>(
            reader,
            &input.thread_id(),
            "accepted input references a missing input gate",
        )?;
        if proof.expected_thread_revision() >= thread.revision()
            || proof.expected_gate_revision() >= gate.revision()
        {
            return invariant("accepted-input admission revisions are not historical");
        }
        let order = require::<AcceptedOrderFamily>(
            reader,
            &ThreadAcceptedKey {
                owner: input.thread_id(),
                ordinal: input.ordinal(),
            },
            "accepted input is missing immutable order membership",
        )?;
        let expected = AcceptedOrderIndexRecord::new(
            input.thread_id(),
            input.ordinal(),
            input.id(),
            input.route_generation(),
        );
        if order != expected {
            return invariant("accepted input and immutable order membership disagree");
        }
        let leaf = require::<AcceptedRouteLeavesFamily>(
            reader,
            &input.id(),
            "accepted input is missing its route leaf",
        )?;
        if leaf.thread_id() != input.thread_id()
            || leaf.ordinal() != input.ordinal()
            || leaf.generation() != input.route_generation()
        {
            return invariant("accepted input and route leaf disagree");
        }
        let generation = require::<AcceptedRouteGenerationsFamily>(
            reader,
            &ThreadRouteKey {
                thread: input.thread_id(),
                generation: input.route_generation(),
            },
            "accepted input references a missing route generation",
        )?;
        if !generation_interval_contains(&generation, input.ordinal()) {
            return invariant("accepted input lies outside its route generation");
        }
        validate_replacement_descendant(reader, &input)?;
        Ok(())
    })
}

fn validate_order(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    let mut prior: Option<crate::AcceptedInputRecord> = None;
    scan::<AcceptedOrderFamily>(reader, |key, order| {
        let input = require::<AcceptedInputsFamily>(
            reader,
            &order.input_id(),
            "accepted order references a missing input",
        )?;
        if key.owner != order.thread_id()
            || key.ordinal != order.ordinal()
            || input.thread_id() != order.thread_id()
            || input.ordinal() != order.ordinal()
            || input.route_generation() != order.route_generation()
        {
            return invariant("accepted order and immutable input disagree");
        }
        match prior.as_ref() {
            None => {
                if input.ordinal() != crate::AcceptedInputOrdinal::FIRST {
                    return invariant("accepted order does not begin at the first ordinal");
                }
            }
            Some(previous) if previous.thread_id() == input.thread_id() => {
                if previous.ordinal().checked_next().ok() != Some(input.ordinal()) {
                    return invariant("accepted order is not strictly contiguous");
                }
                let previous_proof = previous.admission();
                let proof = input.admission();
                if proof.expected_thread_revision() <= previous_proof.expected_thread_revision()
                    || proof.expected_gate_revision() <= previous_proof.expected_gate_revision()
                    || input.admitted_at() < previous.admitted_at()
                {
                    return invariant("accepted admission receipts are not strict descendants");
                }
            }
            Some(previous) => {
                validate_order_frontier(reader, previous)?;
                if input.ordinal() != crate::AcceptedInputOrdinal::FIRST {
                    return invariant("accepted order does not restart at the first ordinal");
                }
            }
        }
        prior = Some(input);
        Ok(())
    })?;
    if let Some(last) = prior.as_ref() {
        validate_order_frontier(reader, last)?;
    }
    Ok(())
}

fn validate_order_frontier(
    reader: &DomainReader<'_, SyndicDomain>,
    last: &crate::AcceptedInputRecord,
) -> Result<(), SyndicValidationError> {
    let gate = require::<InputGatesFamily>(
        reader,
        &last.thread_id(),
        "accepted order has no owning input gate",
    )?;
    if gate.accepted_high_water() != last.ordinal().get() {
        return invariant("accepted order and input-gate high-water disagree");
    }
    Ok(())
}

fn validate_replacement_descendant(
    reader: &DomainReader<'_, SyndicDomain>,
    input: &crate::AcceptedInputRecord,
) -> Result<(), SyndicValidationError> {
    let replacement = input.admission().replacement_draft_id();
    let accepted = point::<AcceptedInputsFamily>(reader, &replacement.accepted_input_id())?;
    let draft = point::<DraftsFamily>(reader, &replacement)?;
    let turn = point::<TurnsFamily>(reader, &replacement.submitted_turn_id())?;
    let present =
        u8::from(accepted.is_some()) + u8::from(draft.is_some()) + u8::from(turn.is_some());
    if present != 1 {
        return invariant("accepted-input replacement descendant is not exclusive");
    }
    let accepted_exact = accepted.as_ref().is_some_and(|next| {
        let proof = next.admission();
        next.thread_id() == input.thread_id()
            && proof.source_draft_id() == replacement
            && next.ordinal().get() > input.ordinal().get()
            && proof.expected_thread_revision() > input.admission().expected_thread_revision()
            && proof.expected_gate_revision() > input.admission().expected_gate_revision()
            && next.admitted_at() >= input.admitted_at()
    });
    let draft_exact = draft.as_ref().is_some_and(|draft| {
        draft.thread_id() == input.thread_id() && draft.created_at() == input.admitted_at()
    });
    let turn_exact = turn.as_ref().is_some_and(|turn| {
        turn.origin_thread_id() == input.thread_id() && turn.submitted_at() >= input.admitted_at()
    });
    if !(accepted_exact || draft_exact || turn_exact) {
        return invariant("accepted-input replacement has no durable descendant");
    }
    Ok(())
}

fn generation_interval_contains(
    generation: &AcceptedRouteGenerationRecord,
    ordinal: crate::AcceptedInputOrdinal,
) -> bool {
    generation
        .first_ordinal()
        .zip(generation.last_ordinal())
        .is_some_and(|(first, last)| first.get() <= ordinal.get() && ordinal.get() <= last.get())
}

fn validate_leaves(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<AcceptedRouteLeavesFamily>(reader, |key, leaf| {
        let input = require::<AcceptedInputsFamily>(
            reader,
            key,
            "accepted-route leaf references a missing input",
        )?;
        if *key != leaf.input_id()
            || input.thread_id() != leaf.thread_id()
            || input.ordinal() != leaf.ordinal()
            || input.route_generation() != leaf.generation()
        {
            return invariant("accepted-route leaf and immutable input disagree");
        }
        if matches!(leaf.state(), AcceptedRouteLeafState::NextTurn(_))
            && leaf.lifecycle().is_terminal()
        {
            return invariant("terminal accepted-route leaf remains next-turn work");
        }
        promotion::validate(reader, &input, &leaf)?;
        transition_witness::validate(reader, &input, &leaf)?;
        Ok(())
    })
}

#[derive(Default)]
struct RouteTotals {
    input: u64,
    ready: u64,
    delivering: u64,
    next: u64,
    terminal: u64,
    live_bytes: u64,
    delivering_bytes: u64,
}

fn validate_generations(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut prior_generation: Option<(
        beryl_model::SyndicThreadId,
        crate::AcceptedRouteGeneration,
    )> = None;
    scan::<AcceptedRouteGenerationsFamily>(reader, |key, generation| {
        if key.thread != generation.thread_id() || key.generation != generation.generation() {
            return invariant("accepted-route generation key and identity disagree");
        }
        match prior_generation {
            None => {
                if generation.generation() != crate::AcceptedRouteGeneration::FIRST {
                    return invariant("accepted-route generations are not sequential");
                }
            }
            Some((prior_thread, prior)) if prior_thread == generation.thread_id() => {
                let Some(expected) = prior.get().checked_add(1) else {
                    return invariant("accepted-route generation high-water overflowed");
                };
                if generation.generation().get() != expected {
                    return invariant("accepted-route generations are not sequential");
                }
            }
            Some(_) => {
                if generation.generation() != crate::AcceptedRouteGeneration::FIRST {
                    return invariant("accepted-route generations are not sequential");
                }
            }
        }
        prior_generation = Some((generation.thread_id(), generation.generation()));
        validate_lost_proof(reader, generation)?;
        let mut totals = RouteTotals::default();
        match (generation.first_ordinal(), generation.last_ordinal()) {
            (None, None) => {}
            (Some(first), Some(last)) => scan_range::<AcceptedOrderFamily>(
                reader,
                ThreadAcceptedKey {
                    owner: generation.thread_id(),
                    ordinal: first,
                },
                ThreadAcceptedKey {
                    owner: generation.thread_id(),
                    ordinal: last,
                },
                |_, order| classify_member(reader, generation, order, &mut totals),
            )?,
            _ => return invariant("accepted-route generation has partial interval bounds"),
        }
        if totals.input != generation.input_count()
            || totals.ready != generation.ready_retryable_count()
            || totals.delivering != generation.delivering_count()
            || totals.next != generation.next_turn_count()
            || totals.terminal != generation.terminal_count()
            || totals.live_bytes != generation.live_logical_utf8_bytes()
            || totals.delivering_bytes != generation.delivering_logical_utf8_bytes()
        {
            return invariant(
                "accepted-route generation aggregates disagree with its bounded leaves",
            );
        }
        let ready_source = point::<AcceptedReadySourcesFamily>(reader, key)?;
        if ready_source != expected_ready_source(reader, generation)? {
            return invariant("accepted-route generation and ready-source authority disagree");
        }
        let next_source = point::<AcceptedNextSourcesFamily>(reader, key)?;
        if (generation.next_turn_count() > 0) != next_source.is_some() {
            return invariant("accepted-route generation and next-source presence disagree");
        }
        Ok(())
    })
}

fn classify_member(
    reader: &DomainReader<'_, SyndicDomain>,
    generation: &AcceptedRouteGenerationRecord,
    order: &AcceptedOrderIndexRecord,
    totals: &mut RouteTotals,
) -> Result<(), SyndicValidationError> {
    if order.thread_id() != generation.thread_id()
        || order.route_generation() != generation.generation()
    {
        return invariant("accepted-route interval crosses generation membership");
    }
    let input = require::<AcceptedInputsFamily>(
        reader,
        &order.input_id(),
        "accepted-route generation references a missing input",
    )?;
    let leaf = require::<AcceptedRouteLeavesFamily>(
        reader,
        &order.input_id(),
        "accepted-route generation references a missing leaf",
    )?;
    let bytes = input.content().summary().logical_utf8_bytes();
    totals.input = add(totals.input, 1)?;

    match (generation.target(), leaf.state(), leaf.lifecycle()) {
        (
            AcceptedRouteTarget::ProjectionLost(_),
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Delivering,
        ) => {
            totals.terminal = add(totals.terminal, 1)?;
        }
        (
            AcceptedRouteTarget::ProjectionLost(_),
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable,
        ) => {
            totals.next = add(totals.next, 1)?;
            totals.live_bytes = add(totals.live_bytes, bytes)?;
        }
        (
            AcceptedRouteTarget::NextTurn(_) | AcceptedRouteTarget::AwaitingTerminal(_),
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable,
        ) => {
            totals.next = add(totals.next, 1)?;
            totals.live_bytes = add(totals.live_bytes, bytes)?;
        }
        (
            _,
            AcceptedRouteLeafState::NextTurn(_),
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable,
        ) => {
            totals.next = add(totals.next, 1)?;
            totals.live_bytes = add(totals.live_bytes, bytes)?;
        }
        (
            AcceptedRouteTarget::AwaitingSteering(_) | AcceptedRouteTarget::Steering(_),
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable,
        ) => {
            totals.ready = add(totals.ready, 1)?;
            totals.live_bytes = add(totals.live_bytes, bytes)?;
        }
        (
            AcceptedRouteTarget::AwaitingSteering(_) | AcceptedRouteTarget::Steering(_),
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Delivering,
        ) => {
            totals.delivering = add(totals.delivering, 1)?;
            totals.live_bytes = add(totals.live_bytes, bytes)?;
            totals.delivering_bytes = add(totals.delivering_bytes, bytes)?;
        }
        (_, AcceptedRouteLeafState::Routed, lifecycle) if lifecycle.is_terminal() => {
            totals.terminal = add(totals.terminal, 1)?;
        }
        _ => return invariant("accepted-route target, leaf state, and lifecycle disagree"),
    }
    Ok(())
}

fn validate_heads(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<AcceptedRouteGenerationHeadsFamily>(reader, |key, head| {
        let route = require::<AcceptedRouteGenerationsFamily>(
            reader,
            &ThreadRouteKey {
                thread: *key,
                generation: head.proof().generation(),
            },
            "accepted-route head references a missing generation",
        )?;
        if head.thread_id() != *key || route.revision() != head.proof().revision() {
            return invariant("accepted-route head and generation revision disagree");
        }
        Ok(())
    })
}

fn validate_ready_sources(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<AcceptedReadySourcesFamily>(reader, |key, source| {
        let generation = require::<AcceptedRouteGenerationsFamily>(
            reader,
            key,
            "accepted-ready source references a missing generation",
        )?;
        if source.thread_id() != key.thread
            || source.generation() != key.generation
            || expected_ready_source(reader, &generation)?.as_ref() != Some(source)
        {
            return invariant("accepted-ready source and current route authority disagree");
        }
        Ok(())
    })
}

fn expected_ready_source(
    reader: &DomainReader<'_, SyndicDomain>,
    generation: &AcceptedRouteGenerationRecord,
) -> Result<Option<AcceptedReadySourceRecord>, SyndicValidationError> {
    if generation.ready_retryable_count() == 0
        || !matches!(generation.target(), AcceptedRouteTarget::Steering(_))
    {
        return Ok(None);
    }
    let gate = require::<InputGatesFamily>(
        reader,
        &generation.thread_id(),
        "accepted-route generation owner is missing its input gate",
    )?;
    let head = require::<AcceptedRouteGenerationHeadsFamily>(
        reader,
        &generation.thread_id(),
        "accepted-route generation owner is missing its route head",
    )?;
    let proof = AcceptedRouteHeadProof::new(generation.generation(), generation.revision());
    if !matches!(gate.state(), InputGateState::Steerable(_))
        || gate.selected_route() != Some(proof)
        || head.proof() != proof
    {
        return Ok(None);
    }
    let first = generation
        .first_ordinal()
        .ok_or(SyndicValidationError::Invariant(
            "accepted-ready generation has no first ordinal",
        ))?;
    let last = generation
        .last_ordinal()
        .ok_or(SyndicValidationError::Invariant(
            "accepted-ready generation has no last ordinal",
        ))?;
    Ok(Some(AcceptedReadySourceRecord::new(
        generation.thread_id(),
        gate.revision(),
        generation.generation(),
        generation.revision(),
        first,
        last,
    )))
}

fn validate_next_sources(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<AcceptedNextSourcesFamily>(reader, |key, source| {
        let generation = require::<AcceptedRouteGenerationsFamily>(
            reader,
            key,
            "accepted-next source references a missing generation",
        )?;
        if source.thread_id() != key.thread
            || source.generation() != key.generation
            || source.generation_revision() != generation.revision()
            || source.first_ordinal()
                != generation
                    .first_ordinal()
                    .ok_or(SyndicValidationError::Invariant(
                        "accepted-next source references an empty generation",
                    ))?
            || source.last_ordinal()
                != generation
                    .last_ordinal()
                    .ok_or(SyndicValidationError::Invariant(
                        "accepted-next source references an empty generation",
                    ))?
            || generation.next_turn_count() == 0
        {
            return invariant("accepted-next source and generation disagree");
        }
        Ok(())
    })
}

fn validate_gates(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<InputGatesFamily>(reader, |key, gate| {
        if *key != gate.thread_id() {
            return invariant("input-gate key and thread disagree");
        }
        let thread = require::<ThreadsFamily>(reader, key, "input gate has no matching thread")?;
        validate_gate_turn(reader, &thread, gate)?;
        let mut steering = 0_u64;
        let mut next = 0_u64;
        let mut live_bytes = 0_u64;
        let mut route_generation_high_water = None;
        scan_range::<AcceptedRouteGenerationsFamily>(
            reader,
            ThreadRouteKey {
                thread: *key,
                generation: crate::AcceptedRouteGeneration::FIRST,
            },
            ThreadRouteKey {
                thread: *key,
                generation: crate::AcceptedRouteGeneration::new(u64::MAX)
                    .expect("maximum is nonzero"),
            },
            |_, route| {
                route_generation_high_water = Some(route.generation());
                if matches!(
                    route.target(),
                    AcceptedRouteTarget::AwaitingSteering(_) | AcceptedRouteTarget::Steering(_)
                ) {
                    steering = add(
                        steering,
                        add(route.ready_retryable_count(), route.delivering_count())?,
                    )?;
                }
                next = add(next, route.next_turn_count())?;
                live_bytes = add(live_bytes, route.live_logical_utf8_bytes())?;
                Ok(())
            },
        )?;
        if gate.route_generation_high_water() != route_generation_high_water {
            return invariant("input-gate route-generation high-water disagrees");
        }
        if let Some(proof) = gate.selected_route()
            && route_generation_high_water
                .map(|high_water| proof.generation() > high_water)
                .unwrap_or(true)
        {
            return invariant("input-gate selected route exceeds generation high-water");
        }
        if steering != gate.live_steering_count()
            || next != gate.live_next_turn_count()
            || live_bytes != gate.live_logical_utf8_bytes()
        {
            return invariant("input-gate aggregates disagree with route generations");
        }
        validate_gate_head(reader, gate)
    })
}

fn validate_gate_turn(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    gate: &crate::InputGateRecord,
) -> Result<(), SyndicValidationError> {
    let Some(turn_id) = gate.state().blocking_turn_id() else {
        if let Some(tail) = thread.committed_tail()
            && require::<TurnStatesFamily>(reader, &tail, "committed-tail turn state is missing")?
                .lifecycle()
                .blocks_same_thread_start()
        {
            return invariant("idle input gate leaves committed turn blocking");
        }
        return Ok(());
    };
    let provider_stop_nonce = provider_stop_nonce(reader, gate)?;
    if matches!(gate.state(), InputGateState::Compacting { .. }) || provider_stop_nonce.is_some() {
        let turn = require::<TurnsFamily>(reader, &turn_id, "compaction gate turn is missing")?;
        let state =
            require::<TurnStatesFamily>(reader, &turn_id, "compaction gate turn state is missing")?;
        let operation_id = crate::CompactionOperationId::new(
            thread.id(),
            crate::CompactionOperationNonce::from_bytes(*turn_id.as_bytes()),
        );
        let operation = require::<CompactionOperationsFamily>(
            reader,
            &operation_id,
            "compaction gate operation is missing",
        )?;
        let operation_matches = provider_stop_nonce.map_or_else(
            || operation.state().is_live(),
            |stop_nonce| {
                operation.state() == &crate::CompactionOperationState::Stopping(stop_nonce)
            },
        );
        if turn.origin_thread_id() != thread.id()
            || turn.kind()
                != crate::TurnKind::ProviderOperation(
                    crate::ProviderOperationKind::ContextCompaction,
                )
            || turn.parent() != crate::ConversationParent::Root
            || !(state.lifecycle().blocks_same_thread_start()
                || state.lifecycle().is_proven_terminal())
            || !operation_matches
        {
            return invariant("compaction gate provider turn disagrees");
        }
        return Ok(());
    }
    if thread.committed_tail() != Some(turn_id) {
        return invariant("input-gate blocking turn is not the committed tail");
    }
    let turn = require::<TurnsFamily>(reader, &turn_id, "input-gate turn is missing")?;
    let state = require::<TurnStatesFamily>(reader, &turn_id, "input-gate turn state is missing")?;
    let valid_lifecycle = match gate.state() {
        InputGateState::FinalizingHistory(_) => state.lifecycle().is_proven_terminal(),
        InputGateState::AwaitingTerminal(_) => {
            state.lifecycle() == crate::TurnLifecycle::UnknownTerminal
        }
        InputGateState::Idle => unreachable!("idle gates have no blocking turn"),
        _ => state.lifecycle().blocks_same_thread_start(),
    };
    if turn.origin_thread_id() != thread.id() || !valid_lifecycle {
        return invariant("input-gate turn does not block the owning thread");
    }
    Ok(())
}

fn validate_gate_head(
    reader: &DomainReader<'_, SyndicDomain>,
    gate: &crate::InputGateRecord,
) -> Result<(), SyndicValidationError> {
    let provider_stopping = provider_stop_nonce(reader, gate)?.is_some();
    let Some(proof) = gate.selected_route() else {
        return if matches!(
            gate.state(),
            InputGateState::Idle
                | InputGateState::PendingTurn(_)
                | InputGateState::Compacting { .. }
                | InputGateState::FinalizingHistory(_)
        ) || provider_stopping
        {
            Ok(())
        } else {
            invariant("active input gate has no selected route generation")
        };
    };
    let head = require::<AcceptedRouteGenerationHeadsFamily>(
        reader,
        &gate.thread_id(),
        "input gate references a missing route head",
    )?;
    let route = require::<AcceptedRouteGenerationsFamily>(
        reader,
        &ThreadRouteKey {
            thread: gate.thread_id(),
            generation: proof.generation(),
        },
        "input gate references a missing route generation",
    )?;
    if head.proof() != proof || route.revision() != proof.revision() {
        return invariant("input gate selected route proof is stale");
    }
    let compatible = matches!(
        (gate.state(), route.target()),
        (
            InputGateState::AwaitingSteering(_),
            AcceptedRouteTarget::AwaitingSteering(_)
        ) | (
            InputGateState::Steerable(_),
            AcceptedRouteTarget::Steering(_)
        ) | (
            InputGateState::AwaitingTerminal(_),
            AcceptedRouteTarget::AwaitingTerminal(_)
        ) | (
            InputGateState::Stopping { .. },
            AcceptedRouteTarget::NextTurn(NextTurnReason::Stop)
        ) | (
            InputGateState::PendingTurn(_),
            AcceptedRouteTarget::NextTurn(_) | AcceptedRouteTarget::ProjectionLost(_)
        ) | (InputGateState::Compacting { .. }, _)
            | (InputGateState::FinalizingHistory(_), _)
            | (InputGateState::Idle, _)
    ) || provider_stopping;
    compatible
        .then_some(())
        .ok_or(SyndicValidationError::Invariant(
            "input gate state and selected route target disagree",
        ))
}

fn provider_stop_nonce(
    reader: &DomainReader<'_, SyndicDomain>,
    gate: &crate::InputGateRecord,
) -> Result<Option<crate::StopOperationNonce>, SyndicValidationError> {
    let InputGateState::Stopping {
        turn_id,
        operation_nonce,
    } = gate.state()
    else {
        return Ok(None);
    };
    let Some(stop) = point::<StopOperationsFamily>(
        reader,
        &crate::StopOperationId::new(gate.thread_id(), *operation_nonce),
    )?
    else {
        return Ok(None);
    };
    Ok((stop.admission().is_provider_operation()
        && stop.target().turn_id() == *turn_id
        && stop.target().turn_kind()
            == crate::TurnKind::ProviderOperation(crate::ProviderOperationKind::ContextCompaction))
    .then_some(*operation_nonce))
}
