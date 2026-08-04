use beryl_home_store::DomainReader;
use beryl_model::InputGateRevision;

use crate::{
    AcceptedRouteTarget, BindingLifecycle, BindingState, InputGateRecord, InputGateState,
    PendingSteeringTargetProof, SteeringTargetProof, StopOperationId, StopOperationRecord,
    StopOperationRevision, StopOperationTarget, SyndicMutationError, TurnLifecycle, codec::*,
    domain::SyndicDomain, mutation::required,
};

pub(super) struct LiveStopAuthority {
    pub(super) gate: InputGateRecord,
    pub(super) record: StopOperationRecord,
    pub(super) steering_target: Option<SteeringTargetProof>,
}

pub(super) fn validate_execution_target(
    reader: &DomainReader<'_, SyndicDomain>,
    target: &StopOperationTarget,
) -> Result<SteeringTargetProof, SyndicMutationError> {
    let thread = required::<ThreadsFamily>(reader, &target.thread_id())?;
    let turn = required::<TurnsFamily>(reader, &target.turn_id())?;
    let turn_state = required::<TurnStatesFamily>(reader, &target.turn_id())?;
    if turn.origin_thread_id() != target.thread_id()
        || turn.kind() != target.turn_kind()
        || !matches!(
            turn_state.lifecycle(),
            TurnLifecycle::Pending | TurnLifecycle::Active
        )
    {
        return Err(SyndicMutationError::LiveTurnConflict);
    }

    let binding_head = required::<BindingHeadsFamily>(reader, &target.thread_id())?;
    if binding_head.revision() != target.binding_revision()
        || binding_head.lifecycle() != BindingLifecycle::Active
        || binding_head.selected_path_digest() != thread.selected_path_digest()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    let binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: target.thread_id(),
            revision: target.binding_revision(),
        },
    )?;
    let BindingState::Active(active) = binding.state() else {
        return Err(SyndicMutationError::BindingStateConflict);
    };
    if binding.thread_id() != target.thread_id()
        || binding.selected_path().tail() != thread.committed_tail()
        || binding.selected_path().digest() != thread.selected_path_digest()
        || binding.selected_path().thread_revision() > thread.revision()
        || active.snapshot_id() != target.snapshot_id()
        || active.turn_id() != target.turn_id()
        || active.usable().cas_thread_id() != target.cas_thread_id()
        || active.usable().execution().runtime_id() != target.runtime_id()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }

    let snapshot = required::<ExecutionSnapshotsFamily>(reader, &target.snapshot_id())?;
    if snapshot.thread_id() != target.thread_id()
        || snapshot.binding_revision() != target.binding_revision()
        || snapshot.active_turn_id() != target.turn_id()
        || snapshot.cas_thread_id() != target.cas_thread_id()
        || snapshot.execution() != active.usable().execution()
        || snapshot.execution().runtime_id() != target.runtime_id()
        || snapshot.loaded_generation() != target.loaded_generation()
        || snapshot.selected_path() != binding.selected_path()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }

    let active_turn = required::<ActiveCasTurnsFamily>(reader, &target.snapshot_id())?;
    if active_turn.snapshot_id() != target.snapshot_id()
        || active_turn.thread_id() != target.thread_id()
        || active_turn.turn_id() != target.turn_id()
        || active_turn.binding_revision() != target.binding_revision()
        || active_turn.cas_thread_id() != target.cas_thread_id()
        || active_turn.cas_turn_id() != target.cas_turn_id()
    {
        return Err(SyndicMutationError::ActiveCasTurnCollision);
    }

    validate_reverse_authority(reader, target)?;
    Ok(steering_target(target))
}

fn validate_reverse_authority(
    reader: &DomainReader<'_, SyndicDomain>,
    target: &StopOperationTarget,
) -> Result<(), SyndicMutationError> {
    let reservation = required::<CasThreadIndexFamily>(
        reader,
        &CasThreadKey::Record(target.cas_thread_id().clone()),
    )?;
    if reservation.thread_id() != target.thread_id()
        || reservation.latest_binding_revision() != target.binding_revision()
        || reservation.retired_binding_revision().is_some()
    {
        return Err(SyndicMutationError::CasThreadRetired);
    }
    let membership = required::<CasThreadBindingIndexFamily>(
        reader,
        &CasThreadBindingKey::Record(target.cas_thread_id().clone(), target.binding_revision()),
    )?;
    if membership.cas_thread_id() != target.cas_thread_id()
        || membership.thread_id() != target.thread_id()
        || membership.binding_revision() != target.binding_revision()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    let cas_turn = required::<CasTurnIndexFamily>(
        reader,
        &CasTurnKey::Record(target.cas_thread_id().clone(), target.cas_turn_id().clone()),
    )?;
    if cas_turn.cas_thread_id() != target.cas_thread_id()
        || cas_turn.cas_turn_id() != target.cas_turn_id()
        || cas_turn.thread_id() != target.thread_id()
        || cas_turn.turn_id() != target.turn_id()
        || cas_turn.binding_revision() != target.binding_revision()
        || cas_turn.snapshot_id() != target.snapshot_id()
    {
        return Err(SyndicMutationError::CasTurnOwnershipConflict);
    }
    Ok(())
}

pub(super) fn steering_target(target: &StopOperationTarget) -> SteeringTargetProof {
    SteeringTargetProof::new(
        PendingSteeringTargetProof::new(
            target.binding_revision(),
            target.snapshot_id(),
            target.turn_id(),
            target.cas_thread_id().clone(),
        ),
        target.cas_turn_id().clone(),
    )
}

pub(super) fn load_live_stop_authority(
    reader: &DomainReader<'_, SyndicDomain>,
    operation_id: StopOperationId,
    target: &StopOperationTarget,
    expected_gate_revision: InputGateRevision,
    expected_stop_revision: StopOperationRevision,
) -> Result<LiveStopAuthority, SyndicMutationError> {
    if operation_id.thread_id() != target.thread_id() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    let provider = matches!(
        target.turn_kind(),
        crate::TurnKind::ProviderOperation(crate::ProviderOperationKind::ContextCompaction)
    );
    let steering_target = if provider {
        None
    } else {
        Some(validate_execution_target(reader, target)?)
    };
    let gate = required::<InputGatesFamily>(reader, &target.thread_id())?;
    if gate.revision() != expected_gate_revision {
        return Err(SyndicMutationError::InputGateRevisionConflict {
            expected: expected_gate_revision,
            current: gate.revision(),
        });
    }
    if gate.state() != &InputGateState::stopping(target.turn_id(), operation_id.nonce()) {
        return Err(SyndicMutationError::InputGateStateConflict);
    }

    let record = required::<StopOperationsFamily>(reader, &operation_id)?;
    if record.id() != operation_id
        || record.target() != target
        || record.revision() != expected_stop_revision
        || !record.state().is_live()
    {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    if provider {
        if !record.admission().is_provider_operation() {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        validate_provider_stop_target(reader, operation_id, target, record.admission())?;
        return Ok(LiveStopAuthority {
            gate,
            record,
            steering_target,
        });
    }
    let selected = gate
        .selected_route()
        .ok_or(SyndicMutationError::ActiveSteeringRouteConflict)?;
    if selected != record.admission().successor_stopped_route() {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    }
    let head = required::<AcceptedRouteGenerationHeadsFamily>(reader, &target.thread_id())?;
    let route = required::<AcceptedRouteGenerationsFamily>(
        reader,
        &ThreadRouteKey {
            thread: target.thread_id(),
            generation: selected.generation(),
        },
    )?;
    if head.proof() != selected
        || route.thread_id() != target.thread_id()
        || route.generation() != selected.generation()
        || route.revision() != selected.revision()
        || route.target() != &AcceptedRouteTarget::NextTurn(crate::NextTurnReason::Stop)
        || route.ready_retryable_count() != 0
        || route.delivering_count() != 0
        || route.delivering_logical_utf8_bytes() != 0
    {
        return Err(SyndicMutationError::ActiveSteeringRouteConflict);
    }

    Ok(LiveStopAuthority {
        gate,
        record,
        steering_target,
    })
}

fn validate_provider_stop_target(
    reader: &DomainReader<'_, SyndicDomain>,
    stop_id: StopOperationId,
    target: &StopOperationTarget,
    admission: crate::StopAdmissionWitness,
) -> Result<(), SyndicMutationError> {
    let operation_id = crate::CompactionOperationId::new(
        target.thread_id(),
        crate::CompactionOperationNonce::from_bytes(*target.turn_id().as_bytes()),
    );
    let operation = required::<CompactionOperationsFamily>(reader, &operation_id)?;
    let snapshot = required::<ExecutionSnapshotsFamily>(reader, &target.snapshot_id())?;
    let binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: target.thread_id(),
            revision: target.binding_revision(),
        },
    )?;
    let BindingState::Valid(usable) = binding.state() else {
        return Err(SyndicMutationError::BindingStateConflict);
    };
    let active = required::<ActiveCasTurnsFamily>(reader, &target.snapshot_id())?;
    let (Some(source_compaction_revision), Some(successor_compaction_revision)) = (
        admission.source_compaction_revision(),
        admission.successor_compaction_revision(),
    ) else {
        return Err(SyndicMutationError::InputGateStateConflict);
    };
    if !operation.stopping_descendant_is_exact(
        stop_id.nonce(),
        source_compaction_revision,
        successor_compaction_revision,
    ) || operation.target().snapshot_id() != target.snapshot_id()
        || operation.target().binding_revision() != target.binding_revision()
        || operation
            .cas_turn()
            .is_none_or(|turn| turn.cas_turn_id() != target.cas_turn_id())
        || snapshot.kind()
            != crate::ExecutionSnapshotKind::ProviderOperation(
                crate::ProviderOperationKind::ContextCompaction,
            )
        || snapshot.active_turn_id() != target.turn_id()
        || usable.cas_thread_id() != target.cas_thread_id()
        || active.cas_turn_id() != target.cas_turn_id()
    {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    Ok(())
}
