use beryl_home_store::DomainReader;

use crate::{
    AcceptedInputLifecycle, AcceptedRouteAbandonmentKind, AcceptedRouteGenerationRecord,
    AcceptedRouteLeafState, AcceptedRouteLeafTransitionKind, AcceptedRouteLeafTransitionProof,
    AcceptedRouteTarget, BindingState, NextTurnReason, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::super::scan::require;
use super::util::invariant;

pub(super) fn validate_lost_proof(
    reader: &DomainReader<'_, SyndicDomain>,
    generation: &AcceptedRouteGenerationRecord,
) -> Result<(), SyndicValidationError> {
    let AcceptedRouteTarget::ProjectionLost(proof) = generation.target() else {
        return Ok(());
    };
    let pending = proof.prior_target().pending();
    let abandonment = proof.abandonment();
    let gate = require::<InputGatesFamily>(
        reader,
        &generation.thread_id(),
        "projection-lost route abandonment references a missing gate",
    )?;
    let route_successor = abandonment.expected_route().revision().checked_next().ok();
    if pending.snapshot_id() != proof.snapshot_id()
        || pending.cas_thread_id() != proof.cas_thread_id()
        || pending.binding_revision() != abandonment.expected_binding_revision()
        || abandonment.expected_binding_revision().checked_next().ok()
            != Some(proof.retirement_binding_revision())
        || abandonment.expected_gate_revision() >= gate.revision()
        || abandonment.expected_route().generation() != generation.generation()
        || route_successor.is_none_or(|revision| revision > generation.revision())
    {
        return invariant("projection-lost route abandonment proof disagrees");
    }

    validate_retirement(reader, generation, proof, pending)?;
    validate_named_rejection(reader, generation, abandonment)
}

fn validate_retirement(
    reader: &DomainReader<'_, SyndicDomain>,
    generation: &AcceptedRouteGenerationRecord,
    proof: &crate::AcceptedRouteProjectionLostProof,
    pending: &crate::PendingSteeringTargetProof,
) -> Result<(), SyndicValidationError> {
    let binding = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: generation.thread_id(),
            revision: proof.retirement_binding_revision(),
        },
        "projection-lost route references a missing stale binding",
    )?;
    let BindingState::Stale(stale) = binding.state() else {
        return invariant("projection-lost route does not reference a stale binding");
    };
    if stale.cas_thread_id() != proof.cas_thread_id() {
        return invariant("projection-lost route stale binding has different CAS thread");
    }
    let reservation = require::<CasThreadIndexFamily>(
        reader,
        &CasThreadKey::Record(proof.cas_thread_id().clone()),
        "projection-lost route references a missing CAS retirement",
    )?;
    if reservation.thread_id() != generation.thread_id()
        || reservation.retired_binding_revision() != Some(proof.retirement_binding_revision())
    {
        return invariant("projection-lost route CAS retirement proof disagrees");
    }
    let snapshot = require::<ExecutionSnapshotsFamily>(
        reader,
        &proof.snapshot_id(),
        "projection-lost route references a missing snapshot",
    )?;
    if snapshot.thread_id() != generation.thread_id()
        || snapshot.cas_thread_id() != proof.cas_thread_id()
        || snapshot.binding_revision() != pending.binding_revision()
        || snapshot.active_turn_id() != pending.active_turn_id()
        || snapshot.id() != pending.snapshot_id()
    {
        return invariant("projection-lost route snapshot proof disagrees");
    }
    Ok(())
}

fn validate_named_rejection(
    reader: &DomainReader<'_, SyndicDomain>,
    generation: &AcceptedRouteGenerationRecord,
    abandonment: crate::AcceptedRouteAbandonmentProof,
) -> Result<(), SyndicValidationError> {
    let AcceptedRouteAbandonmentKind::ExactRejectedInput {
        input_id,
        expected_input_revision,
    } = abandonment.kind()
    else {
        return Ok(());
    };
    let input = require::<AcceptedInputsFamily>(
        reader,
        &input_id,
        "named abandonment references a missing accepted input",
    )?;
    let leaf = require::<AcceptedRouteLeavesFamily>(
        reader,
        &input_id,
        "named abandonment references a missing route leaf",
    )?;
    let successor = expected_input_revision.checked_next().map_err(|_| {
        SyndicValidationError::Invariant("named abandonment input revision is exhausted")
    })?;
    if input.thread_id() != generation.thread_id()
        || input.route_generation() != generation.generation()
        || leaf.input_id() != input.id()
        || leaf.thread_id() != input.thread_id()
        || leaf.generation() != input.route_generation()
        || leaf.ordinal() != input.ordinal()
        || leaf.revision() < successor
    {
        return invariant("named abandonment input witness disagrees");
    }
    if leaf.revision() == successor
        && (leaf.state() != AcceptedRouteLeafState::NextTurn(NextTurnReason::ProjectionLost)
            || leaf.lifecycle() != AcceptedInputLifecycle::Retryable
            || leaf.last_transition()
                != Some(AcceptedRouteLeafTransitionProof::new(
                    abandonment.expected_gate_revision(),
                    abandonment.expected_route(),
                    expected_input_revision,
                    AcceptedRouteLeafTransitionKind::ProjectionLostExactRejection,
                )))
    {
        return invariant("named abandonment immediate successor witness disagrees");
    }
    Ok(())
}
