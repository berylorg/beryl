use super::{access::Access, *};
use beryl_model::{JobRevision, SyndicThreadId};
use beryl_state::BranchHandoffJobLifecycle;
use std::sync::Weak;
use syndic_storage::SyndicPointReadLimit;

#[must_use]
pub struct DiscussionParentDispatchReservation {
    service: DiscussionSettlementService,
    job_id: JobId,
    job_revision: JobRevision,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    flight: Arc<Flight>,
    permit: ProcessExecutionPermit,
}

#[must_use]
pub struct ReservedDiscussionNondispatch {
    reservation: DiscussionParentDispatchReservation,
    evidence: DiscussionParentNondispatch,
    last: Weak<Attempt>,
}

impl DiscussionSettlementService {
    pub fn reserve_parent_dispatch(
        &self,
        job_id: JobId,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Result<DiscussionParentDispatchReservation, DiscussionSettlementError> {
        let permit = self.operations.permit();
        permit.commit(|| ())?;
        let flight = Arc::new(self.operations.acquire(job_id)?);
        let access = Access::Ordinary(&self.store);
        let revision = access.revision()?;
        let job = access.job(&self.state, job_id)?;
        let authenticated = self
            .state
            .durable_jobs()
            .admitted_handoff_request(&self.store, job.request())?;
        let latest = self
            .state
            .durable_jobs()
            .latest_attempt(&self.store, job.discussion_thread_id())?;
        if authenticated.as_ref() != Some(&job)
            || latest.is_none_or(|latest| {
                latest.job_id() != job_id || latest.attempt_ordinal() != job.attempt_ordinal()
            })
        {
            return Err(DiscussionSettlementError::IdentityMismatch);
        }
        if job.job_id() != job_id
            || job.lifecycle() != BranchHandoffJobLifecycle::StartingParent
            || job.parent_thread_id() != thread_id
            || job
                .state()
                .parent()
                .is_none_or(|parent| parent.turn_id() != turn_id)
        {
            return Err(DiscussionSettlementError::IdentityMismatch);
        }
        super::candidate::validate_job_sources(access, &self.syndic, &job)?;
        let pending = self.syndic.pending_dispatch_evidence(
            &self.store,
            thread_id,
            SyndicPointReadLimit::new(400_000).expect("bounded parent dispatch evidence"),
        )?;
        if pending
            .is_none_or(|pending| pending.thread_id() != thread_id || pending.turn_id() != turn_id)
        {
            return Err(DiscussionSettlementError::IdentityMismatch);
        }
        if access.revision()? != revision {
            return Err(DiscussionSettlementError::ConcurrentChange);
        }
        permit.commit(|| ())?;
        Ok(DiscussionParentDispatchReservation {
            service: self.clone(),
            job_id,
            job_revision: job.revision(),
            thread_id,
            turn_id,
            flight,
            permit,
        })
    }
}

impl DiscussionParentDispatchReservation {
    pub fn nondispatched(
        self,
        evidence: DiscussionParentNondispatch,
    ) -> Result<ReservedDiscussionNondispatch, DiscussionSettlementError> {
        if evidence.request.thread_id() != self.thread_id
            || evidence.request.turn_id() != self.turn_id
        {
            return Err(DiscussionSettlementError::IdentityMismatch);
        }
        Ok(ReservedDiscussionNondispatch {
            reservation: self,
            evidence,
            last: Weak::new(),
        })
    }
}

impl ReservedDiscussionNondispatch {
    pub fn prepare(
        &mut self,
        cancellation: CommandCancellation,
    ) -> Result<PreparedDiscussionSettlement<'_>, DiscussionSettlementError> {
        if self.last.upgrade().is_some() {
            return Err(DiscussionSettlementError::DuplicateIdentity);
        }
        let reservation = &self.reservation;
        let prepared = super::prepare::prepare_reserved(
            &reservation.service,
            reservation.job_id,
            reservation.job_revision,
            self.evidence.clone(),
            cancellation,
            Arc::clone(&reservation.flight),
            reservation.permit.clone(),
        )?;
        self.last = Arc::downgrade(&prepared.audit.0);
        Ok(prepared)
    }
}
