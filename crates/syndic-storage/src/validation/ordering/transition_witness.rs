use beryl_home_store::DomainReader;
use beryl_model::AcceptedInputRevision;

use crate::{
    AcceptedInputLifecycle, AcceptedInputRecord, AcceptedRouteLeafRecord, AcceptedRouteLeafState,
    AcceptedRouteLeafTransitionKind, NextTurnReason, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::super::scan::require;
use super::util::invariant;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
    input: &AcceptedInputRecord,
    leaf: &AcceptedRouteLeafRecord,
) -> Result<(), SyndicValidationError> {
    let Some(proof) = leaf.last_transition() else {
        if let Some(promotion) = leaf.promotion() {
            return if promotion.expected_input_revision()
                == AcceptedInputRevision::new(1).expect("first accepted-input revision")
            {
                Ok(())
            } else {
                invariant("promoted accepted-route leaf has no witness for its revised predecessor")
            };
        }
        let admission_state = matches!(
            leaf.state(),
            AcceptedRouteLeafState::Routed
                | AcceptedRouteLeafState::NextTurn(
                    NextTurnReason::PendingTurn
                        | NextTurnReason::Compaction
                        | NextTurnReason::Stop
                        | NextTurnReason::TerminalHistory
                        | NextTurnReason::UnknownTerminal
                )
        );
        if leaf.revision().get() == 1
            && leaf.lifecycle() == AcceptedInputLifecycle::Admitted
            && admission_state
        {
            return Ok(());
        }
        return invariant("transitioned accepted-route leaf is missing its witness");
    };
    let gate = require::<InputGatesFamily>(
        reader,
        &input.thread_id(),
        "accepted-route transition witness references a missing gate",
    )?;
    let generation = require::<AcceptedRouteGenerationsFamily>(
        reader,
        &ThreadRouteKey {
            thread: input.thread_id(),
            generation: leaf.generation(),
        },
        "accepted-route transition witness references a missing generation",
    )?;
    let successor_revision = leaf.promotion().map_or(leaf.revision(), |promotion| {
        promotion.expected_input_revision()
    });
    let successor_agrees = match leaf.promotion() {
        Some(promotion) => {
            proof.expected_gate_revision() < promotion.expected_gate_revision()
                && proof.expected_route().revision() < promotion.expected_route().revision()
                && transition_can_precede_promotion(generation.target(), proof.kind())
        }
        None => transition_successor_agrees(leaf, proof.kind()),
    };
    if proof.expected_route().generation() != leaf.generation()
        || proof.expected_input_revision().checked_next().ok() != Some(successor_revision)
        || input.admission_gate_revision() >= proof.expected_gate_revision()
        || proof.expected_gate_revision() >= gate.revision()
        || proof.expected_route().revision() >= generation.revision()
        || !successor_agrees
    {
        return invariant("accepted-route leaf transition proof disagrees");
    }
    Ok(())
}

fn transition_can_precede_promotion(
    target: &crate::AcceptedRouteTarget,
    kind: AcceptedRouteLeafTransitionKind,
) -> bool {
    match kind {
        AcceptedRouteLeafTransitionKind::Retry => is_queue_only_steering_descendant(target),
        AcceptedRouteLeafTransitionKind::SteeringRejected => {
            matches!(
                target,
                crate::AcceptedRouteTarget::AwaitingSteering(_)
                    | crate::AcceptedRouteTarget::Steering(_)
            ) || is_queue_only_steering_descendant(target)
        }
        AcceptedRouteLeafTransitionKind::ProjectionLostExactRejection => {
            matches!(target, crate::AcceptedRouteTarget::ProjectionLost(_))
        }
        AcceptedRouteLeafTransitionKind::Begin | AcceptedRouteLeafTransitionKind::Complete => false,
    }
}

fn is_queue_only_steering_descendant(target: &crate::AcceptedRouteTarget) -> bool {
    matches!(
        target,
        crate::AcceptedRouteTarget::AwaitingTerminal(_)
            | crate::AcceptedRouteTarget::ProjectionLost(_)
            | crate::AcceptedRouteTarget::NextTurn(
                NextTurnReason::Stop | NextTurnReason::TerminalHistory
            )
    )
}

fn transition_successor_agrees(
    leaf: &AcceptedRouteLeafRecord,
    kind: AcceptedRouteLeafTransitionKind,
) -> bool {
    match kind {
        AcceptedRouteLeafTransitionKind::Begin => {
            leaf.state() == AcceptedRouteLeafState::Routed
                && leaf.lifecycle() == AcceptedInputLifecycle::Delivering
        }
        AcceptedRouteLeafTransitionKind::Retry => {
            leaf.state() == AcceptedRouteLeafState::Routed
                && leaf.lifecycle() == AcceptedInputLifecycle::Retryable
        }
        AcceptedRouteLeafTransitionKind::Complete => {
            leaf.state() == AcceptedRouteLeafState::Routed
                && leaf.lifecycle() == AcceptedInputLifecycle::Delivered
        }
        AcceptedRouteLeafTransitionKind::SteeringRejected => {
            leaf.state() == AcceptedRouteLeafState::NextTurn(NextTurnReason::SteeringRejected)
                && leaf.lifecycle() == AcceptedInputLifecycle::Retryable
        }
        AcceptedRouteLeafTransitionKind::ProjectionLostExactRejection => {
            leaf.state() == AcceptedRouteLeafState::NextTurn(NextTurnReason::ProjectionLost)
                && leaf.lifecycle() == AcceptedInputLifecycle::Retryable
        }
    }
}
