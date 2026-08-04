use beryl_backend::SteeredTurn;
use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, SyndicAcceptedInputId};
use syndic_storage::{SyndicStorage, TurnIncompleteReason};

use super::super::{
    model::{
        ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome, ActiveSteeringUnknownCause,
    },
    predispatch::lose_lifecycle,
    settle::{self, ExactDisposition},
    target::ActiveSteeringTarget,
};
use crate::cas_projection::connection::{
    ActiveBindingLossDisposition, ActiveSteeringAttemptPermit, CheckedSteeringLifecycleOwner,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn finish(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    target: &ActiveSteeringTarget,
    input_id: SyndicAcceptedInputId,
    attempt: ActiveSteeringAttemptPermit,
    mut owner: CheckedSteeringLifecycleOwner,
    response: SteeredTurn,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    if response.turn_id() != attempt.cas_turn_id() {
        return lose(
            target,
            attempt,
            owner,
            ActiveSteeringUnknownCause::DeliveringRouteUnavailable,
        );
    }
    if let Err(source) = owner.wait_started(&attempt) {
        return lose_lifecycle(target, attempt, owner, source);
    }
    if let Err(source) = owner.wait_completed(&attempt) {
        return lose_lifecycle(target, attempt, owner, source);
    }
    let route = match settle::read_delivering(home, storage, input_id) {
        Ok(Some(route)) if attempt.matches_delivering(&route) => route,
        Ok(_) => {
            ensure_attempt_current(&attempt)?;
            return lose(
                target,
                attempt,
                owner,
                ActiveSteeringUnknownCause::DeliveringRouteUnavailable,
            );
        }
        Err(source) => {
            observe_attempt_failure(&attempt)?;
            return lose(
                target,
                attempt,
                owner,
                ActiveSteeringUnknownCause::DeliveringRouteRead(source),
            );
        }
    };
    if let Err(primary) = settle::complete(home, home_id, home_generation, storage, &route) {
        observe_attempt_failure(&attempt)?;
        return lose(
            target,
            attempt,
            owner,
            ActiveSteeringUnknownCause::Disposition(primary),
        );
    }
    settle::settle_exact(target, attempt, owner, ExactDisposition::Delivered)
}

fn lose(
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
        TurnIncompleteReason::CompletionMismatch,
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
