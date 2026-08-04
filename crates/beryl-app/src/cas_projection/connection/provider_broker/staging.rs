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
            self.await_current()?;
            if self.cancelled.load(Ordering::Acquire) {
                return Err(StageCommitError::Cancelled);
            }
            let verification = self
                .command
                .verification_join(self.home, self.home_id, self.home_generation)
                .map_err(|source| self.failure(StageCommitError::Authority(source)))?;
            match self.home.execute_current(
                self.storage
                    .current_stage_provider_observation_batch(batch.clone()),
            ) {
                Ok(_) => return Ok(()),
                Err(error) if ambiguous_stage_error(&error) => {
                    match verification.wait_after_ambiguous() {
                        Ok(true) => {}
                        Ok(false) => {
                            return Err(self.failure(StageCommitError::Command(Box::new(error))));
                        }
                        Err(source) => {
                            return Err(self.failure(StageCommitError::Authority(source)));
                        }
                    }
                    let current = self.read_current_build()?;
                    match batch.classify_current(current.as_ref()) {
                        ProviderObservationStageBatchState::Next => return Ok(()),
                        ProviderObservationStageBatchState::Expected => continue,
                        ProviderObservationStageBatchState::Conflict => {
                            return Err(StageCommitError::Conflict);
                        }
                    }
                }
                Err(error) => {
                    return Err(self.failure(StageCommitError::Command(Box::new(error))));
                }
            }
        }
    }
}

impl StageCommitter<'_> {
    fn await_current(&self) -> Result<(), StageCommitError> {
        if self.home.home_id() != self.home_id {
            return Err(self.failure(StageCommitError::HomeIdentity));
        }
        self.command
            .await_current_or_verification(self.home, self.home_id, self.home_generation)
            .map_err(|source| self.failure(StageCommitError::Authority(source)))
    }

    fn read_current_build(
        &self,
    ) -> Result<Option<ProviderObservationBuildRecord>, StageCommitError> {
        loop {
            self.await_current()?;
            let verification = self
                .command
                .verification_join(self.home, self.home_id, self.home_generation)
                .map_err(|source| self.failure(StageCommitError::Authority(source)))?;
            let current = self.storage.provider_observation_build(
                self.home,
                self.identity,
                provider_point_limit(),
            );
            match verification.wait_after_ambiguous() {
                Ok(true) => continue,
                Ok(false) => {
                    return current
                        .map_err(|source| self.failure(StageCommitError::Read(Box::new(source))));
                }
                Err(source) => {
                    return Err(self.failure(StageCommitError::Authority(source)));
                }
            }
        }
    }

    fn failure(&self, error: StageCommitError) -> StageCommitError {
        let _ = self.command.observe_persistent_failure();
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
    #[error("provider staging lost exact verification authority: {0}")]
    Authority(#[source] crate::cas_projection::LiveCommandAdmissionError),
    #[error("provider staging reconciliation read failed: {0}")]
    Read(#[source] Box<syndic_storage::SyndicReadError>),
    #[error("provider staging reconciliation found a conflicting frontier")]
    Conflict,
}
