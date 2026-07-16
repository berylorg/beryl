use beryl_home_store::DomainReader;

use crate::{
    ActiveCasBinding, ActiveCasTurnRecord, BindingState, CasTurnIndexRecord, SourceEventSequence,
    TurnStateRecord, codec::*, domain::SyndicDomain, error::SyndicValidationError,
};

use super::{invariant, proofs::validate_usable, require_reservation};
use crate::validation::scan::{point, require, scan, scan_range};

pub(super) fn validate_active(
    reader: &DomainReader<'_, SyndicDomain>,
    binding: &crate::BindingRecord,
    active: &ActiveCasBinding,
    is_current_head: bool,
) -> Result<(), SyndicValidationError> {
    if binding.selected_path().tail() != Some(active.turn_id()) {
        return invariant("active binding turn is not its selected-path tail");
    }
    validate_usable(reader, binding.selected_path(), active.usable())?;
    active
        .usable()
        .native_turn_count()
        .checked_next()
        .map_err(|_| {
            SyndicValidationError::Invariant("active CAS native turn count is exhausted")
        })?;
    validate_active_base_prefix(reader, binding, active.usable().represented_prefix())?;
    require_reservation(
        reader,
        active.usable().cas_thread_id(),
        binding.thread_id(),
        binding.revision(),
        false,
    )?;
    let snapshot = require::<ExecutionSnapshotsFamily>(
        reader,
        &active.snapshot_id(),
        "active binding snapshot is missing",
    )?;
    if snapshot.thread_id() != binding.thread_id()
        || snapshot.binding_revision() != binding.revision()
        || snapshot.activation_gate_revision() != active.activation_gate_revision()
        || snapshot.active_turn_id() != active.turn_id()
        || snapshot.cas_thread_id() != active.usable().cas_thread_id()
        || snapshot.selected_path() != binding.selected_path()
        || snapshot.represented_base_prefix() != active.usable().represented_prefix()
        || snapshot.represented_base_native_turn_count() != active.usable().native_turn_count()
        || snapshot.tool_profile() != active.usable().tool_profile()
        || snapshot.lineage() != active.usable().lineage()
        || snapshot.execution() != active.usable().execution()
        || snapshot.started_at() != active.started_at()
    {
        return invariant("active binding and execution snapshot disagree");
    }
    if let Some(required) = active.usable().lineage().recovered_loaded_generation()
        && snapshot.loaded_generation() != required
    {
        return invariant("recovered execution snapshot loaded generation disagrees");
    }
    if let Some(completed_at) = active.usable().lineage().recovered_completed_at()
        && snapshot.started_at() < completed_at
    {
        return invariant("recovered execution snapshot predates injection completion");
    }
    let turn = require::<TurnsFamily>(reader, &active.turn_id(), "active binding turn is missing")?;
    if turn.origin_thread_id() != binding.thread_id() {
        return invariant("active binding turn belongs to another thread");
    }
    if is_current_head {
        let state = require::<TurnStatesFamily>(
            reader,
            &active.turn_id(),
            "current active binding turn state is missing",
        )?;
        if !state.lifecycle().blocks_same_thread_start() {
            return invariant("current active binding turn lifecycle is not blocking");
        }
        match point::<ActiveCasTurnsFamily>(reader, &active.snapshot_id())? {
            Some(cas_turn) => {
                validate_active_cas_turn(reader, &cas_turn)?;
                validate_exact_active_source_history(reader, active, &cas_turn, &state)?;
            }
            None if state.lifecycle() == crate::TurnLifecycle::Pending
                && state.source_event_count() == 0 => {}
            None => return invariant("current active binding has uncorrelated source history"),
        }
    } else if let Some(cas_turn) = point::<ActiveCasTurnsFamily>(reader, &active.snapshot_id())? {
        validate_active_cas_turn(reader, &cas_turn)?;
    }
    Ok(())
}

pub(super) fn validate_exact_active_source_history(
    reader: &DomainReader<'_, SyndicDomain>,
    active: &ActiveCasBinding,
    cas_turn: &ActiveCasTurnRecord,
    state: &TurnStateRecord,
) -> Result<(), SyndicValidationError> {
    if state.source_event_count() == 0 {
        return Ok(());
    }
    let last = SourceEventSequence::new(state.source_event_count())
        .map_err(|_| SyndicValidationError::Invariant("active source-event frontier is invalid"))?;
    let mut expected = 1_u64;
    scan_range::<SourceEventsFamily>(
        reader,
        TurnEventKey {
            owner: active.turn_id(),
            ordinal: SourceEventSequence::FIRST,
        },
        TurnEventKey {
            owner: active.turn_id(),
            ordinal: last,
        },
        |key, event| {
            if key.owner != active.turn_id()
                || key.ordinal.get() != expected
                || event.turn_id() != active.turn_id()
                || event.sequence() != key.ordinal
                || event.source().is_none_or(|source| {
                    source.thread_id() != cas_turn.cas_thread_id()
                        || source.turn_id() != cas_turn.cas_turn_id()
                })
            {
                return invariant("active source event lacks exact CAS-turn authority");
            }
            expected = expected
                .checked_add(1)
                .ok_or(SyndicValidationError::Invariant(
                    "active source-event frontier exhausted",
                ))?;
            Ok(())
        },
    )?;
    if expected != state.source_event_count().saturating_add(1) {
        return invariant("active source-event history is incomplete");
    }
    Ok(())
}

pub(super) fn validate_abandoned_active_source_history(
    reader: &DomainReader<'_, SyndicDomain>,
    active: &ActiveCasBinding,
) -> Result<(), SyndicValidationError> {
    let state = require::<TurnStatesFamily>(
        reader,
        &active.turn_id(),
        "abandoned active turn state is missing",
    )?;
    if state.source_event_count() == 0 {
        return Ok(());
    }
    let cas_turn = point::<ActiveCasTurnsFamily>(reader, &active.snapshot_id())?;
    if let Some(cas_turn) = &cas_turn {
        validate_active_cas_turn(reader, cas_turn)?;
    }
    let last = SourceEventSequence::new(state.source_event_count()).map_err(|_| {
        SyndicValidationError::Invariant("abandoned source-event frontier is invalid")
    })?;
    let mut expected = 1_u64;
    scan_range::<SourceEventsFamily>(
        reader,
        TurnEventKey {
            owner: active.turn_id(),
            ordinal: SourceEventSequence::FIRST,
        },
        TurnEventKey {
            owner: active.turn_id(),
            ordinal: last,
        },
        |key, event| {
            let exact_cas_source = cas_turn.as_ref().is_some_and(|cas_turn| {
                event.source().is_some_and(|source| {
                    source.thread_id() == cas_turn.cas_thread_id()
                        && source.turn_id() == cas_turn.cas_turn_id()
                })
            });
            let local_terminal = key.ordinal == last
                && event.source().is_none()
                && matches!(
                    event.payload(),
                    crate::SourceEventPayload::TurnEnded(status)
                        if state.end_status() == Some(*status)
                            && status.outcome() != crate::TurnTerminalOutcome::Complete
                );
            if key.owner != active.turn_id()
                || key.ordinal.get() != expected
                || event.turn_id() != active.turn_id()
                || event.sequence() != key.ordinal
                || (!exact_cas_source && !local_terminal)
            {
                return invariant("abandoned active source history lacks exact authority");
            }
            expected = expected
                .checked_add(1)
                .ok_or(SyndicValidationError::Invariant(
                    "abandoned source-event frontier exhausted",
                ))?;
            Ok(())
        },
    )?;
    if expected != state.source_event_count().saturating_add(1) {
        return invariant("abandoned active source-event history is incomplete");
    }
    Ok(())
}

pub(super) fn validate_current_active_gate(
    reader: &DomainReader<'_, SyndicDomain>,
    binding: &crate::BindingRecord,
    active: &ActiveCasBinding,
) -> Result<(), SyndicValidationError> {
    let gate = require::<InputGatesFamily>(
        reader,
        &binding.thread_id(),
        "current active binding input gate is missing",
    )?;
    if gate.revision() < active.activation_gate_revision() {
        return invariant("current active gate predates binding activation");
    }
    let pending = match gate.state() {
        crate::InputGateState::AwaitingSteering(target) => target,
        crate::InputGateState::Steerable(target) | crate::InputGateState::Stopping(target) => {
            target.pending()
        }
        crate::InputGateState::Idle
        | crate::InputGateState::PendingTurn(_)
        | crate::InputGateState::Compacting(_) => {
            return invariant("current active binding has no active gate correlation");
        }
    };
    if pending.binding_revision() != binding.revision()
        || pending.snapshot_id() != active.snapshot_id()
        || pending.active_turn_id() != active.turn_id()
        || pending.cas_thread_id() != active.usable().cas_thread_id()
    {
        return invariant("current active binding and input gate correlation disagree");
    }
    Ok(())
}

fn validate_active_base_prefix(
    reader: &DomainReader<'_, SyndicDomain>,
    binding: &crate::BindingRecord,
    prefix: crate::CasRepresentedPrefixProof,
) -> Result<(), SyndicValidationError> {
    let turn = require::<TurnsFamily>(
        reader,
        &binding
            .selected_path()
            .tail()
            .ok_or(SyndicValidationError::Invariant(
                "active binding has an empty selected path",
            ))?,
        "active binding turn is missing",
    )?;
    let (expected_tail, expected_digest) = match turn.parent().turn() {
        Some(parent) => {
            let parent = require::<TurnsFamily>(reader, &parent, "active turn parent is missing")?;
            (Some(parent.id()), parent.chain_digest())
        }
        None => (None, crate::empty_selected_path_digest()),
    };
    if prefix.tail() != expected_tail
        || prefix.digest() != expected_digest
        || prefix.source_thread_revision() != binding.selected_path().thread_revision()
    {
        return invariant("active binding represented base is not exactly the turn parent");
    }
    Ok(())
}

pub(super) fn validate_snapshots(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ExecutionSnapshotsFamily>(reader, |key, snapshot| {
        if *key != snapshot.id() {
            return invariant("execution snapshot key and identity disagree");
        }
        let binding = require::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: snapshot.thread_id(),
                revision: snapshot.binding_revision(),
            },
            "snapshot binding record is missing",
        )?;
        let BindingState::Active(active) = binding.state() else {
            return invariant("snapshot binding record is not active");
        };
        if active.snapshot_id() != snapshot.id() {
            return invariant("snapshot reverse binding disagrees");
        }
        validate_active(reader, &binding, active, false)
    })
}

pub(super) fn validate_cas_turns(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ActiveCasTurnsFamily>(reader, |key, record| {
        if *key != record.snapshot_id() {
            return invariant("active CAS-turn key and snapshot disagree");
        }
        validate_active_cas_turn(reader, record)
    })?;
    scan::<CasTurnIndexFamily>(reader, |key, index| {
        let CasTurnKey::Record(cas_thread, cas_turn) = key else {
            return invariant("stored CAS-turn cursor sentinel");
        };
        if cas_thread != &index.cas_thread_id || cas_turn != &index.cas_turn_id {
            return invariant("CAS-turn index key disagrees");
        }
        let primary = require::<ActiveCasTurnsFamily>(
            reader,
            &index.snapshot_id(),
            "CAS-turn primary record is missing",
        )?;
        if primary.cas_thread_id() != cas_thread
            || primary.cas_turn_id() != cas_turn
            || primary.thread_id() != index.thread_id()
            || primary.turn_id() != index.turn_id()
            || primary.binding_revision() != index.binding_revision()
        {
            return invariant("CAS-turn index and primary record disagree");
        }
        Ok(())
    })
}

pub(super) fn validate_active_cas_turn(
    reader: &DomainReader<'_, SyndicDomain>,
    record: &ActiveCasTurnRecord,
) -> Result<(), SyndicValidationError> {
    let snapshot = require::<ExecutionSnapshotsFamily>(
        reader,
        &record.snapshot_id(),
        "active CAS-turn snapshot is missing",
    )?;
    if snapshot.thread_id() != record.thread_id()
        || snapshot.active_turn_id() != record.turn_id()
        || snapshot.binding_revision() != record.binding_revision()
        || snapshot.cas_thread_id() != record.cas_thread_id()
        || record.published_at() < snapshot.started_at()
    {
        return invariant("active CAS-turn and immutable snapshot disagree");
    }
    let binding = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: record.thread_id(),
            revision: record.binding_revision(),
        },
        "active CAS-turn binding is missing",
    )?;
    let BindingState::Active(active) = binding.state() else {
        return invariant("active CAS-turn binding is not active");
    };
    if active.snapshot_id() != record.snapshot_id()
        || active.turn_id() != record.turn_id()
        || active.usable().cas_thread_id() != record.cas_thread_id()
    {
        return invariant("active CAS-turn and active binding disagree");
    }
    let key = CasTurnKey::Record(record.cas_thread_id().clone(), record.cas_turn_id().clone());
    let post_turn_native_count = snapshot
        .represented_base_native_turn_count()
        .checked_next()
        .map_err(|_| {
            SyndicValidationError::Invariant("active CAS-turn native count is exhausted")
        })?;
    let expected = CasTurnIndexRecord::new(
        record.cas_thread_id().clone(),
        record.cas_turn_id().clone(),
        record.thread_id(),
        record.turn_id(),
        record.binding_revision(),
        record.snapshot_id(),
        post_turn_native_count,
    );
    if require::<CasTurnIndexFamily>(reader, &key, "active CAS-turn index is missing")? != expected
    {
        return invariant("active CAS-turn primary and index disagree");
    }
    Ok(())
}
