use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};
use beryl_model::ProjectionRevision;

use crate::{
    ItemProjectionBuildPhase, ItemProjectionBuildRecord, ProjectionFormatVersion,
    SyndicMutationError, codec::*, domain::SyndicDomain,
};

use super::{StartBuildMutation, lifecycle};
use crate::mutation::{point, required};

pub struct StartBuildRecords {
    build: ItemProjectionBuildRecord,
    superseded: Option<ItemProjectionBuildRecord>,
}

impl StartBuildMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<StartBuildRecords, SyndicMutationError> {
        let item = required::<CanonicalItemsFamily>(reader, &self.request.item_id)?;
        if item.revision() != self.request.expected_item_revision
            || item.projection_source().is_none()
        {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        let source = item
            .projection_source()
            .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
        lifecycle::validate_projection_source(reader, &item, source)?;
        if let Some(head) = point::<ItemProjectionHeadsFamily>(reader, &item.id())?
            && head.lifecycle() == crate::ProjectionLifecycle::Current
            && head.source_item_revision() == item.revision()
        {
            return Err(SyndicMutationError::ProjectionAlreadyCurrent);
        }
        let latest_build = lifecycle::latest_build(reader, item.id())?;
        let latest_set = lifecycle::latest_set(reader, item.id())?;
        let latest_generation = latest_build
            .as_ref()
            .map(ItemProjectionBuildRecord::generation)
            .into_iter()
            .chain(
                latest_set
                    .as_ref()
                    .map(crate::ItemProjectionSetRecord::generation),
            )
            .max();
        let expected_generation = match latest_generation {
            Some(generation) => generation.checked_next()?,
            None => crate::ItemProjectionGeneration::FIRST,
        };
        if self.request.generation != expected_generation {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        let seed = lifecycle::projection_seed(latest_build.as_ref(), latest_set.as_ref(), source)?;
        let superseded = match latest_build {
            Some(build) => match build.phase() {
                ItemProjectionBuildPhase::Parsing(checkpoint) => {
                    if build.source_item_revision() == item.revision() && build.source() == source {
                        return Err(SyndicMutationError::ProjectionBuildConflict);
                    }
                    Some(ItemProjectionBuildRecord::new(
                        build.item_id(),
                        build.generation(),
                        build.revision().checked_next()?,
                        build.format(),
                        build.source_item_revision(),
                        build.source(),
                        build.source_bytes(),
                        build.projection_count(),
                        build.resource_count(),
                        build.output_digest(),
                        ItemProjectionBuildPhase::Superseded(checkpoint.clone()),
                    ))
                }
                ItemProjectionBuildPhase::Superseded(_) => None,
            },
            None => None,
        };
        let build = ItemProjectionBuildRecord::new(
            item.id(),
            self.request.generation,
            ProjectionRevision::new(1).expect("initial build revision is nonzero"),
            ProjectionFormatVersion::V1,
            item.revision(),
            source,
            source.logical_utf8_bytes(),
            seed.projection_count,
            seed.resource_count,
            seed.output_digest,
            ItemProjectionBuildPhase::Parsing(seed.checkpoint),
        );
        Ok(StartBuildRecords { build, superseded })
    }
}

impl DomainMutation<SyndicDomain> for StartBuildMutation {
    type Error = SyndicMutationError;
    type Prepared = StartBuildRecords;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        self.records(reader)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ItemProjectionBuildsCodec>(2)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let records = prepared;
        if let Some(build) = records.superseded {
            mutations.put::<ItemProjectionBuildsCodec>(
                &ItemProjectionSetKey {
                    item: build.item_id(),
                    generation: build.generation(),
                },
                &build,
            )?;
        }
        mutations.put::<ItemProjectionBuildsCodec>(
            &ItemProjectionSetKey {
                item: records.build.item_id(),
                generation: records.build.generation(),
            },
            &records.build,
        )?;
        Ok(())
    }
}
