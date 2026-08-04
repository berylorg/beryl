use beryl_home_store::{
    CommandError, CurrentDomainCommand, HomeGeneration, HomeHealthState, HomeStore,
};
use beryl_model::BerylHomeId;
use syndic_storage::{
    AcceptedInputDeliveryTransitionStatus, BeginAcceptedInputDelivery,
    CompleteAcceptedInputDelivery, RetryAcceptedInputDelivery, SteeringRejection,
    SyndicPointReadLimit, SyndicReadError, SyndicStorage,
};

use crate::cas_projection::{ProjectionCoordinatorError, ProjectionPublicationFailure};

pub(super) fn begin(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: SyndicStorage,
    request: BeginAcceptedInputDelivery,
    limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    publish_reconciled(
        store,
        expected_home_id,
        expected_home_generation,
        storage.current_begin_accepted_input_delivery(request.clone()),
        || storage.begin_accepted_input_delivery_status(store, &request, limit),
    )
}

pub(super) fn retry(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: SyndicStorage,
    request: RetryAcceptedInputDelivery,
    limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    publish_reconciled(
        store,
        expected_home_id,
        expected_home_generation,
        storage.current_retry_accepted_input_delivery(request.clone()),
        || storage.retry_accepted_input_delivery_status(store, &request, limit),
    )
}

pub(super) fn complete(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: SyndicStorage,
    request: CompleteAcceptedInputDelivery,
    limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    publish_reconciled(
        store,
        expected_home_id,
        expected_home_generation,
        storage.current_complete_accepted_input_delivery(request.clone()),
        || storage.complete_accepted_input_delivery_status(store, &request, limit),
    )
}

pub(super) fn reject(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: SyndicStorage,
    request: SteeringRejection,
    limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    publish_reconciled(
        store,
        expected_home_id,
        expected_home_generation,
        storage.current_record_steering_rejection(request.clone()),
        || storage.steering_rejection_status(store, &request, limit),
    )
}

fn publish_reconciled(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    command: CurrentDomainCommand,
    status: impl Fn() -> Result<AcceptedInputDeliveryTransitionStatus, SyndicReadError>,
) -> Result<(), ProjectionPublicationFailure> {
    let dispatch = store.execute_current(command);
    let primary = match status() {
        Ok(AcceptedInputDeliveryTransitionStatus::Exact) => return Ok(()),
        Ok(AcceptedInputDeliveryTransitionStatus::Prior) => dispatch_failure_or_prior(dispatch),
        Ok(AcceptedInputDeliveryTransitionStatus::Collision) => {
            ProjectionPublicationFailure::Collision
        }
        Err(source) => ProjectionPublicationFailure::Reconciliation(source),
    };

    verify_same_home_generation(store, expected_home_id, expected_home_generation)?;
    match status().map_err(ProjectionPublicationFailure::Reconciliation)? {
        AcceptedInputDeliveryTransitionStatus::Exact => Ok(()),
        AcceptedInputDeliveryTransitionStatus::Prior => Err(primary),
        AcceptedInputDeliveryTransitionStatus::Collision => {
            Err(ProjectionPublicationFailure::Collision)
        }
    }
}

fn dispatch_failure_or_prior(
    dispatch: Result<beryl_home_store::CommitReceipt, CommandError>,
) -> ProjectionPublicationFailure {
    match dispatch {
        Ok(_) => ProjectionPublicationFailure::Prior,
        Err(source) => ProjectionPublicationFailure::Command(source),
    }
}

fn verify_same_home_generation(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
) -> Result<(), ProjectionPublicationFailure> {
    if store.home_id() != expected_home_id {
        return Err(ProjectionPublicationFailure::HomeAuthorityLost(
            ProjectionCoordinatorError::HomeIdentityMismatch {
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
            ProjectionCoordinatorError::HomeGenerationMismatch {
                expected: expected_home_generation,
                actual: health.generation(),
                state: health.state(),
            },
        ))
    }
}
