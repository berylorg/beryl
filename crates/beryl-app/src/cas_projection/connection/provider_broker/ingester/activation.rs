use beryl_home_store::{
    CommandError, DomainCallbackSource, DomainHandleError, HealthGateError, HomeGeneration,
    HomeHealthState, ReadError, SidecarError,
};
use syndic_storage::{
    ActiveCasTurnPublicationStatus, LiveSourceEvent, LiveSourceEventStatus, SyndicPointReadLimit,
    SyndicStorage,
};

use super::{Ingester, ProviderBrokerControl, ProviderBrokerResponseActivationFailure};
use crate::cas_projection::{
    ProjectionPublicationFailure,
    connection::router::{
        ResponseActivationProofError, SourcePublicationPermit, TargetRegistrationProof,
    },
    publication,
};

#[derive(Debug)]
pub(super) enum SourceActivationError {
    Authority(crate::cas_projection::LiveCommandAdmissionError),
    PermitGeneration(HomeGeneration),
    Reacquire(DomainHandleError),
    Publication(ProjectionPublicationFailure),
    Record(syndic_storage::SyndicRecordError),
    PublicationPanicked,
    Target,
}

impl SourceActivationError {
    pub(super) const fn authority(
        &self,
    ) -> Option<crate::cas_projection::LiveCommandAdmissionError> {
        match self {
            Self::Authority(source) => Some(*source),
            _ => None,
        }
    }

    fn verification_ambiguous(&self, expected_generation: HomeGeneration) -> bool {
        match self {
            Self::Reacquire(DomainHandleError::HealthGate(source)) => {
                health_gate_matches(source, expected_generation)
            }
            Self::Publication(source) => publication_is_health_gated(source, expected_generation),
            _ => false,
        }
    }
}

impl Ingester {
    pub(super) fn publish_source_activation(
        &self,
        permit: &SourcePublicationPermit,
        limit: SyndicPointReadLimit,
    ) -> Result<
        (
            beryl_home_store::HomeGeneration,
            syndic_storage::SyndicStorage,
        ),
        SourceActivationError,
    > {
        let (home_generation, storage) = self.source_activation_authority(permit)?;
        let Some(active_turn) = permit.active_turn_request() else {
            return Ok((home_generation, storage));
        };
        let (storage, published_gate) =
            self.publish_active_turn_once(home_generation, storage, &active_turn, limit)?;
        let activation = permit
            .activation_event(published_gate)
            .map_err(SourceActivationError::Record)?
            .ok_or(SourceActivationError::Target)?;
        let storage =
            self.publish_activation_event_once(home_generation, storage, &activation, limit)?;
        Ok((home_generation, storage))
    }

    fn source_activation_authority(
        &self,
        permit: &SourcePublicationPermit,
    ) -> Result<(beryl_home_store::HomeGeneration, SyndicStorage), SourceActivationError> {
        let mut verified_continuation = false;
        loop {
            let verification = if verified_continuation {
                None
            } else {
                Some(
                    self.live_command()
                        .await_current_or_verification(
                            &self.home,
                            self.home_id,
                            self.home_generation,
                        )
                        .map_err(SourceActivationError::Authority)?,
                )
            };
            let authority = self.source_activation_authority_once(permit);
            let Some(verification) = verification else {
                if authority
                    .as_ref()
                    .is_err_and(|error| error.verification_ambiguous(self.home_generation))
                {
                    verified_continuation = false;
                    continue;
                }
                return authority;
            };
            let settlement = verification
                .settle_after_operation()
                .map_err(SourceActivationError::Authority)?;
            if settlement.verified_current() {
                match &authority {
                    Ok(_) => {
                        verified_continuation = true;
                        continue;
                    }
                    Err(error) if error.verification_ambiguous(self.home_generation) => {
                        verified_continuation = true;
                        continue;
                    }
                    Err(_) => {}
                }
            }
            return authority;
        }
    }

    fn source_activation_authority_once(
        &self,
        permit: &SourcePublicationPermit,
    ) -> Result<(beryl_home_store::HomeGeneration, SyndicStorage), SourceActivationError> {
        #[cfg(all(test, feature = "test-faults"))]
        let inject_non_health =
            super::submitted_user::tests::pause_source_activation_authority_and_take_non_health_failure(
                self.home_id,
            );
        #[cfg(not(all(test, feature = "test-faults")))]
        let inject_non_health = false;
        if inject_non_health || !permit.matches_home_generation(self.home_generation) {
            return Err(SourceActivationError::PermitGeneration(
                self.home_generation,
            ));
        }
        let storage =
            SyndicStorage::reacquire(&self.home).map_err(SourceActivationError::Reacquire)?;
        Ok((self.home_generation, storage))
    }
}

impl ProviderBrokerControl {
    pub(in crate::cas_projection::connection) fn prove_response_activation(
        &self,
        registration: &TargetRegistrationProof,
        turn_id: &beryl_model::CasTurnId,
    ) -> Result<(), ProviderBrokerResponseActivationFailure> {
        let proof = match self.router.prove_response_activation(registration, turn_id) {
            Ok(proof) => proof,
            Err(ResponseActivationProofError::Target(reason)) => {
                return Err(ProviderBrokerResponseActivationFailure::Target(reason));
            }
            Err(ResponseActivationProofError::Router) => {
                return Err(self.retire_response_proof());
            }
        };
        let health = self.home.health();
        if self.home.home_id() != self.home_id
            || health.state() != beryl_home_store::HomeHealthState::Healthy
            || health.generation().map(|generation| generation.get())
                != Some(proof.home_generation())
        {
            return Err(self.retire_response_proof());
        }
        Ok(())
    }

    fn retire_response_proof(&self) -> ProviderBrokerResponseActivationFailure {
        let _ = self.failure_notification.notify();
        if self.commands.is_persistent_failure_cut() {
            return ProviderBrokerResponseActivationFailure::Router;
        }
        let _ = self.authority.retire();
        self.router
            .retire(crate::cas_projection::LiveEventTargetCloseReason::StreamFailure);
        ProviderBrokerResponseActivationFailure::Router
    }
}

impl Ingester {
    fn publish_active_turn_once(
        &self,
        home_generation: beryl_home_store::HomeGeneration,
        storage: SyndicStorage,
        active_turn: &syndic_storage::PublishActiveCasTurn,
        limit: SyndicPointReadLimit,
    ) -> Result<(SyndicStorage, beryl_model::InputGateRevision), SourceActivationError> {
        let verification = self
            .live_command()
            .await_current_or_verification(&self.home, self.home_id, home_generation)
            .map_err(SourceActivationError::Authority)?;
        #[cfg(all(test, feature = "test-faults"))]
        super::submitted_user::tests::record_source_activation_dispatch(self.home_id);
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            publication::publish_active_turn(&self.home, storage, active_turn, limit)
        }))
        .map_err(|_| SourceActivationError::PublicationPanicked)
        .and_then(|result| result.map_err(SourceActivationError::Publication));
        let settlement = verification
            .settle_after_operation()
            .map_err(SourceActivationError::Authority)?;
        if settlement.verified_current()
            && attempt
                .as_ref()
                .is_err_and(|error| error.verification_ambiguous(home_generation))
        {
            let storage =
                SyndicStorage::reacquire(&self.home).map_err(SourceActivationError::Reacquire)?;
            return self.reconcile_active_turn(storage, active_turn, limit, attempt.unwrap_err());
        }
        attempt.map(|published_gate| (storage, published_gate))
    }

    fn reconcile_active_turn(
        &self,
        storage: SyndicStorage,
        active_turn: &syndic_storage::PublishActiveCasTurn,
        limit: SyndicPointReadLimit,
        primary: SourceActivationError,
    ) -> Result<(SyndicStorage, beryl_model::InputGateRevision), SourceActivationError> {
        let mut storage = storage;
        loop {
            let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                storage.active_cas_turn_publication_status(&self.home, active_turn, limit)
            }))
            .map_err(|_| SourceActivationError::PublicationPanicked)
            .and_then(|result| {
                result.map_err(|source| {
                    SourceActivationError::Publication(
                        ProjectionPublicationFailure::Reconciliation(source),
                    )
                })
            });
            if status
                .as_ref()
                .is_err_and(|error| error.verification_ambiguous(self.home_generation))
            {
                let verification = self
                    .live_command()
                    .await_current_or_verification(&self.home, self.home_id, self.home_generation)
                    .map_err(SourceActivationError::Authority)?;
                storage = SyndicStorage::reacquire(&self.home)
                    .map_err(SourceActivationError::Reacquire)?;
                let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    storage.active_cas_turn_publication_status(&self.home, active_turn, limit)
                }))
                .map_err(|_| SourceActivationError::PublicationPanicked)
                .and_then(|result| {
                    result.map_err(|source| {
                        SourceActivationError::Publication(
                            ProjectionPublicationFailure::Reconciliation(source),
                        )
                    })
                });
                let settlement = verification
                    .settle_after_operation()
                    .map_err(SourceActivationError::Authority)?;
                if settlement.verified_current()
                    && status
                        .as_ref()
                        .is_err_and(|error| error.verification_ambiguous(self.home_generation))
                {
                    storage = SyndicStorage::reacquire(&self.home)
                        .map_err(SourceActivationError::Reacquire)?;
                    continue;
                }
                return reconcile_active_turn_status(storage, active_turn, primary, status);
            }
            return reconcile_active_turn_status(storage, active_turn, primary, status);
        }
    }

    fn publish_activation_event_once(
        &self,
        home_generation: beryl_home_store::HomeGeneration,
        storage: SyndicStorage,
        activation: &LiveSourceEvent,
        limit: SyndicPointReadLimit,
    ) -> Result<SyndicStorage, SourceActivationError> {
        let verification = self
            .live_command()
            .await_current_or_verification(&self.home, self.home_id, home_generation)
            .map_err(SourceActivationError::Authority)?;
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            publication::admit_live_event(&self.home, storage, activation, limit)
        }))
        .map_err(|_| SourceActivationError::PublicationPanicked)
        .and_then(|result| result.map_err(SourceActivationError::Publication));
        let settlement = verification
            .settle_after_operation()
            .map_err(SourceActivationError::Authority)?;
        if settlement.verified_current()
            && attempt
                .as_ref()
                .is_err_and(|error| error.verification_ambiguous(home_generation))
        {
            let storage =
                SyndicStorage::reacquire(&self.home).map_err(SourceActivationError::Reacquire)?;
            return self.reconcile_activation_event(
                storage,
                activation,
                limit,
                attempt.unwrap_err(),
            );
        }
        attempt.map(|()| storage)
    }

    fn reconcile_activation_event(
        &self,
        storage: SyndicStorage,
        activation: &LiveSourceEvent,
        limit: SyndicPointReadLimit,
        primary: SourceActivationError,
    ) -> Result<SyndicStorage, SourceActivationError> {
        let mut storage = storage;
        loop {
            let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                storage.live_source_event_status(&self.home, activation, limit)
            }))
            .map_err(|_| SourceActivationError::PublicationPanicked)
            .and_then(|result| {
                result.map_err(|source| {
                    SourceActivationError::Publication(
                        ProjectionPublicationFailure::Reconciliation(source),
                    )
                })
            });
            if status
                .as_ref()
                .is_err_and(|error| error.verification_ambiguous(self.home_generation))
            {
                let verification = self
                    .live_command()
                    .await_current_or_verification(&self.home, self.home_id, self.home_generation)
                    .map_err(SourceActivationError::Authority)?;
                storage = SyndicStorage::reacquire(&self.home)
                    .map_err(SourceActivationError::Reacquire)?;
                let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    storage.live_source_event_status(&self.home, activation, limit)
                }))
                .map_err(|_| SourceActivationError::PublicationPanicked)
                .and_then(|result| {
                    result.map_err(|source| {
                        SourceActivationError::Publication(
                            ProjectionPublicationFailure::Reconciliation(source),
                        )
                    })
                });
                let settlement = verification
                    .settle_after_operation()
                    .map_err(SourceActivationError::Authority)?;
                if settlement.verified_current()
                    && status
                        .as_ref()
                        .is_err_and(|error| error.verification_ambiguous(self.home_generation))
                {
                    storage = SyndicStorage::reacquire(&self.home)
                        .map_err(SourceActivationError::Reacquire)?;
                    continue;
                }
                return reconcile_activation_event_status(storage, primary, status);
            }
            return reconcile_activation_event_status(storage, primary, status);
        }
    }
}

fn reconcile_active_turn_status(
    storage: SyndicStorage,
    active_turn: &syndic_storage::PublishActiveCasTurn,
    primary: SourceActivationError,
    status: Result<ActiveCasTurnPublicationStatus, SourceActivationError>,
) -> Result<(SyndicStorage, beryl_model::InputGateRevision), SourceActivationError> {
    match status? {
        ActiveCasTurnPublicationStatus::Exact => active_turn
            .expected_gate_revision()
            .checked_next()
            .map(|published_gate| (storage, published_gate))
            .map_err(|_| {
                SourceActivationError::Publication(
                    ProjectionPublicationFailure::InputGateRevisionExhausted,
                )
            }),
        ActiveCasTurnPublicationStatus::Absent => Err(primary),
        ActiveCasTurnPublicationStatus::Collision => Err(SourceActivationError::Publication(
            ProjectionPublicationFailure::Collision,
        )),
    }
}

fn reconcile_activation_event_status(
    storage: SyndicStorage,
    primary: SourceActivationError,
    status: Result<LiveSourceEventStatus, SourceActivationError>,
) -> Result<SyndicStorage, SourceActivationError> {
    match status? {
        LiveSourceEventStatus::Exact => Ok(storage),
        LiveSourceEventStatus::Absent => Err(primary),
        LiveSourceEventStatus::Collision => Err(SourceActivationError::Publication(
            ProjectionPublicationFailure::Collision,
        )),
    }
}

fn publication_is_health_gated(
    source: &ProjectionPublicationFailure,
    expected_generation: HomeGeneration,
) -> bool {
    match source {
        ProjectionPublicationFailure::HomeRead(source) => {
            read_is_health_gated(source, expected_generation)
        }
        ProjectionPublicationFailure::Command(source) => {
            command_is_health_gated(source, expected_generation)
        }
        ProjectionPublicationFailure::Reconciliation(syndic_storage::SyndicReadError::Read(
            source,
        )) => read_is_health_gated(source, expected_generation),
        _ => false,
    }
}

fn command_is_health_gated(source: &CommandError, expected_generation: HomeGeneration) -> bool {
    match source {
        CommandError::HealthGate(source) => health_gate_matches(source, expected_generation),
        CommandError::RevisionRead { source } => read_is_health_gated(source, expected_generation),
        CommandError::ContributorAccess { source, .. } => match source {
            DomainCallbackSource::Read(source) => read_is_health_gated(source, expected_generation),
            DomainCallbackSource::Sidecar(SidecarError::HealthGate(source)) => {
                health_gate_matches(source, expected_generation)
            }
            DomainCallbackSource::Sidecar(_) => false,
        },
        _ => false,
    }
}

fn read_is_health_gated(source: &ReadError, expected_generation: HomeGeneration) -> bool {
    matches!(
        source,
        ReadError::HealthGate(gate) if health_gate_matches(gate, expected_generation)
    )
}

fn health_gate_matches(source: &HealthGateError, expected_generation: HomeGeneration) -> bool {
    source.state() == HomeHealthState::Verifying && source.generation() == expected_generation
}
