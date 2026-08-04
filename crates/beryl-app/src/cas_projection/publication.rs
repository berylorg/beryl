use beryl_home_store::{
    CommandError, CurrentDomainCommand, HomeGeneration, HomeHealthState, HomeStore,
};
use beryl_model::{BerylHomeId, BindingRevision, InputGateRevision};
use syndic_storage::{
    AbandonActiveBinding, AbandonStopOperation, ActivateBinding, ActiveCasTurnPublicationStatus,
    BindingPublicationStatus, CancelBindingActivation, LiveSourceEvent, LiveSourceEventStatus,
    PublishActiveCasTurn, PublishStaleBinding, PublishValidBinding, StopOperationTransitionStatus,
    SyndicPointReadLimit, SyndicStorage,
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

pub(super) fn publish_active_turn_reconciled(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: SyndicStorage,
    request: &PublishActiveCasTurn,
    limit: SyndicPointReadLimit,
) -> Result<InputGateRevision, ProjectionPublicationFailure> {
    let primary = match publish_active_turn(store, storage, request, limit) {
        Ok(revision) => return Ok(revision),
        Err(primary) => primary,
    };
    verify_same_home_generation(store, expected_home_id, expected_home_generation)?;
    match storage
        .active_cas_turn_publication_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?
    {
        ActiveCasTurnPublicationStatus::Exact => {
            next_gate_revision(request.expected_gate_revision())
        }
        ActiveCasTurnPublicationStatus::Absent => Err(primary),
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

pub(super) fn abandon_active_reconciled(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: SyndicStorage,
    request: &AbandonActiveBinding,
    limit: SyndicPointReadLimit,
) -> Result<BindingRevision, ProjectionPublicationFailure> {
    let primary = match abandon_active(store, storage, request, limit) {
        Ok(revision) => return Ok(revision),
        Err(primary) => primary,
    };
    verify_same_home_generation(store, expected_home_id, expected_home_generation)?;
    match storage
        .abandoned_active_binding_publication_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?
    {
        BindingPublicationStatus::Exact => {
            next_binding_revision(request.expected_binding_revision())
        }
        BindingPublicationStatus::Prior => Err(primary),
        BindingPublicationStatus::Collision => Err(ProjectionPublicationFailure::Collision),
    }
}

pub(super) fn abandon_stop(
    store: &HomeStore,
    storage: SyndicStorage,
    request: &AbandonStopOperation,
    limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    let dispatch = dispatch(
        store,
        storage.current_abandon_stop_operation(request.clone()),
    );
    match storage
        .stop_abandonment_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?
    {
        StopOperationTransitionStatus::Exact => Ok(()),
        StopOperationTransitionStatus::Prior => Err(dispatch_failure_or_prior(dispatch)),
        StopOperationTransitionStatus::Collision => Err(ProjectionPublicationFailure::Collision),
    }
}

pub(super) fn abandon_stop_reconciled(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: SyndicStorage,
    request: &AbandonStopOperation,
    limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    let primary = match abandon_stop(store, storage, request, limit) {
        Ok(()) => return Ok(()),
        Err(primary) => primary,
    };
    verify_same_home_generation(store, expected_home_id, expected_home_generation)?;
    match storage
        .stop_abandonment_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?
    {
        StopOperationTransitionStatus::Exact => Ok(()),
        StopOperationTransitionStatus::Prior => Err(primary),
        StopOperationTransitionStatus::Collision => Err(ProjectionPublicationFailure::Collision),
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

pub(super) fn admit_live_event_reconciled(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: SyndicStorage,
    request: &LiveSourceEvent,
    limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    let primary = match admit_live_event(store, storage, request, limit) {
        Ok(()) => return Ok(()),
        Err(primary) => primary,
    };
    verify_same_home_generation(store, expected_home_id, expected_home_generation)?;
    match storage
        .live_source_event_status(store, request, limit)
        .map_err(ProjectionPublicationFailure::Reconciliation)?
    {
        LiveSourceEventStatus::Exact => Ok(()),
        LiveSourceEventStatus::Absent => Err(primary),
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

fn verify_same_home_generation(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
) -> Result<(), ProjectionPublicationFailure> {
    if store.home_id() != expected_home_id {
        return Err(ProjectionPublicationFailure::HomeAuthorityLost(
            super::ProjectionCoordinatorError::HomeIdentityMismatch {
                expected: expected_home_id,
                actual: store.home_id(),
            },
        ));
    }
    let health = store.health();
    if health.state() == HomeHealthState::Healthy
        && health.generation() == Some(expected_home_generation)
    {
        Ok(())
    } else {
        Err(ProjectionPublicationFailure::HomeAuthorityLost(
            super::ProjectionCoordinatorError::HomeGenerationMismatch {
                expected: expected_home_generation,
                actual: health.generation(),
                state: health.state(),
            },
        ))
    }
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
