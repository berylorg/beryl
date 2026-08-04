use std::{sync::Arc, time::Duration};

use beryl_backend::{ClientUserMessageId, TurnSteerOutcome};
use syndic_storage::{
    SyndicDeliveringSteeringInput, SyndicReadySteeringInput, TurnIncompleteReason,
};

use crate::cas_projection::{
    ProjectionCoordinatorError,
    connection::{
        ActiveBindingLossDisposition, ActiveSteeringAttemptAcquireError,
        ActiveSteeringAttemptPermit, ActiveSteeringTargetLookupError,
        CheckedSteeringLifecycleArmError, CheckedSteeringLifecycleOwner, ConnectionCommandOutcome,
        ProjectionConnection, ProviderBrokerLossError, ProviderBrokerLossOutcome,
        StreamedInputBrokerService, TargetAuthorizationFailure, TargetRegistrationProof,
    },
};

/// Narrow non-polling authority for one exact current live target.
///
/// This capability deliberately has no target receiver and cannot poll, lease,
/// unregister, or hand off the presentation-owned [`super::super::LiveEventTarget`].
pub(in crate::cas_projection) struct ActiveSteeringTarget {
    connection: Arc<ProjectionConnection>,
    registration: TargetRegistrationProof,
    request_timeout: Duration,
}

impl ActiveSteeringTarget {
    pub(in crate::cas_projection) fn lookup(
        connection: Arc<ProjectionConnection>,
        ready: &SyndicReadySteeringInput,
    ) -> Result<Self, ActiveSteeringTargetLookupError> {
        let (registration, request_timeout) = connection.active_steering_target_registration(
            ready.input().thread_id(),
            ready.target(),
            ready.loaded_generation(),
        )?;
        Ok(Self {
            connection,
            registration,
            request_timeout,
        })
    }

    pub(in crate::cas_projection) fn lookup_exact_registration(
        connection: Arc<ProjectionConnection>,
        ready: &SyndicReadySteeringInput,
        registration: u64,
    ) -> Result<Self, ActiveSteeringTargetLookupError> {
        let target = Self::lookup(connection, ready)?;
        if target.registration.registration() != registration {
            return Err(ActiveSteeringTargetLookupError::MissingOrStale);
        }
        Ok(target)
    }

    pub(in crate::cas_projection) fn acquire_attempt(
        &self,
        ready: &SyndicReadySteeringInput,
        arm_waiter: bool,
    ) -> Result<ActiveSteeringAttemptPermit, ActiveSteeringAttemptAcquireError> {
        if arm_waiter {
            self.connection.acquire_active_steering_attempt(
                &self.registration,
                ready.target(),
                ready.loaded_generation(),
                true,
            )
        } else {
            self.connection.acquire_active_steering_attempt(
                &self.registration,
                ready.target(),
                ready.loaded_generation(),
                false,
            )
        }
    }

    pub(super) fn arm_checked_lifecycle(
        &self,
        attempt: &ActiveSteeringAttemptPermit,
        route: &SyndicDeliveringSteeringInput,
        home_generation: beryl_home_store::HomeGeneration,
        correlation: &ClientUserMessageId,
    ) -> Result<CheckedSteeringLifecycleOwner, CheckedSteeringLifecycleArmError> {
        self.connection
            .arm_active_steering_lifecycle(attempt, route, home_generation, correlation)
    }

    pub(super) fn steer_streamed_input(
        &self,
        attempt: &ActiveSteeringAttemptPermit,
        correlation: ClientUserMessageId,
        service: impl StreamedInputBrokerService,
    ) -> Result<
        Result<ConnectionCommandOutcome<TurnSteerOutcome>, TargetAuthorizationFailure>,
        ProjectionCoordinatorError,
    > {
        self.connection.steer_target_streamed_input(
            &self.registration,
            attempt,
            correlation,
            self.request_timeout,
            service,
        )
    }

    pub(super) fn converge_owned_loss(
        &self,
        attempt: ActiveSteeringAttemptPermit,
        owner: CheckedSteeringLifecycleOwner,
        disposition: ActiveBindingLossDisposition,
        cause: TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        self.connection
            .converge_owned_active_steering_loss(attempt, owner, disposition, cause)
    }

    pub(super) fn converge_unarmed_loss(
        &self,
        attempt: ActiveSteeringAttemptPermit,
        disposition: ActiveBindingLossDisposition,
        cause: TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        self.connection
            .converge_unarmed_active_steering_loss(attempt, disposition, cause)
    }

    pub(super) fn converge_settled_loss(
        &self,
        cause: TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        self.connection
            .converge_settled_active_steering_loss(&self.registration, cause)
    }

    #[cfg(test)]
    pub(super) const fn registration(&self) -> &TargetRegistrationProof {
        &self.registration
    }
}
