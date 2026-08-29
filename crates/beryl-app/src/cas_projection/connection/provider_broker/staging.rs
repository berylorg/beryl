use std::sync::atomic::AtomicBool;

use beryl_home_store::{CommandOutcome, HomeGeneration, HomeStore};
use beryl_model::{BerylHomeId, ProviderObservationId};
#[cfg(feature = "test-faults")]
use syndic_storage::ProviderObservationChunkPayload;
use syndic_storage::{
    ProviderObservationStageBatch, ProviderObservationStageCallback, SyndicStorage,
};

pub(super) struct StageCommitter<'a> {
    pub(super) home: &'a HomeStore,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) storage: &'a SyndicStorage,
    pub(super) identity: ProviderObservationId,
    pub(super) cancelled: &'a AtomicBool,
    pub(super) command: &'a crate::cas_projection::LiveCommandPermit,
    #[cfg(feature = "test-faults")]
    pub(super) test_metrics: &'a crate::cas_projection::test_faults::ProviderBrokerTestMetrics,
}

impl ProviderObservationStageCallback for StageCommitter<'_> {
    fn stage_batch(&mut self, batch: &ProviderObservationStageBatch) -> CommandOutcome {
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
        self.home.execute_current(
            self.storage
                .current_stage_provider_observation_batch(batch.clone()),
        )
    }
}
