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
    generation: beryl_home_store::HomeGeneration,
    pending: syndic_storage::PendingDispatchEvidence,
    binding: beryl_model::ExecutionBinding,
}

#[must_use]
pub struct ReservedDiscussionNondispatch {
    reservation: DiscussionParentDispatchReservation,
    proof: Arc<RetainedNondispatch>,
    _owner: Arc<()>,
}

pub(super) struct RetainedNondispatch {
    pub(super) home_id: BerylHomeId,
    pub(super) generation: beryl_home_store::HomeGeneration,
    pub(super) job_id: JobId,
    pub(super) job_revision: JobRevision,
    pub(super) evidence: DiscussionParentNondispatch,
    pub(super) flight: Arc<Flight>,
    pub(super) owner: Weak<()>,
    pub(super) last: Mutex<Weak<Attempt>>,
}

impl DiscussionSettlementService {
    pub fn reserve_parent_dispatch(
        &self,
        job_id: JobId,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Result<DiscussionParentDispatchReservation, DiscussionSettlementError> {
        self.reserve_parent(job_id, thread_id, turn_id, false)?
            .ok_or(DiscussionSettlementError::IdentityMismatch)
    }

    pub(crate) fn reserve_parent_preparation(
        &self,
        job_id: JobId,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
    ) -> Result<Option<DiscussionParentDispatchReservation>, DiscussionSettlementError> {
        self.reserve_parent(job_id, thread_id, turn_id, true)
    }

    fn reserve_parent(
        &self,
        job_id: JobId,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        allow_retryable: bool,
    ) -> Result<Option<DiscussionParentDispatchReservation>, DiscussionSettlementError> {
        let permit = self.operations.permit();
        permit.commit(|| ())?;
        let generation = self
            .store
            .health()
            .generation()
            .ok_or(DiscussionSettlementError::ForeignHome)?;
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
            || !(job.lifecycle() == BranchHandoffJobLifecycle::StartingParent
                || (allow_retryable
                    && job.lifecycle() == BranchHandoffJobLifecycle::RetryableFailed))
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
        let pending = pending.ok_or(DiscussionSettlementError::IdentityMismatch)?;
        if pending.thread_id() != thread_id || pending.turn_id() != turn_id {
            return Err(DiscussionSettlementError::IdentityMismatch);
        }
        let execution = self
            .syndic
            .thread_execution(
                &self.store,
                thread_id,
                SyndicPointReadLimit::new(400_000).expect("bounded execution binding evidence"),
            )?
            .ok_or(DiscussionSettlementError::IdentityMismatch)?;
        if execution.thread_id() != thread_id {
            return Err(DiscussionSettlementError::IdentityMismatch);
        }
        if access.revision()? != revision {
            return Err(DiscussionSettlementError::ConcurrentChange);
        }
        permit.commit(|| ())?;
        if job.lifecycle() == BranchHandoffJobLifecycle::RetryableFailed {
            return Ok(None);
        }
        Ok(Some(DiscussionParentDispatchReservation {
            service: self.clone(),
            job_id,
            job_revision: job.revision(),
            thread_id,
            turn_id,
            flight,
            permit,
            generation,
            pending,
            binding: execution.execution().clone(),
        }))
    }
}

impl DiscussionParentDispatchReservation {
    pub(crate) fn execution_binding(&self) -> &beryl_model::ExecutionBinding {
        &self.binding
    }
    pub(crate) fn preparation_failed(
        self,
        failure: super::nondispatch::DiscussionPreparationFailure,
    ) -> Result<ReservedDiscussionNondispatch, DiscussionSettlementError> {
        let evidence = DiscussionParentNondispatch::before_activation(
            self.pending,
            self.binding.clone(),
            failure,
        );
        self.nondispatched(evidence)
    }

    #[cfg(feature = "test-faults")]
    pub fn preparation_failed_for_test(
        self,
        kind: beryl_state::HandoffFailureKind,
    ) -> Result<ReservedDiscussionNondispatch, DiscussionSettlementError> {
        use super::nondispatch::DiscussionPreparationFailure;
        let failure = match kind {
            beryl_state::HandoffFailureKind::RuntimeUnavailable => {
                DiscussionPreparationFailure::Runtime
            }
            beryl_state::HandoffFailureKind::RootUnavailable => DiscussionPreparationFailure::Root,
            beryl_state::HandoffFailureKind::CasUnavailable => DiscussionPreparationFailure::Cas,
            _ => return Err(DiscussionSettlementError::IdentityMismatch),
        };
        self.preparation_failed(failure)
    }

    pub fn nondispatched(
        self,
        evidence: DiscussionParentNondispatch,
    ) -> Result<ReservedDiscussionNondispatch, DiscussionSettlementError> {
        if evidence.thread_id() != self.thread_id || evidence.turn_id() != self.turn_id {
            return Err(DiscussionSettlementError::IdentityMismatch);
        }
        let owner = Arc::new(());
        let proof = Arc::new(RetainedNondispatch {
            home_id: self.service.store.home_id(),
            generation: self.generation,
            job_id: self.job_id,
            job_revision: self.job_revision,
            evidence,
            flight: Arc::clone(&self.flight),
            owner: Arc::downgrade(&owner),
            last: Mutex::new(Weak::new()),
        });
        self.flight
            .custody()?
            .retain_nondispatch(self.job_id, Arc::clone(&proof));
        Ok(ReservedDiscussionNondispatch {
            reservation: self,
            proof,
            _owner: owner,
        })
    }
}

impl ReservedDiscussionNondispatch {
    pub fn prepare(
        &mut self,
        cancellation: CommandCancellation,
    ) -> Result<PreparedDiscussionSettlement<'_>, DiscussionSettlementError> {
        let mut last = self
            .proof
            .last
            .lock()
            .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
        if last.upgrade().is_some() {
            return Err(DiscussionSettlementError::DuplicateIdentity);
        }
        let reservation = &self.reservation;
        let prepared = super::prepare::prepare_reserved(
            &reservation.service,
            reservation.job_id,
            reservation.job_revision,
            self.proof.evidence.clone(),
            cancellation,
            Arc::clone(&reservation.flight),
            reservation.permit.clone(),
        )?;
        *last = Arc::downgrade(&prepared.audit.0);
        Ok(prepared)
    }
}
