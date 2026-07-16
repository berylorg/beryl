use beryl_home_store::DomainReader;
use beryl_model::CasNativeTurnCount;

use crate::{
    BindingHeadRecord, BindingState, CasLineageProof, CasRepresentedPrefixProof,
    CasThreadIndexRecord, NativeCasLineage, StaleCasBinding, UsableCasBinding, codec::*,
    domain::SyndicDomain, error::SyndicValidationError,
};

use super::scan::{point, require, scan};

mod active;
mod history;
mod proofs;
mod reservations;

use active::{
    validate_active, validate_cas_turns, validate_current_active_gate, validate_snapshots,
};
use history::validate_history;
use proofs::{validate_current_usable_prefix, validate_stale, validate_usable};
use reservations::{require_reservation, validate_cas_thread_reservations};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    validate_heads(reader)?;
    validate_history(reader)?;
    validate_cas_thread_memberships(reader)?;
    validate_cas_thread_reservations(reader)?;
    validate_snapshots(reader)?;
    validate_cas_turns(reader)
}

fn validate_heads(reader: &DomainReader<'_, SyndicDomain>) -> Result<(), SyndicValidationError> {
    scan::<ThreadsFamily>(reader, |_, thread| {
        let head =
            require::<BindingHeadsFamily>(reader, &thread.id(), "thread binding head is missing")?;
        let binding = require::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: thread.id(),
                revision: head.revision(),
            },
            "binding head record is missing",
        )?;
        let expected = BindingHeadRecord::new(
            thread.id(),
            binding.revision(),
            binding.state().lifecycle(),
            binding.selected_path().digest(),
        );
        if head != expected {
            return invariant("binding head and current record disagree");
        }
        if binding.selected_path().tail() != thread.committed_tail()
            || binding.selected_path().digest() != thread.selected_path_digest()
            || binding.selected_path().thread_revision() > thread.revision()
        {
            return invariant("current binding selected-path proof disagrees with thread");
        }
        validate_current_state(reader, &binding)?;
        Ok(())
    })?;
    scan::<BindingHeadsFamily>(reader, |key, head| {
        if *key != head.thread_id() || point::<ThreadsFamily>(reader, key)?.is_none() {
            return invariant("binding head has no matching thread");
        }
        Ok(())
    })
}

fn validate_current_state(
    reader: &DomainReader<'_, SyndicDomain>,
    binding: &crate::BindingRecord,
) -> Result<(), SyndicValidationError> {
    match binding.state() {
        BindingState::Valid(usable) => {
            validate_usable(reader, binding.selected_path(), usable)?;
            validate_current_usable_prefix(reader, binding, usable.represented_prefix())?;
            require_reservation(
                reader,
                usable.cas_thread_id(),
                binding.thread_id(),
                binding.revision(),
                false,
            )
        }
        BindingState::Active(active) => {
            validate_active(reader, binding, active, true)?;
            validate_current_active_gate(reader, binding, active)
        }
        BindingState::Stale(stale) => {
            validate_stale(reader, binding, stale)?;
            require_reservation(
                reader,
                stale.cas_thread_id(),
                binding.thread_id(),
                binding.revision(),
                true,
            )?;
            validate_current_abandoned_gate(reader, binding)
        }
        BindingState::Unbound { .. } => Ok(()),
    }
}

fn validate_current_abandoned_gate(
    reader: &DomainReader<'_, SyndicDomain>,
    binding: &crate::BindingRecord,
) -> Result<(), SyndicValidationError> {
    let Some(prior_revision) = binding.revision().get().checked_sub(1) else {
        return Ok(());
    };
    let prior_revision = beryl_model::BindingRevision::new(prior_revision).map_err(|_| {
        SyndicValidationError::Invariant("stale binding predecessor revision is invalid")
    })?;
    let Some(prior) = point::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: binding.thread_id(),
            revision: prior_revision,
        },
    )?
    else {
        return Ok(());
    };
    let BindingState::Active(active) = prior.state() else {
        return Ok(());
    };
    let state = require::<TurnStatesFamily>(
        reader,
        &active.turn_id(),
        "abandoned active turn state is missing",
    )?;
    let gate = require::<InputGatesFamily>(
        reader,
        &binding.thread_id(),
        "abandoned active input gate is missing",
    )?;
    if gate.live_steering_count() != 0 {
        return invariant("abandoned active gate retains live steering");
    }
    if state.lifecycle().blocks_same_thread_start() {
        if gate.state() != &crate::InputGateState::PendingTurn(active.turn_id()) {
            return invariant("abandoned blocking turn is not pending recovery delivery");
        }
    } else if state.lifecycle().is_proven_terminal() {
        if gate.state() != &crate::InputGateState::Idle {
            return invariant("terminal abandoned turn does not reopen an idle gate");
        }
    } else {
        return invariant("abandoned active turn lifecycle is unsupported");
    }
    Ok(())
}

fn require_membership(
    reader: &DomainReader<'_, SyndicDomain>,
    cas_thread: &beryl_model::CasThreadId,
    thread: beryl_model::SyndicThreadId,
    revision: beryl_model::BindingRevision,
) -> Result<(), SyndicValidationError> {
    let membership = require::<CasThreadBindingIndexFamily>(
        reader,
        &CasThreadBindingKey::Record(cas_thread.clone(), revision),
        "CAS thread binding membership is missing",
    )?;
    if membership.cas_thread_id() != cas_thread
        || membership.thread_id() != thread
        || membership.binding_revision() != revision
    {
        return invariant("CAS thread binding membership disagrees");
    }
    Ok(())
}

fn validate_cas_thread_memberships(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut current_cas = None;
    let mut current_index: Option<CasThreadIndexRecord> = None;
    let mut first_revision = None;
    let mut previous: Option<crate::BindingRecord> = None;
    scan::<CasThreadBindingIndexFamily>(reader, |key, membership| {
        let CasThreadBindingKey::Record(cas_thread, revision) = key else {
            return invariant("stored CAS-thread binding cursor sentinel");
        };
        if current_cas.as_ref() != Some(cas_thread) {
            finish_membership_group(current_index.as_ref(), first_revision, previous.as_ref())?;
            current_cas = Some(cas_thread.clone());
            current_index = Some(require::<CasThreadIndexFamily>(
                reader,
                &CasThreadKey::Record(cas_thread.clone()),
                "CAS thread binding membership has no owner reservation",
            )?);
            first_revision = None;
            previous = None;
        }
        let index = current_index
            .as_ref()
            .expect("membership group initializes its reservation");
        if membership.cas_thread_id() != cas_thread
            || membership.thread_id() != index.thread_id()
            || membership.binding_revision() != *revision
        {
            return invariant("CAS thread binding membership key or owner disagrees");
        }
        let binding = require::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: membership.thread_id(),
                revision: *revision,
            },
            "CAS thread binding membership names a missing binding",
        )?;
        if binding.state().cas_thread_id() != Some(cas_thread) {
            return invariant("CAS thread binding membership names another CAS thread");
        }
        if let Some(prior) = previous.as_ref() {
            if prior.revision() >= binding.revision() {
                return invariant("CAS thread binding membership revisions do not advance");
            }
            validate_membership_continuity(reader, prior, &binding)?;
        } else {
            first_revision = Some(binding.revision());
            validate_first_membership(&binding)?;
        }
        previous = Some(binding);
        Ok(())
    })?;
    finish_membership_group(current_index.as_ref(), first_revision, previous.as_ref())
}

fn finish_membership_group(
    index: Option<&CasThreadIndexRecord>,
    first_revision: Option<beryl_model::BindingRevision>,
    last: Option<&crate::BindingRecord>,
) -> Result<(), SyndicValidationError> {
    let Some(index) = index else {
        return Ok(());
    };
    let Some(last) = last else {
        return invariant("CAS thread binding membership group is empty");
    };
    if first_revision != Some(index.first_binding_revision())
        || last.revision() != index.latest_binding_revision()
    {
        return invariant("CAS thread binding membership frontier disagrees with reservation");
    }
    match (index.retired_binding_revision(), last.state()) {
        (Some(retired), BindingState::Stale(_)) if retired == last.revision() => Ok(()),
        (None, BindingState::Valid(_) | BindingState::Active(_)) => Ok(()),
        _ => invariant("CAS thread binding membership has invalid terminal state"),
    }
}

fn validate_first_membership(binding: &crate::BindingRecord) -> Result<(), SyndicValidationError> {
    let usable = match binding.state() {
        BindingState::Valid(usable) => usable,
        BindingState::Active(active) => active.usable(),
        BindingState::Stale(stale) => {
            return if valid_first_stale_position(stale) {
                Ok(())
            } else {
                invariant("first stale CAS reservation has an unproven native turn position")
            };
        }
        BindingState::Unbound { .. } => {
            return invariant("unbound binding has a CAS membership");
        }
    };
    if usable.lineage().established_prefix() != usable.represented_prefix() {
        return invariant(
            "first usable CAS membership was not established at its represented prefix",
        );
    }
    match usable.lineage() {
        CasLineageProof::Native {
            mechanism: NativeCasLineage::Fresh,
            ..
        } if usable.represented_prefix().tail().is_none()
            && usable.native_turn_count() == CasNativeTurnCount::ZERO =>
        {
            Ok(())
        }
        CasLineageProof::Native {
            mechanism: NativeCasLineage::Fresh,
            ..
        } => invariant("fresh native CAS reservation is not empty at count zero"),
        CasLineageProof::RecoveredInjection(_)
            if usable.native_turn_count() != CasNativeTurnCount::ZERO =>
        {
            invariant("recovered CAS reservation establishment has a nonzero native turn count")
        }
        CasLineageProof::RecoveredInjection(_) | CasLineageProof::Native { .. } => Ok(()),
    }
}

fn valid_first_stale_position(stale: &StaleCasBinding) -> bool {
    match stale.observed_native_turn_count() {
        None | Some(CasNativeTurnCount::ZERO) => true,
        Some(_) => {
            let Some(prefix) = stale.observed_prefix() else {
                return false;
            };
            matches!(
                stale.observed_lineage(),
                Some(CasLineageProof::Native {
                    mechanism: NativeCasLineage::Fork,
                    established_prefix,
                }) if prefix.tail().is_some() && established_prefix == prefix
            )
        }
    }
}

fn validate_membership_continuity(
    reader: &DomainReader<'_, SyndicDomain>,
    prior: &crate::BindingRecord,
    next: &crate::BindingRecord,
) -> Result<(), SyndicValidationError> {
    if matches!(prior.state(), BindingState::Stale(_)) {
        return invariant("retired CAS thread has a later binding membership");
    }
    let prior_usable = binding_usable(prior).ok_or(SyndicValidationError::Invariant(
        "CAS membership predecessor is not usable",
    ))?;
    if let BindingState::Stale(stale) = next.state() {
        return validate_stale_membership(prior_usable, stale);
    }
    let next_usable = binding_usable(next).ok_or(SyndicValidationError::Invariant(
        "CAS membership successor is not usable",
    ))?;
    if matches!(prior.state(), BindingState::Active(_))
        && matches!(next.state(), BindingState::Valid(_))
        && prior.revision().checked_next().ok() == Some(next.revision())
    {
        let BindingState::Active(active) = prior.state() else {
            unreachable!("active predecessor matched above")
        };
        if point::<ActiveCasTurnsFamily>(reader, &active.snapshot_id())?.is_none() {
            return if next_usable == active.usable() {
                Ok(())
            } else {
                invariant("cancelled CAS activation changes usable authority")
            };
        }
        let expected = prior_usable
            .native_turn_count()
            .checked_next()
            .map_err(|_| {
                SyndicValidationError::Invariant("terminal CAS native turn count is exhausted")
            })?;
        return if next_usable.native_turn_count() == expected
            && next_usable.tool_profile() == prior_usable.tool_profile()
        {
            Ok(())
        } else {
            invariant("terminal CAS binding does not advance its native turn count exactly once")
        };
    }
    if prior_usable.execution() != next_usable.execution()
        || prior_usable.cas_thread_id() != next_usable.cas_thread_id()
        || prior_usable.tool_profile() != next_usable.tool_profile()
    {
        return invariant("CAS membership reuse changes execution or identity");
    }
    if prior_usable.lineage() == next_usable.lineage()
        && same_stable_prefix(
            prior_usable.represented_prefix(),
            next_usable.represented_prefix(),
        )
    {
        return if prior_usable.native_turn_count() == next_usable.native_turn_count() {
            Ok(())
        } else {
            invariant("stable CAS membership changes its native turn count")
        };
    }
    match next_usable.lineage() {
        CasLineageProof::Native {
            mechanism: NativeCasLineage::Resume,
            established_prefix,
        } if matches!(prior_usable.lineage(), CasLineageProof::Native { .. })
            && established_prefix == prior_usable.represented_prefix()
            && same_stable_prefix(
                prior_usable.represented_prefix(),
                next_usable.represented_prefix(),
            )
            && next_usable.native_turn_count() == prior_usable.native_turn_count() =>
        {
            Ok(())
        }
        _ => invariant("CAS membership lineage does not continue its prior authority"),
    }
}

fn binding_usable(binding: &crate::BindingRecord) -> Option<&UsableCasBinding> {
    match binding.state() {
        BindingState::Valid(usable) => Some(usable),
        BindingState::Active(active) => Some(active.usable()),
        BindingState::Unbound { .. } | BindingState::Stale(_) => None,
    }
}

fn validate_stale_membership(
    prior: &UsableCasBinding,
    stale: &StaleCasBinding,
) -> Result<(), SyndicValidationError> {
    if prior.execution() != stale.execution()
        || prior.cas_thread_id() != stale.cas_thread_id()
        || stale.observed_tool_profile() != Some(prior.tool_profile())
        || stale
            .observed_prefix()
            .is_some_and(|prefix| prefix != prior.represented_prefix())
        || stale
            .observed_lineage()
            .is_some_and(|lineage| lineage != prior.lineage())
        || stale.observed_native_turn_count() != Some(prior.native_turn_count())
    {
        return invariant("CAS retirement provenance changes its prior usable authority");
    }
    Ok(())
}

fn same_stable_prefix(prior: CasRepresentedPrefixProof, next: CasRepresentedPrefixProof) -> bool {
    prior.tail() == next.tail()
        && prior.digest() == next.digest()
        && prior.source_thread_revision() <= next.source_thread_revision()
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
