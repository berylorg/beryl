use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
use beryl_state::{AssetOwner, BerylState};
use syndic_storage::{SyndicDeliveringSteeringInput, SyndicStorage, TurnIncompleteReason};

use super::{
    model::{
        ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome,
        ActiveSteeringPreparationFailure, ActiveSteeringRetryCause, ActiveSteeringRetryPolicy,
        ActiveSteeringUnknownCause,
    },
    settle::{self, ExactDisposition},
    target::ActiveSteeringTarget,
};
use crate::cas_projection::{
    ProjectionCancellationToken,
    connection::{
        ActiveBindingLossDisposition, ActiveSteeringAttemptPermit,
        CheckedSteeringLifecycleArmError, CheckedSteeringLifecycleOwner,
        CheckedSteeringLifecycleWaitError, TargetAuthorizationFailure,
    },
    input_replay::{AcceptedInputReplayContext, AcceptedInputReplayFactory},
};

pub(super) fn prepare_replay(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    route: &SyndicDeliveringSteeringInput,
    cancellation: &ProjectionCancellationToken,
) -> Result<AcceptedInputReplayFactory, ActiveSteeringPreparationFailure> {
    let state = BerylState::reacquire(home).map_err(ActiveSteeringPreparationFailure::State)?;
    let owner_head = state
        .assets()
        .owner_head(home, AssetOwner::AcceptedInput(route.input().id()))
        .map_err(ActiveSteeringPreparationFailure::Asset)?;
    AcceptedInputReplayFactory::prepare(
        home,
        storage,
        state.assets(),
        AcceptedInputReplayContext::new(
            home_id,
            home_generation,
            route.execution().root_path().mode().clone(),
        ),
        route.input().clone(),
        owner_head,
        cancellation,
    )
    .map_err(ActiveSteeringPreparationFailure::Replay)
}

pub(super) fn lose_unarmed_route(
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    cause: ActiveSteeringUnknownCause,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    settle::lose_unarmed(
        target,
        attempt,
        ActiveBindingLossDisposition::Generic,
        TurnIncompleteReason::AuthorityLost,
        ActiveSteeringDeliveryOutcome::DeliveryUnknown { cause },
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_arm_failure(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    route: &SyndicDeliveringSteeringInput,
    failure: CheckedSteeringLifecycleArmError,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    if let Err(primary) = settle::retry(home, home_id, home_generation, storage, route) {
        observe_attempt_failure(&attempt)?;
        return settle::lose_unarmed(
            target,
            attempt,
            ActiveBindingLossDisposition::Generic,
            TurnIncompleteReason::AuthorityLost,
            ActiveSteeringDeliveryOutcome::DeliveryUnknown {
                cause: ActiveSteeringUnknownCause::Disposition(primary),
            },
        );
    }
    #[cfg(test)]
    super::test_support::pause_delivery_if_requested(
        route.input().id(),
        super::test_support::DeliveryPause::AfterRetryDisposition,
    );
    settle::settle_exact_unarmed_after_target_failure(
        target,
        attempt,
        ExactDisposition::LifecycleArm(failure),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn retry_before_dispatch(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    mut owner: CheckedSteeringLifecycleOwner,
    route: &SyndicDeliveringSteeringInput,
    cause: ActiveSteeringRetryCause,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    if let Err(source) = owner.seal_without_lifecycle() {
        return lose_lifecycle(target, attempt, owner, source);
    }
    if let Err(primary) = settle::retry(home, home_id, home_generation, storage, route) {
        observe_attempt_failure(&attempt)?;
        return settle::lose_owned(
            target,
            attempt,
            owner,
            ActiveBindingLossDisposition::Generic,
            TurnIncompleteReason::AuthorityLost,
            ActiveSteeringDeliveryOutcome::DeliveryUnknown {
                cause: ActiveSteeringUnknownCause::Disposition(primary),
            },
        );
    }
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

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_authorization_failure(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    mut owner: CheckedSteeringLifecycleOwner,
    route: &SyndicDeliveringSteeringInput,
    failure: TargetAuthorizationFailure,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    if let Err(source) = owner.seal_without_lifecycle() {
        return lose_lifecycle(target, attempt, owner, source);
    }
    if let Err(primary) = settle::retry(home, home_id, home_generation, storage, route) {
        observe_attempt_failure(&attempt)?;
        return settle::lose_owned(
            target,
            attempt,
            owner,
            ActiveBindingLossDisposition::Generic,
            TurnIncompleteReason::AuthorityLost,
            ActiveSteeringDeliveryOutcome::DeliveryUnknown {
                cause: ActiveSteeringUnknownCause::Disposition(primary),
            },
        );
    }
    #[cfg(test)]
    super::test_support::pause_delivery_if_requested(
        route.input().id(),
        super::test_support::DeliveryPause::AfterRetryDisposition,
    );
    settle::settle_exact_after_target_failure(
        target,
        attempt,
        owner,
        ExactDisposition::TargetAuthorization(failure),
    )
}

pub(super) fn lose_lifecycle(
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    owner: CheckedSteeringLifecycleOwner,
    source: CheckedSteeringLifecycleWaitError,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    settle::lose_owned(
        target,
        attempt,
        owner,
        ActiveBindingLossDisposition::Generic,
        TurnIncompleteReason::CompletionMismatch,
        ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::Lifecycle(source),
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
