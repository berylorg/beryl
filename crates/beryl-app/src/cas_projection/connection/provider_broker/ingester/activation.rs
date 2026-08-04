use syndic_storage::SyndicPointReadLimit;

use super::{Ingester, ProviderBrokerControl, ProviderBrokerResponseActivationFailure};
use crate::cas_projection::{
    connection::router::{
        ResponseActivationProofError, SourcePublicationPermit, TargetRegistrationProof,
    },
    publication,
};

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
        (),
    > {
        let home_generation = permit.home_generation(&self.home).ok_or(())?;
        let storage = syndic_storage::SyndicStorage::reacquire(&self.home).map_err(|_| ())?;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            publish_activation(
                &self.home,
                self.home_id,
                permit.active_turn_request(),
                |published_gate| permit.activation_event(published_gate),
                home_generation,
                storage,
                limit,
            )
        }))
        .map_err(|_| ())??;
        Ok((home_generation, storage))
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

#[allow(clippy::too_many_arguments)]
fn publish_activation(
    home: &beryl_home_store::HomeStore,
    home_id: beryl_model::BerylHomeId,
    active_turn: Option<syndic_storage::PublishActiveCasTurn>,
    activation_event: impl FnOnce(
        beryl_model::InputGateRevision,
    ) -> Result<
        Option<syndic_storage::LiveSourceEvent>,
        syndic_storage::SyndicRecordError,
    >,
    home_generation: beryl_home_store::HomeGeneration,
    storage: syndic_storage::SyndicStorage,
    limit: SyndicPointReadLimit,
) -> Result<(), ()> {
    let Some(active_turn) = active_turn else {
        return Ok(());
    };
    let published_gate = publication::publish_active_turn_reconciled(
        home,
        home_id,
        home_generation,
        storage,
        &active_turn,
        limit,
    )
    .map_err(|_| ())?;
    let activation = activation_event(published_gate)
        .map_err(|_| ())?
        .ok_or(())?;
    publication::admit_live_event_reconciled(
        home,
        home_id,
        home_generation,
        storage,
        &activation,
        limit,
    )
    .map_err(|_| ())
}
