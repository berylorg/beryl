use super::*;
use beryl_home_store::ReconciliationResolution;
use beryl_model::{HomeRevision, SyndicThreadId};
use beryl_state::{
    BranchHandoffJobRecord, PreparedResolvingTransition, ResolvingTransition,
    ResolvingTransitionStatus,
};
use syndic_storage::{
    DiscussionChildSettlement, DiscussionHandoffGateRecord, DiscussionHandoffStatus,
    SyndicPointReadLimit,
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
    pub(super) fn transition(
        self,
        state: &BerylState,
        job: &BranchHandoffJobRecord,
        transition: ResolvingTransition,
    ) -> Result<PreparedResolvingTransition, DiscussionSettlementError> {
        Ok(match self {
            Self::Ordinary(s) => state.durable_jobs().prepare_resolving_transition(
                s,
                job.job_id(),
                job.revision(),
                transition,
            )?,
            Self::Candidate(s) => state
                .durable_jobs()
                .prepare_resolving_transition_candidate(
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
    ) -> Result<ResolvingTransitionStatus, DiscussionSettlementError> {
        let before = self.revision()?;
        let (job, gate) = match self {
            Self::Ordinary(s) => (
                state
                    .durable_jobs()
                    .resolving_transition_status(s, &audit.0.job)?,
                audit
                    .0
                    .gate
                    .as_ref()
                    .map(|g| syndic.discussion_handoff_status(s, g))
                    .transpose()?,
            ),
            Self::Candidate(s) => (
                state
                    .durable_jobs()
                    .resolving_transition_status_candidate(s, &audit.0.job)?,
                audit
                    .0
                    .gate
                    .as_ref()
                    .map(|g| syndic.discussion_handoff_status_candidate(s, g))
                    .transpose()?,
            ),
        };
        if before != self.revision()? {
            return Err(DiscussionSettlementError::ConcurrentChange);
        }
        Ok(match (job, gate) {
            (
                ResolvingTransitionStatus::ExactOld,
                None | Some(DiscussionHandoffStatus::ExactOld),
            ) => ResolvingTransitionStatus::ExactOld,
            (
                ResolvingTransitionStatus::ExactNew,
                None | Some(DiscussionHandoffStatus::ExactNew),
            ) => ResolvingTransitionStatus::ExactNew,
            _ => ResolvingTransitionStatus::Collision,
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
}
