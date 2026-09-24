use beryl_home_store::{CursorPage, HomeCandidateRecoveryAccess};

use super::*;

#[derive(Debug)]
pub enum DurableJobReadError {
    Read(ReadError),
    InvalidLiveEntry { key: JobId },
}

impl fmt::Display for DurableJobReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(formatter),
            Self::InvalidLiveEntry { key } => write!(
                formatter,
                "live handoff entry {key} has a contradictory identity or lifecycle"
            ),
        }
    }
}
impl Error for DurableJobReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ReadError> for DurableJobReadError {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

impl DurableJobState {
    pub fn revision_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
    ) -> Result<DomainRevision, ReadError> {
        access.domain_revision(&self.handle)
    }

    pub fn job_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        job_id: JobId,
    ) -> Result<Option<BranchHandoffJobRecord>, ReadError> {
        access.read_point::<DurableJobDomain, JobRecordCodec>(
            &self.handle,
            &job_id,
            job_point_limit(),
        )
    }

    pub fn request_admission_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        request: &ResolutionRequestIdentity,
    ) -> Result<Option<ResolutionRequestAdmission>, ReadError> {
        access.read_point::<DurableJobDomain, RequestIdempotencyIndexCodec>(
            &self.handle,
            &RequestIndexKey::new(request.clone()),
            request_point_limit(),
        )
    }

    pub fn latest_attempt_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        discussion_thread_id: SyndicThreadId,
    ) -> Result<Option<LatestBranchHandoffAttempt>, ReadError> {
        access.read_point::<DurableJobDomain, LatestAttemptIndexCodec>(
            &self.handle,
            &discussion_thread_id,
            small_point_limit(),
        )
    }

    pub fn list_live_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        after: Option<JobId>,
        limits: CursorReadLimits,
    ) -> Result<StatePage<BranchHandoffJobRecord>, DurableJobReadError> {
        let page = access.read_cursor::<DurableJobDomain, LiveJobIndexCodec>(
            &self.handle,
            &live_range(after),
            CursorDirection::Forward,
            limits,
        )?;
        live_page(page)
    }
}

pub(super) fn live_range(after: Option<JobId>) -> CursorRange<JobId> {
    let start = after.unwrap_or_else(|| JobId::from_bytes([0; 16]));
    let end = JobId::from_bytes([u8::MAX; 16]);
    if after.is_some() {
        CursorRange::after(start, end)
    } else {
        CursorRange::closed(start, end)
    }
}

pub(super) fn live_page(
    page: CursorPage<JobId, BranchHandoffJobRecord>,
) -> Result<StatePage<BranchHandoffJobRecord>, DurableJobReadError> {
    let stored_bytes = page.stored_bytes();
    let decoded_bytes = page.decoded_bytes();
    let has_more = page.has_more();
    let records = page
        .into_records()
        .into_iter()
        .map(|entry| {
            let (key, record) = entry.into_parts();
            if key != record.job_id() || !record.lifecycle().is_live() {
                return Err(DurableJobReadError::InvalidLiveEntry { key });
            }
            Ok(record)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(StatePage {
        records,
        stored_bytes,
        decoded_bytes,
        has_more,
    })
}
