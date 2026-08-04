use std::sync::atomic::{AtomicBool, Ordering};

use beryl_home_store::{CommandError, HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, ProviderObservationId};
#[cfg(feature = "test-faults")]
use syndic_storage::ProviderObservationChunkPayload;
use syndic_storage::{
    ProviderObservationBuildRecord, ProviderObservationStageBatch,
    ProviderObservationStageBatchState, ProviderObservationStageCallback, SyndicPointReadLimit,
    SyndicStorage,
};
use thiserror::Error;

use super::PROVIDER_POINT_READ_BYTES;

pub(super) struct StageCommitter<'a> {
    pub(super) home: &'a HomeStore,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) storage: SyndicStorage,
    pub(super) identity: ProviderObservationId,
    pub(super) cancelled: &'a AtomicBool,
    pub(super) command: &'a crate::cas_projection::LiveCommandPermit,
    #[cfg(feature = "test-faults")]
    pub(super) test_metrics: &'a crate::cas_projection::test_faults::ProviderBrokerTestMetrics,
}

impl ProviderObservationStageCallback for StageCommitter<'_> {
    type Error = StageCommitError;

    fn stage_batch(&mut self, batch: &ProviderObservationStageBatch) -> Result<(), Self::Error> {
        #[cfg(feature = "test-faults")]
        self.test_metrics.record_provider_staging_batch();
        #[cfg(feature = "test-faults")]
        let _staged_fragment = if batch.chunk().is_some_and(|chunk| {
            matches!(
                chunk.payload(),
                ProviderObservationChunkPayload::Fragment { .. }
            )
        }) {
            let guard = self.test_metrics.begin_staged_fragment();
            crate::cas_projection::test_faults::pause_provider_fragment_stage(
                crate::cas_projection::test_faults::ProviderTestKey::new(
                    self.home_id,
                    std::ptr::from_ref(self.cancelled) as usize,
                ),
                self.identity,
            );
            Some(guard)
        } else {
            None
        };
        loop {
            if self.home.home_id() != self.home_id {
                return Err(self.failure(StageCommitError::HomeIdentity));
            }
            let verification = self
                .command
                .await_current_or_verification(self.home, self.home_id, self.home_generation)
                .map_err(StageCommitError::Authority)?;
            if self.cancelled.load(Ordering::Acquire) {
                return Err(StageCommitError::Cancelled);
            }
            let dispatch = self.home.execute_current(
                self.storage
                    .current_stage_provider_observation_batch(batch.clone()),
            );
            let verified = verification
                .settle_after_operation()
                .map_err(StageCommitError::Authority)?
                .verified_current();
            let ambiguous = dispatch.as_ref().err().is_some_and(ambiguous_stage_error);
            if !verified && !ambiguous {
                return dispatch
                    .map(|_| ())
                    .map_err(|error| self.failure(StageCommitError::Command(Box::new(error))));
            }
            let current = self.read_current_build()?;
            match batch.classify_current(current.as_ref()) {
                ProviderObservationStageBatchState::Next => return Ok(()),
                ProviderObservationStageBatchState::Expected => match dispatch {
                    Ok(_) => {
                        return Err(self.failure(StageCommitError::ReportedSuccessWithoutAdvance));
                    }
                    Err(error) if ambiguous_stage_error(&error) => continue,
                    Err(error) => {
                        return Err(self.failure(StageCommitError::Command(Box::new(error))));
                    }
                },
                ProviderObservationStageBatchState::Conflict => {
                    return Err(StageCommitError::Conflict);
                }
            }
        }
    }
}

impl StageCommitter<'_> {
    fn read_current_build(
        &self,
    ) -> Result<Option<ProviderObservationBuildRecord>, StageCommitError> {
        loop {
            if self.home.home_id() != self.home_id {
                return Err(self.failure(StageCommitError::HomeIdentity));
            }
            let verification = self
                .command
                .await_current_or_verification(self.home, self.home_id, self.home_generation)
                .map_err(StageCommitError::Authority)?;
            let current = self.storage.provider_observation_build(
                self.home,
                self.identity,
                provider_point_limit(),
            );
            match verification.settle_after_operation() {
                Ok(settlement) if settlement.verified_current() => continue,
                Ok(_) => {
                    return current
                        .map_err(|source| self.failure(StageCommitError::Read(Box::new(source))));
                }
                Err(source) => return Err(StageCommitError::Authority(source)),
            }
        }
    }

    fn failure(&self, error: StageCommitError) -> StageCommitError {
        if !matches!(&error, StageCommitError::Authority(_)) {
            let _ = self.command.observe_persistent_failure();
        }
        error
    }
}

fn provider_point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(PROVIDER_POINT_READ_BYTES)
        .expect("provider point-read bound is nonzero")
}

fn ambiguous_stage_error(error: &CommandError) -> bool {
    matches!(
        error,
        CommandError::HealthGate(_)
            | CommandError::RevisionRead { .. }
            | CommandError::ContributorAccess { .. }
            | CommandError::Commit { .. }
            | CommandError::Persistence { .. }
    )
}

#[derive(Debug, Error)]
pub(super) enum StageCommitError {
    #[error("provider staging was cancelled")]
    Cancelled,
    #[error("provider staging home identity changed")]
    HomeIdentity,
    #[error("provider staging command failed before an ambiguous commit: {0}")]
    Command(#[source] Box<CommandError>),
    #[error("provider staging command reported success without a durable advance")]
    ReportedSuccessWithoutAdvance,
    #[error("provider staging lost exact verification authority: {0}")]
    Authority(#[source] crate::cas_projection::LiveCommandAdmissionError),
    #[error("provider staging reconciliation read failed: {0}")]
    Read(#[source] Box<syndic_storage::SyndicReadError>),
    #[error("provider staging reconciliation found a conflicting frontier")]
    Conflict,
}

impl StageCommitError {
    pub(super) fn authority(&self) -> Option<crate::cas_projection::LiveCommandAdmissionError> {
        match self {
            Self::Authority(source) => Some(*source),
            _ => None,
        }
    }
}
