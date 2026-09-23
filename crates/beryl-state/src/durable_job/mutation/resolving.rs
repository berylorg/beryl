use std::sync::Arc;

use beryl_home_store::{
    DomainHandle, HomeCandidateRecoveryAccess, HomeStore, MutationContribution,
};
use beryl_model::{BerylHomeId, DomainRevision};

use super::super::{
    BranchHandoffJobLifecycle, DurableJobState, HandoffFailureEvidence, HandoffFailureKind,
};
use super::*;

mod access;
use access::{ReadAccess, read_authenticated};

#[derive(Clone, Debug)]
pub enum ResolvingTransition {
    Complete,
    ChildInputPending(HandoffFailureEvidence),
}

impl ResolvingTransition {
    fn successor(
        &self,
        job: BranchHandoffJobRecord,
    ) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
        if job.lifecycle() != BranchHandoffJobLifecycle::WaitingResolvingTurn {
            return Err(DurableJobMutationError::InvalidTransition {
                expected: "waiting resolving turn",
                current: job.lifecycle(),
            });
        }
        match self {
            Self::Complete => super::transition::complete_resolving_job(job),
            Self::ChildInputPending(evidence) => {
                if evidence.kind() != HandoffFailureKind::ChildInputPending {
                    return Err(DurableJobMutationError::FailureKindMismatch {
                        expected: "child input pending",
                        actual: evidence.kind(),
                    });
                }
                super::transition::terminal_failed_job(job, evidence.clone())
            }
        }
    }
}

#[derive(Clone)]
pub struct ResolvingTransitionWitness(Arc<TransitionRecords>);

struct TransitionRecords {
    home_id: BerylHomeId,
    old: BranchHandoffJobRecord,
    new: BranchHandoffJobRecord,
}

impl ResolvingTransitionWitness {
    pub fn old_job(&self) -> &BranchHandoffJobRecord {
        &self.0.old
    }
    pub fn new_job(&self) -> &BranchHandoffJobRecord {
        &self.0.new
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvingTransitionStatus {
    ExactOld,
    ExactNew,
    Collision,
}

pub struct PreparedResolvingTransition {
    handle: DomainHandle<DurableJobDomain>,
    revision: DomainRevision,
    witness: ResolvingTransitionWitness,
}

impl PreparedResolvingTransition {
    pub fn witness(&self) -> &ResolvingTransitionWitness {
        &self.witness
    }
    pub fn contribution(self) -> MutationContribution {
        self.handle.clone().contribution(self.revision, self)
    }
}

impl DurableJobState {
    pub fn prepare_resolving_transition(
        &self,
        store: &HomeStore,
        job_id: JobId,
        expected: JobRevision,
        transition: ResolvingTransition,
    ) -> Result<PreparedResolvingTransition, DurableJobMutationError> {
        self.prepare_resolving(ReadAccess::Ordinary(store), job_id, expected, transition)
    }

    pub fn prepare_resolving_transition_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        job_id: JobId,
        expected: JobRevision,
        transition: ResolvingTransition,
    ) -> Result<PreparedResolvingTransition, DurableJobMutationError> {
        self.prepare_resolving(ReadAccess::Candidate(access), job_id, expected, transition)
    }

    fn prepare_resolving(
        &self,
        access: ReadAccess<'_>,
        job_id: JobId,
        expected: JobRevision,
        transition: ResolvingTransition,
    ) -> Result<PreparedResolvingTransition, DurableJobMutationError> {
        let revision = access.revision(&self.handle)?;
        let result: Result<_, DurableJobMutationError> = (|| {
            let old = read_authenticated(&access.reader(&self.handle), job_id)?;
            ensure_revision(expected, old.revision())?;
            let new = transition.successor(old.clone())?;
            Ok(ResolvingTransitionWitness(Arc::new(TransitionRecords {
                home_id: access.home_id(),
                old,
                new,
            })))
        })();
        access.confirm(&self.handle, revision)?;
        Ok(PreparedResolvingTransition {
            handle: self.handle.clone(),
            revision,
            witness: result?,
        })
    }

    pub fn resolving_transition_status(
        &self,
        store: &HomeStore,
        witness: &ResolvingTransitionWitness,
    ) -> Result<ResolvingTransitionStatus, DurableJobMutationError> {
        self.resolving_status(ReadAccess::Ordinary(store), witness)
    }

    pub fn resolving_transition_status_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        witness: &ResolvingTransitionWitness,
    ) -> Result<ResolvingTransitionStatus, DurableJobMutationError> {
        self.resolving_status(ReadAccess::Candidate(access), witness)
    }

    fn resolving_status(
        &self,
        access: ReadAccess<'_>,
        witness: &ResolvingTransitionWitness,
    ) -> Result<ResolvingTransitionStatus, DurableJobMutationError> {
        let revision = access.revision(&self.handle)?;
        if access.home_id() != witness.0.home_id {
            return Err(DurableJobMutationError::Invariant(
                "resolving witness belongs to another home",
            ));
        }
        let result = access.outcome(&self.handle, witness);
        access.confirm(&self.handle, revision)?;
        result
    }
}

impl DomainMutation<DurableJobDomain> for PreparedResolvingTransition {
    type Error = DurableJobMutationError;
    type Prepared = ResolvingTransitionWitness;

    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let current = read_authenticated(reader, self.witness.old_job().job_id())?;
        if &current != self.witness.old_job() {
            return Err(DurableJobMutationError::Invariant(
                "resolving transition source changed",
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
