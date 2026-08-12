use beryl_home_store::{DomainHandleError, HomeGeneration};
use syndic_storage::{LiveSourceEvent, SyndicPointReadLimit, SyndicStorage};

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
        let verification = self
            .live_command()
            .enter_current_home(&self.home, self.home_id, self.home_generation)
            .map_err(SourceActivationError::Authority)?;
        let authority = self.source_activation_authority_once(permit);
        verification
            .settle_after_operation()
            .map_err(SourceActivationError::Authority)?;
        authority
    }

    fn source_activation_authority_once(
        &self,
        permit: &SourcePublicationPermit,
    ) -> Result<(beryl_home_store::HomeGeneration, SyndicStorage), SourceActivationError> {
        if !permit.matches_home_generation(self.home_generation) {
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
            .enter_current_home(&self.home, self.home_id, home_generation)
            .map_err(SourceActivationError::Authority)?;
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            publication::publish_active_turn(&self.home, storage, active_turn, limit)
        }))
        .map_err(|_| SourceActivationError::PublicationPanicked)
        .and_then(|result| result.map_err(SourceActivationError::Publication));
        verification
            .settle_after_operation()
            .map_err(SourceActivationError::Authority)?;
        attempt.map(|published_gate| (storage, published_gate))
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
            .enter_current_home(&self.home, self.home_id, home_generation)
            .map_err(SourceActivationError::Authority)?;
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            publication::admit_live_event(&self.home, storage, activation, limit)
        }))
        .map_err(|_| SourceActivationError::PublicationPanicked)
        .and_then(|result| result.map_err(SourceActivationError::Publication));
        verification
            .settle_after_operation()
            .map_err(SourceActivationError::Authority)?;
        attempt.map(|()| storage)
    }
}
