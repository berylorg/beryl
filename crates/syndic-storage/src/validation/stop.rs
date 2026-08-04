use crate::{
    AcceptedRouteAbandonmentKind, AcceptedRouteGenerationRecord, AcceptedRouteHeadProof,
    AcceptedRouteLostTarget, AcceptedRouteTarget, BindingState, InputGateRecord, InputGateState,
    NextTurnReason, SourceEventPayload, SourceEventSequence, SteeringTargetProof, StopCause,
    StopOperationId, StopOperationRecord, StopOperationState, StopOperationTarget,
    TurnIncompleteReason, TurnLifecycle, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::scan::{require, scan};

mod consumed;

use consumed::{
    validate_abandoned, validate_abandoned_route, validate_matching_terminal, validate_safe_reopen,
};

type Reader<'a> = beryl_home_store::DomainReader<'a, SyndicDomain>;

pub(super) fn validate(reader: &Reader<'_>) -> Result<(), SyndicValidationError> {
    scan::<StopOperationsFamily>(reader, |key, record| {
        if *key != record.id() {
            return invariant("stop-operation key and record identity disagree");
        }
        validate_target(reader, record)?;
        validate_admission_route(reader, record)?;
        match record.state() {
            StopOperationState::Admitted | StopOperationState::DispatchClaimed => {
                validate_live(reader, record)
            }
            StopOperationState::SafeReopened(witness) => {
                validate_safe_reopen(reader, record, witness)
            }
            StopOperationState::MatchingTerminal(witness) => {
                validate_matching_terminal(reader, record, witness)
            }
            StopOperationState::Abandoned(witness) => validate_abandoned(reader, record, witness),
        }
    })?;

    scan::<InputGatesFamily>(reader, |key, gate| {
        let InputGateState::Stopping {
            turn_id,
            operation_nonce,
        } = gate.state()
        else {
            return Ok(());
        };
        let id = StopOperationId::new(*key, *operation_nonce);
        let record = require::<StopOperationsFamily>(
            reader,
            &id,
            "stopping gate has no matching stop-operation record",
        )?;
        if !record.state().is_live() || record.target().turn_id() != *turn_id {
            return invariant("stopping gate and live stop-operation record disagree");
        }
        Ok(())
    })
}

fn validate_target(
    reader: &Reader<'_>,
    record: &StopOperationRecord,
) -> Result<(), SyndicValidationError> {
    let target = record.target();
    let turn = require::<TurnsFamily>(
        reader,
        &target.turn_id(),
        "stop operation target turn is missing",
    )?;
    let binding = require::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: target.thread_id(),
            revision: target.binding_revision(),
        },
        "stop operation target binding is missing",
    )?;
    let snapshot = require::<ExecutionSnapshotsFamily>(
        reader,
        &target.snapshot_id(),
        "stop operation target snapshot is missing",
    )?;
    let cas_turn = require::<ActiveCasTurnsFamily>(
        reader,
        &target.snapshot_id(),
        "stop operation target active CAS turn is missing",
    )?;

    if is_provider_operation(record) {
        let BindingState::Valid(usable) = binding.state() else {
            return invariant("provider-operation stop target binding is not valid");
        };
        if turn.id() != target.turn_id()
            || turn.origin_thread_id() != target.thread_id()
            || turn.kind() != target.turn_kind()
            || turn.parent() != crate::ConversationParent::Root
            || usable.execution().runtime_id() != target.runtime_id()
            || usable.cas_thread_id() != target.cas_thread_id()
            || snapshot.kind()
                != crate::ExecutionSnapshotKind::ProviderOperation(
                    crate::ProviderOperationKind::ContextCompaction,
                )
            || snapshot.thread_id() != target.thread_id()
            || snapshot.binding_revision() != target.binding_revision()
            || snapshot.active_turn_id() != target.turn_id()
            || snapshot.cas_thread_id() != target.cas_thread_id()
            || snapshot.execution().runtime_id() != target.runtime_id()
            || snapshot.loaded_generation() != target.loaded_generation()
            || cas_turn.thread_id() != target.thread_id()
            || cas_turn.turn_id() != target.turn_id()
            || cas_turn.binding_revision() != target.binding_revision()
            || cas_turn.cas_thread_id() != target.cas_thread_id()
            || cas_turn.cas_turn_id() != target.cas_turn_id()
        {
            return invariant("provider-operation stop immutable target authority disagrees");
        }
        return Ok(());
    }
    let BindingState::Active(active) = binding.state() else {
        return invariant("stop operation target binding is not active");
    };
    if turn.id() != target.turn_id()
        || turn.origin_thread_id() != target.thread_id()
        || turn.kind() != target.turn_kind()
        || active.turn_id() != target.turn_id()
        || active.snapshot_id() != target.snapshot_id()
        || active.usable().execution().runtime_id() != target.runtime_id()
        || active.usable().cas_thread_id() != target.cas_thread_id()
        || snapshot.thread_id() != target.thread_id()
        || snapshot.binding_revision() != target.binding_revision()
        || snapshot.active_turn_id() != target.turn_id()
        || snapshot.cas_thread_id() != target.cas_thread_id()
        || snapshot.execution().runtime_id() != target.runtime_id()
        || snapshot.loaded_generation() != target.loaded_generation()
        || cas_turn.thread_id() != target.thread_id()
        || cas_turn.turn_id() != target.turn_id()
        || cas_turn.binding_revision() != target.binding_revision()
        || cas_turn.cas_thread_id() != target.cas_thread_id()
        || cas_turn.cas_turn_id() != target.cas_turn_id()
    {
        return invariant("stop operation immutable target authority disagrees");
    }

    if record.state().is_live() {
        let head = require::<BindingHeadsFamily>(
            reader,
            &target.thread_id(),
            "live stop operation binding head is missing",
        )?;
        let state = require::<TurnStatesFamily>(
            reader,
            &target.turn_id(),
            "live stop operation turn state is missing",
        )?;
        if head.revision() != target.binding_revision()
            || head.lifecycle() != crate::BindingLifecycle::Active
            || !matches!(
                state.lifecycle(),
                TurnLifecycle::Pending | TurnLifecycle::Active
            )
        {
            return invariant("live stop operation no longer owns exact active authority");
        }
    }

    Ok(())
}

fn validate_admission_route(
    reader: &Reader<'_>,
    record: &StopOperationRecord,
) -> Result<(), SyndicValidationError> {
    if is_provider_operation(record) {
        return if record.admission().is_provider_operation()
            && record.admission().source_compaction_revision().is_some()
            && record.admission().successor_compaction_revision().is_some()
        {
            Ok(())
        } else {
            invariant("provider-operation stop has ordinary admission authority")
        };
    }
    let target = record.target();
    let stopped = record.admission().successor_stopped_route();
    let route = require::<AcceptedRouteGenerationsFamily>(
        reader,
        &ThreadRouteKey {
            thread: target.thread_id(),
            generation: stopped.generation(),
        },
        "stop admission stopped route is missing",
    )?;
    if route.thread_id() != target.thread_id()
        || route.generation() != stopped.generation()
        || route.revision() < stopped.revision()
        || route.ready_retryable_count() != 0
        || route.delivering_count() != 0
    {
        return invariant("stop admission stopped-route descendant disagrees");
    }

    match record.state() {
        StopOperationState::Abandoned(witness) => validate_abandoned_route(record, witness, &route),
        StopOperationState::Admitted
        | StopOperationState::DispatchClaimed
        | StopOperationState::SafeReopened(_)
        | StopOperationState::MatchingTerminal(_) => {
            if !matches!(
                route.target(),
                AcceptedRouteTarget::NextTurn(NextTurnReason::Stop)
            ) {
                return invariant("stop admission route lost its stop classification");
            }
            if record.state().is_live() && route.revision() != stopped.revision() {
                return invariant("live stop operation has a changed stopped route");
            }
            Ok(())
        }
    }
}

fn validate_live(
    reader: &Reader<'_>,
    record: &StopOperationRecord,
) -> Result<(), SyndicValidationError> {
    if record.state() == StopOperationState::DispatchClaimed
        && record.revision() == crate::StopOperationRevision::FIRST
    {
        return invariant("dispatch-claimed stop operation is still at its admitted revision");
    }
    let target = record.target();
    let gate = require::<InputGatesFamily>(
        reader,
        &target.thread_id(),
        "live stop operation input gate is missing",
    )?;
    if is_provider_operation(record) {
        let operation_id = crate::CompactionOperationId::new(
            target.thread_id(),
            crate::CompactionOperationNonce::from_bytes(*target.turn_id().as_bytes()),
        );
        let operation = require::<CompactionOperationsFamily>(
            reader,
            &operation_id,
            "live provider stop compaction operation is missing",
        )?;
        let (Some(source_compaction_revision), Some(successor_compaction_revision)) = (
            record.admission().source_compaction_revision(),
            record.admission().successor_compaction_revision(),
        ) else {
            return invariant("live provider stop admission omits compaction revisions");
        };
        if gate.revision() < record.admission().successor_gate_revision()
            || gate.live_steering_count() != 0
            || gate.state() != &InputGateState::stopping(target.turn_id(), record.id().nonce())
            || !operation.stopping_descendant_is_exact(
                record.id().nonce(),
                source_compaction_revision,
                successor_compaction_revision,
            )
        {
            return invariant("live provider stop and compaction authority disagree");
        }
        return Ok(());
    }
    let stopped = record.admission().successor_stopped_route();
    let head = require::<AcceptedRouteGenerationHeadsFamily>(
        reader,
        &target.thread_id(),
        "live stop operation route head is missing",
    )?;
    if gate.revision() < record.admission().successor_gate_revision()
        || gate.selected_route() != Some(stopped)
        || gate.live_steering_count() != 0
        || head.proof() != stopped
        || !matches!(
            gate.state(),
            InputGateState::Stopping {
                turn_id,
                operation_nonce,
            } if *turn_id == target.turn_id()
                && *operation_nonce == record.id().nonce()
        )
    {
        return invariant("live stop operation and stopping gate disagree");
    }
    Ok(())
}

fn successor_binding_revision(
    target: &StopOperationTarget,
) -> Result<beryl_model::BindingRevision, SyndicValidationError> {
    match target.binding_revision().checked_next() {
        Ok(revision) => Ok(revision),
        Err(_) => invariant("stop binding revision is exhausted"),
    }
}

fn is_provider_operation(record: &StopOperationRecord) -> bool {
    record.admission().is_provider_operation()
        && record.target().turn_kind()
            == crate::TurnKind::ProviderOperation(crate::ProviderOperationKind::ContextCompaction)
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}
