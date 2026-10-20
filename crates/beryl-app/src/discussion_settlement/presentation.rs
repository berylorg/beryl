use super::*;
use beryl_model::{DiscussionContextOwnerId, JobRevision, ResolutionIntentId, SyndicThreadId};
use beryl_state::ResolutionAttemptOrdinal;
use std::sync::{Mutex, TryLockError, Weak};

pub(crate) struct LocalBoundary(Mutex<Option<u64>>);

impl LocalBoundary {
    pub(super) fn new() -> Self {
        Self(Mutex::new(Some(0)))
    }

    pub(super) fn mutate<T>(&self, mutation: impl FnOnce() -> T) -> T {
        let mut revision = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let result = mutation();
        *revision = revision.and_then(|value| value.checked_add(1));
        result
    }

    pub(super) fn mutate_at<T>(
        &self,
        expected: u64,
        mutation: impl FnOnce() -> T,
    ) -> Result<T, DiscussionSettlementError> {
        let mut revision = self
            .0
            .lock()
            .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
        if *revision != Some(expected) {
            return Err(DiscussionSettlementError::ConcurrentChange);
        }
        let result = mutation();
        *revision = revision.and_then(|value| value.checked_add(1));
        Ok(result)
    }

    pub(crate) fn revision(&self) -> Result<u64, DiscussionSettlementError> {
        self.0
            .try_lock()
            .map_err(|_| DiscussionSettlementError::ConcurrentChange)?
            .ok_or(DiscussionSettlementError::CustodyUnavailable)
    }

    pub(crate) fn elect<T>(
        &self,
        revision: u64,
        publish: impl FnOnce() -> T,
    ) -> Result<T, DiscussionSettlementError> {
        let current = self
            .0
            .try_lock()
            .map_err(|_| DiscussionSettlementError::ConcurrentChange)?;
        if *current != Some(revision) {
            return Err(DiscussionSettlementError::ConcurrentChange);
        }
        Ok(publish())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalDisposition {
    Prepared,
    Pending,
    Committed,
    NotCommitted,
    Collision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LocalFact {
    pub home: BerylHomeId,
    pub child: SyndicThreadId,
    pub parent: SyndicThreadId,
    pub owner: DiscussionContextOwnerId,
    pub job: JobId,
    pub intent: ResolutionIntentId,
    pub attempt: ResolutionAttemptOrdinal,
    pub job_revision: JobRevision,
    pub disposition: LocalDisposition,
    pub admission: bool,
}

impl DiscussionSettlementAudit {
    pub(super) fn register(self) -> Self {
        let mut active = self
            .0
            ._flight
            .flights
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let job = self.job_record().job_id();
        if let Some(slot) = active.jobs.get_mut(&job) {
            self.mutate(|| slot.audit = Some(Arc::downgrade(&self.0)));
        }
        drop(active);
        self
    }

    pub(super) fn mutate<T>(&self, mutation: impl FnOnce() -> T) -> T {
        self.0._flight.flights.presentation.mutate(mutation)
    }

    fn job_record(&self) -> &beryl_state::BranchHandoffJobRecord {
        match &self.0.job {
            JobWitness::Admission(witness) => witness.new_job(),
            JobWitness::Transition(witness) => witness.new_job(),
        }
    }

    pub(super) fn fact(&self) -> Result<LocalFact, DiscussionSettlementError> {
        let disposition = match self.0.disposition.try_lock() {
            Ok(disposition) => disposition,
            Err(TryLockError::WouldBlock) => {
                return Err(DiscussionSettlementError::ConcurrentChange);
            }
            Err(TryLockError::Poisoned(_)) => {
                return Err(DiscussionSettlementError::CustodyUnavailable);
            }
        };
        let job = self.job_record();
        Ok(LocalFact {
            home: self.0.home_id,
            child: job.discussion_thread_id(),
            parent: job.parent_thread_id(),
            owner: job.context_owner_id(),
            job: job.job_id(),
            intent: job.intent_id(),
            attempt: job.attempt_ordinal(),
            job_revision: job.revision(),
            disposition: match &*disposition {
                Disposition::Prepared => LocalDisposition::Prepared,
                Disposition::NotCommitted => LocalDisposition::NotCommitted,
                Disposition::Committed => LocalDisposition::Committed,
                Disposition::Indeterminate(_) => LocalDisposition::Pending,
                Disposition::Collision => LocalDisposition::Collision,
            },
            admission: matches!(&self.0.job, JobWitness::Admission(_)),
        })
    }

    pub(crate) fn retain_original(
        &self,
        original: Arc<RetryOriginal>,
    ) -> Result<(), DiscussionSettlementError> {
        let mut retained = self
            .0
            .retry_original
            .lock()
            .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
        self.mutate(|| *retained = Some(original));
        drop(retained);
        let custody = self.0._flight.custody()?;
        self.0._flight.retain(&custody, self.clone());
        Ok(())
    }
}

impl DiscussionSettlementOperations {
    fn presentation_audits(&self) -> Result<Vec<Weak<Attempt>>, DiscussionSettlementError> {
        let active = self
            .flights
            .active
            .try_lock()
            .map_err(|_| DiscussionSettlementError::ConcurrentChange)?;
        Ok(active
            .jobs
            .values()
            .filter_map(|slot| slot.audit.clone())
            .collect())
    }

    pub(crate) fn selected_presentation_audit(
        &self,
        child: SyndicThreadId,
    ) -> Result<Option<DiscussionSettlementAudit>, DiscussionSettlementError> {
        let mut result = None;
        for slot in self.presentation_audits()? {
            let Some(attempt) = slot.upgrade() else {
                continue;
            };
            let audit = DiscussionSettlementAudit(attempt);
            if audit.job_record().discussion_thread_id() == child
                && !matches!(audit.fact()?.disposition, LocalDisposition::Prepared)
            {
                if result.is_some() {
                    return Err(DiscussionSettlementError::IdentityMismatch);
                }
                result = Some(audit);
            }
        }
        Ok(result)
    }
    pub(crate) fn presentation_boundary(&self) -> Weak<LocalBoundary> {
        Arc::downgrade(&self.flights.presentation)
    }

    pub(crate) fn presentation_preparing(
        &self,
        child: SyndicThreadId,
        job: Option<JobId>,
    ) -> Result<bool, DiscussionSettlementError> {
        let active = self
            .flights
            .active
            .try_lock()
            .map_err(|_| DiscussionSettlementError::ConcurrentChange)?;
        Ok(active.jobs.iter().any(|(id, slot)| {
            slot.audit.is_none() && (slot.child == Some(child) || job == Some(*id))
        }))
    }

    pub(crate) fn presentation_fact(
        &self,
        child: SyndicThreadId,
    ) -> Result<Option<LocalFact>, DiscussionSettlementError> {
        let mut result: Option<LocalFact> = None;
        for slot in self.presentation_audits()? {
            let Some(attempt) = slot.upgrade() else {
                continue;
            };
            let audit = DiscussionSettlementAudit(attempt);
            if audit.job_record().discussion_thread_id() != child {
                continue;
            }
            let fact = audit.fact()?;
            if let Some(prior) = result {
                if prior.job != fact.job {
                    return Err(DiscussionSettlementError::IdentityMismatch);
                }
            }
            result = Some(fact);
        }
        Ok(result)
    }
}

impl DiscussionSettlementService {
    pub(crate) fn home(&self) -> &HomeStore {
        &self.store
    }
    pub(crate) fn presentation_parts(
        &self,
    ) -> (&BerylState, &SyndicStorage, &DiscussionSettlementOperations) {
        (&self.state, &self.syndic, &self.operations)
    }
}

pub(crate) struct RetryOrigin {
    source: Weak<dyn crate::discussion_presentation::DiscussionPresentationAccess>,
    home: BerylHomeId,
    path: std::path::PathBuf,
    generation: beryl_home_store::HomeGenerationIdentity,
}

impl RetryOrigin {
    pub(crate) fn new(
        source: Weak<dyn crate::discussion_presentation::DiscussionPresentationAccess>,
        store: &HomeStore,
    ) -> Result<Self, beryl_home_store::ReadError> {
        Ok(Self {
            source,
            home: store.home_id(),
            path: store.canonical_path().to_owned(),
            generation: store.generation_identity()?,
        })
    }

    fn recovered_by(&self, candidate: &mut beryl_home_store::HomeRecoveryCandidate) -> bool {
        self.source.upgrade().is_none()
            && candidate.home_id() == self.home
            && candidate
                .recovery_access()
                .is_ok_and(|access| access.canonical_path() == self.path)
            && candidate
                .generation_identity()
                .is_ok_and(|identity| identity != self.generation)
    }
}

pub(crate) struct RetryOriginal {
    outcome: RetryOriginalOutcome,
    origin: RetryOrigin,
    recovered: std::sync::atomic::AtomicBool,
}

enum RetryOriginalOutcome {
    NotCommitted {
        evidence: DiscussionSettlementError,
    },
    Committed {
        result: DiscussionSettlementResult,
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
    },
    Indeterminate {
        failure: CommandError,
    },
}

impl RetryOriginal {
    pub(crate) fn from_outcome(outcome: DiscussionSettlementOutcome, origin: RetryOrigin) -> Self {
        let outcome = match outcome {
            DiscussionSettlementOutcome::NotCommitted { evidence } => {
                RetryOriginalOutcome::NotCommitted { evidence }
            }
            DiscussionSettlementOutcome::Committed {
                result,
                receipt,
                later_failure,
                local_finalization,
            } => RetryOriginalOutcome::Committed {
                result,
                receipt,
                later_failure,
                local_finalization,
            },
            DiscussionSettlementOutcome::Indeterminate { failure, .. } => {
                RetryOriginalOutcome::Indeterminate { failure }
            }
        };
        Self {
            outcome,
            origin,
            recovered: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub(crate) fn has_failure(&self) -> bool {
        match &self.outcome {
            RetryOriginalOutcome::NotCommitted { .. }
            | RetryOriginalOutcome::Indeterminate { .. } => true,
            RetryOriginalOutcome::Committed {
                later_failure,
                local_finalization,
                ..
            } => later_failure.is_some() || local_finalization.is_some(),
        }
    }

    fn holds_obligations(&self) -> bool {
        matches!(&self.outcome, RetryOriginalOutcome::Committed { later_failure, local_finalization, .. }
            if later_failure.is_some() || local_finalization.is_some())
            && !self.recovered.load(std::sync::atomic::Ordering::Acquire)
    }
}

impl DiscussionSettlementAudit {
    pub(super) fn original_allows_release(&self) -> Result<bool, DiscussionSettlementError> {
        Ok(self
            .0
            .retry_original
            .lock()
            .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?
            .as_ref()
            .is_none_or(|original| !original.holds_obligations()))
    }

    #[cfg(feature = "test-faults")]
    pub fn retains_retry_obligations_for_test(&self) -> bool {
        self.0.retry_original.lock().is_ok_and(|original| {
            original
                .as_ref()
                .is_some_and(|original| original.holds_obligations())
        })
    }
}

impl DiscussionSettlementOperations {
    pub fn qualify_recovered_originals(
        &self,
        candidate: &mut beryl_home_store::HomeRecoveryCandidate,
        state: &BerylState,
        syndic: &SyndicStorage,
    ) -> Result<(), DiscussionSettlementError> {
        for _ in 0..self.configured_slots().get() {
            let audit = {
                let retained = self
                    .retained
                    .audits
                    .lock()
                    .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
                retained
                    .values()
                    .find_map(|audit| match audit.original_allows_release() {
                        Ok(false) => Some(Ok(audit.clone())),
                        Ok(true) => None,
                        Err(error) => Some(Err(error)),
                    })
                    .transpose()?
            };
            let Some(audit) = audit else {
                return Ok(());
            };
            let original = audit
                .0
                .retry_original
                .lock()
                .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?
                .clone()
                .ok_or(DiscussionSettlementError::CustodyUnavailable)?;
            if !original.origin.recovered_by(candidate) {
                return Err(DiscussionSettlementError::CustodyUnavailable);
            }
            let access = candidate
                .recovery_access()
                .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
            match audit.reconcile_candidate(&access, syndic, state)? {
                DiscussionSettlementAuditOutcome::Settled(_) => {}
                _ => return Err(DiscussionSettlementError::CustodyUnavailable),
            }
            audit.mutate(|| {
                original
                    .recovered
                    .store(true, std::sync::atomic::Ordering::Release)
            });
            audit.0._flight.release_retained();
        }
        let remaining = self
            .retained
            .audits
            .lock()
            .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
        for audit in remaining.values() {
            if !audit.original_allows_release()? {
                return Err(DiscussionSettlementError::CustodyUnavailable);
            }
        }
        Ok(())
    }
}
