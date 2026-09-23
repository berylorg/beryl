use beryl_home_store::{CursorPage, HomeCandidateRecoveryAccess};

use super::*;

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
    ) -> Result<StatePage<BranchHandoffJobRecord>, ReadError> {
        let page = access.read_cursor::<DurableJobDomain, LiveJobIndexCodec>(
            &self.handle,
            &live_range(after),
            CursorDirection::Forward,
            limits,
        )?;
        Ok(live_page(page))
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
) -> StatePage<BranchHandoffJobRecord> {
    let stored_bytes = page.stored_bytes();
    let decoded_bytes = page.decoded_bytes();
    let has_more = page.has_more();
    StatePage {
        records: page
            .into_records()
            .into_iter()
            .map(|entry| entry.into_parts().1)
            .collect(),
        stored_bytes,
        decoded_bytes,
        has_more,
    }
}
