use beryl_home_store::{CommandError, CurrentDomainCommand, HomeStore};
use beryl_model::{BindingRevision, InputGateRevision};
use syndic_storage::{
    AbandonActiveBinding, ActivateBinding, ActiveCasTurnPublicationStatus,
    BindingPublicationStatus, CancelBindingActivation, LiveSourceEvent, LiveSourceEventStatus,
    PublishActiveCasTurn, PublishStaleBinding, PublishValidBinding, SyndicPointReadLimit,
    SyndicStorage,
};

use super::ProjectionPublicationFailure;

enum DispatchFailure {
    Command(CommandError),
}

pub(super) fn publish_valid(
    store: &HomeStore,
    storage: SyndicStorage,
    request: &PublishValidBinding,
    limit: SyndicPointReadLimit,
) -> Result<BindingRevision, ProjectionPublicationFailure> {
    let dispatch = dispatch(
        store,
        storage.current_publish_valid_binding(request.clone()),
    );
    let status = storage
        .valid_binding_publication_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?;
    match status {
        BindingPublicationStatus::Exact => request
            .expected_binding_revision()
            .checked_next()
            .map_err(|_| ProjectionPublicationFailure::BindingRevisionExhausted),
        BindingPublicationStatus::Prior => Err(dispatch_failure_or_prior(dispatch)),
        BindingPublicationStatus::Collision => Err(ProjectionPublicationFailure::Collision),
    }
}

pub(super) fn publish_stale(
    store: &HomeStore,
    storage: SyndicStorage,
    request: &PublishStaleBinding,
    limit: SyndicPointReadLimit,
) -> Result<BindingRevision, ProjectionPublicationFailure> {
    let dispatch = dispatch(
        store,
        storage.current_publish_stale_binding(request.clone()),
    );
    let status = storage
        .stale_binding_publication_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?;
    match status {
        BindingPublicationStatus::Exact => request
            .expected_binding_revision()
            .checked_next()
            .map_err(|_| ProjectionPublicationFailure::BindingRevisionExhausted),
        BindingPublicationStatus::Prior => Err(dispatch_failure_or_prior(dispatch)),
        BindingPublicationStatus::Collision => Err(ProjectionPublicationFailure::Collision),
    }
}

pub(super) fn activate(
    store: &HomeStore,
    storage: SyndicStorage,
    request: &ActivateBinding,
    limit: SyndicPointReadLimit,
) -> Result<(BindingRevision, InputGateRevision), ProjectionPublicationFailure> {
    let dispatch = dispatch(store, storage.current_activate_binding(request.clone()));
    let status = storage
        .binding_activation_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?;
    match status {
        BindingPublicationStatus::Exact => Ok((
            next_binding_revision(request.expected_binding_revision())?,
            next_gate_revision(request.expected_gate_revision())?,
        )),
        BindingPublicationStatus::Prior => Err(dispatch_failure_or_prior(dispatch)),
        BindingPublicationStatus::Collision => Err(ProjectionPublicationFailure::Collision),
    }
}

pub(super) fn cancel_activation(
    store: &HomeStore,
    storage: SyndicStorage,
    request: &CancelBindingActivation,
    limit: SyndicPointReadLimit,
) -> Result<(BindingRevision, InputGateRevision), ProjectionPublicationFailure> {
    let dispatch = dispatch(
        store,
        storage.current_cancel_binding_activation(request.clone()),
    );
    let status = storage
        .cancelled_binding_activation_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?;
    match status {
        BindingPublicationStatus::Exact => Ok((
            next_binding_revision(request.expected_binding_revision())?,
            next_gate_revision(request.expected_gate_revision())?,
        )),
        BindingPublicationStatus::Prior => Err(dispatch_failure_or_prior(dispatch)),
        BindingPublicationStatus::Collision => Err(ProjectionPublicationFailure::Collision),
    }
}

pub(super) fn publish_active_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    request: &PublishActiveCasTurn,
    limit: SyndicPointReadLimit,
) -> Result<InputGateRevision, ProjectionPublicationFailure> {
    let dispatch = dispatch(
        store,
        storage.current_publish_active_cas_turn(request.clone()),
    );
    match storage
        .active_cas_turn_publication_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?
    {
        ActiveCasTurnPublicationStatus::Exact => {
            next_gate_revision(request.expected_gate_revision())
        }
        ActiveCasTurnPublicationStatus::Absent => Err(dispatch_failure_or_prior(dispatch)),
        ActiveCasTurnPublicationStatus::Collision => Err(ProjectionPublicationFailure::Collision),
    }
}

pub(super) fn abandon_active(
    store: &HomeStore,
    storage: SyndicStorage,
    request: &AbandonActiveBinding,
    limit: SyndicPointReadLimit,
) -> Result<BindingRevision, ProjectionPublicationFailure> {
    let dispatch = dispatch(
        store,
        storage.current_abandon_active_binding(request.clone()),
    );
    match storage
        .abandoned_active_binding_publication_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?
    {
        BindingPublicationStatus::Exact => {
            next_binding_revision(request.expected_binding_revision())
        }
        BindingPublicationStatus::Prior => Err(dispatch_failure_or_prior(dispatch)),
        BindingPublicationStatus::Collision => Err(ProjectionPublicationFailure::Collision),
    }
}

pub(super) fn admit_live_event(
    store: &HomeStore,
    storage: SyndicStorage,
    request: &LiveSourceEvent,
    limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    let dispatch = dispatch(
        store,
        storage.current_admit_live_source_event(request.clone()),
    );
    match storage
        .live_source_event_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?
    {
        LiveSourceEventStatus::Exact => Ok(()),
        LiveSourceEventStatus::Absent => Err(dispatch_failure_or_prior(dispatch)),
        LiveSourceEventStatus::Collision => Err(ProjectionPublicationFailure::Collision),
    }
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

fn dispatch(store: &HomeStore, command: CurrentDomainCommand) -> Result<(), DispatchFailure> {
    store
        .execute_current(command)
        .map(|_| ())
        .map_err(DispatchFailure::Command)
}

fn dispatch_failure_or_prior(
    dispatch: Result<(), DispatchFailure>,
) -> ProjectionPublicationFailure {
    match dispatch {
        Ok(()) => ProjectionPublicationFailure::Prior,
        Err(DispatchFailure::Command(source)) => ProjectionPublicationFailure::Command(source),
    }
}
