mod prepare;
mod staging;

use beryl_home_store::HomeStore;

use crate::{
    ProviderFrameStageCallback, ProviderItemBuildRecord, SyndicPointReadLimit, SyndicStorage,
};

use super::{
    PreparedProviderObservationFrame, ProviderObservationFramePreparationError,
    ProviderObservationFramePreparationPlan, ProviderObservationFrameStageError,
    ProviderObservationReplay,
};

pub(super) fn normalized_item_kind(
    begin: super::super::ProviderObservationBegin,
) -> crate::ProviderItemKind {
    crate::provider_observation_item_kind(begin)
}

pub(super) fn observation_summary(
    reader: &super::replay::ObservationReplayReader<'_>,
) -> Result<crate::ProviderFrameObservationSummaryV1, ProviderObservationFramePreparationError> {
    prepare::observation_summary(reader)
}

pub(super) fn prepare(
    storage: &SyndicStorage,
    store: &HomeStore,
    replay: &ProviderObservationReplay,
    plan: ProviderObservationFramePreparationPlan,
    limit: SyndicPointReadLimit,
) -> Result<ProviderItemBuildRecord, ProviderObservationFramePreparationError> {
    prepare::prepare(storage, store, replay, plan, limit)
}

/// Replays one prepared immutable observation and offers exact bounded staging batches.
pub fn stage_provider_observation_frame<C: ProviderFrameStageCallback>(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedProviderObservationFrame,
    current: ProviderItemBuildRecord,
    limit: SyndicPointReadLimit,
    callback: &mut C,
) -> Result<ProviderItemBuildRecord, ProviderObservationFrameStageError<C::Error>> {
    staging::stage(storage, store, prepared, current, limit, callback)
}
