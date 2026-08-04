use super::super::ServiceAdoptionAttempt;
use super::ProjectionCandidateReauthenticationReason;
#[cfg(test)]
use super::test_support::CandidateReauthenticationFactFault;
use crate::cas_projection::{
    connection::{
        PendingProjectionLeaseOwner, StableProjectionConnectionObservation,
        registry::LoadedRegistryRecoveryObservation,
    },
    persistent_failure::{PendingProjectionGroupIdentity, PendingProjectionWitness},
};

pub(super) fn validate_candidate(
    attempt: &ServiceAdoptionAttempt,
    identity: &PendingProjectionGroupIdentity,
    witness: &PendingProjectionWitness,
    owner: &PendingProjectionLeaseOwner,
    #[cfg(test)] fact_fault: Option<CandidateReauthenticationFactFault>,
) -> Result<(), ProjectionCandidateReauthenticationReason> {
    #[cfg(test)]
    {
        let observations = CandidateFactObservations::new(identity, witness, owner, fact_fault);
        validate_observations(
            attempt,
            &observations.identity,
            &observations.witness,
            &observations.registry_connection,
            &observations.registry,
        )
    }
    #[cfg(not(test))]
    {
        validate_observations(
            attempt,
            identity,
            witness,
            owner.stable_connection_observation(),
            owner.registry_observation(),
        )
    }
}

fn validate_observations(
    attempt: &ServiceAdoptionAttempt,
    identity: &PendingProjectionGroupIdentity,
    witness: &PendingProjectionWitness,
    registry_connection: &StableProjectionConnectionObservation,
    registry: &LoadedRegistryRecoveryObservation,
) -> Result<(), ProjectionCandidateReauthenticationReason> {
    validate_candidate_identity(attempt, identity, witness)?;
    validate_candidate_owner(identity, registry_connection, registry)
}

fn validate_candidate_owner(
    identity: &PendingProjectionGroupIdentity,
    registry_connection: &StableProjectionConnectionObservation,
    registry: &LoadedRegistryRecoveryObservation,
) -> Result<(), ProjectionCandidateReauthenticationReason> {
    if !registry_connection.same_connection(identity.connection())
        || registry.key() != identity.key()
        || registry.owner() != *identity.owner()
        || registry.loaded_generation() != *identity.loaded_generation()
    {
        return Err(ProjectionCandidateReauthenticationReason::CandidateConnectionMismatch);
    }
    Ok(())
}

pub(super) fn validate_candidate_identity(
    attempt: &ServiceAdoptionAttempt,
    identity: &PendingProjectionGroupIdentity,
    witness: &PendingProjectionWitness,
) -> Result<(), ProjectionCandidateReauthenticationReason> {
    if *witness.home_id() != attempt.metadata.home_id()
        || *witness.home_generation() != attempt.metadata.old_home_generation()
        || identity.owner() != witness.syndic_thread_id()
        || identity.loaded_generation() != witness.loaded_session_generation()
    {
        return Err(ProjectionCandidateReauthenticationReason::CandidateWitnessMismatch);
    }
    Ok(())
}

#[cfg(test)]
struct CandidateFactObservations {
    identity: PendingProjectionGroupIdentity,
    witness: PendingProjectionWitness,
    registry_connection: StableProjectionConnectionObservation,
    registry: LoadedRegistryRecoveryObservation,
}

#[cfg(test)]
impl CandidateFactObservations {
    fn new(
        identity: &PendingProjectionGroupIdentity,
        witness: &PendingProjectionWitness,
        owner: &PendingProjectionLeaseOwner,
        fact_fault: Option<CandidateReauthenticationFactFault>,
    ) -> Self {
        let mut observations = Self {
            identity: identity.clone(),
            witness: witness.clone(),
            registry_connection: owner.stable_connection_observation().clone(),
            registry: owner.registry_observation().clone(),
        };
        match fact_fault {
            None => {}
            Some(CandidateReauthenticationFactFault::RegistryConnectionIdentity) => {
                observations.registry_connection.corrupt_identity_for_test()
            }
            Some(CandidateReauthenticationFactFault::RegistryKey(cas_thread_id)) => observations
                .registry
                .replace_cas_thread_id_for_test(cas_thread_id),
            Some(CandidateReauthenticationFactFault::RegistrySyndicOwner(owner)) => {
                observations.registry.replace_owner_for_test(owner);
            }
            Some(CandidateReauthenticationFactFault::RegistryLoadedGeneration(generation)) => {
                observations
                    .registry
                    .replace_loaded_generation_for_test(generation);
            }
            Some(CandidateReauthenticationFactFault::WitnessHomeId(home_id)) => {
                observations.witness.replace_home_id_for_test(home_id);
            }
            Some(CandidateReauthenticationFactFault::WitnessHomeGeneration(generation)) => {
                observations
                    .witness
                    .replace_home_generation_for_test(generation);
            }
            Some(CandidateReauthenticationFactFault::WitnessSyndicOwner(owner)) => {
                observations
                    .witness
                    .replace_syndic_thread_id_for_test(owner);
            }
            Some(CandidateReauthenticationFactFault::WitnessLoadedGeneration(generation)) => {
                observations
                    .witness
                    .replace_loaded_session_generation_for_test(generation);
            }
            Some(CandidateReauthenticationFactFault::GroupConnectionKey(cas_thread_id)) => {
                observations
                    .identity
                    .replace_cas_thread_id_for_test(cas_thread_id);
            }
        }
        observations
    }
}
