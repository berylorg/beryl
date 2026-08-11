use beryl_home_store::{DomainHandleError, HomeGeneration, HomeHealthState};
use beryl_state::{AssetReadError, BerylStateReacquireError};
use syndic_storage::SyndicReadError;

use crate::cas_projection::{
    active_steering::{
        ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome,
        ActiveSteeringPreparationFailure, ActiveSteeringRetryCause, ActiveSteeringUnknownCause,
    },
    connection::ProviderBrokerLossError,
    input_replay::AcceptedInputReplayError,
};

use super::super::WorkerDisposition;
use super::{
    health::{
        from_syndic_read, is_current_health_loss_gate, is_current_health_loss_read,
        is_current_health_loss_sidecar, is_cut_correlated_gate, is_cut_correlated_read,
        is_cut_correlated_sidecar,
    },
    provenance::{
        is_current_health_loss_coordinator, is_current_health_loss_publication,
        is_cut_correlated_coordinator, is_cut_correlated_publication,
    },
    types::SchedulerFailure,
};

fn is_current_health_loss_replay(
    error: &AcceptedInputReplayError,
    expected: HomeGeneration,
) -> bool {
    match error {
        AcceptedInputReplayError::HomeNotHealthy {
            state,
            expected_home_id,
            actual_home_id,
            expected_generation,
            actual_generation: Some(actual_generation),
        } => {
            *state != HomeHealthState::Healthy
                && expected_home_id == actual_home_id
                && *expected_generation == expected
                && *actual_generation == expected
        }
        AcceptedInputReplayError::HomeGenerationMismatch {
            expected: bound,
            actual: Some(actual),
            state,
        } => *state != HomeHealthState::Healthy && *bound == expected && *actual == expected,
        AcceptedInputReplayError::HomeRead(source) => is_current_health_loss_read(source, expected),
        AcceptedInputReplayError::SyndicRead(SyndicReadError::Read(source)) => {
            is_current_health_loss_read(source, expected)
        }
        AcceptedInputReplayError::AssetRead(AssetReadError::Read(source)) => {
            is_current_health_loss_read(source, expected)
        }
        AcceptedInputReplayError::Sidecar(source) => {
            is_current_health_loss_sidecar(source, expected)
        }
        _ => false,
    }
}

fn is_cut_correlated_replay(error: &AcceptedInputReplayError, expected: HomeGeneration) -> bool {
    match error {
        AcceptedInputReplayError::HomeNotHealthy {
            state: HomeHealthState::Failed,
            expected_home_id,
            actual_home_id,
            expected_generation,
            actual_generation: Some(actual_generation),
        } => {
            expected_home_id == actual_home_id
                && *expected_generation == expected
                && *actual_generation == expected
        }
        AcceptedInputReplayError::HomeGenerationMismatch {
            expected: bound,
            actual: Some(actual),
            state: HomeHealthState::Failed,
        } => *bound == expected && *actual == expected,
        AcceptedInputReplayError::HomeRead(source) => is_cut_correlated_read(source, expected),
        AcceptedInputReplayError::SyndicRead(SyndicReadError::Read(source)) => {
            is_cut_correlated_read(source, expected)
        }
        AcceptedInputReplayError::AssetRead(AssetReadError::Read(source)) => {
            is_cut_correlated_read(source, expected)
        }
        AcceptedInputReplayError::Sidecar(source) => is_cut_correlated_sidecar(source, expected),
        _ => false,
    }
}

fn is_current_health_loss_steering_retry(
    cause: &ActiveSteeringRetryCause,
    expected: HomeGeneration,
) -> bool {
    let ActiveSteeringRetryCause::Preparation(source) = cause else {
        return false;
    };
    match source {
        ActiveSteeringPreparationFailure::State(BerylStateReacquireError::Domain {
            source: DomainHandleError::HealthGate(source),
            ..
        }) => is_current_health_loss_gate(source, expected),
        ActiveSteeringPreparationFailure::Asset(source) => {
            is_current_health_loss_read(source, expected)
        }
        ActiveSteeringPreparationFailure::Replay(source) => {
            is_current_health_loss_replay(source, expected)
        }
        _ => false,
    }
}

fn is_cut_correlated_steering_retry(
    cause: &ActiveSteeringRetryCause,
    expected: HomeGeneration,
) -> bool {
    let ActiveSteeringRetryCause::Preparation(source) = cause else {
        return false;
    };
    match source {
        ActiveSteeringPreparationFailure::State(BerylStateReacquireError::Domain {
            source: DomainHandleError::HealthGate(source),
            ..
        }) => is_cut_correlated_gate(source, expected),
        ActiveSteeringPreparationFailure::Asset(source) => is_cut_correlated_read(source, expected),
        ActiveSteeringPreparationFailure::Replay(source) => {
            is_cut_correlated_replay(source, expected)
        }
        _ => false,
    }
}

fn is_current_health_loss_broker_loss(
    error: &ProviderBrokerLossError,
    expected: HomeGeneration,
) -> bool {
    match error {
        ProviderBrokerLossError::Coordinator(source) => {
            is_current_health_loss_coordinator(source, expected)
        }
        ProviderBrokerLossError::Read(SyndicReadError::Read(source)) => {
            is_current_health_loss_read(source, expected)
        }
        ProviderBrokerLossError::Publication(source) => {
            is_current_health_loss_publication(source, expected)
        }
        ProviderBrokerLossError::LiveSource(
            crate::cas_projection::live_source::LiveSourcePublicationError::Read(
                SyndicReadError::Read(source),
            ),
        ) => is_current_health_loss_read(source, expected),
        ProviderBrokerLossError::LiveSource(
            crate::cas_projection::live_source::LiveSourcePublicationError::Publication(source),
        ) => is_current_health_loss_publication(source, expected),
        ProviderBrokerLossError::Stop(crate::cas_projection::StopCoordinationError::Read(
            SyndicReadError::Read(source),
        )) => is_current_health_loss_read(source, expected),
        _ => false,
    }
}

fn is_cut_correlated_broker_loss(
    error: &ProviderBrokerLossError,
    expected: HomeGeneration,
) -> bool {
    match error {
        ProviderBrokerLossError::Coordinator(source) => {
            is_cut_correlated_coordinator(source, expected)
        }
        ProviderBrokerLossError::Read(SyndicReadError::Read(source)) => {
            is_cut_correlated_read(source, expected)
        }
        ProviderBrokerLossError::Publication(source) => {
            is_cut_correlated_publication(source, expected)
        }
        ProviderBrokerLossError::LiveSource(
            crate::cas_projection::live_source::LiveSourcePublicationError::Read(
                SyndicReadError::Read(source),
            ),
        ) => is_cut_correlated_read(source, expected),
        ProviderBrokerLossError::LiveSource(
            crate::cas_projection::live_source::LiveSourcePublicationError::Publication(source),
        ) => is_cut_correlated_publication(source, expected),
        ProviderBrokerLossError::Stop(crate::cas_projection::StopCoordinationError::Read(
            SyndicReadError::Read(source),
        )) => is_cut_correlated_read(source, expected),
        _ => false,
    }
}

pub(in crate::cas_projection::accepted_input_scheduler) fn classify_active_steering_delivery(
    result: &Result<ActiveSteeringDeliveryOutcome, ActiveSteeringDeliveryError>,
    home_generation: HomeGeneration,
) -> WorkerDisposition {
    match result {
        Ok(ActiveSteeringDeliveryOutcome::Retryable { cause })
            if is_current_health_loss_steering_retry(cause, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(ActiveSteeringDeliveryOutcome::Retryable { cause })
            if is_cut_correlated_steering_retry(cause, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(ActiveSteeringDeliveryOutcome::Retryable { .. }) => WorkerDisposition::Parked,
        Err(ActiveSteeringDeliveryError::PersistentFailureCut) => WorkerDisposition::Parked,
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::Disposition(error),
        }) if is_current_health_loss_publication(error, home_generation) => {
            // Target-loss convergence has already consumed the attempt/lifecycle owners. The
            // Durable delivery state remains authoritative; the generation closes without
            // retaining an ownerless in-memory attempt.
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::DeliveringRouteRead(SyndicReadError::Read(error)),
        }) if is_current_health_loss_read(error, home_generation) => {
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::Coordinator(error),
        }) if is_current_health_loss_coordinator(error, home_generation) => {
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::Disposition(error),
        }) if is_cut_correlated_publication(error, home_generation) => {
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::DeliveringRouteRead(error),
        }) if matches!(
            from_syndic_read(error, home_generation),
            SchedulerFailure::PersistentHomeFailure
        ) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::Coordinator(error),
        }) if is_cut_correlated_coordinator(error, home_generation) => {
            WorkerDisposition::PersistentHomeFailure
        }
        Ok(
            ActiveSteeringDeliveryOutcome::Delivered
            | ActiveSteeringDeliveryOutcome::SteeringRejected { .. }
            | ActiveSteeringDeliveryOutcome::ProjectionLost { .. }
            | ActiveSteeringDeliveryOutcome::DeliveryUnknown { .. },
        ) => WorkerDisposition::Settled,
        Ok(
            ActiveSteeringDeliveryOutcome::NotReady
            | ActiveSteeringDeliveryOutcome::Saturated { .. },
        ) => WorkerDisposition::Fatal,
        Err(ActiveSteeringDeliveryError::Coordinator(error))
            if is_current_health_loss_coordinator(error, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(ActiveSteeringDeliveryError::Read(SyndicReadError::Read(error)))
            if is_current_health_loss_read(error, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(ActiveSteeringDeliveryError::Publication(error))
            if is_current_health_loss_publication(error, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(ActiveSteeringDeliveryError::Loss(error))
            if is_current_health_loss_broker_loss(error, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(ActiveSteeringDeliveryError::Coordinator(error))
            if is_cut_correlated_coordinator(error, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(ActiveSteeringDeliveryError::Read(error))
            if matches!(
                from_syndic_read(error, home_generation),
                SchedulerFailure::PersistentHomeFailure
            ) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(ActiveSteeringDeliveryError::Publication(error))
            if is_cut_correlated_publication(error, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(ActiveSteeringDeliveryError::Loss(error))
            if is_cut_correlated_broker_loss(error, home_generation) =>
        {
            WorkerDisposition::PersistentHomeFailure
        }
        Err(_) => WorkerDisposition::Fatal,
    }
}
