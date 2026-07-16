use beryl_home_store::DomainReader;
use beryl_model::SyndicTurnId;

use crate::mutation::binding::{advance_reservation, membership};
use crate::mutation::{point, required};
use crate::{
    BindingHeadRecord, BindingLifecycle, BindingRecord, BindingState, CasRepresentedPrefixProof,
    CasThreadBindingIndexRecord, CasThreadIndexRecord, CasTurnSource, InputGateRecord,
    InputGateState, SyndicMutationError, TurnLifecycle, UsableCasBinding, codec::*,
    domain::SyndicDomain,
};

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

pub(super) fn terminal_gate(
    current: &InputGateRecord,
    turn: SyndicTurnId,
    lifecycle: TurnLifecycle,
) -> Result<InputGateRecord, SyndicMutationError> {
    if current.live_steering_count() != 0 || !gate_targets_turn(current.state(), turn) {
        return Err(SyndicMutationError::InputGateStateConflict);
    }
    let state = if lifecycle.is_proven_terminal() {
        InputGateState::Idle
    } else {
        match current.state() {
            InputGateState::AwaitingSteering(target) => {
                InputGateState::AwaitingSteering(target.clone())
            }
            InputGateState::Steerable(target) | InputGateState::Stopping(target) => {
                InputGateState::Stopping(target.clone())
            }
            InputGateState::PendingTurn(_) | InputGateState::Compacting(_) => {
                InputGateState::PendingTurn(turn)
            }
            InputGateState::Idle => return Err(SyndicMutationError::InputGateStateConflict),
        }
    };
    Ok(InputGateRecord::new(
        current.thread_id(),
        current.revision().checked_next()?,
        state,
        current.accepted_high_water(),
        current.live_steering_count(),
        current.live_next_turn_count(),
        current.live_logical_utf8_bytes(),
    )?)
}

fn gate_targets_turn(state: &InputGateState, turn: SyndicTurnId) -> bool {
    match state {
        InputGateState::PendingTurn(current) | InputGateState::Compacting(current) => {
            *current == turn
        }
        InputGateState::AwaitingSteering(target) => target.active_turn_id() == turn,
        InputGateState::Steerable(target) | InputGateState::Stopping(target) => {
            target.pending().active_turn_id() == turn
        }
        InputGateState::Idle => false,
    }
}
