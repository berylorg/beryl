use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, PointReadLimit, ReconciliationReservation,
};
use beryl_model::{JobId, JobRevision, SyndicThreadId};

use super::{
    codec::{
        DiscussionAttemptIndexCodec, DiscussionAttemptKey, JobRecordCodec, LatestAttemptIndexCodec,
        LiveJobIndexCodec, RequestIdempotencyIndexCodec, RequestIndexKey,
    },
    BranchHandoffJobAdmission, BranchHandoffJobRecord, DurableJobDomain, DurableJobMutationError,
    LatestBranchHandoffAttempt, ResolutionRequestAdmission, BRANCH_HANDOFF_JOB_RECORD_LIMIT,
    REQUEST_IDEMPOTENCY_RECORD_LIMIT,
};

mod transition;

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

    fn validate(&self, reader: &DomainReader<'_, DurableJobDomain>) -> Result<(), Self::Error> {
        let job_id = self.admission.job_id();
        if let Some(existing) = read_request(reader, self.admission.request())? {
            return Err(DurableJobMutationError::RequestAlreadyAdmitted { existing });
        }
        if read_job(reader, job_id)?.is_some() {
            return Err(DurableJobMutationError::JobAlreadyExists { job_id });
        }
        if read_live_job(reader, job_id)?.is_some() {
            return Err(DurableJobMutationError::Invariant(
                "live-job index exists without its authoritative job record",
            ));
        }

        let attempt_key = DiscussionAttemptKey::new(
            self.admission.discussion_thread_id(),
            self.admission.attempt_ordinal(),
        );
        if read_attempt(reader, attempt_key)?.is_some() {
            return Err(DurableJobMutationError::AttemptAlreadyExists {
                discussion_thread_id: self.admission.discussion_thread_id(),
                attempt_ordinal: self.admission.attempt_ordinal(),
            });
        }

        let expected_ordinal = match read_latest(reader, self.admission.discussion_thread_id())? {
            None => super::ResolutionAttemptOrdinal::FIRST,
            Some(latest) => {
                expected_after_latest(reader, self.admission.discussion_thread_id(), latest)?
            }
        };
        if self.admission.attempt_ordinal() != expected_ordinal {
            return Err(DurableJobMutationError::AttemptOrdinalConflict {
                expected: expected_ordinal,
                actual: self.admission.attempt_ordinal(),
            });
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<JobRecordCodec>(1)?;
        reservation.reserve_records::<LiveJobIndexCodec>(1)?;
        reservation.reserve_records::<RequestIdempotencyIndexCodec>(1)?;
        reservation.reserve_records::<DiscussionAttemptIndexCodec>(1)?;
        reservation.reserve_records::<LatestAttemptIndexCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, DurableJobDomain>,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        let job = BranchHandoffJobRecord::initial(&self.admission);
        mutations.put::<JobRecordCodec>(&job.job_id, &job)?;
        mutations.put::<LiveJobIndexCodec>(&job.job_id, &job)?;
        mutations.put::<RequestIdempotencyIndexCodec>(
            &RequestIndexKey::new(job.request.clone()),
            &ResolutionRequestAdmission::from_job(&job),
        )?;
        mutations.put::<DiscussionAttemptIndexCodec>(
            &DiscussionAttemptKey::new(job.discussion_thread_id, job.attempt_ordinal),
            &job.job_id,
        )?;
        mutations.put::<LatestAttemptIndexCodec>(
            &job.discussion_thread_id,
            &LatestBranchHandoffAttempt::from_job(&job),
        )?;
        Ok(())
    }
}

fn expected_after_latest(
    reader: &DomainReader<'_, DurableJobDomain>,
    discussion_thread_id: SyndicThreadId,
    latest: LatestBranchHandoffAttempt,
) -> Result<super::ResolutionAttemptOrdinal, DurableJobMutationError> {
    let prior = required_job(reader, latest.job_id())?;
    if prior.discussion_thread_id != discussion_thread_id
        || prior.attempt_ordinal != latest.attempt_ordinal()
    {
        return Err(DurableJobMutationError::Invariant(
            "latest-attempt index does not match its authoritative job",
        ));
    }
    match prior.lifecycle() {
        super::BranchHandoffJobLifecycle::TerminalFailed => {
            prior.attempt_ordinal.checked_next().map_err(Into::into)
        }
        super::BranchHandoffJobLifecycle::Succeeded => {
            Err(DurableJobMutationError::SuccessfulAttemptExists {
                job_id: prior.job_id,
            })
        }
        _ => Err(DurableJobMutationError::LiveAttemptExists {
            job_id: prior.job_id,
        }),
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

fn read_live_job(
    reader: &DomainReader<'_, DurableJobDomain>,
    job_id: JobId,
) -> Result<Option<BranchHandoffJobRecord>, DurableJobMutationError> {
    reader
        .point::<LiveJobIndexCodec>(&job_id, job_point_limit())
        .map_err(Into::into)
}

fn read_request(
    reader: &DomainReader<'_, DurableJobDomain>,
    request: &super::ResolutionRequestIdentity,
) -> Result<Option<ResolutionRequestAdmission>, DurableJobMutationError> {
    reader
        .point::<RequestIdempotencyIndexCodec>(
            &RequestIndexKey::new(request.clone()),
            request_point_limit(),
        )
        .map_err(Into::into)
}

fn read_attempt(
    reader: &DomainReader<'_, DurableJobDomain>,
    key: DiscussionAttemptKey,
) -> Result<Option<JobId>, DurableJobMutationError> {
    reader
        .point::<DiscussionAttemptIndexCodec>(&key, small_point_limit())
        .map_err(Into::into)
}

fn read_latest(
    reader: &DomainReader<'_, DurableJobDomain>,
    discussion_thread_id: SyndicThreadId,
) -> Result<Option<LatestBranchHandoffAttempt>, DurableJobMutationError> {
    reader
        .point::<LatestAttemptIndexCodec>(&discussion_thread_id, small_point_limit())
        .map_err(Into::into)
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
