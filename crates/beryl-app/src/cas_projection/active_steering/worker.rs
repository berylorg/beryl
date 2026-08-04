#[cfg(test)]
use std::time::Duration;

use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, SyndicAcceptedInputId};
use syndic_storage::{
    BeginAcceptedInputDelivery, SyndicReadySteeringInput, SyndicStorage, TurnIncompleteReason,
};

use super::{
    model::{
        ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome, ActiveSteeringRetryCause,
        ActiveSteeringUnknownCause,
    },
    outcome::classify_command,
    predispatch::{
        handle_arm_failure, handle_authorization_failure, lose_unarmed_route, prepare_replay,
        retry_before_dispatch,
    },
    publication, settle,
    target::ActiveSteeringTarget,
};
#[cfg(test)]
use crate::cas_projection::{
    LiveEventTarget,
    active_steering::model::ActiveSteeringSaturationCause,
    connection::{ActiveSteeringAttemptAcquireError, ActiveSteeringTargetLookupError},
    service_config::{ProjectionWorkerPermitError, ProjectionWorkerPool},
};
use crate::cas_projection::{
    ProjectionCancellationToken, ProjectionPublicationFailure,
    connection::{ActiveBindingLossDisposition, ActiveSteeringAttemptPermit},
    input_replay::{encode_accepted_input_steering_correlation, point_limit},
    service_config::ProjectionWorkerPermit,
};

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(in crate::cas_projection) fn deliver(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    workers: &ProjectionWorkerPool,
    target: &LiveEventTarget,
    input_id: SyndicAcceptedInputId,
    cancellation: &ProjectionCancellationToken,
    _request_timeout: Duration,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    let worker = match workers.try_acquire_steering_critical() {
        Ok(worker) => worker,
        Err(ProjectionWorkerPermitError::CapacityFull { .. }) => {
            return Ok(ActiveSteeringDeliveryOutcome::Saturated {
                cause: ActiveSteeringSaturationCause::WorkerPoolFull,
            });
        }
        Err(ProjectionWorkerPermitError::Poisoned) => {
            return Err(
                crate::cas_projection::ProjectionCoordinatorError::ProjectionWorkerPoolPoisoned
                    .into(),
            );
        }
    };

    let Some(ready) = storage.ready_steering_input(home, input_id, point_limit())? else {
        return Ok(ActiveSteeringDeliveryOutcome::NotReady);
    };
    ensure_ready_target(target, home_id, home_generation, &ready)?;
    let capability = target
        .active_steering_capability(&ready)
        .map_err(|error| match error {
            ActiveSteeringTargetLookupError::MissingOrStale => {
                ActiveSteeringDeliveryError::Attempt(
                    ActiveSteeringAttemptAcquireError::TargetMismatch,
                )
            }
            ActiveSteeringTargetLookupError::Router => {
                ActiveSteeringDeliveryError::Attempt(ActiveSteeringAttemptAcquireError::Router)
            }
        })?;
    let attempt = match capability.acquire_attempt(&ready, false) {
        Ok(attempt) => attempt,
        Err(ActiveSteeringAttemptAcquireError::Busy) => {
            return Ok(ActiveSteeringDeliveryOutcome::Saturated {
                cause: ActiveSteeringSaturationCause::ConnectionAttemptBusy,
            });
        }
        Err(error) => return Err(ActiveSteeringDeliveryError::Attempt(error)),
    };
    deliver_prepared(
        home,
        home_id,
        home_generation,
        storage,
        &worker,
        capability,
        ready,
        cancellation,
        attempt,
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::cas_projection) fn deliver_prepared(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    _worker: &ProjectionWorkerPermit,
    target: ActiveSteeringTarget,
    ready: SyndicReadySteeringInput,
    cancellation: &ProjectionCancellationToken,
    attempt: ActiveSteeringAttemptPermit,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    let input_id = ready.input().id();
    #[cfg(test)]
    super::test_support::pause_delivery_if_requested(
        input_id,
        super::test_support::DeliveryPause::BeforeDeliveryClaim,
    );
    let begin = BeginAcceptedInputDelivery::new(
        ready.input().thread_id(),
        ready.input().id(),
        ready.accepted_input_revision(),
        ready.target().clone(),
    );
    let begin_result = publication::begin(
        home,
        home_id,
        home_generation,
        storage,
        begin,
        point_limit(),
    );
    if let Err(primary) = begin_result {
        observe_attempt_failure(&attempt)?;
        if matches!(
            &primary,
            ProjectionPublicationFailure::Reconciliation(_)
                | ProjectionPublicationFailure::HomeAuthorityLost(_)
        ) {
            let _ = target.converge_unarmed_loss(
                attempt,
                ActiveBindingLossDisposition::Generic,
                TurnIncompleteReason::AuthorityLost,
            )?;
        } else {
            attempt
                .finish()
                .map_err(ActiveSteeringDeliveryError::AttemptFinish)?;
        }
        return Err(primary.into());
    }
    ensure_attempt_current(&attempt)?;

    continue_claimed(
        home,
        home_id,
        home_generation,
        storage,
        &target,
        input_id,
        cancellation,
        attempt,
    )
}

#[cfg(test)]
fn ensure_ready_target(
    target: &LiveEventTarget,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    ready: &SyndicReadySteeringInput,
) -> Result<(), ActiveSteeringDeliveryError> {
    if target.home_id() != home_id
        || target.home_generation() != home_generation
        || target.syndic_thread_id() != ready.input().thread_id()
        || target.cas_thread_id() != ready.target().pending().cas_thread_id()
        || target.loaded_session_generation() != ready.loaded_generation()
    {
        return Err(ActiveSteeringDeliveryError::Attempt(
            ActiveSteeringAttemptAcquireError::TargetMismatch,
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn continue_claimed(
    home: &HomeStore,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    target: &ActiveSteeringTarget,
    input_id: SyndicAcceptedInputId,
    cancellation: &ProjectionCancellationToken,
    attempt: ActiveSteeringAttemptPermit,
) -> Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError> {
    ensure_attempt_current(&attempt)?;
    let route = match settle::read_delivering(home, storage, input_id) {
        Ok(Some(route))
            if attempt.matches_delivering(&route)
                && attempt.home_generation(home) == Some(home_generation) =>
        {
            route
        }
        Ok(_) => {
            ensure_attempt_current(&attempt)?;
            return lose_unarmed_route(
                target,
                attempt,
                ActiveSteeringUnknownCause::DeliveringRouteUnavailable,
            );
        }
        Err(source) => {
            observe_attempt_failure(&attempt)?;
            return lose_unarmed_route(
                target,
                attempt,
                ActiveSteeringUnknownCause::DeliveringRouteRead(source),
            );
        }
    };
    let correlation = encode_accepted_input_steering_correlation(input_id);
    #[cfg(test)]
    super::test_support::pause_delivery_if_requested(
        input_id,
        super::test_support::DeliveryPause::BeforeLifecycleArm,
    );
    let owner = match target.arm_checked_lifecycle(&attempt, &route, home_generation, &correlation)
    {
        Ok(owner) => owner,
        Err(error) => {
            observe_attempt_failure(&attempt)?;
            return handle_arm_failure(
                home,
                home_id,
                home_generation,
                storage,
                target,
                attempt,
                &route,
                error,
            );
        }
    };
    let replay = match prepare_replay(
        home,
        home_id,
        home_generation,
        storage,
        &route,
        cancellation,
    ) {
        Ok(replay) => replay,
        Err(failure) => {
            observe_attempt_failure(&attempt)?;
            return retry_before_dispatch(
                home,
                home_id,
                home_generation,
                storage,
                target,
                attempt,
                owner,
                &route,
                ActiveSteeringRetryCause::Preparation(failure),
            );
        }
    };
    let mut source = replay.fresh_source();
    #[cfg(test)]
    super::test_support::pause_delivery_if_requested(
        input_id,
        super::test_support::DeliveryPause::BeforeCommandAuthorization,
    );
    let command = match target.steer_streamed_input(
        &attempt,
        correlation,
        source.service(home, cancellation),
    ) {
        Ok(Ok(command)) => {
            observe_attempt_failure(&attempt)?;
            command
        }
        Ok(Err(failure)) => {
            observe_attempt_failure(&attempt)?;
            return handle_authorization_failure(
                home,
                home_id,
                home_generation,
                storage,
                target,
                attempt,
                owner,
                &route,
                failure,
            );
        }
        Err(source) => {
            observe_attempt_failure(&attempt)?;
            return settle::lose_owned(
                target,
                attempt,
                owner,
                ActiveBindingLossDisposition::Generic,
                TurnIncompleteReason::AuthorityLost,
                ActiveSteeringDeliveryOutcome::DeliveryUnknown {
                    cause: ActiveSteeringUnknownCause::Coordinator(source),
                },
            );
        }
    };
    classify_command(
        home,
        home_id,
        home_generation,
        storage,
        target,
        input_id,
        attempt,
        owner,
        command,
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
