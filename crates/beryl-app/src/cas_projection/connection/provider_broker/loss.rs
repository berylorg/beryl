use beryl_model::{CasLoadedSessionGeneration, CasThreadId};
use syndic_storage::{
    BindingState, ExactRejectedInputDelivery, SourceEventPayload, SyndicPointReadLimit,
    SyndicStorage, TurnEndStatus, TurnIncompleteReason,
};
use thiserror::Error;

mod abandonment;

use super::{CheckedSteeringLifecycleOwner, ProviderBrokerControl};
use crate::cas_projection::{
    LiveCommandAdmissionError, LiveCommandPermit, PendingTurnActivation,
    ProjectionCoordinatorError, ProjectionPublicationFailure,
    connection::{
        registry,
        router::{
            ActiveSteeringAttemptPermit, LiveEventTargetCloseReason, ProvenTerminalOutcome,
            TargetLossAcquisition, TargetLossFinishError, TargetLossRequestError,
            TargetRegistrationProof,
        },
    },
    live_source::{
        LiveSourceFrontier, LiveSourcePublicationError, LiveSourceTarget, publish_reconciled,
    },
    publication,
};
use abandonment::abandon_exact_active;

const STALE_REASON: &str = "ordinary turn lost live CAS projection authority";

pub(in crate::cas_projection) enum ProviderBrokerLossOutcome {
    Incomplete,
    ProvenTerminal(ProvenTerminalOutcome),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) enum ActiveBindingLossDisposition {
    Generic,
    ExactRejected(ExactRejectedInputDelivery),
}

#[derive(Debug, Error)]
pub(in crate::cas_projection) enum ProviderBrokerLossError {
    #[error(transparent)]
    LiveCommand(#[from] LiveCommandAdmissionError),
    #[error("persistent home failure cut the broker loss command")]
    PersistentFailureCut,
    #[error(transparent)]
    Coordinator(#[from] ProjectionCoordinatorError),
    #[error("the broker loss publisher could not lock its fixed control cell")]
    ControlPoisoned,
    #[error("the broker loss publisher could not reacquire the exact Syndic storage generation")]
    StorageUnavailable,
    #[error("the target loss authority is no longer available")]
    TargetUnavailable,
    #[error("the target loss authority disagrees with durable activation state")]
    TargetMismatch,
    #[error("the target loss frontier changed during its bounded proof")]
    ConcurrentChange,
    #[error("the target loss router authority failed")]
    Router,
    #[error("the checked steering lifecycle owner could not be settled after target loss")]
    LifecycleOwnership,
    #[error(transparent)]
    Read(#[from] syndic_storage::SyndicReadError),
    #[error(transparent)]
    Record(#[from] syndic_storage::SyndicRecordError),
    #[error(transparent)]
    Publication(#[from] ProjectionPublicationFailure),
    #[error(transparent)]
    LiveSource(#[from] LiveSourcePublicationError),
    #[error("durable stop loss convergence failed")]
    Stop(#[source] crate::cas_projection::StopCoordinationError),
    #[error("durable context-compaction loss convergence failed")]
    ContextCompaction(#[source] crate::cas_projection::context_compaction::ContextCompactionError),
}

/// Non-cloneable rollback authority retained only when provisional registration fails.
pub(in crate::cas_projection::connection) struct DetachedActivationAuthority {
    cas_thread_id: CasThreadId,
    loaded_generation: CasLoadedSessionGeneration,
    home_generation: u64,
    activation: PendingTurnActivation,
}

impl DetachedActivationAuthority {
    pub(in crate::cas_projection::connection) const fn new(
        cas_thread_id: CasThreadId,
        loaded_generation: CasLoadedSessionGeneration,
        home_generation: u64,
        activation: PendingTurnActivation,
    ) -> Self {
        Self {
            cas_thread_id,
            loaded_generation,
            home_generation,
            activation,
        }
    }
}

impl ProviderBrokerControl {
    pub(in crate::cas_projection::connection) fn converge_target_loss(
        &self,
        registration: &TargetRegistrationProof,
        cause: TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        let admission = self.commands.authorize()?;
        let authority = match self.router.acquire_target_loss(admission, registration) {
            Ok(TargetLossAcquisition::Authority(authority)) => authority,
            Ok(TargetLossAcquisition::Incomplete) => {
                return Ok(ProviderBrokerLossOutcome::Incomplete);
            }
            Ok(TargetLossAcquisition::ProvenTerminal(outcome)) => {
                return Ok(ProviderBrokerLossOutcome::ProvenTerminal(outcome));
            }
            Err(TargetLossRequestError::TargetClosed) => {
                return Err(ProviderBrokerLossError::TargetUnavailable);
            }
            Err(TargetLossRequestError::Router) => {
                return Err(if self.commands.is_persistent_failure_cut() {
                    ProviderBrokerLossError::PersistentFailureCut
                } else {
                    ProviderBrokerLossError::Router
                });
            }
        };
        let command = self.commands.authorize()?;
        let _loss = match self.loss.lock() {
            Ok(loss) => loss,
            Err(_) => {
                self.finish_failed_loss(authority, &command);
                return Err(self.current_or_persistent_cut(
                    &command,
                    ProviderBrokerLossError::ControlPoisoned,
                ));
            }
        };
        let publication = match registration.compaction() {
            Some(compaction) => self
                .context_compaction
                .abandon_target_loss(compaction)
                .map_err(ProviderBrokerLossError::ContextCompaction),
            None => {
                self.publish_target_loss(&authority, ActiveBindingLossDisposition::Generic, cause)
            }
        };
        if let Err(primary) = publication {
            let cut = self.observe_persistent_failure(&command);
            self.finish_failed_loss(authority, &command);
            return Err(if cut {
                ProviderBrokerLossError::PersistentFailureCut
            } else {
                primary
            });
        }
        if !command.is_current() {
            return Err(ProviderBrokerLossError::PersistentFailureCut);
        }
        let (invalidation, connection_retired) = match authority.finish() {
            Ok(finished) => finished,
            Err(TargetLossFinishError::Router) => {
                if !command.is_current() {
                    return Err(ProviderBrokerLossError::PersistentFailureCut);
                }
                let _ = self.authority.retire();
                self.router
                    .retire(LiveEventTargetCloseReason::StreamFailure);
                return Err(ProviderBrokerLossError::Router);
            }
        };
        if !command.is_current() {
            return Err(ProviderBrokerLossError::PersistentFailureCut);
        }
        self.invalidate_loss_target(invalidation, connection_retired);
        Ok(ProviderBrokerLossOutcome::Incomplete)
    }

    pub(in crate::cas_projection) fn converge_active_steering_loss(
        &self,
        attempt: ActiveSteeringAttemptPermit,
        owner: CheckedSteeringLifecycleOwner,
        disposition: ActiveBindingLossDisposition,
        cause: TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        self.converge_owned_active_steering_loss(attempt, Some(owner), disposition, cause)
    }

    /// Retires an attempt that failed before it could arm a lifecycle owner.
    pub(in crate::cas_projection) fn converge_unarmed_active_steering_loss(
        &self,
        attempt: ActiveSteeringAttemptPermit,
        disposition: ActiveBindingLossDisposition,
        cause: TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        self.close_checked_steering_lifecycles_for_loss();
        self.converge_owned_active_steering_loss(attempt, None, disposition, cause)
    }

    fn converge_owned_active_steering_loss(
        &self,
        attempt: ActiveSteeringAttemptPermit,
        owner: Option<CheckedSteeringLifecycleOwner>,
        disposition: ActiveBindingLossDisposition,
        cause: TurnIncompleteReason,
    ) -> Result<ProviderBrokerLossOutcome, ProviderBrokerLossError> {
        let authority = attempt
            .transfer_to_target_loss()
            .map_err(|error| match error {
                TargetLossRequestError::TargetClosed => ProviderBrokerLossError::TargetUnavailable,
                TargetLossRequestError::Router => {
                    if self.commands.is_persistent_failure_cut() {
                        ProviderBrokerLossError::PersistentFailureCut
                    } else {
                        ProviderBrokerLossError::Router
                    }
                }
            })?;
        let command = self.commands.authorize()?;
        let _loss = match self.loss.lock() {
            Ok(loss) => loss,
            Err(_) => {
                drop(owner);
                self.finish_failed_loss(authority, &command);
                return Err(self.current_or_persistent_cut(
                    &command,
                    ProviderBrokerLossError::ControlPoisoned,
                ));
            }
        };
        if let Err(primary) = self.publish_target_loss(&authority, disposition, cause) {
            let cut = self.observe_persistent_failure(&command);
            drop(owner);
            self.finish_failed_loss(authority, &command);
            return Err(if cut {
                ProviderBrokerLossError::PersistentFailureCut
            } else {
                primary
            });
        }
        if !command.is_current() {
            drop(owner);
            return Err(ProviderBrokerLossError::PersistentFailureCut);
        }
        let owner_result = owner
            .map(CheckedSteeringLifecycleOwner::release_after_loss)
            .transpose();
        let (invalidation, connection_retired) = match authority.finish() {
            Ok(finished) => finished,
            Err(TargetLossFinishError::Router) => {
                if !command.is_current() {
                    return Err(ProviderBrokerLossError::PersistentFailureCut);
                }
                let _ = self.authority.retire();
                self.router
                    .retire(LiveEventTargetCloseReason::StreamFailure);
                return Err(ProviderBrokerLossError::Router);
            }
        };
        if !command.is_current() {
            return Err(ProviderBrokerLossError::PersistentFailureCut);
        }
        self.invalidate_loss_target(invalidation, connection_retired);
        if owner_result.is_err() {
            let _ = self.authority.retire();
            self.router
                .retire(LiveEventTargetCloseReason::StreamFailure);
            return Err(ProviderBrokerLossError::LifecycleOwnership);
        }
        Ok(ProviderBrokerLossOutcome::Incomplete)
    }

    pub(in crate::cas_projection::connection) fn abandon_detached_activation(
        &self,
        authority: DetachedActivationAuthority,
    ) -> Result<(), ProviderBrokerLossError> {
        let command = self.commands.authorize()?;
        let _loss = self
            .loss
            .lock()
            .map_err(|_| ProviderBrokerLossError::ControlPoisoned)?;
        if !command.is_current() {
            return Err(ProviderBrokerLossError::PersistentFailureCut);
        }
        let (home_generation, storage) = match self.loss_storage(authority.home_generation) {
            Ok(storage) => storage,
            Err(error) => {
                return Err(if self.observe_persistent_failure(&command) {
                    ProviderBrokerLossError::PersistentFailureCut
                } else {
                    error
                });
            }
        };
        let result = abandon_exact_active(
            &self.home,
            self.home_id,
            home_generation,
            storage,
            &authority.activation,
            &authority.cas_thread_id,
            authority.loaded_generation,
            point_limit(),
            ActiveBindingLossDisposition::Generic,
        );
        if result.is_err() && self.observe_persistent_failure(&command) {
            return Err(ProviderBrokerLossError::PersistentFailureCut);
        }
        result
    }

    fn publish_target_loss(
        &self,
        authority: &crate::cas_projection::connection::router::TargetLossPublicationAuthority,
        disposition: ActiveBindingLossDisposition,
        cause: TurnIncompleteReason,
    ) -> Result<(), ProviderBrokerLossError> {
        let home_generation = authority
            .home_generation(&self.home)
            .ok_or(ProviderBrokerLossError::StorageUnavailable)?;
        let storage = SyndicStorage::reacquire(&self.home)
            .map_err(|_| ProviderBrokerLossError::StorageUnavailable)?;
        let limit = point_limit();
        let binding = storage
            .current_binding(&self.home, authority.syndic_thread_id(), limit)?
            .ok_or(ProviderBrokerLossError::TargetMismatch)?;
        if matches!(binding.binding().state(), BindingState::Active(_))
            && !authority.activation_durable()
            && authority.cas_turn_id().is_some()
        {
            publish_loss_activation(
                &self.home,
                self.home_id,
                home_generation,
                storage,
                authority,
                limit,
            )?;
        }
        if self
            .stop_coordinator
            .abandon_for_authority_loss(
                authority.syndic_thread_id(),
                authority.activation().turn_id(),
            )
            .map_err(ProviderBrokerLossError::Stop)?
        {
            return Ok(());
        }
        abandon_exact_active(
            &self.home,
            self.home_id,
            home_generation,
            storage,
            authority.activation(),
            authority.cas_thread_id(),
            authority.loaded_generation(),
            limit,
            disposition,
        )?;
        let target = LiveSourceTarget::new_source_less(
            authority.activation().thread_id(),
            authority.activation().turn_id(),
        );
        let frontier = LiveSourceFrontier::read(&self.home, storage, &target, limit)?;
        let terminal = frontier.event(
            &target,
            None,
            SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(cause)),
        )?;
        publish_reconciled(
            &self.home,
            self.home_id,
            home_generation,
            storage,
            &terminal,
            limit,
        )?;
        Ok(())
    }

    fn finish_failed_loss(
        &self,
        authority: crate::cas_projection::connection::router::TargetLossPublicationAuthority,
        command: &LiveCommandPermit,
    ) {
        if !command.is_current() {
            return;
        }
        match authority.fail() {
            Ok((invalidation, connection_retired)) => {
                if !command.is_current() {
                    return;
                }
                self.invalidate_loss_target(invalidation, connection_retired);
            }
            Err(TargetLossFinishError::Router) => {
                if !command.is_current() {
                    return;
                }
                let _ = self.authority.retire();
                self.router
                    .retire(LiveEventTargetCloseReason::StreamFailure);
            }
        }
    }

    fn observe_persistent_failure(&self, command: &LiveCommandPermit) -> bool {
        let _ = command.observe_persistent_failure();
        !command.is_current()
    }

    fn router_or_persistent_cut(&self, command: &LiveCommandPermit) -> ProviderBrokerLossError {
        self.current_or_persistent_cut(command, ProviderBrokerLossError::Router)
    }

    fn current_or_persistent_cut(
        &self,
        command: &LiveCommandPermit,
        current: ProviderBrokerLossError,
    ) -> ProviderBrokerLossError {
        if command.is_current() {
            current
        } else {
            ProviderBrokerLossError::PersistentFailureCut
        }
    }

    fn invalidate_loss_target(
        &self,
        invalidation: crate::cas_projection::connection::router::TargetInvalidation,
        connection_retired: bool,
    ) {
        let registry_failed = registry::invalidate_exact_generation(
            &invalidation.key,
            self.authority.generation,
            invalidation.owner,
            invalidation.loaded_generation,
        )
        .is_err();
        if registry_failed || connection_retired {
            let _ = self.authority.retire();
            self.router.retire(if connection_retired {
                LiveEventTargetCloseReason::RetiredThreadLaneCapacity
            } else {
                LiveEventTargetCloseReason::StreamFailure
            });
        }
    }

    fn loss_storage(
        &self,
        expected_generation: u64,
    ) -> Result<(beryl_home_store::HomeGeneration, SyndicStorage), ProviderBrokerLossError> {
        if self.home.home_id() != self.home_id {
            return Err(ProviderBrokerLossError::StorageUnavailable);
        }
        let health = self.home.health();
        let generation = (health.state() == beryl_home_store::HomeHealthState::Healthy)
            .then(|| health.generation())
            .flatten()
            .filter(|generation| generation.get() == expected_generation)
            .ok_or(ProviderBrokerLossError::StorageUnavailable)?;
        let storage = SyndicStorage::reacquire(&self.home)
            .map_err(|_| ProviderBrokerLossError::StorageUnavailable)?;
        let confirmed = self.home.health();
        if confirmed.state() != beryl_home_store::HomeHealthState::Healthy
            || confirmed.generation() != Some(generation)
        {
            return Err(ProviderBrokerLossError::StorageUnavailable);
        }
        Ok((generation, storage))
    }
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(super::PROVIDER_POINT_READ_BYTES)
        .expect("provider broker point-read bound is nonzero")
}

fn publish_loss_activation(
    home: &beryl_home_store::HomeStore,
    home_id: beryl_model::BerylHomeId,
    home_generation: beryl_home_store::HomeGeneration,
    storage: SyndicStorage,
    authority: &crate::cas_projection::connection::router::TargetLossPublicationAuthority,
    limit: SyndicPointReadLimit,
) -> Result<(), ProviderBrokerLossError> {
    let active_turn = authority
        .active_turn_request()
        .ok_or(ProviderBrokerLossError::TargetMismatch)?;
    let published_gate = publication::publish_active_turn_reconciled(
        home,
        home_id,
        home_generation,
        storage,
        &active_turn,
        limit,
    )?;
    let activation = authority
        .activation_event(published_gate)?
        .ok_or(ProviderBrokerLossError::TargetMismatch)?;
    publication::admit_live_event_reconciled(
        home,
        home_id,
        home_generation,
        storage,
        &activation,
        limit,
    )?;
    Ok(())
}
