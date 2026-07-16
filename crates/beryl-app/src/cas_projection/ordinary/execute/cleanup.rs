use beryl_home_store::HomeStore;
use beryl_model::{BindingRevision, InputGateRevision};
use syndic_storage::{
    AbandonActiveBinding, LiveSourceEvent, SourceEventPayload, SourceEventSequence,
    StaleCasBinding, SyndicPointReadLimit, SyndicStorage, TurnEndStatus, TurnIncompleteReason,
};

use crate::cas_projection::ordinary::{
    OrdinaryTurnExecutionError,
    capture::{LiveCapture, system_timestamp_at_least},
    preflight::PendingOrdinaryExecution,
};
use crate::cas_projection::publication;

#[allow(clippy::too_many_arguments)]
pub(super) fn abandon_without_cas_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    pending: &PendingOrdinaryExecution,
    active_binding_revision: BindingRevision,
    active_gate_revision: InputGateRevision,
    stale: StaleCasBinding,
    incomplete_reason: TurnIncompleteReason,
    limit: SyndicPointReadLimit,
) -> Result<(), OrdinaryTurnExecutionError> {
    publication::abandon_active(
        store,
        storage,
        &AbandonActiveBinding::new(
            pending.thread_id,
            active_binding_revision,
            active_gate_revision,
            pending.selected_path,
            stale,
        ),
        limit,
    )?;
    let abandoned_gate_revision = next_gate_revision(active_gate_revision)?;
    let observed_at = system_timestamp_at_least(pending.minimum_observed_at)?;
    let terminal = LiveSourceEvent::new(
        pending.thread_id,
        pending.turn_id,
        pending.state_revision,
        abandoned_gate_revision,
        SourceEventSequence::new(1).expect("first source-event sequence is nonzero"),
        None,
        SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(incomplete_reason)),
        observed_at,
    )?;
    publication::admit_live_event(store, storage, &terminal, limit)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn abandon_and_close_incomplete(
    store: &HomeStore,
    storage: SyndicStorage,
    capture: &mut LiveCapture,
    pending: &PendingOrdinaryExecution,
    active_binding_revision: BindingRevision,
    stale: StaleCasBinding,
    incomplete_reason: TurnIncompleteReason,
    limit: SyndicPointReadLimit,
) -> Result<(), OrdinaryTurnExecutionError> {
    capture.flush_for_loss(store, storage, limit)?;
    publication::abandon_active(
        store,
        storage,
        &AbandonActiveBinding::new(
            pending.thread_id,
            active_binding_revision,
            capture.gate_revision(),
            pending.selected_path,
            stale,
        ),
        limit,
    )?;
    capture.close_incomplete_after_abandon(store, storage, limit, incomplete_reason)
}

fn next_gate_revision(
    revision: InputGateRevision,
) -> Result<InputGateRevision, OrdinaryTurnExecutionError> {
    revision
        .checked_next()
        .map_err(|_| OrdinaryTurnExecutionError::Invariant("input-gate revision exhausted"))
}
