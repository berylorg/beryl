use beryl_home_store::{CommandOutcome, CurrentDomainCommand, HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
use syndic_storage::{
    BeginAcceptedInputDelivery, CompleteAcceptedInputDelivery, RetryAcceptedInputDelivery,
    SteeringRejection, SyndicPointReadLimit, SyndicStorage,
};

use crate::cas_projection::ProjectionPublicationFailure;

pub(super) fn begin(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: &SyndicStorage,
    request: BeginAcceptedInputDelivery,
    _limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    publish_reconciled(
        store,
        expected_home_id,
        expected_home_generation,
        storage.current_begin_accepted_input_delivery(request.clone()),
    )
}

pub(super) fn retry(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: &SyndicStorage,
    request: RetryAcceptedInputDelivery,
    _limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    publish_reconciled(
        store,
        expected_home_id,
        expected_home_generation,
        storage.current_retry_accepted_input_delivery(request.clone()),
    )
}

pub(super) fn complete(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: &SyndicStorage,
    request: CompleteAcceptedInputDelivery,
    _limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    publish_reconciled(
        store,
        expected_home_id,
        expected_home_generation,
        storage.current_complete_accepted_input_delivery(request.clone()),
    )
}

pub(super) fn reject(
    store: &HomeStore,
    expected_home_id: BerylHomeId,
    expected_home_generation: HomeGeneration,
    storage: &SyndicStorage,
    request: SteeringRejection,
    _limit: SyndicPointReadLimit,
) -> Result<(), ProjectionPublicationFailure> {
    publish_reconciled(
        store,
        expected_home_id,
        expected_home_generation,
        storage.current_record_steering_rejection(request.clone()),
    )
}

fn publish_reconciled(
    store: &HomeStore,
    _expected_home_id: BerylHomeId,
    _expected_home_generation: HomeGeneration,
    command: CurrentDomainCommand,
) -> Result<(), ProjectionPublicationFailure> {
    match store.execute_current(command) {
        CommandOutcome::NotCommitted { evidence } => {
            Err(ProjectionPublicationFailure::Command(evidence))
        }
        CommandOutcome::Committed {
            receipt: _,
            later_failure: None,
            local_finalization: _,
        } => Ok(()),
        CommandOutcome::Committed {
            receipt,
            later_failure: Some(later_failure),
            local_finalization: _,
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
