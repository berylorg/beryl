use super::{
    AdoptedProjectionCandidateReauthenticationLedger, ProjectionCandidateId,
    test_support::CandidateReauthenticationFactFault,
};

impl AdoptedProjectionCandidateReauthenticationLedger {
    fn install_candidate_fact_fault_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
        fault: CandidateReauthenticationFactFault,
    ) {
        assert!(
            self.candidate(candidate_id).is_some(),
            "the faulted candidate belongs to this ledger"
        );
        self.test_pauses.install_fact_fault(candidate_id, fault);
    }

    pub(in crate::cas_projection) fn inject_registry_connection_identity_mismatch_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
    ) {
        self.install_candidate_fact_fault_for_test(
            candidate_id,
            CandidateReauthenticationFactFault::RegistryConnectionIdentity,
        );
    }

    pub(in crate::cas_projection) fn replace_candidate_registry_key_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
        cas_thread_id: beryl_model::CasThreadId,
    ) {
        self.install_candidate_fact_fault_for_test(
            candidate_id,
            CandidateReauthenticationFactFault::RegistryKey(cas_thread_id),
        );
    }

    pub(in crate::cas_projection) fn replace_candidate_registry_owner_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
        owner: beryl_model::SyndicThreadId,
    ) {
        self.install_candidate_fact_fault_for_test(
            candidate_id,
            CandidateReauthenticationFactFault::RegistrySyndicOwner(owner),
        );
    }

    pub(in crate::cas_projection) fn replace_candidate_registry_loaded_generation_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
        generation: beryl_model::CasLoadedSessionGeneration,
    ) {
        self.install_candidate_fact_fault_for_test(
            candidate_id,
            CandidateReauthenticationFactFault::RegistryLoadedGeneration(generation),
        );
    }

    pub(in crate::cas_projection) fn replace_candidate_witness_home_id_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
        home_id: beryl_model::BerylHomeId,
    ) {
        self.install_candidate_fact_fault_for_test(
            candidate_id,
            CandidateReauthenticationFactFault::WitnessHomeId(home_id),
        );
    }

    pub(in crate::cas_projection) fn replace_candidate_witness_home_generation_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
        generation: beryl_home_store::HomeGeneration,
    ) {
        self.install_candidate_fact_fault_for_test(
            candidate_id,
            CandidateReauthenticationFactFault::WitnessHomeGeneration(generation),
        );
    }

    pub(in crate::cas_projection) fn inject_candidate_witness_owner_mismatch_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
        owner: beryl_model::SyndicThreadId,
    ) {
        self.install_candidate_fact_fault_for_test(
            candidate_id,
            CandidateReauthenticationFactFault::WitnessSyndicOwner(owner),
        );
    }

    pub(in crate::cas_projection) fn replace_candidate_witness_loaded_generation_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
        generation: beryl_model::CasLoadedSessionGeneration,
    ) {
        self.install_candidate_fact_fault_for_test(
            candidate_id,
            CandidateReauthenticationFactFault::WitnessLoadedGeneration(generation),
        );
    }

    pub(in crate::cas_projection) fn inject_candidate_group_connection_key_mismatch_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
        cas_thread_id: beryl_model::CasThreadId,
    ) {
        self.install_candidate_fact_fault_for_test(
            candidate_id,
            CandidateReauthenticationFactFault::GroupConnectionKey(cas_thread_id),
        );
    }
}
