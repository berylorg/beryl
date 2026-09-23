use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};
use beryl_model::{JobId, JobRevision};

use super::codec::{JobRecordCodec, LiveJobIndexCodec};
use super::{
    BranchHandoffCheckpoint, BranchHandoffJobState, DurableJobDomain, DurableJobMutationError,
    HandoffFailureEvidence,
    mutation::{
        advance, ensure_revision, put_live_transition, put_terminal_transition, required_job,
    },
};

#[derive(Clone)]
pub enum HandoffJobIndexFault {
    MissingJob,
    MissingLive,
    MissingRequest,
    MissingAttempt,
    MissingLatest,
    LiveCopy(super::BranchHandoffJobRecord),
}

struct CorruptResolvingIndex {
    job: super::BranchHandoffJobRecord,
    fault: HandoffJobIndexFault,
}

impl super::DurableJobState {
    pub fn corrupt_handoff_job_index_for_test(
        &self,
        revision: beryl_model::DomainRevision,
        job: super::BranchHandoffJobRecord,
        fault: HandoffJobIndexFault,
    ) -> beryl_home_store::MutationContribution {
        self.handle
            .contribution(revision, CorruptResolvingIndex { job, fault })
    }
}

impl DomainMutation<DurableJobDomain> for CorruptResolvingIndex {
    type Error = DurableJobMutationError;
    type Prepared = Self;
    fn prepare(self, _: &DomainReader<'_, DurableJobDomain>) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        use super::codec::*;
        match self.fault {
            HandoffJobIndexFault::MissingJob => reservation.reserve_records::<JobRecordCodec>(1)?,
            HandoffJobIndexFault::MissingLive | HandoffJobIndexFault::LiveCopy(_) => {
                reservation.reserve_records::<LiveJobIndexCodec>(1)?
            }
            HandoffJobIndexFault::MissingRequest => {
                reservation.reserve_records::<RequestIdempotencyIndexCodec>(1)?
            }
            HandoffJobIndexFault::MissingAttempt => {
                reservation.reserve_records::<DiscussionAttemptIndexCodec>(1)?
            }
            HandoffJobIndexFault::MissingLatest => {
                reservation.reserve_records::<LatestAttemptIndexCodec>(1)?
            }
        }
        Ok(())
    }
    fn contribute(
        prepared: Self,
        builder: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        use super::codec::*;
        let job = prepared.job;
        match prepared.fault {
            HandoffJobIndexFault::MissingJob => builder.delete::<JobRecordCodec>(&job.job_id())?,
            HandoffJobIndexFault::MissingLive => {
                builder.delete::<LiveJobIndexCodec>(&job.job_id())?
            }
            HandoffJobIndexFault::MissingRequest => builder
                .delete::<RequestIdempotencyIndexCodec>(&RequestIndexKey::new(
                    job.request().clone(),
                ))?,
            HandoffJobIndexFault::MissingAttempt => builder.delete::<DiscussionAttemptIndexCodec>(
                &DiscussionAttemptKey::new(job.discussion_thread_id(), job.attempt_ordinal()),
            )?,
            HandoffJobIndexFault::MissingLatest => {
                builder.delete::<LatestAttemptIndexCodec>(&job.discussion_thread_id())?
            }
            HandoffJobIndexFault::LiveCopy(copy) => {
                builder.put::<LiveJobIndexCodec>(&job.job_id(), &copy)?
            }
        }
        Ok(())
    }
}

pub(super) struct CorruptFailureState {
    pub(super) job_id: JobId,
    pub(super) expected_job_revision: JobRevision,
    pub(super) checkpoint: BranchHandoffCheckpoint,
    pub(super) evidence: HandoffFailureEvidence,
    pub(super) retryable: bool,
}

pub(super) struct PreparedCorruptFailureState {
    job: super::BranchHandoffJobRecord,
    retryable: bool,
}

impl DomainMutation<DurableJobDomain> for CorruptFailureState {
    type Error = DurableJobMutationError;
    type Prepared = PreparedCorruptFailureState;

    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let mut job = required_job(reader, self.job_id)?;
        ensure_revision(self.expected_job_revision, job.revision)?;
        job.state = if self.retryable {
            BranchHandoffJobState::RetryableFailed {
                resume: self.checkpoint,
                evidence: self.evidence,
            }
        } else {
            BranchHandoffJobState::TerminalFailed {
                stopped_at: self.checkpoint,
                evidence: self.evidence,
            }
        };
        advance(&mut job)?;
        Ok(PreparedCorruptFailureState {
            job,
            retryable: self.retryable,
        })
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
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        if prepared.retryable {
            put_live_transition(mutations, &prepared.job)
        } else {
            put_terminal_transition(mutations, &prepared.job)
        }
    }
}
