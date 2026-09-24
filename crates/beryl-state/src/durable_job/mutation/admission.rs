use super::super::{BranchHandoffJobLifecycle, DurableJobState, ResolutionAttemptOrdinal};
use super::{
    access::{ReadAccess, Reader, read_identity},
    *,
};
use beryl_home_store::{
    DomainHandle, HomeCandidateRecoveryAccess, HomeStore, MutationContribution,
};
use beryl_model::{BerylHomeId, DomainRevision};
use std::sync::Arc;

mod outcome;

#[derive(Clone)]
pub struct HandoffJobAdmissionWitness(Arc<AdmissionRecords>);

struct AdmissionRecords {
    home_id: BerylHomeId,
    job: BranchHandoffJobRecord,
    prior: Option<BranchHandoffJobRecord>,
}

impl HandoffJobAdmissionWitness {
    pub fn new_job(&self) -> &BranchHandoffJobRecord {
        &self.0.job
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandoffJobAdmissionStatus {
    ExactOld,
    ExactNew,
    Collision,
}

pub struct PreparedHandoffJobAdmission {
    handle: DomainHandle<DurableJobDomain>,
    revision: DomainRevision,
    witness: HandoffJobAdmissionWitness,
}

impl PreparedHandoffJobAdmission {
    pub fn witness(&self) -> &HandoffJobAdmissionWitness {
        &self.witness
    }
    pub fn contribution(self) -> MutationContribution {
        self.handle.clone().contribution(self.revision, self)
    }
}

impl DurableJobState {
    pub fn prepare_handoff_job_admission(
        &self,
        store: &HomeStore,
        admission: BranchHandoffJobAdmission,
    ) -> Result<PreparedHandoffJobAdmission, DurableJobMutationError> {
        let access = ReadAccess::Ordinary(store);
        let revision = access.revision(&self.handle)?;
        let job = BranchHandoffJobRecord::initial(&admission);
        let prior = validate_new(&access.reader(&self.handle), &job);
        access.confirm(&self.handle, revision)?;
        Ok(PreparedHandoffJobAdmission {
            handle: self.handle.clone(),
            revision,
            witness: HandoffJobAdmissionWitness(Arc::new(AdmissionRecords {
                home_id: access.home_id(),
                job,
                prior: prior?,
            })),
        })
    }
}

impl DomainMutation<DurableJobDomain> for PreparedHandoffJobAdmission {
    type Error = DurableJobMutationError;
    type Prepared = HandoffJobAdmissionWitness;
    fn prepare(
        self,
        reader: &DomainReader<'_, DurableJobDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if validate_new(reader, self.witness.new_job())? != self.witness.0.prior {
            return Err(DurableJobMutationError::Invariant(
                "handoff admission predecessor changed",
            ));
        }
        Ok(self.witness)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        reserve(reservation)
    }
    fn contribute(
        witness: Self::Prepared,
        mutations: &mut MutationBuilder<'_, DurableJobDomain>,
    ) -> Result<(), Self::Error> {
        write(witness.new_job(), mutations)
    }
}

pub(super) fn validate_new(
    reader: &impl Reader,
    job: &BranchHandoffJobRecord,
) -> Result<Option<BranchHandoffJobRecord>, DurableJobMutationError> {
    if let Some(existing) = reader.point::<RequestIdempotencyIndexCodec>(
        &RequestIndexKey::new(job.request().clone()),
        request_point_limit(),
    )? {
        return Err(DurableJobMutationError::RequestAlreadyAdmitted { existing });
    }
    if reader
        .point::<JobRecordCodec>(&job.job_id(), job_point_limit())?
        .is_some()
    {
        return Err(DurableJobMutationError::JobAlreadyExists {
            job_id: job.job_id(),
        });
    }
    if reader
        .point::<LiveJobIndexCodec>(&job.job_id(), job_point_limit())?
        .is_some()
    {
        return Err(DurableJobMutationError::Invariant(
            "live-job index exists without its authoritative job record",
        ));
    }
    let key = DiscussionAttemptKey::new(job.discussion_thread_id(), job.attempt_ordinal());
    if reader
        .point::<DiscussionAttemptIndexCodec>(&key, small_point_limit())?
        .is_some()
    {
        return Err(DurableJobMutationError::AttemptAlreadyExists {
            discussion_thread_id: job.discussion_thread_id(),
            attempt_ordinal: job.attempt_ordinal(),
        });
    }
    let latest = reader
        .point::<LatestAttemptIndexCodec>(&job.discussion_thread_id(), small_point_limit())?;
    let prior = latest
        .map(|latest| {
            let prior = read_identity(reader, latest.job_id())?;
            if prior.discussion_thread_id() != job.discussion_thread_id()
                || LatestBranchHandoffAttempt::from_job(&prior) != latest
            {
                return Err(DurableJobMutationError::Invariant(
                    "latest-attempt index does not match its authoritative job",
                ));
            }
            match prior.lifecycle() {
                BranchHandoffJobLifecycle::TerminalFailed => Ok(prior),
                BranchHandoffJobLifecycle::Succeeded => {
                    Err(DurableJobMutationError::SuccessfulAttemptExists {
                        job_id: prior.job_id(),
                    })
                }
                _ => Err(DurableJobMutationError::LiveAttemptExists {
                    job_id: prior.job_id(),
                }),
            }
        })
        .transpose()?;
    let expected = match &prior {
        Some(prior) => prior.attempt_ordinal().checked_next()?,
        None => ResolutionAttemptOrdinal::FIRST,
    };
    if job.attempt_ordinal() != expected {
        return Err(DurableJobMutationError::AttemptOrdinalConflict {
            expected,
            actual: job.attempt_ordinal(),
        });
    }
    Ok(prior)
}

pub(super) fn reserve(
    reservation: &mut ReconciliationReservation<'_, DurableJobDomain>,
) -> Result<(), DurableJobMutationError> {
    reservation.reserve_records::<JobRecordCodec>(1)?;
    reservation.reserve_records::<LiveJobIndexCodec>(1)?;
    reservation.reserve_records::<RequestIdempotencyIndexCodec>(1)?;
    reservation.reserve_records::<DiscussionAttemptIndexCodec>(1)?;
    reservation.reserve_records::<LatestAttemptIndexCodec>(1)?;
    Ok(())
}
pub(super) fn write(
    job: &BranchHandoffJobRecord,
    mutations: &mut MutationBuilder<'_, DurableJobDomain>,
) -> Result<(), DurableJobMutationError> {
    mutations.put::<JobRecordCodec>(&job.job_id(), job)?;
    mutations.put::<LiveJobIndexCodec>(&job.job_id(), job)?;
    mutations.put::<RequestIdempotencyIndexCodec>(
        &RequestIndexKey::new(job.request().clone()),
        &ResolutionRequestAdmission::from_job(job),
    )?;
    mutations.put::<DiscussionAttemptIndexCodec>(
        &DiscussionAttemptKey::new(job.discussion_thread_id(), job.attempt_ordinal()),
        &job.job_id(),
    )?;
    mutations.put::<LatestAttemptIndexCodec>(
        &job.discussion_thread_id(),
        &LatestBranchHandoffAttempt::from_job(job),
    )?;
    Ok(())
}
