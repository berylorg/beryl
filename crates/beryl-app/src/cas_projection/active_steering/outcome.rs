mod rejection;
mod success;

use beryl_backend::{ManagedBackendError, NonIdempotentRequestOutcome, TurnSteerOutcome};
use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, SyndicAcceptedInputId};
use syndic_storage::{SyndicStorage, TurnIncompleteReason};

use super::{
    model::{
        ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome, ActiveSteeringRetryCause,
        ActiveSteeringRetryPolicy, ActiveSteeringUnknownCause,
    },
    predispatch::lose_lifecycle,
    settle::{self, ExactDisposition},
    target::ActiveSteeringTarget,
};
use crate::cas_projection::connection::{
    ActiveBindingLossDisposition, ActiveSteeringAttemptPermit, CheckedSteeringLifecycleOwner,
    ConnectionCommandOutcome,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn classify_command(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    target: &ActiveSteeringTarget,
    input_id: SyndicAcceptedInputId,
    attempt: ActiveSteeringAttemptPermit,
    owner: CheckedSteeringLifecycleOwner,
    command: ConnectionCommandOutcome<TurnSteerOutcome>,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    let (outcome, _routing_failure) = command.into_parts();
    match outcome {
        NonIdempotentRequestOutcome::ExactResponse { response } => success::finish(
            home,
            home_id,
            home_generation,
            storage,
            target,
            input_id,
            attempt,
            owner,
            response,
        ),
        NonIdempotentRequestOutcome::ExactRejection { error } => rejection::finish(
            home,
            home_id,
            home_generation,
            storage,
            target,
            input_id,
            attempt,
            owner,
            error,
        ),
        NonIdempotentRequestOutcome::ProvenNotDispatched { error } => finish_proven_not_dispatched(
            home,
            home_id,
            home_generation,
            storage,
            target,
            input_id,
            attempt,
            owner,
            error,
        ),
        NonIdempotentRequestOutcome::CompletionUnknown { error } => {
            observe_attempt_failure(&attempt)?;
            settle::lose_owned(
                target,
                attempt,
                owner,
                ActiveBindingLossDisposition::Generic,
                TurnIncompleteReason::StreamLost,
                ActiveSteeringDeliveryOutcome::DeliveryUnknown {
                    cause: ActiveSteeringUnknownCause::Backend(error),
                },
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_proven_not_dispatched(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    target: &ActiveSteeringTarget,
    input_id: SyndicAcceptedInputId,
    attempt: ActiveSteeringAttemptPermit,
    mut owner: CheckedSteeringLifecycleOwner,
    error: Box<ManagedBackendError>,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    if let Err(source) = owner.seal_without_lifecycle() {
        return lose_lifecycle(target, attempt, owner, source);
    }
    let route = match settle::read_delivering(home, storage, input_id) {
        Ok(Some(route)) if attempt.matches_delivering(&route) => route,
        Ok(_) => {
            ensure_attempt_current(&attempt)?;
            return lose_route(
                target,
                attempt,
                owner,
                ActiveSteeringUnknownCause::DeliveringRouteUnavailable,
            );
        }
        Err(source) => {
            observe_attempt_failure(&attempt)?;
            return lose_route(
                target,
                attempt,
                owner,
                ActiveSteeringUnknownCause::DeliveringRouteRead(source),
            );
        }
    };
    if let Err(primary) = settle::retry(home, home_id, home_generation, storage, &route) {
        observe_attempt_failure(&attempt)?;
        return lose_route(
            target,
            attempt,
            owner,
            ActiveSteeringUnknownCause::Disposition(primary),
        );
    }
    let cause = ActiveSteeringRetryCause::ProvenNotDispatched(error);
    match cause.policy() {
        ActiveSteeringRetryPolicy::ParkUntilLifecycleWake => {
            settle::settle_exact(target, attempt, owner, ExactDisposition::Retryable(cause))
        }
        ActiveSteeringRetryPolicy::FailCloseProjection => {
            settle::settle_exact_after_target_failure(
                target,
                attempt,
                owner,
                ExactDisposition::Retryable(cause),
            )
        }
    }
}

pub(super) fn lose_route(
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    owner: CheckedSteeringLifecycleOwner,
    cause: ActiveSteeringUnknownCause,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    settle::lose_owned(
        target,
        attempt,
        owner,
        ActiveBindingLossDisposition::Generic,
        TurnIncompleteReason::AuthorityLost,
        ActiveSteeringDeliveryOutcome::DeliveryUnknown { cause },
    )
}

fn ensure_attempt_current(
    attempt: &ActiveSteeringAttemptPermit,
) -> Result<(), ActiveSteeringDeliveryError> {
    if attempt.command_is_current() {
        Ok(())
    } else {
        Err(ActiveSteeringDeliveryError::PersistentFailureCut)
    }
}

fn observe_attempt_failure(
    attempt: &ActiveSteeringAttemptPermit,
) -> Result<(), ActiveSteeringDeliveryError> {
    if attempt.observe_persistent_failure() {
        Err(ActiveSteeringDeliveryError::PersistentFailureCut)
    } else {
        Ok(())
    }
}
