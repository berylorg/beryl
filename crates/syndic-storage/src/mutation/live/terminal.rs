use beryl_home_store::DomainReader;
use beryl_model::SyndicTurnId;

use crate::mutation::binding::{advance_reservation, membership};
use crate::mutation::{point, required};
use crate::{
    AcceptedRouteTarget, BindingHeadRecord, BindingLifecycle, BindingRecord, BindingState,
    CasRepresentedPrefixProof, CasThreadBindingIndexRecord, CasThreadIndexRecord, CasTurnSource,
    InputGateRecord, InputGateState, StopDispositionSource, StopMatchingTerminalWitness,
    StopOperationId, StopOperationRecord, StopOperationState, SyndicMutationError, TurnRecord,
    TurnStateRevision, UsableCasBinding, codec::*, domain::SyndicDomain,
};

mod gate;

pub(super) use gate::{LiveGateEffect, activation_gate_effect, terminal_gate_effect};

pub(super) fn terminal_stop_operation(
    reader: &DomainReader<'_, SyndicDomain>,
    turn: &TurnRecord,
    source: Option<&CasTurnSource>,
    current_gate: &InputGateRecord,
    successor_gate_revision: beryl_model::InputGateRevision,
    successor_turn_state_revision: TurnStateRevision,
) -> Result<Option<StopOperationRecord>, SyndicMutationError> {
    let InputGateState::Stopping {
        turn_id,
        operation_nonce,
    } = current_gate.state()
    else {
        return Ok(None);
    };
    if *turn_id != turn.id() {
        return Err(SyndicMutationError::InputGateStateConflict);
    }

    let id = StopOperationId::new(current_gate.thread_id(), *operation_nonce);
    let current = required::<StopOperationsFamily>(reader, &id)?;
    let target = current.target();
    let source = source.ok_or(SyndicMutationError::SourceIdentityConflict)?;
    let route_proof = current_gate
        .selected_route()
        .ok_or(SyndicMutationError::ActiveSteeringRouteConflict)?;
    let route = required::<AcceptedRouteGenerationsFamily>(
        reader,
        &ThreadRouteKey {
            thread: current_gate.thread_id(),
            generation: route_proof.generation(),
        },
    )?;
    let head = required::<BindingHeadsFamily>(reader, &current_gate.thread_id())?;
    let binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: current_gate.thread_id(),
            revision: head.revision(),
        },
    )?;
    let BindingState::Active(active) = binding.state() else {
        return Err(SyndicMutationError::BindingStateConflict);
    };
    let snapshot = required::<ExecutionSnapshotsFamily>(reader, &active.snapshot_id())?;
    let active_turn = required::<ActiveCasTurnsFamily>(reader, &active.snapshot_id())?;

    if current.id() != id
        || !current.state().is_live()
        || target.thread_id() != current_gate.thread_id()
        || target.turn_id() != turn.id()
        || target.turn_kind() != turn.kind()
        || target.binding_revision() != binding.revision()
        || target.snapshot_id() != active.snapshot_id()
        || target.runtime_id() != snapshot.execution().runtime_id()
        || target.loaded_generation() != snapshot.loaded_generation()
        || target.cas_thread_id() != source.thread_id()
        || target.cas_turn_id() != source.turn_id()
        || snapshot.thread_id() != current_gate.thread_id()
        || snapshot.binding_revision() != binding.revision()
        || snapshot.active_turn_id() != turn.id()
        || snapshot.cas_thread_id() != source.thread_id()
        || active_turn.snapshot_id() != snapshot.id()
        || active_turn.thread_id() != current_gate.thread_id()
        || active_turn.turn_id() != turn.id()
        || active_turn.binding_revision() != binding.revision()
        || active_turn.cas_thread_id() != source.thread_id()
        || active_turn.cas_turn_id() != source.turn_id()
        || current.admission().successor_stopped_route() != route_proof
        || route.thread_id() != current_gate.thread_id()
        || route.generation() != route_proof.generation()
        || route.revision() != route_proof.revision()
        || !matches!(
            route.target(),
            AcceptedRouteTarget::NextTurn(crate::NextTurnReason::Stop)
        )
        || route.ready_retryable_count() != 0
        || route.delivering_count() != 0
    {
        return Err(SyndicMutationError::InputGateStateConflict);
    }

    let witness = StopMatchingTerminalWitness::new(
        StopDispositionSource::new(current_gate.revision(), current.revision()),
        successor_gate_revision,
        successor_turn_state_revision,
    );
    StopOperationRecord::new(
        current.id(),
        current.target().clone(),
        current.admission(),
        current.revision().checked_next()?,
        current.cause_first_revisions(),
        current.dispatch_claim(),
        StopOperationState::MatchingTerminal(witness),
    )
    .map(Some)
    .map_err(|_| SyndicMutationError::InputGateStateConflict)
}

pub(super) fn terminal_valid_binding(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    turn_id: SyndicTurnId,
    source: Option<&CasTurnSource>,
) -> Result<
    Option<(
        BindingRecord,
        BindingHeadRecord,
        CasThreadIndexRecord,
        CasThreadBindingIndexRecord,
    )>,
    SyndicMutationError,
> {
    let head = required::<BindingHeadsFamily>(reader, &thread.id())?;
    let current = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: thread.id(),
            revision: head.revision(),
        },
    )?;
    let BindingState::Active(active) = current.state() else {
        return Ok(None);
    };
    if active.turn_id() != turn_id || current.selected_path().tail() != Some(turn_id) {
        return Err(SyndicMutationError::BindingStateConflict);
    }
    let source = source.ok_or(SyndicMutationError::SourceIdentityConflict)?;
    let active_turn = required::<ActiveCasTurnsFamily>(reader, &active.snapshot_id())?;
    if active_turn.thread_id() != thread.id()
        || active_turn.turn_id() != turn_id
        || active_turn.binding_revision() != current.revision()
        || active_turn.cas_thread_id() != source.thread_id()
        || active_turn.cas_turn_id() != source.turn_id()
    {
        return Err(SyndicMutationError::SourceIdentityConflict);
    }
    let native_turn_count = active.usable().native_turn_count().checked_next()?;
    let turn_index = required::<CasTurnIndexFamily>(
        reader,
        &CasTurnKey::Record(source.thread_id().clone(), source.turn_id().clone()),
    )?;
    if turn_index.thread_id() != thread.id()
        || turn_index.turn_id() != turn_id
        || turn_index.binding_revision() != current.revision()
        || turn_index.snapshot_id() != active.snapshot_id()
        || turn_index.post_turn_native_count() != native_turn_count
    {
        return Err(SyndicMutationError::SourceIdentityConflict);
    }
    let revision = current.revision().checked_next()?;
    if point::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: thread.id(),
            revision,
        },
    )?
    .is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    let represented = CasRepresentedPrefixProof::new(
        Some(turn_id),
        current.selected_path().thread_revision(),
        current.selected_path().digest(),
    );
    let usable = UsableCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        represented,
        native_turn_count,
        active.usable().tool_profile(),
        active.usable().lineage(),
    );
    let binding = BindingRecord::new(
        thread.id(),
        revision,
        current.selected_path(),
        BindingState::valid(usable),
    );
    let head = BindingHeadRecord::new(
        thread.id(),
        revision,
        BindingLifecycle::Valid,
        current.selected_path().digest(),
    );
    let reservation = advance_reservation(
        reader,
        active.usable().cas_thread_id(),
        thread.id(),
        current.revision(),
        revision,
    )?;
    let membership = membership(
        reader,
        active.usable().cas_thread_id(),
        thread.id(),
        revision,
    )?;
    Ok(Some((binding, head, reservation, membership)))
}
