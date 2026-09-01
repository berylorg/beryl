use std::{error::Error, fmt};

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainReader, DomainValidator, HomeStore, ReadError,
};
use beryl_model::SyndicThreadId;

use super::{
    BranchHandoffJobRecord, DurableJobDomain, DurableJobState, JobRecordCodec,
    LatestAttemptIndexCodec, LatestBranchHandoffAttempt, LiveJobIndexCodec, job_point_limit,
    small_point_limit,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadReuseJobGuard {
    thread_id: SyndicThreadId,
    latest: Option<LatestBranchHandoffAttempt>,
    job: Option<BranchHandoffJobRecord>,
}

impl ThreadReuseJobGuard {
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
}

pub(super) fn thread_reuse_guard(
    state: &DurableJobState,
    store: &HomeStore,
    thread_id: SyndicThreadId,
) -> Result<Option<ThreadReuseJobGuard>, ThreadReuseJobGuardError> {
    let latest = store.read_point::<DurableJobDomain, LatestAttemptIndexCodec>(
        &state.handle,
        &thread_id,
        small_point_limit(),
    )?;
    let Some(latest) = latest else {
        return Ok(Some(ThreadReuseJobGuard {
            thread_id,
            latest: None,
            job: None,
        }));
    };
    let job = store
        .read_point::<DurableJobDomain, JobRecordCodec>(
            &state.handle,
            &latest.job_id(),
            job_point_limit(),
        )?
        .ok_or(ThreadReuseJobGuardError::LatestJobMissing { thread_id })?;
    validate_latest(thread_id, latest, &job)?;
    if job.lifecycle().is_live() {
        return Ok(None);
    }
    let live = store.read_point::<DurableJobDomain, LiveJobIndexCodec>(
        &state.handle,
        &job.job_id(),
        job_point_limit(),
    )?;
    if live.is_some() {
        return Err(ThreadReuseJobGuardError::TerminalJobStillLive { thread_id });
    }
    Ok(Some(ThreadReuseJobGuard {
        thread_id,
        latest: Some(latest),
        job: Some(job),
    }))
}

impl DomainValidator<DurableJobDomain> for ThreadReuseJobGuard {
    type Error = ThreadReuseJobGuardError;

    fn validate(&self, reader: &DomainReader<'_, DurableJobDomain>) -> Result<(), Self::Error> {
        let latest =
            reader.point::<LatestAttemptIndexCodec>(&self.thread_id, small_point_limit())?;
        if latest != self.latest {
            return Err(ThreadReuseJobGuardError::SourceChanged {
                thread_id: self.thread_id,
            });
        }
        let Some(expected_job) = &self.job else {
            return Ok(());
        };
        let job = reader.point::<JobRecordCodec>(&expected_job.job_id(), job_point_limit())?;
        if job.as_ref() != Some(expected_job) {
            return Err(ThreadReuseJobGuardError::SourceChanged {
                thread_id: self.thread_id,
            });
        }
        let latest = self
            .latest
            .ok_or(ThreadReuseJobGuardError::LatestJobMissing {
                thread_id: self.thread_id,
            })?;
        validate_latest(self.thread_id, latest, expected_job)?;
        if expected_job.lifecycle().is_live() {
            return Err(ThreadReuseJobGuardError::LiveJob {
                thread_id: self.thread_id,
            });
        }
        let live = reader.point::<LiveJobIndexCodec>(&expected_job.job_id(), job_point_limit())?;
        if live.is_some() {
            return Err(ThreadReuseJobGuardError::TerminalJobStillLive {
                thread_id: self.thread_id,
            });
        }
        Ok(())
    }
}

fn validate_latest(
    thread_id: SyndicThreadId,
    latest: LatestBranchHandoffAttempt,
    job: &BranchHandoffJobRecord,
) -> Result<(), ThreadReuseJobGuardError> {
    if job.discussion_thread_id() != thread_id
        || job.job_id() != latest.job_id()
        || job.attempt_ordinal() != latest.attempt_ordinal()
    {
        return Err(ThreadReuseJobGuardError::LatestJobMismatch { thread_id });
    }
    Ok(())
}

#[derive(Debug)]
pub enum ThreadReuseJobGuardError {
    Read(ReadError),
    LatestJobMissing { thread_id: SyndicThreadId },
    LatestJobMismatch { thread_id: SyndicThreadId },
    LiveJob { thread_id: SyndicThreadId },
    TerminalJobStillLive { thread_id: SyndicThreadId },
    SourceChanged { thread_id: SyndicThreadId },
}

impl fmt::Display for ThreadReuseJobGuardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::LatestJobMissing { thread_id } => {
                write!(formatter, "latest durable job is missing for {thread_id}")
            }
            Self::LatestJobMismatch { thread_id } => {
                write!(formatter, "latest durable job disagrees for {thread_id}")
            }
            Self::LiveJob { thread_id } => {
                write!(formatter, "thread {thread_id} has a live durable job")
            }
            Self::TerminalJobStillLive { thread_id } => {
                write!(
                    formatter,
                    "terminal durable job remains live for {thread_id}"
                )
            }
            Self::SourceChanged { thread_id } => {
                write!(formatter, "durable job reuse guard changed for {thread_id}")
            }
        }
    }
}

impl Error for ThreadReuseJobGuardError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            _ => None,
        }
    }
}

impl DomainCallbackError for ThreadReuseJobGuardError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for ThreadReuseJobGuardError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}
