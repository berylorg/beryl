use super::*;
use crate::discussion_settlement::{
    DiscussionSettlementAudit, DiscussionSettlementAuditOutcome, PreparedDiscussionSettlement,
    presentation::{RetryOrigin, RetryOriginal},
};
use beryl_home_store::CommandCancellation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscussionRetryObservation {
    Committed,
    NotCommitted,
    Pending,
    Unavailable,
}

pub struct PreparedDiscussionRetry {
    prepared: PreparedDiscussionSettlement<'static>,
    service: DiscussionSettlementService,
    origin: RetryOrigin,
}

pub struct DiscussionRetryOutcome {
    audit: DiscussionSettlementAudit,
    original: Arc<RetryOriginal>,
    observation: DiscussionRetryObservation,
}

impl DiscussionPresentationReader {
    pub fn prepare_retry(
        &self,
        descriptor: &DiscussionRetryDescriptor,
        cancellation: CommandCancellation,
    ) -> Result<PreparedDiscussionRetry, DiscussionPresentationError> {
        if descriptor.home != self.home
            || descriptor.generation != self.generation
            || descriptor.service_generation != self.service_generation
            || !self.current()
        {
            return Err(DiscussionPresentationError::Retired);
        }
        descriptor.fence.elect(|| ())?;
        let source = self
            .source
            .upgrade()
            .ok_or(DiscussionPresentationError::Retired)?;
        let service = source
            .service()
            .ok_or(DiscussionPresentationError::Retired)?;
        let observed = self.observe(descriptor.thread)?;
        if observed.retry_pending() || observed.retry() != Some(descriptor) {
            return Err(DiscussionPresentationError::Changed);
        }
        let origin = RetryOrigin::new(self.source.clone(), service.home())?;
        let prepared = service.prepare_retry(
            descriptor.attempt.job,
            descriptor.job_revision,
            cancellation,
        )?;
        Ok(PreparedDiscussionRetry {
            prepared,
            service,
            origin,
        })
    }

    pub fn reconcile_retry(
        &self,
        outcome: &DiscussionRetryOutcome,
    ) -> Result<DiscussionRetryObservation, DiscussionPresentationError> {
        let source = self
            .source
            .upgrade()
            .ok_or(DiscussionPresentationError::Retired)?;
        if !source.current() {
            return Err(DiscussionPresentationError::Retired);
        }
        let service = source
            .service()
            .ok_or(DiscussionPresentationError::Retired)?;
        let (state, storage, _) = service.presentation_parts();
        Ok(classify(outcome.audit.reconcile(
            service.home(),
            storage,
            state,
        )?))
    }
}

impl PreparedDiscussionRetry {
    pub fn audit(&self) -> DiscussionSettlementAudit {
        self.prepared.audit()
    }

    pub fn execute(self) -> DiscussionRetryOutcome {
        let audit = self.prepared.audit();
        let original = Arc::new(RetryOriginal::from_outcome(
            self.prepared.execute(),
            self.origin,
        ));
        let retained = audit.retain_original(original.clone());
        let (state, storage, _) = self.service.presentation_parts();
        let observation = if retained.is_err() {
            DiscussionRetryObservation::Unavailable
        } else {
            audit
                .reconcile(self.service.home(), storage, state)
                .map(classify)
                .unwrap_or(DiscussionRetryObservation::Pending)
        };
        DiscussionRetryOutcome {
            audit,
            original,
            observation,
        }
    }
}

impl DiscussionRetryOutcome {
    pub fn observation(&self) -> DiscussionRetryObservation {
        self.observation
    }
    pub fn has_original_failure(&self) -> bool {
        self.original.has_failure()
    }
}

fn classify(outcome: DiscussionSettlementAuditOutcome) -> DiscussionRetryObservation {
    match outcome {
        DiscussionSettlementAuditOutcome::Pending => DiscussionRetryObservation::Pending,
        DiscussionSettlementAuditOutcome::Collision => DiscussionRetryObservation::Unavailable,
        DiscussionSettlementAuditOutcome::NotCommitted => DiscussionRetryObservation::NotCommitted,
        DiscussionSettlementAuditOutcome::Settled(_) => DiscussionRetryObservation::Committed,
    }
}
