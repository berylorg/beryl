use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, PointReadLimit, ReconciliationReservation,
};
use beryl_model::{JobId, JobRevision};

use super::{
    BRANCH_HANDOFF_JOB_RECORD_LIMIT, BranchHandoffJobAdmission, BranchHandoffJobRecord,
    DurableJobDomain, DurableJobMutationError, LatestBranchHandoffAttempt,
    REQUEST_IDEMPOTENCY_RECORD_LIMIT, ResolutionRequestAdmission,
    codec::{
        DiscussionAttemptIndexCodec, DiscussionAttemptKey, JobRecordCodec, LatestAttemptIndexCodec,
        LiveJobIndexCodec, RequestIdempotencyIndexCodec, RequestIndexKey,
    },
};

mod access;
mod admission;
pub use admission::{
    HandoffJobAdmissionStatus, HandoffJobAdmissionWitness, PreparedHandoffJobAdmission,
};
mod prepared;
mod transition;
pub use prepared::{
    HandoffJobTransition, HandoffJobTransitionStatus, HandoffJobTransitionWitness,
    PreparedHandoffJobTransition,
};

pub use transition::{
    CompleteResolvingTurn, RecordParentCasAcceptance, RecordRetryableHandoffFailure,
    RecordTerminalHandoffFailure, RetryBranchHandoff, StartParentHandoff, SucceedBranchHandoff,
};

/// Admit one fresh immutable intent and its derived durable handoff job.
pub struct AdmitBranchHandoffJob {
    admission: BranchHandoffJobAdmission,
}

impl AdmitBranchHandoffJob {
    #[must_use]
    pub const fn new(admission: BranchHandoffJobAdmission) -> Self {
        Self { admission }
    }

    #[must_use]
    pub const fn admission(&self) -> &BranchHandoffJobAdmission {
        &self.admission
    }
}

impl DomainMutation<DurableJobDomain> for AdmitBranchHandoffJob {
    type Error = DurableJobMutationError;
    type Prepared = BranchHandoffJobRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let job = BranchHandoffJobRecord::initial(&self.admission);
        admission::validate_new(reader, &job)?;
        Ok(job)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        admission::reserve(reservation)
    }

    fn contribute(
        job: Self::Prepared,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        admission::write(&job, mutations)
    }
}

pub(super) fn read_job(
    reader: &DomainReader<'_, DurableJobDomain>,
    job_id: JobId,
) -> Result<Option<BranchHandoffJobRecord>, DurableJobMutationError> {
    reader
        .point::<JobRecordCodec>(&job_id, job_point_limit())
        .map_err(Into::into)
}

pub(super) fn required_job(
    reader: &DomainReader<'_, DurableJobDomain>,
    job_id: JobId,
) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
    read_job(reader, job_id)?.ok_or(DurableJobMutationError::JobMissing { job_id })
}

pub(super) fn ensure_revision(
    expected: JobRevision,
    current: JobRevision,
) -> Result<(), DurableJobMutationError> {
    if expected == current {
        Ok(())
    } else {
        Err(DurableJobMutationError::JobRevisionConflict { expected, current })
    }
}

pub(super) fn advance(job: &mut BranchHandoffJobRecord) -> Result<(), DurableJobMutationError> {
    job.revision = job.revision.checked_next()?;
    Ok(())
}

pub(super) fn put_live_transition(
    mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    job: &BranchHandoffJobRecord,
) -> Result<(), DurableJobMutationError> {
    mutations.put::<JobRecordCodec>(&job.job_id, job)?;
    mutations.put::<LiveJobIndexCodec>(&job.job_id, job)?;
    Ok(())
}

pub(super) fn put_terminal_transition(
    mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    job: &BranchHandoffJobRecord,
) -> Result<(), DurableJobMutationError> {
    mutations.put::<JobRecordCodec>(&job.job_id, job)?;
    mutations.delete::<LiveJobIndexCodec>(&job.job_id)?;
    Ok(())
}

fn job_point_limit() -> PointReadLimit {
    PointReadLimit::new(BRANCH_HANDOFF_JOB_RECORD_LIMIT + 4)
        .expect("durable job point limit is nonzero")
}

fn request_point_limit() -> PointReadLimit {
    PointReadLimit::new(REQUEST_IDEMPOTENCY_RECORD_LIMIT + 4)
        .expect("request-idempotency point limit is nonzero")
}

fn small_point_limit() -> PointReadLimit {
    PointReadLimit::new(64).expect("durable job index point limit is nonzero")
}
