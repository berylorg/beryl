use beryl_home_store::{CommandOutcome, CurrentDomainCommand, HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, BindingRevision, InputGateRevision};
use syndic_storage::{
    AbandonActiveBinding, AbandonStopOperation, ActivateBinding, CancelBindingActivation,
    LiveSourceEvent, PublishActiveCasTurn, PublishStaleBinding, PublishValidBinding,
    SyndicPointReadLimit, SyndicStorage,
};

use super::ProjectionPublicationFailure;

pub(super) fn publish_valid(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &PublishValidBinding,
    _limit: SyndicPointReadLimit,
) -> Result<BindingRevision, ProjectionPublicationFailure> {
    dispatch(
        store,
        storage.current_publish_valid_binding(request.clone()),
    )?;
    next_binding_revision(request.expected_binding_revision())
}

pub(super) fn publish_stale(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &PublishStaleBinding,
    _limit: SyndicPointReadLimit,
) -> Result<BindingRevision, ProjectionPublicationFailure> {
    dispatch(
        store,
        storage.current_publish_stale_binding(request.clone()),
    )?;
    next_binding_revision(request.expected_binding_revision())
}

pub(super) fn activate(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &ActivateBinding,
    _limit: SyndicPointReadLimit,
) -> Result<(BindingRevision, InputGateRevision), ProjectionPublicationFailure> {
    dispatch(store, storage.current_activate_binding(request.clone()))?;
    Ok((
        next_binding_revision(request.expected_binding_revision())?,
        next_gate_revision(request.expected_gate_revision())?,
    ))
}

pub(super) fn cancel_activation(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &CancelBindingActivation,
    _limit: SyndicPointReadLimit,
) -> Result<(BindingRevision, InputGateRevision), ProjectionPublicationFailure> {
    dispatch(
        store,
        storage.current_cancel_binding_activation(request.clone()),
    )?;
    Ok((
        next_binding_revision(request.expected_binding_revision())?,
        next_gate_revision(request.expected_gate_revision())?,
    ))
}

pub(super) fn publish_active_turn(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &PublishActiveCasTurn,
    _limit: SyndicPointReadLimit,
) -> Result<InputGateRevision, ProjectionPublicationFailure> {
    dispatch(
        store,
        storage.current_publish_active_cas_turn(request.clone()),
    )?;
    next_gate_revision(request.expected_gate_revision())
}

pub(super) fn publish_active_turn_reconciled(
    store: &HomeStore,
    _expected_home_id: BerylHomeId,
    _expected_home_generation: HomeGeneration,
    storage: &SyndicStorage,
    request: &PublishActiveCasTurn,
    limit: SyndicPointReadLimit,
) -> Result<InputGateRevision, ProjectionPublicationFailure> {
    publish_active_turn(store, storage, request, limit)
}

pub(super) fn abandon_active(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &AbandonActiveBinding,
    _limit: SyndicPointReadLimit,
) -> Result<BindingRevision, ProjectionPublicationFailure> {
    dispatch(
        store,
        storage.current_abandon_active_binding(request.clone()),
    )?;
    next_binding_revision(request.expected_binding_revision())
}

pub(super) fn abandon_active_reconciled(
    store: &HomeStore,
    _expected_home_id: BerylHomeId,
    _expected_home_generation: HomeGeneration,
    storage: &SyndicStorage,
    request: &AbandonActiveBinding,
    limit: SyndicPointReadLimit,
) -> Result<BindingRevision, ProjectionPublicationFailure> {
    abandon_active(store, storage, request, limit)
}

pub(super) fn abandon_stop(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &AbandonStopOperation,
    _limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    dispatch(
        store,
        storage.current_abandon_stop_operation(request.clone()),
    )
}

pub(super) fn abandon_stop_reconciled(
    store: &HomeStore,
    _expected_home_id: BerylHomeId,
    _expected_home_generation: HomeGeneration,
    storage: &SyndicStorage,
    request: &AbandonStopOperation,
    limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    abandon_stop(store, storage, request, limit)
}

pub(super) fn admit_live_event(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &LiveSourceEvent,
    _limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    dispatch(
        store,
        storage.current_admit_live_source_event(request.clone()),
    )
}

pub(super) fn admit_live_event_reconciled(
    store: &HomeStore,
    _expected_home_id: BerylHomeId,
    _expected_home_generation: HomeGeneration,
    storage: &SyndicStorage,
    request: &LiveSourceEvent,
    limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    admit_live_event(store, storage, request, limit)
}

fn next_binding_revision(
    revision: BindingRevision,
) -> Result<BindingRevision, ProjectionPublicationFailure> {
    revision
        .checked_next()
        .map_err(|_| ProjectionPublicationFailure::BindingRevisionExhausted)
}

fn next_gate_revision(
    revision: InputGateRevision,
) -> Result<InputGateRevision, ProjectionPublicationFailure> {
    revision
        .checked_next()
        .map_err(|_| ProjectionPublicationFailure::InputGateRevisionExhausted)
}

fn dispatch(
    store: &HomeStore,
    command: CurrentDomainCommand,
) -> Result<(), ProjectionPublicationFailure> {
    match store.execute_current(command) {
        CommandOutcome::NotCommitted { evidence } => {
            Err(ProjectionPublicationFailure::Command(evidence))
        }
        CommandOutcome::Committed {
            receipt: _,
            later_failure: None,
        } => Ok(()),
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(later_failure),
        } => Err(ProjectionPublicationFailure::CommandCommitted {
            receipt,
            later_failure,
        }),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            Err(ProjectionPublicationFailure::CommandIndeterminate { failure })
        }
    }
}
