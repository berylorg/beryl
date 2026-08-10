use super::super::super::super::super::service_config::ProjectionWorkerPoolDiagnostics;
use super::super::{
    AdoptedProjectionCandidateReauthenticationLedger, CandidateLedgerEntry, ProjectionCandidateId,
};
use super::{
    CandidateReauthenticationPauseController, CandidateReauthenticationPauseStage,
    CandidateSetSealPauseController,
};

impl AdoptedProjectionCandidateReauthenticationLedger {
    pub(in crate::cas_projection) fn replace_candidate_witness_owner_for_test(
        &mut self,
        candidate_id: ProjectionCandidateId,
        owner: beryl_model::SyndicThreadId,
    ) {
        self.groups[candidate_id.group_index()]
            .witness
            .replace_syndic_thread_id_for_test(owner);
    }

    pub(in crate::cas_projection) fn replace_candidate_connection_key_for_test(
        &mut self,
        candidate_id: ProjectionCandidateId,
        cas_thread_id: beryl_model::CasThreadId,
    ) {
        self.groups[candidate_id.group_index()]
            .identity
            .replace_cas_thread_id_for_test(cas_thread_id);
    }

    pub(in crate::cas_projection) fn corrupt_candidate_stable_connection_identity_for_test(
        &mut self,
        candidate_id: ProjectionCandidateId,
    ) {
        let group = &mut self.groups[candidate_id.group_index()];
        group.identity.corrupt_connection_identity_for_test();
        let entry = group.entries[candidate_id.candidate_index()]
            .as_mut()
            .expect("the test candidate remains present");
        match entry {
            CandidateLedgerEntry::Unprocessed(owner)
            | CandidateLedgerEntry::Rejected { owner, .. } => {
                owner.corrupt_stable_connection_identity_for_test();
            }
            CandidateLedgerEntry::Accepted(_) | CandidateLedgerEntry::Disposed => {
                panic!("only an owning pending candidate can receive test identity corruption");
            }
        }
    }

    pub(in crate::cas_projection) fn force_candidate_set_topology_mismatch_for_test(&mut self) {
        self.connection_owners[0].force_candidate_set_topology_mismatch_for_test();
    }

    pub(in crate::cas_projection) fn pause_candidate_after_pre_authentication_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
    ) -> CandidateReauthenticationPauseController {
        self.test_pauses.install(
            candidate_id,
            CandidateReauthenticationPauseStage::AfterPreAuth,
        )
    }

    pub(in crate::cas_projection) fn pause_candidate_before_stable_read_confirmation_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
    ) -> CandidateReauthenticationPauseController {
        self.test_pauses.install(
            candidate_id,
            CandidateReauthenticationPauseStage::BeforeStableReadConfirmation,
        )
    }

    pub(in crate::cas_projection) fn pause_candidate_after_stable_read_for_test(
        &self,
        candidate_id: ProjectionCandidateId,
    ) -> CandidateReauthenticationPauseController {
        self.test_pauses.install(
            candidate_id,
            CandidateReauthenticationPauseStage::AfterStableRead,
        )
    }

    pub(in crate::cas_projection) fn pause_candidate_set_seal_before_transfer_for_test(
        &self,
    ) -> CandidateSetSealPauseController {
        self.test_pauses.install_seal_pause()
    }

    pub(in crate::cas_projection) fn replacement_worker_diagnostics_for_test(
        &self,
    ) -> ProjectionWorkerPoolDiagnostics {
        self.attempt
            .as_ref()
            .and_then(|attempt| attempt.adopted_service.as_ref())
            .expect("an owning ledger retains its adopted replacement service")
            .worker_pool_diagnostics()
    }

    pub(in crate::cas_projection) fn remove_adopted_service_membership_for_test(&self) {
        self.attempt
            .as_ref()
            .and_then(|attempt| attempt.adopted_service.as_ref())
            .expect("an owning ledger retains its adopted replacement service")
            .connections
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    pub(in crate::cas_projection) fn poison_adopted_service_membership_for_test(&self) {
        self.attempt
            .as_ref()
            .and_then(|attempt| attempt.adopted_service.as_ref())
            .expect("an owning ledger retains its adopted replacement service")
            .connections
            .poison_for_test();
    }
}
