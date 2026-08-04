use beryl_home_store::DomainReader;

use crate::{
    BindingState, SourceEventPayload, SourceEventSequence, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::active::{
    validate_abandoned_active_source_history, validate_active, validate_active_cas_turn,
    validate_exact_active_source_history,
};
use super::proofs::{
    validate_pending_prefix, validate_selected_path, validate_stale, validate_usable,
};
use super::{invariant, require_membership, require_reservation};
use crate::validation::scan::{point, require, scan};

pub(super) fn validate_history(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut current_thread = None;
    let mut expected = 1_u64;
    let mut observed_last = None;
    let mut previous_binding: Option<crate::BindingRecord> = None;
    scan::<BindingsFamily>(reader, |key, binding| {
        if current_thread != Some(key.thread) {
            finish_history(reader, current_thread, observed_last)?;
            current_thread = Some(key.thread);
            expected = 1;
            observed_last = None;
            previous_binding = None;
        }
        if key.thread != binding.thread_id()
            || key.revision != binding.revision()
            || binding.revision().get() != expected
        {
            return invariant("binding key or contiguous revision history disagrees");
        }
        if expected == 1
            && (!matches!(binding.state(), BindingState::Unbound { .. })
                || binding.selected_path().thread_revision().get() != 1)
        {
            return invariant("initial binding is not the creation-time unbound revision");
        }
        require::<ThreadsFamily>(
            reader,
            &binding.thread_id(),
            "binding owner thread is missing",
        )?;
        let canonical = require::<ThreadExecutionsFamily>(
            reader,
            &binding.thread_id(),
            "binding owner execution is missing",
        )?;
        if binding
            .state()
            .execution()
            .is_some_and(|execution| execution != canonical.execution())
        {
            return invariant("binding execution disagrees with canonical thread execution");
        }
        validate_selected_path(reader, binding.selected_path())?;
        if let Some(cas_thread) = binding.state().cas_thread_id() {
            require_reservation(
                reader,
                cas_thread,
                binding.thread_id(),
                binding.revision(),
                matches!(binding.state(), BindingState::Stale(_)),
            )?;
            require_membership(reader, cas_thread, binding.thread_id(), binding.revision())?;
        }
        if let BindingState::Valid(usable) = binding.state() {
            validate_usable(reader, binding.selected_path(), usable)?;
            validate_pending_prefix(reader, binding, usable.represented_prefix())?;
        }
        if let BindingState::Active(active) = binding.state() {
            validate_active(reader, binding, active, false)?;
        }
        if let BindingState::Stale(stale) = binding.state() {
            validate_stale(reader, binding, stale)?;
        }
        if let Some(previous) = previous_binding.as_ref() {
            validate_binding_transition(reader, previous, binding)?;
        }
        expected = expected
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "binding revision exhausted",
            ))?;
        observed_last = Some(binding.revision());
        previous_binding = Some(binding.clone());
        Ok(())
    })?;
    finish_history(reader, current_thread, observed_last)
}

fn finish_history(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: Option<beryl_model::SyndicThreadId>,
    observed_last: Option<beryl_model::BindingRevision>,
) -> Result<(), SyndicValidationError> {
    let Some(thread) = thread else {
        return Ok(());
    };
    let head = require::<BindingHeadsFamily>(reader, &thread, "binding history head is missing")?;
    if Some(head.revision()) != observed_last {
        return invariant("binding head does not select the latest contiguous revision");
    }
    Ok(())
}

fn validate_binding_transition(
    reader: &DomainReader<'_, SyndicDomain>,
    previous: &crate::BindingRecord,
    current: &crate::BindingRecord,
) -> Result<(), SyndicValidationError> {
    if previous.thread_id() != current.thread_id() {
        return invariant("binding successor changes owner");
    }
    if current.selected_path().thread_revision() < previous.selected_path().thread_revision() {
        return invariant("binding successor selected-path revision regresses");
    }
    if let BindingState::Active(active) = current.state() {
        let BindingState::Valid(prior) = previous.state() else {
            return invariant("active binding does not succeed a valid binding");
        };
        let Some(expected) =
            prior.advance_represented_source_revision(current.selected_path().thread_revision())
        else {
            return invariant("active binding represented-prefix revision regresses");
        };
        if !current
            .selected_path()
            .is_compatible_descendant_of(previous.selected_path())
            || active.usable() != &expected
        {
            return invariant("active binding does not preserve compatible prior valid authority");
        }
        return Ok(());
    }
    let BindingState::Active(active) = previous.state() else {
        if !current
            .selected_path()
            .is_compatible_descendant_of(previous.selected_path())
            && !matches!(current.state(), BindingState::Unbound { .. })
        {
            return invariant(
                "incompatible selected-path change does not publish an unbound binding",
            );
        }
        return Ok(());
    };
    if !current
        .selected_path()
        .is_compatible_descendant_of(previous.selected_path())
    {
        return invariant("active binding successor changes selected path incompatibly");
    }
    match current.state() {
        BindingState::Valid(usable) => {
            let Some(active_turn) = point::<ActiveCasTurnsFamily>(reader, &active.snapshot_id())?
            else {
                if usable == active.usable() {
                    return Ok(());
                }
                return invariant("cancelled active successor changes usable CAS authority");
            };
            validate_active_cas_turn(reader, &active_turn)?;
            validate_terminal_event_authority(reader, active, &active_turn)?;
            let expected_native_turn_count = active
                .usable()
                .native_turn_count()
                .checked_next()
                .map_err(|_| {
                    SyndicValidationError::Invariant("terminal CAS native turn count is exhausted")
                })?;
            if active_turn.binding_revision() != previous.revision()
                || usable.execution() != active.usable().execution()
                || usable.cas_thread_id() != active.usable().cas_thread_id()
                || usable.tool_profile() != active.usable().tool_profile()
                || usable.lineage() != active.usable().lineage()
                || usable.native_turn_count() != expected_native_turn_count
                || usable.represented_prefix().tail() != current.selected_path().tail()
                || usable.represented_prefix().digest() != current.selected_path().digest()
                || usable.represented_prefix().source_thread_revision()
                    != current.selected_path().thread_revision()
            {
                return invariant("valid active successor lacks exact terminal CAS authority");
            }
        }
        BindingState::Stale(stale) => {
            let snapshot = require::<ExecutionSnapshotsFamily>(
                reader,
                &active.snapshot_id(),
                "stale active successor lacks execution snapshot",
            )?;
            if stale.execution() != active.usable().execution()
                || stale.cas_thread_id() != active.usable().cas_thread_id()
                || stale.observed_tool_profile() != Some(active.usable().tool_profile())
                || stale.observed_prefix() != Some(active.usable().represented_prefix())
                || stale.observed_lineage() != Some(active.usable().lineage())
                || stale.observed_native_turn_count() != Some(active.usable().native_turn_count())
                || stale.loaded_generation() != Some(snapshot.loaded_generation())
                || stale.observed_at() < active.started_at()
            {
                return invariant("stale active successor loses exact abandoned authority");
            }
            validate_abandoned_active_source_history(reader, active)?;
        }
        BindingState::Unbound { .. } => {
            return invariant("active binding has an unsupported successor state");
        }
        BindingState::Active(_) => unreachable!("active successor handled above"),
    }
    Ok(())
}

fn validate_terminal_event_authority(
    reader: &DomainReader<'_, SyndicDomain>,
    active: &crate::ActiveCasBinding,
    active_turn: &crate::ActiveCasTurnRecord,
) -> Result<(), SyndicValidationError> {
    let state = require::<TurnStatesFamily>(
        reader,
        &active.turn_id(),
        "valid active successor turn-state is missing",
    )?;
    if !state.lifecycle().is_proven_terminal() || state.source_event_count() == 0 {
        return invariant("valid active successor lacks exact terminal CAS event authority");
    }
    let sequence = SourceEventSequence::new(state.source_event_count()).map_err(|_| {
        SyndicValidationError::Invariant(
            "valid active successor terminal event sequence is invalid",
        )
    })?;
    let event = require::<SourceEventsFamily>(
        reader,
        &TurnEventKey {
            owner: active.turn_id(),
            ordinal: sequence,
        },
        "valid active successor terminal event is missing",
    )?;
    let Some(source) = event.source() else {
        return invariant("valid active successor lacks exact terminal CAS event authority");
    };
    let terminal_matches = matches!(
        event.payload(),
        SourceEventPayload::TurnEnded(status) if state.end_status() == Some(*status)
    );
    if event.turn_id() != active.turn_id()
        || event.sequence() != sequence
        || !terminal_matches
        || source.thread_id() != active_turn.cas_thread_id()
        || source.turn_id() != active_turn.cas_turn_id()
    {
        return invariant("valid active successor lacks exact terminal CAS event authority");
    }
    validate_exact_active_source_history(reader, active, active_turn, &state)?;
    Ok(())
}
