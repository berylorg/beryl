use beryl_backend::JsonRpcError;
use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, SyndicAcceptedInputId};
use syndic_storage::{
    CompleteAcceptedInputDelivery, ExactRejectedInputDelivery, RetryAcceptedInputDelivery,
    SteeringRejection, SyndicDeliveringSteeringInput, SyndicReadError, SyndicStorage,
    TurnIncompleteReason,
};

use super::{
    model::{
        ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome,
        ActiveSteeringProjectionLossCause, ActiveSteeringRetryCause,
    },
    publication,
    target::ActiveSteeringTarget,
};
use crate::cas_projection::{
    ProjectionPublicationFailure,
    connection::{
        ActiveBindingLossDisposition, ActiveSteeringAttemptFinishOutcome,
        ActiveSteeringAttemptPermit, CheckedSteeringLifecycleArmError,
        CheckedSteeringLifecycleOwner, ProviderBrokerLossOutcome, TargetAuthorizationFailure,
    },
    input_replay::point_limit,
};

pub(super) enum ExactDisposition {
    Delivered,
    Retryable(ActiveSteeringRetryCause),
    LifecycleArm(CheckedSteeringLifecycleArmError),
    TargetAuthorization(TargetAuthorizationFailure),
    SteeringRejected(JsonRpcError),
}

pub(super) fn read_delivering(
    home: &HomeStore,
    storage: SyndicStorage,
    input_id: SyndicAcceptedInputId,
) -> Result<Option<SyndicDeliveringSteeringInput>, SyndicReadError> {
    storage.delivering_steering_input(home, input_id, point_limit())
}

pub(super) fn retry(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    route: &SyndicDeliveringSteeringInput,
) -> Result<(), ProjectionPublicationFailure> {
    #[cfg(test)]
    super::test_support::pause_delivery_if_requested(
        route.input().id(),
        super::test_support::DeliveryPause::BeforeRetryDisposition,
    );
    publication::retry(
        home,
        home_id,
        home_generation,
        storage,
        RetryAcceptedInputDelivery::new(
            route.input().thread_id(),
            route.input().id(),
            route.accepted_input_revision(),
            route.target().clone(),
        ),
        point_limit(),
    )
}

pub(super) fn complete(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    route: &SyndicDeliveringSteeringInput,
) -> Result<(), ProjectionPublicationFailure> {
    #[cfg(test)]
    super::test_support::pause_delivery_if_requested(
        route.input().id(),
        super::test_support::DeliveryPause::BeforeCompleteDisposition,
    );
    publication::complete(
        home,
        home_id,
        home_generation,
        storage,
        CompleteAcceptedInputDelivery::new(
            route.input().thread_id(),
            route.input().id(),
            route.accepted_input_revision(),
            route.target().clone(),
        ),
        point_limit(),
    )
}

pub(super) fn reject(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    route: &SyndicDeliveringSteeringInput,
) -> Result<(), ProjectionPublicationFailure> {
    #[cfg(test)]
    super::test_support::pause_delivery_if_requested(
        route.input().id(),
        super::test_support::DeliveryPause::BeforeRejectionDisposition,
    );
    publication::reject(
        home,
        home_id,
        home_generation,
        storage,
        SteeringRejection::new(
            route.input().thread_id(),
            route.input().id(),
            route.accepted_input_revision(),
            route.target().clone(),
        ),
        point_limit(),
    )
}

pub(super) const fn exact_rejected(
    route: &SyndicDeliveringSteeringInput,
) -> ExactRejectedInputDelivery {
    ExactRejectedInputDelivery::new(route.input().id(), route.accepted_input_revision())
}

pub(super) fn settle_exact(
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    owner: CheckedSteeringLifecycleOwner,
    disposition: ExactDisposition,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    owner
        .release_after_disposition()
        .map_err(ActiveSteeringDeliveryError::LifecycleRelease)?;
    finish_exact(target, attempt, disposition, false)
}

pub(super) fn settle_exact_after_target_failure(
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    owner: CheckedSteeringLifecycleOwner,
    disposition: ExactDisposition,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    owner
        .release_after_disposition()
        .map_err(ActiveSteeringDeliveryError::LifecycleRelease)?;
    finish_exact(target, attempt, disposition, true)
}

pub(super) fn settle_exact_unarmed_after_target_failure(
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    disposition: ExactDisposition,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    finish_exact(target, attempt, disposition, true)
}

fn finish_exact(
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    disposition: ExactDisposition,
    loss_required: bool,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    let finish = attempt
        .finish()
        .map_err(ActiveSteeringDeliveryError::AttemptFinish)?;
    if finish == ActiveSteeringAttemptFinishOutcome::ProvenTerminal {
        return Ok(disposition.normal_outcome());
    }
    if finish == ActiveSteeringAttemptFinishOutcome::Settled && !loss_required {
        return Ok(disposition.normal_outcome());
    }
    match target.converge_settled_loss(TurnIncompleteReason::AuthorityLost)? {
        ProviderBrokerLossOutcome::Incomplete => Ok(disposition.after_target_loss()),
        ProviderBrokerLossOutcome::ProvenTerminal(_) => Ok(disposition.normal_outcome()),
    }
}

pub(super) fn lose_owned(
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    owner: CheckedSteeringLifecycleOwner,
    disposition: ActiveBindingLossDisposition,
    cause: TurnIncompleteReason,
    outcome: ActiveSteeringDeliveryOutcome,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    let _ = target.converge_owned_loss(attempt, owner, disposition, cause)?;
    Ok(outcome)
}

pub(super) fn lose_unarmed(
    target: &ActiveSteeringTarget,
    attempt: ActiveSteeringAttemptPermit,
    disposition: ActiveBindingLossDisposition,
    cause: TurnIncompleteReason,
    outcome: ActiveSteeringDeliveryOutcome,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    let _ = target.converge_unarmed_loss(attempt, disposition, cause)?;
    Ok(outcome)
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

impl ExactDisposition {
    fn normal_outcome(self) -> ActiveSteeringDeliveryOutcome {
        match self {
            Self::Delivered => ActiveSteeringDeliveryOutcome::Delivered,
            Self::Retryable(cause) => ActiveSteeringDeliveryOutcome::Retryable { cause },
            Self::LifecycleArm(failure) => ActiveSteeringDeliveryOutcome::Retryable {
                cause: ActiveSteeringRetryCause::LifecycleArm(failure),
            },
            Self::TargetAuthorization(failure) => ActiveSteeringDeliveryOutcome::Retryable {
                cause: ActiveSteeringRetryCause::TargetAuthorization(failure),
            },
            Self::SteeringRejected(rejection) => {
                ActiveSteeringDeliveryOutcome::SteeringRejected { rejection }
            }
        }
    }

    fn after_target_loss(self) -> ActiveSteeringDeliveryOutcome {
        match self {
            Self::Delivered => ActiveSteeringDeliveryOutcome::Delivered,
            Self::Retryable(_) => ActiveSteeringDeliveryOutcome::ProjectionLost {
                cause: ActiveSteeringProjectionLossCause::TargetClosed,
            },
            Self::LifecycleArm(failure) => ActiveSteeringDeliveryOutcome::ProjectionLost {
                cause: ActiveSteeringProjectionLossCause::LifecycleArm(failure),
            },
            Self::TargetAuthorization(failure) => ActiveSteeringDeliveryOutcome::ProjectionLost {
                cause: ActiveSteeringProjectionLossCause::TargetAuthorization(failure),
            },
            Self::SteeringRejected(rejection) => {
                ActiveSteeringDeliveryOutcome::SteeringRejected { rejection }
            }
        }
    }
}
