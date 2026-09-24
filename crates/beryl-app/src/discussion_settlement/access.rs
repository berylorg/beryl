use super::*;
use beryl_home_store::ReconciliationResolution;
use beryl_model::{HomeRevision, SyndicThreadId};
use beryl_state::{
    BranchHandoffJobRecord, HandoffJobTransition, HandoffJobTransitionStatus,
    PreparedHandoffJobTransition,
};
use syndic_storage::{
    DiscussionChildSettlement, DiscussionHandoffGateRecord, DiscussionHandoffStatus,
    DiscussionParentEligibility, DiscussionParentRequest, SyndicPointReadLimit,
};

#[derive(Clone, Copy)]
pub(super) enum Access<'a> {
    Ordinary(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}
impl Access<'_> {
    pub(super) fn home_id(self) -> BerylHomeId {
        match self {
            Self::Ordinary(s) => s.home_id(),
            Self::Candidate(s) => s.home_id(),
        }
    }
    pub(super) fn revision(self) -> Result<HomeRevision, DiscussionSettlementError> {
        Ok(match self {
            Self::Ordinary(s) => s.home_revision()?,
            Self::Candidate(s) => s.home_revision()?,
        })
    }
    pub(super) fn job(
        self,
        state: &BerylState,
        id: JobId,
    ) -> Result<BranchHandoffJobRecord, DiscussionSettlementError> {
        match self {
            Self::Ordinary(s) => state.durable_jobs().job(s, id)?,
            Self::Candidate(s) => state.durable_jobs().job_candidate(s, id)?,
        }
        .ok_or(DiscussionSettlementError::IdentityMismatch)
    }
    pub(super) fn gate(
        self,
        syndic: &SyndicStorage,
        id: SyndicThreadId,
    ) -> Result<DiscussionHandoffGateRecord, DiscussionSettlementError> {
        let limit = SyndicPointReadLimit::new(256).expect("fixed handoff gate limit");
        match self {
            Self::Ordinary(s) => syndic.discussion_handoff_gate(s, id, limit)?,
            Self::Candidate(s) => syndic.discussion_handoff_gate_candidate(s, id, limit)?,
        }
        .ok_or(DiscussionSettlementError::IdentityMismatch)
    }
    pub(super) fn child(
        self,
        syndic: &SyndicStorage,
        gate: DiscussionHandoffGateRecord,
    ) -> Result<DiscussionChildSettlement, DiscussionSettlementError> {
        Ok(match self {
            Self::Ordinary(s) => syndic.prepare_discussion_child_settlement(s, gate)?,
            Self::Candidate(s) => syndic.prepare_discussion_child_settlement_candidate(s, gate)?,
        })
    }
    pub(super) fn parent(
        self,
        syndic: &SyndicStorage,
        job: &BranchHandoffJobRecord,
        gate: DiscussionHandoffGateRecord,
    ) -> Result<DiscussionParentEligibility, DiscussionSettlementError> {
        let request = DiscussionParentRequest {
            child_gate: gate,
            parent_thread_id: job.parent_thread_id(),
            context_owner: job.context_owner_id(),
            context_digest: job.context_digest(),
        };
        Ok(match self {
            Self::Ordinary(s) => syndic.prepare_discussion_parent(s, request)?,
            Self::Candidate(s) => syndic.prepare_discussion_parent_candidate(s, request)?,
        })
    }
    pub(super) fn transition(
        self,
        state: &BerylState,
        job: &BranchHandoffJobRecord,
        transition: HandoffJobTransition,
    ) -> Result<PreparedHandoffJobTransition, DiscussionSettlementError> {
        Ok(match self {
            Self::Ordinary(s) => state.durable_jobs().prepare_handoff_job_transition(
                s,
                job.job_id(),
                job.revision(),
                transition,
            )?,
            Self::Candidate(s) => state
                .durable_jobs()
                .prepare_handoff_job_transition_candidate(
                    s,
                    job.job_id(),
                    job.revision(),
                    transition,
                )?,
        })
    }
    pub(super) fn natural(
        self,
        audit: &DiscussionSettlementAudit,
        syndic: &SyndicStorage,
        state: &BerylState,
    ) -> Result<HandoffJobTransitionStatus, DiscussionSettlementError> {
        let before = self.revision()?;
        let job = match &audit.0.job {
            JobWitness::Transition(witness) => match self {
                Self::Ordinary(s) => state
                    .durable_jobs()
                    .handoff_job_transition_status(s, witness)?,
                Self::Candidate(s) => state
                    .durable_jobs()
                    .handoff_job_transition_status_candidate(s, witness)?,
            },
            JobWitness::Admission(witness) => {
                use beryl_state::HandoffJobAdmissionStatus;
                match match self {
                    Self::Ordinary(s) => state
                        .durable_jobs()
                        .handoff_job_admission_status(s, witness)?,
                    Self::Candidate(s) => state
                        .durable_jobs()
                        .handoff_job_admission_status_candidate(s, witness)?,
                } {
                    HandoffJobAdmissionStatus::ExactOld => HandoffJobTransitionStatus::ExactOld,
                    HandoffJobAdmissionStatus::ExactNew => HandoffJobTransitionStatus::ExactNew,
                    HandoffJobAdmissionStatus::Collision => HandoffJobTransitionStatus::Collision,
                }
            }
        };
        let syndic_status = audit
            .0
            .syndic
            .as_ref()
            .map(|intent| self.syndic_status(syndic, intent))
            .transpose()?;
        if before != self.revision()? {
            return Err(DiscussionSettlementError::ConcurrentChange);
        }
        Ok(match (job, syndic_status) {
            (
                HandoffJobTransitionStatus::ExactOld,
                None | Some(HandoffJobTransitionStatus::ExactOld),
            ) => HandoffJobTransitionStatus::ExactOld,
            (
                HandoffJobTransitionStatus::ExactNew,
                None | Some(HandoffJobTransitionStatus::ExactNew),
            ) => HandoffJobTransitionStatus::ExactNew,
            _ => HandoffJobTransitionStatus::Collision,
        })
    }
    pub(super) fn reconcile(
        self,
        handle: &ReconciliationHandle,
    ) -> Result<ReconciliationResolution, DiscussionSettlementError> {
        Ok(match self {
            Self::Ordinary(s) => s.retry_reconciliation(handle)?,
            Self::Candidate(s) => s.retry_reconciliation(handle)?,
        })
    }
    fn syndic_status(
        self,
        syndic: &SyndicStorage,
        intent: &SyndicSettlementIntent,
    ) -> Result<HandoffJobTransitionStatus, DiscussionSettlementError> {
        use syndic_storage::GeneratedDiscussionInputStatus;
        Ok(match intent {
            SyndicSettlementIntent::Cancellation(request) => match match self {
                Self::Ordinary(s) => syndic.cancelled_binding_activation_status(
                    s,
                    request,
                    SyndicPointReadLimit::new(400_000).expect("bounded cancellation evidence"),
                )?,
                Self::Candidate(s) => syndic.cancelled_binding_activation_status_candidate(
                    s,
                    request,
                    SyndicPointReadLimit::new(400_000).expect("bounded cancellation evidence"),
                )?,
            } {
                syndic_storage::BindingPublicationStatus::Prior => {
                    HandoffJobTransitionStatus::ExactOld
                }
                syndic_storage::BindingPublicationStatus::Exact => {
                    HandoffJobTransitionStatus::ExactNew
                }
                syndic_storage::BindingPublicationStatus::Collision => {
                    HandoffJobTransitionStatus::Collision
                }
            },
            SyndicSettlementIntent::Gate(intent) => match match self {
                Self::Ordinary(s) => syndic.discussion_handoff_status(s, intent)?,
                Self::Candidate(s) => syndic.discussion_handoff_status_candidate(s, intent)?,
            } {
                DiscussionHandoffStatus::ExactOld => HandoffJobTransitionStatus::ExactOld,
                DiscussionHandoffStatus::ExactNew => HandoffJobTransitionStatus::ExactNew,
                DiscussionHandoffStatus::Collision => HandoffJobTransitionStatus::Collision,
            },
            SyndicSettlementIntent::Input(intent) => match match self {
                Self::Ordinary(s) => syndic.generated_discussion_input_status(s, intent)?,
                Self::Candidate(s) => {
                    syndic.generated_discussion_input_status_candidate(s, intent)?
                }
            } {
                GeneratedDiscussionInputStatus::ExactOld => HandoffJobTransitionStatus::ExactOld,
                GeneratedDiscussionInputStatus::ExactNew => HandoffJobTransitionStatus::ExactNew,
                GeneratedDiscussionInputStatus::Collision => HandoffJobTransitionStatus::Collision,
            },
        })
    }
}
