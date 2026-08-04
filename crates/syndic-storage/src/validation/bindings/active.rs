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
    if let Some(injection_generation) = active.usable().lineage().recovered_injection_generation()
        && snapshot.loaded_generation().process() != injection_generation.process()
    {
        return invariant("recovered execution snapshot process generation disagrees");
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
    let turn = match gate.state() {
        crate::InputGateState::AwaitingSteering(turn)
        | crate::InputGateState::Steerable(turn)
        | crate::InputGateState::AwaitingTerminal(turn) => *turn,
        crate::InputGateState::Stopping { turn_id, .. } => *turn_id,
        crate::InputGateState::Idle
        | crate::InputGateState::PendingTurn(_)
        | crate::InputGateState::Compacting { .. }
        | crate::InputGateState::FinalizingHistory(_) => {
            return invariant("current active binding has no active gate correlation");
        }
    };
    let proof = gate
        .selected_route()
        .ok_or(SyndicValidationError::Invariant(
            "current active binding gate has no selected route",
        ))?;
    let route = require::<AcceptedRouteGenerationsFamily>(
        reader,
        &ThreadRouteKey {
            thread: binding.thread_id(),
            generation: proof.generation(),
        },
        "current active binding route generation is missing",
    )?;
    if turn != active.turn_id() || route.revision() != proof.revision() {
        return invariant("current active binding and input gate correlation disagree");
    }
    match gate.state() {
        crate::InputGateState::Stopping {
            operation_nonce, ..
        } => {
            validate_stopping_gate_target(reader, binding, active, proof, *operation_nonce, &route)?
        }
        crate::InputGateState::AwaitingSteering(_) | crate::InputGateState::Steerable(_) => {
            let pending = match route.target() {
                crate::AcceptedRouteTarget::AwaitingSteering(target) => target,
                crate::AcceptedRouteTarget::Steering(target) => target.pending(),
                crate::AcceptedRouteTarget::AwaitingTerminal(_)
                | crate::AcceptedRouteTarget::NextTurn(_)
                | crate::AcceptedRouteTarget::ProjectionLost(_) => {
                    return invariant("current active binding route has no active target proof");
                }
            };
            if pending.binding_revision() != binding.revision()
                || pending.snapshot_id() != active.snapshot_id()
                || pending.active_turn_id() != active.turn_id()
                || pending.cas_thread_id() != active.usable().cas_thread_id()
            {
                return invariant("current active binding and input gate correlation disagree");
            }
        }
        crate::InputGateState::AwaitingTerminal(_) => {
            let crate::AcceptedRouteTarget::AwaitingTerminal(target) = route.target() else {
                return invariant("current awaiting-terminal gate has no retained active target");
            };
            let state = require::<TurnStatesFamily>(
                reader,
                &active.turn_id(),
                "current awaiting-terminal turn state is missing",
            )?;
            let cas_turn = require::<ActiveCasTurnsFamily>(
                reader,
                &active.snapshot_id(),
                "current awaiting-terminal CAS turn is missing",
            )?;
            if state.lifecycle() != crate::TurnLifecycle::UnknownTerminal
                || gate.live_steering_count() != 0
                || route.ready_retryable_count() != 0
                || route.delivering_count() != 0
                || route.delivering_logical_utf8_bytes() != 0
                || target.pending().binding_revision() != binding.revision()
                || target.pending().snapshot_id() != active.snapshot_id()
                || target.pending().active_turn_id() != active.turn_id()
                || target.pending().cas_thread_id() != active.usable().cas_thread_id()
                || cas_turn.snapshot_id() != active.snapshot_id()
                || cas_turn.thread_id() != binding.thread_id()
                || cas_turn.turn_id() != active.turn_id()
                || cas_turn.binding_revision() != binding.revision()
                || cas_turn.cas_thread_id() != active.usable().cas_thread_id()
                || cas_turn.cas_turn_id() != target.cas_turn_id()
            {
                return invariant("current awaiting-terminal authority disagrees");
            }
        }
        _ => unreachable!("active gate states were closed above"),
    }
    Ok(())
}

fn validate_stopping_gate_target(
    reader: &DomainReader<'_, SyndicDomain>,
    binding: &crate::BindingRecord,
    active: &ActiveCasBinding,
    route: crate::AcceptedRouteHeadProof,
    nonce: crate::StopOperationNonce,
    generation: &crate::AcceptedRouteGenerationRecord,
) -> Result<(), SyndicValidationError> {
    let operation_id = crate::StopOperationId::new(binding.thread_id(), nonce);
    let stop = require::<StopOperationsFamily>(
        reader,
        &operation_id,
        "current stopping gate has no stop-operation authority",
    )?;
    let target = stop.target();
    let snapshot = require::<ExecutionSnapshotsFamily>(
        reader,
        &active.snapshot_id(),
        "current stopping target snapshot is missing",
    )?;
    let cas_turn = require::<ActiveCasTurnsFamily>(
        reader,
        &active.snapshot_id(),
        "current stopping target CAS turn is missing",
    )?;
    if !stop.state().is_live()
        || stop.admission().successor_stopped_route() != route
        || generation.target() != &crate::AcceptedRouteTarget::NextTurn(crate::NextTurnReason::Stop)
        || target.thread_id() != binding.thread_id()
        || target.turn_id() != active.turn_id()
        || target.binding_revision() != binding.revision()
        || target.snapshot_id() != active.snapshot_id()
        || target.runtime_id() != active.usable().execution().runtime_id()
        || target.loaded_generation() != snapshot.loaded_generation()
        || target.cas_thread_id() != active.usable().cas_thread_id()
        || target.cas_turn_id() != cas_turn.cas_turn_id()
    {
        return invariant("current stopping gate has no exact active target proof");
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
        let canonical = require::<ThreadExecutionsFamily>(
            reader,
            &snapshot.thread_id(),
            "execution snapshot owner execution is missing",
        )?;
        if snapshot.execution() != canonical.execution() {
            return invariant("execution snapshot disagrees with canonical thread execution");
        }
        if *key != snapshot.id() {
            return invariant("execution snapshot key and identity disagree");
        }
        if matches!(
            snapshot.kind(),
            crate::ExecutionSnapshotKind::ProviderOperation(
                crate::ProviderOperationKind::ContextCompaction,
            )
        ) {
            let operation_id = crate::CompactionOperationId::new(
                snapshot.thread_id(),
                crate::CompactionOperationNonce::from_bytes(*snapshot.active_turn_id().as_bytes()),
            );
            let operation = require::<CompactionOperationsFamily>(
                reader,
                &operation_id,
                "provider snapshot compaction operation is missing",
            )?;
            return if operation.target().snapshot_id() == snapshot.id()
                && operation.target().turn_id() == snapshot.active_turn_id()
                && operation.target().binding_revision() == snapshot.binding_revision()
            {
                Ok(())
            } else {
                invariant("provider snapshot and compaction operation disagree")
            };
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
    if matches!(
        snapshot.kind(),
        crate::ExecutionSnapshotKind::ProviderOperation(
            crate::ProviderOperationKind::ContextCompaction,
        )
    ) {
        let binding = require::<BindingsFamily>(
            reader,
            &BindingKey {
                thread: record.thread_id(),
                revision: record.binding_revision(),
            },
            "provider CAS-turn binding is missing",
        )?;
        let BindingState::Valid(usable) = binding.state() else {
            return invariant("provider CAS-turn binding is not valid");
        };
        if usable.cas_thread_id() != record.cas_thread_id() {
            return invariant("provider CAS-turn and valid binding disagree");
        }
        let key = CasTurnKey::Record(record.cas_thread_id().clone(), record.cas_turn_id().clone());
        let expected = CasTurnIndexRecord::new(
            record.cas_thread_id().clone(),
            record.cas_turn_id().clone(),
            record.thread_id(),
            record.turn_id(),
            record.binding_revision(),
            record.snapshot_id(),
            snapshot.represented_base_native_turn_count(),
        );
        return if require::<CasTurnIndexFamily>(reader, &key, "provider CAS-turn index is missing")?
            == expected
        {
            Ok(())
        } else {
            invariant("provider CAS-turn primary and index disagree")
        };
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
