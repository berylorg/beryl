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
