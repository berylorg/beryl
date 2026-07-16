use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder};
use beryl_model::{JobId, JobRevision};

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

impl DomainMutation<DurableJobDomain> for CorruptFailureState {
    type Error = DurableJobMutationError;

    fn validate(&self, reader: &DomainReader<'_, DurableJobDomain>) -> Result<(), Self::Error> {
        let job = required_job(reader, self.job_id)?;
        ensure_revision(self.expected_job_revision, job.revision)
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, DurableJobDomain>,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        let mut job = required_job(reader, self.job_id)?;
        job.state = if self.retryable {
            BranchHandoffJobState::RetryableFailed {
                resume: self.checkpoint.clone(),
                evidence: self.evidence.clone(),
            }
        } else {
            BranchHandoffJobState::TerminalFailed {
                stopped_at: self.checkpoint.clone(),
                evidence: self.evidence.clone(),
            }
        };
        advance(&mut job)?;
        if self.retryable {
            put_live_transition(mutations, &job)
        } else {
            put_terminal_transition(mutations, &job)
        }
    }
}
