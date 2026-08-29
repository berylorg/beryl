use beryl_backend::{JsonRpcError, JsonRpcErrorVerdict};
use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, SyndicAcceptedInputId};
use syndic_storage::{SyndicStorage, TurnIncompleteReason};

use super::super::{
    model::{
        ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome,
        ActiveSteeringProjectionLossCause,
    },
    outcome::lose_route,
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
    storage: &SyndicStorage,
    target: &ActiveSteeringTarget,
    input_id: SyndicAcceptedInputId,
    attempt: ActiveSteeringAttemptPermit,
    mut owner: CheckedSteeringLifecycleOwner,
    error: JsonRpcError,
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
                super::super::model::ActiveSteeringUnknownCause::DeliveringRouteUnavailable,
            );
        }
        Err(source) => {
            observe_attempt_failure(&attempt)?;
            return lose_route(
                target,
                attempt,
                owner,
                super::super::model::ActiveSteeringUnknownCause::DeliveringRouteRead(source),
            );
        }
    };
    if matches!(
        error.verdict(),
        Some(JsonRpcErrorVerdict::ActiveTurnNotSteerable { .. })
    ) {
        if let Err(primary) = settle::reject(home, home_id, home_generation, storage, &route) {
            observe_attempt_failure(&attempt)?;
            return lose_route(
                target,
                attempt,
                owner,
                super::super::model::ActiveSteeringUnknownCause::Disposition(primary),
            );
        }
        return settle::settle_exact(
            target,
            attempt,
            owner,
            ExactDisposition::SteeringRejected(error),
        );
    }
    settle::lose_owned(
        target,
        attempt,
        owner,
        ActiveBindingLossDisposition::ExactRejected(settle::exact_rejected(&route)),
        TurnIncompleteReason::AuthorityLost,
        ActiveSteeringDeliveryOutcome::ProjectionLost {
            cause: ActiveSteeringProjectionLossCause::UnconfirmedRejection(error),
        },
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
