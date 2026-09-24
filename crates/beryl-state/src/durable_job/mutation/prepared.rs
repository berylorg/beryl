use std::sync::Arc;

use beryl_home_store::{
    DomainHandle, HomeCandidateRecoveryAccess, HomeStore, MutationContribution,
};
use beryl_model::{BerylHomeId, DomainRevision};

use super::super::{
    DurableJobState, HandoffFailureEvidence, HandoffFailureKind, ParentCasIdentity,
    ParentHandoffIdentity,
};
use super::*;

mod access;
use access::{ReadAccess, read_authenticated};

#[derive(Clone, Debug)]
pub enum HandoffJobTransition {
    CompleteResolving,
    ChildInputPending(HandoffFailureEvidence),
    StartParent(ParentHandoffIdentity),
    ParentArchived(HandoffFailureEvidence),
    ParentAccepted(ParentCasIdentity),
    RetryableFailure(HandoffFailureEvidence),
    TerminalFailure(HandoffFailureEvidence),
    Succeed,
}

impl HandoffJobTransition {
    fn successor(
        &self,
        job: BranchHandoffJobRecord,
    ) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
        match self {
            Self::CompleteResolving => super::transition::complete_resolving_job(job),
            Self::StartParent(parent) => super::transition::start_parent_job(job, *parent),
            Self::ParentAccepted(cas) => super::transition::parent_accepted_job(job, cas.clone()),
            Self::RetryableFailure(evidence) => {
                super::transition::retryable_failed_job(job, evidence.clone())
            }
            Self::TerminalFailure(evidence) => {
                super::transition::terminal_failed_job(job, evidence.clone())
            }
            Self::Succeed => super::transition::succeeded_job(job),
            Self::ChildInputPending(evidence) => {
                if evidence.kind() != HandoffFailureKind::ChildInputPending {
                    return Err(DurableJobMutationError::FailureKindMismatch {
                        expected: "child input pending",
                        actual: evidence.kind(),
                    });
                }
                super::transition::terminal_failed_job(job, evidence.clone())
            }
            Self::ParentArchived(evidence) => {
                if evidence.kind() != HandoffFailureKind::ParentArchived {
                    return Err(DurableJobMutationError::FailureKindMismatch {
                        expected: "parent archived",
                        actual: evidence.kind(),
                    });
                }
                super::transition::terminal_failed_job(job, evidence.clone())
            }
        }
    }
}

#[derive(Clone)]
pub struct HandoffJobTransitionWitness(Arc<TransitionRecords>);

struct TransitionRecords {
    home_id: BerylHomeId,
    old: BranchHandoffJobRecord,
    new: BranchHandoffJobRecord,
}

impl HandoffJobTransitionWitness {
    pub fn old_job(&self) -> &BranchHandoffJobRecord {
        &self.0.old
    }
    pub fn new_job(&self) -> &BranchHandoffJobRecord {
        &self.0.new
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandoffJobTransitionStatus {
    ExactOld,
    ExactNew,
    Collision,
}

pub struct PreparedHandoffJobTransition {
    handle: DomainHandle<DurableJobDomain>,
    revision: DomainRevision,
    witness: HandoffJobTransitionWitness,
}

impl PreparedHandoffJobTransition {
    pub fn witness(&self) -> &HandoffJobTransitionWitness {
        &self.witness
    }
    pub fn contribution(self) -> MutationContribution {
        self.handle.clone().contribution(self.revision, self)
    }
}

impl DurableJobState {
    pub fn prepare_handoff_job_transition(
        &self,
        store: &HomeStore,
        job_id: JobId,
        expected: JobRevision,
        transition: HandoffJobTransition,
    ) -> Result<PreparedHandoffJobTransition, DurableJobMutationError> {
        self.prepare_handoff_job(ReadAccess::Ordinary(store), job_id, expected, transition)
    }

    pub fn prepare_handoff_job_transition_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        job_id: JobId,
        expected: JobRevision,
        transition: HandoffJobTransition,
    ) -> Result<PreparedHandoffJobTransition, DurableJobMutationError> {
        self.prepare_handoff_job(ReadAccess::Candidate(access), job_id, expected, transition)
    }

    fn prepare_handoff_job(
        &self,
        access: ReadAccess<'_>,
        job_id: JobId,
        expected: JobRevision,
        transition: HandoffJobTransition,
    ) -> Result<PreparedHandoffJobTransition, DurableJobMutationError> {
        let revision = access.revision(&self.handle)?;
        let result: Result<_, DurableJobMutationError> = (|| {
            let old = read_authenticated(&access.reader(&self.handle), job_id)?;
            ensure_revision(expected, old.revision())?;
            let new = transition.successor(old.clone())?;
            Ok(HandoffJobTransitionWitness(Arc::new(TransitionRecords {
                home_id: access.home_id(),
                old,
                new,
            })))
        })();
        access.confirm(&self.handle, revision)?;
        Ok(PreparedHandoffJobTransition {
            handle: self.handle.clone(),
            revision,
            witness: result?,
        })
    }

    pub fn handoff_job_transition_status(
        &self,
        store: &HomeStore,
        witness: &HandoffJobTransitionWitness,
    ) -> Result<HandoffJobTransitionStatus, DurableJobMutationError> {
        self.handoff_job_status(ReadAccess::Ordinary(store), witness)
    }

    pub fn handoff_job_transition_status_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        witness: &HandoffJobTransitionWitness,
    ) -> Result<HandoffJobTransitionStatus, DurableJobMutationError> {
        self.handoff_job_status(ReadAccess::Candidate(access), witness)
    }

    fn handoff_job_status(
        &self,
        access: ReadAccess<'_>,
        witness: &HandoffJobTransitionWitness,
    ) -> Result<HandoffJobTransitionStatus, DurableJobMutationError> {
        let revision = access.revision(&self.handle)?;
        if access.home_id() != witness.0.home_id {
            return Err(DurableJobMutationError::Invariant(
                "handoff job witness belongs to another home",
            ));
        }
        let result = access.outcome(&self.handle, witness);
        access.confirm(&self.handle, revision)?;
        result
    }
}

impl DomainMutation<DurableJobDomain> for PreparedHandoffJobTransition {
    type Error = DurableJobMutationError;
    type Prepared = HandoffJobTransitionWitness;

    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let current = read_authenticated(reader, self.witness.old_job().job_id())?;
        if &current != self.witness.old_job() {
            return Err(DurableJobMutationError::Invariant(
                "handoff job transition source changed",
            ));
        }
        Ok(self.witness)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<JobRecordCodec>(1)?;
        reservation.reserve_records::<LiveJobIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        witness: Self::Prepared,
        builder: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        if witness.new_job().lifecycle().is_live() {
            put_live_transition(builder, witness.new_job())
        } else {
            put_terminal_transition(builder, witness.new_job())
        }
    }
}
