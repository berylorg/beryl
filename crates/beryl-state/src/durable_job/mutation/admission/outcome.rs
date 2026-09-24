use super::super::super::super::ResolutionRequestIdentity;
use super::*;

impl DurableJobState {
    pub fn handoff_job_admission_status(
        &self,
        store: &HomeStore,
        witness: &HandoffJobAdmissionWitness,
    ) -> Result<HandoffJobAdmissionStatus, DurableJobMutationError> {
        self.admission_status(ReadAccess::Ordinary(store), witness)
    }
    pub fn handoff_job_admission_status_candidate(
        &self,
        candidate: &HomeCandidateRecoveryAccess<'_>,
        witness: &HandoffJobAdmissionWitness,
    ) -> Result<HandoffJobAdmissionStatus, DurableJobMutationError> {
        self.admission_status(ReadAccess::Candidate(candidate), witness)
    }
    fn admission_status(
        &self,
        access: ReadAccess<'_>,
        witness: &HandoffJobAdmissionWitness,
    ) -> Result<HandoffJobAdmissionStatus, DurableJobMutationError> {
        let revision = access.revision(&self.handle)?;
        if access.home_id() != witness.0.home_id {
            return Err(DurableJobMutationError::Invariant(
                "handoff admission witness belongs to another home",
            ));
        }
        let result = classify(&access.reader(&self.handle), witness);
        access.confirm(&self.handle, revision)?;
        result
    }
    pub fn admitted_handoff_request(
        &self,
        store: &HomeStore,
        request: &ResolutionRequestIdentity,
    ) -> Result<Option<BranchHandoffJobRecord>, DurableJobMutationError> {
        self.admitted_request(ReadAccess::Ordinary(store), request)
    }
    pub fn admitted_handoff_request_candidate(
        &self,
        candidate: &HomeCandidateRecoveryAccess<'_>,
        request: &ResolutionRequestIdentity,
    ) -> Result<Option<BranchHandoffJobRecord>, DurableJobMutationError> {
        self.admitted_request(ReadAccess::Candidate(candidate), request)
    }
    fn admitted_request(
        &self,
        access: ReadAccess<'_>,
        request: &ResolutionRequestIdentity,
    ) -> Result<Option<BranchHandoffJobRecord>, DurableJobMutationError> {
        let revision = access.revision(&self.handle)?;
        let reader = access.reader(&self.handle);
        let result = (|| {
            let Some(admission) = reader.point::<RequestIdempotencyIndexCodec>(
                &RequestIndexKey::new(request.clone()),
                request_point_limit(),
            )?
            else {
                return Ok(None);
            };
            let job = read_identity(&reader, admission.job_id())?;
            if job.request() != request || admission != ResolutionRequestAdmission::from_job(&job) {
                return Err(DurableJobMutationError::Invariant(
                    "request admission disagrees with its historical job",
                ));
            }
            Ok(Some(job))
        })();
        access.confirm(&self.handle, revision)?;
        result
    }
}

fn classify(
    reader: &impl Reader,
    witness: &HandoffJobAdmissionWitness,
) -> Result<HandoffJobAdmissionStatus, DurableJobMutationError> {
    let expected = witness.new_job();
    if let Some(prior) = &witness.0.prior {
        if reader
            .point::<JobRecordCodec>(&prior.job_id(), job_point_limit())?
            .as_ref()
            != Some(prior)
            || reader
                .point::<LiveJobIndexCodec>(&prior.job_id(), job_point_limit())?
                .is_some()
            || reader.point::<RequestIdempotencyIndexCodec>(
                &RequestIndexKey::new(prior.request().clone()),
                request_point_limit(),
            )? != Some(ResolutionRequestAdmission::from_job(prior))
            || reader.point::<DiscussionAttemptIndexCodec>(
                &DiscussionAttemptKey::new(prior.discussion_thread_id(), prior.attempt_ordinal()),
                small_point_limit(),
            )? != Some(prior.job_id())
        {
            return Ok(HandoffJobAdmissionStatus::Collision);
        }
    }
    let job = reader.point::<JobRecordCodec>(&expected.job_id(), job_point_limit())?;
    let live = reader.point::<LiveJobIndexCodec>(&expected.job_id(), job_point_limit())?;
    let request = reader.point::<RequestIdempotencyIndexCodec>(
        &RequestIndexKey::new(expected.request().clone()),
        request_point_limit(),
    )?;
    let attempt = reader.point::<DiscussionAttemptIndexCodec>(
        &DiscussionAttemptKey::new(expected.discussion_thread_id(), expected.attempt_ordinal()),
        small_point_limit(),
    )?;
    let latest = reader
        .point::<LatestAttemptIndexCodec>(&expected.discussion_thread_id(), small_point_limit())?;
    Ok(
        if job.is_none()
            && live.is_none()
            && request.is_none()
            && attempt.is_none()
            && latest
                == witness
                    .0
                    .prior
                    .as_ref()
                    .map(LatestBranchHandoffAttempt::from_job)
        {
            HandoffJobAdmissionStatus::ExactOld
        } else if job.as_ref() == Some(expected)
            && live.as_ref() == Some(expected)
            && request == Some(ResolutionRequestAdmission::from_job(expected))
            && attempt == Some(expected.job_id())
            && latest == Some(LatestBranchHandoffAttempt::from_job(expected))
        {
            HandoffJobAdmissionStatus::ExactNew
        } else {
            HandoffJobAdmissionStatus::Collision
        },
    )
}
