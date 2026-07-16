use beryl_home_store::{DomainReader, ReadError};

use crate::{
    ItemProjectionIndexRecord, ItemProjectionSetRecord, ProjectionOrdinal,
    codec::{
        ExactCodec, ItemProjectionKey, ItemProjectionsFamily, StableItemProjectionKey,
        StableItemProjectionsFamily, family_point_limit,
    },
    domain::SyndicDomain,
};

/// Resolves one logical item-projection ordinal across the immutable closed prefix and the
/// generation-owned EOF suffix without assembling either collection.
pub(crate) fn point(
    reader: &DomainReader<'_, SyndicDomain>,
    set: &ItemProjectionSetRecord,
    ordinal: ProjectionOrdinal,
) -> Result<Option<ItemProjectionIndexRecord>, ReadError> {
    if ordinal.get() > set.projection_count() {
        return Ok(None);
    }
    if ordinal.get() <= set.stable_projection_count() {
        return reader
            .point::<ExactCodec<StableItemProjectionsFamily>>(
                &StableItemProjectionKey {
                    item: set.item_id(),
                    ordinal,
                },
                family_point_limit::<StableItemProjectionsFamily>(),
            )
            .map(|record| {
                record.map(|record| {
                    ItemProjectionIndexRecord::new(
                        record.item_id(),
                        set.generation(),
                        record.ordinal(),
                        record.projection_id(),
                        record.projection_revision(),
                    )
                })
            });
    }
    reader.point::<ExactCodec<ItemProjectionsFamily>>(
        &ItemProjectionKey {
            item: set.item_id(),
            generation: set.generation(),
            ordinal,
        },
        family_point_limit::<ItemProjectionsFamily>(),
    )
}
