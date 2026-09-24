use super::*;
use beryl_home_store::{
    DomainHandle, HomeCandidateRecoveryAccess, HomeStore, ReadError, RecordCodec,
};
use beryl_model::{BerylHomeId, DomainRevision};

pub(super) trait Reader {
    fn point<R: RecordCodec<DurableJobDomain>>(
        &self,
        key: &R::Key,
        limit: PointReadLimit,
    ) -> Result<Option<R::Value>, ReadError>;
}

impl Reader for DomainReader<'_, DurableJobDomain> {
    fn point<R: RecordCodec<DurableJobDomain>>(
        &self,
        key: &R::Key,
        limit: PointReadLimit,
    ) -> Result<Option<R::Value>, ReadError> {
        self.point::<R>(key, limit)
    }
}

pub(super) fn read_authenticated(
    reader: &impl Reader,
    job_id: JobId,
) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
    let job = read_identity(reader, job_id)?;
    let latest = reader
        .point::<LatestAttemptIndexCodec>(&job.discussion_thread_id(), small_point_limit())?;
    if !job.lifecycle().is_live() || latest != Some(LatestBranchHandoffAttempt::from_job(&job)) {
        return Err(DurableJobMutationError::Invariant(
            "handoff job latest-attempt closure disagrees",
        ));
    }
    Ok(job)
}

pub(super) fn read_identity(
    reader: &impl Reader,
    job_id: JobId,
) -> Result<BranchHandoffJobRecord, DurableJobMutationError> {
    let job = reader
        .point::<JobRecordCodec>(&job_id, job_point_limit())?
        .ok_or(DurableJobMutationError::JobMissing { job_id })?;
    let live = reader.point::<LiveJobIndexCodec>(&job_id, job_point_limit())?;
    let request = reader.point::<RequestIdempotencyIndexCodec>(
        &RequestIndexKey::new(job.request().clone()),
        request_point_limit(),
    )?;
    let attempt = reader.point::<DiscussionAttemptIndexCodec>(
        &DiscussionAttemptKey::new(job.discussion_thread_id(), job.attempt_ordinal()),
        small_point_limit(),
    )?;
    if job.job_id() != job_id
        || live.as_ref() != job.lifecycle().is_live().then_some(&job)
        || request != Some(ResolutionRequestAdmission::from_job(&job))
        || attempt != Some(job_id)
    {
        return Err(DurableJobMutationError::Invariant(
            "handoff job index closure disagrees",
        ));
    }
    Ok(job)
}

#[derive(Clone, Copy)]
pub(super) enum ReadAccess<'a> {
    Ordinary(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}

pub(super) struct AccessReader<'a> {
    access: ReadAccess<'a>,
    handle: &'a DomainHandle<DurableJobDomain>,
}

impl Reader for AccessReader<'_> {
    fn point<R: RecordCodec<DurableJobDomain>>(
        &self,
        key: &R::Key,
        limit: PointReadLimit,
    ) -> Result<Option<R::Value>, ReadError> {
        match self.access {
            ReadAccess::Ordinary(store) => {
                store.read_point::<DurableJobDomain, R>(self.handle, key, limit)
            }
            ReadAccess::Candidate(access) => {
                access.read_point::<DurableJobDomain, R>(self.handle, key, limit)
            }
        }
    }
}

impl ReadAccess<'_> {
    pub(super) fn home_id(self) -> BerylHomeId {
        match self {
            Self::Ordinary(store) => store.home_id(),
            Self::Candidate(access) => access.home_id(),
        }
    }
    pub(super) fn revision(
        self,
        handle: &DomainHandle<DurableJobDomain>,
    ) -> Result<DomainRevision, ReadError> {
        match self {
            Self::Ordinary(store) => store.domain_revision(handle),
            Self::Candidate(access) => access.domain_revision(handle),
        }
    }
    pub(super) fn confirm(
        self,
        handle: &DomainHandle<DurableJobDomain>,
        expected: DomainRevision,
    ) -> Result<(), DurableJobMutationError> {
        if self.revision(handle)? != expected {
            return Err(DurableJobMutationError::Invariant(
                "handoff job source revision changed",
            ));
        }
        Ok(())
    }
    pub(super) fn reader<'a>(self, handle: &'a DomainHandle<DurableJobDomain>) -> AccessReader<'a>
    where
        Self: 'a,
    {
        AccessReader {
            access: self,
            handle,
        }
    }
    pub(super) fn outcome(
        self,
        handle: &DomainHandle<DurableJobDomain>,
        witness: &HandoffJobTransitionWitness,
    ) -> Result<HandoffJobTransitionStatus, DurableJobMutationError> {
        let reader = self.reader(handle);
        let key = witness.old_job().job_id();
        let job = reader.point::<JobRecordCodec>(&key, job_point_limit())?;
        let live = reader.point::<LiveJobIndexCodec>(&key, job_point_limit())?;
        Ok(
            if job.as_ref() == Some(witness.old_job()) && live.as_ref() == Some(witness.old_job()) {
                HandoffJobTransitionStatus::ExactOld
            } else if job.as_ref() == Some(witness.new_job())
                && live.as_ref()
                    == witness
                        .new_job()
                        .lifecycle()
                        .is_live()
                        .then_some(witness.new_job())
            {
                HandoffJobTransitionStatus::ExactNew
            } else {
                HandoffJobTransitionStatus::Collision
            },
        )
    }
}
