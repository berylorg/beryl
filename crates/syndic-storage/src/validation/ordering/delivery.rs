use beryl_home_store::DomainReader;

use crate::{
    BindingState, SteeringTargetProof, codec::*, domain::SyndicDomain, error::SyndicValidationError,
};

use super::super::scan::{point, require};

const RETIREMENT_HISTORY_ERROR: &str =
    "delivery-unknown CAS projection lacks exact retirement history";

pub(super) fn validate_delivery_unknown_proof(
    reader: &DomainReader<'_, SyndicDomain>,
    input: &crate::AcceptedInputRecord,
    target: &SteeringTargetProof,
) -> Result<(), SyndicValidationError> {
    let pending = target.pending();
    let turn = require::<TurnsFamily>(
        reader,
        &pending.active_turn_id(),
        "delivery-unknown steering target turn is missing",
    )?;
    if turn.origin_thread_id() != input.thread_id() {
        return invariant("delivery-unknown steering target belongs to another thread");
    }
    let binding = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: input.thread_id(),
            revision: pending.binding_revision(),
        },
        "delivery-unknown active binding provenance is missing",
    )?;
    let BindingState::Active(active) = binding.state() else {
        return invariant("delivery-unknown binding provenance was not active");
    };
    if active.snapshot_id() != pending.snapshot_id()
        || active.turn_id() != pending.active_turn_id()
        || active.usable().cas_thread_id() != pending.cas_thread_id()
    {
        return invariant("delivery-unknown binding provenance disagrees");
    }
    let snapshot = require::<ExecutionSnapshotsFamily>(
        reader,
        &pending.snapshot_id(),
        "delivery-unknown execution snapshot is missing",
    )?;
    if snapshot.thread_id() != input.thread_id()
        || snapshot.binding_revision() != pending.binding_revision()
        || snapshot.active_turn_id() != pending.active_turn_id()
        || snapshot.cas_thread_id() != pending.cas_thread_id()
    {
        return invariant("delivery-unknown execution snapshot disagrees");
    }
    validate_retirement(reader, input, target, &binding)?;
    validate_active_turn(reader, input, target)
}

fn validate_retirement(
    reader: &DomainReader<'_, SyndicDomain>,
    input: &crate::AcceptedInputRecord,
    target: &SteeringTargetProof,
    active_binding: &crate::BindingRecord,
) -> Result<(), SyndicValidationError> {
    let pending = target.pending();
    let retired_revision = pending
        .binding_revision()
        .checked_next()
        .map_err(|_| SyndicValidationError::Invariant(RETIREMENT_HISTORY_ERROR))?;
    let Some(retired_binding) = point::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: input.thread_id(),
            revision: retired_revision,
        },
    )?
    else {
        return invariant(RETIREMENT_HISTORY_ERROR);
    };
    let Some(owner) = point::<CasThreadIndexFamily>(
        reader,
        &CasThreadKey::Record(pending.cas_thread_id().clone()),
    )?
    else {
        return invariant(RETIREMENT_HISTORY_ERROR);
    };
    let Some(membership) = point::<CasThreadBindingIndexFamily>(
        reader,
        &CasThreadBindingKey::Record(pending.cas_thread_id().clone(), retired_revision),
    )?
    else {
        return invariant(RETIREMENT_HISTORY_ERROR);
    };
    let BindingState::Stale(stale) = retired_binding.state() else {
        return invariant(RETIREMENT_HISTORY_ERROR);
    };
    let expected_membership = crate::CasThreadBindingIndexRecord::new(
        pending.cas_thread_id().clone(),
        input.thread_id(),
        retired_revision,
    );
    if retired_binding.selected_path() != active_binding.selected_path()
        || stale.cas_thread_id() != pending.cas_thread_id()
        || owner.cas_thread_id() != pending.cas_thread_id()
        || owner.thread_id() != input.thread_id()
        || owner.first_binding_revision() > pending.binding_revision()
        || owner.latest_binding_revision() != retired_revision
        || owner.retired_binding_revision() != Some(retired_revision)
        || membership != expected_membership
    {
        return invariant(RETIREMENT_HISTORY_ERROR);
    }
    Ok(())
}

fn validate_active_turn(
    reader: &DomainReader<'_, SyndicDomain>,
    input: &crate::AcceptedInputRecord,
    target: &SteeringTargetProof,
) -> Result<(), SyndicValidationError> {
    let pending = target.pending();
    let active_turn = require::<ActiveCasTurnsFamily>(
        reader,
        &pending.snapshot_id(),
        "delivery-unknown CAS turn correlation is missing",
    )?;
    if active_turn.thread_id() != input.thread_id()
        || active_turn.turn_id() != pending.active_turn_id()
        || active_turn.binding_revision() != pending.binding_revision()
        || active_turn.cas_thread_id() != pending.cas_thread_id()
        || active_turn.cas_turn_id() != target.cas_turn_id()
    {
        return invariant("delivery-unknown CAS turn correlation disagrees");
    }
    Ok(())
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
