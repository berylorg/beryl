mod finish;

use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use crate::{
    ItemProjectionBuildPhase, ItemProjectionBuildRecord, SyndicMutationError, codec::*,
    domain::SyndicDomain,
};

use super::{AdvanceBuildMutation, materialize, parser, range};
use crate::mutation::{point, required};

struct AdvanceBuildRecords {
    build: ItemProjectionBuildRecord,
    next_build: Option<ItemProjectionBuildRecord>,
    set: Option<crate::ItemProjectionSetRecord>,
    head: Option<crate::ItemProjectionHeadRecord>,
    projections: Vec<crate::ProjectionRecord>,
    stable_projection_indexes: Vec<crate::StableItemProjectionIndexRecord>,
    projection_indexes: Vec<crate::ItemProjectionIndexRecord>,
    resources: Vec<crate::ResourceMetadataRecord>,
    resource_indexes: Vec<crate::ProjectionResourceIndexRecord>,
}

impl AdvanceBuildMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<AdvanceBuildRecords, SyndicMutationError> {
        let key = ItemProjectionSetKey {
            item: self.request.item_id,
            generation: self.request.generation,
        };
        let build = required::<ItemProjectionBuildsFamily>(reader, &key)?;
        if build.revision() != self.request.expected_build_revision
            || !matches!(build.phase(), ItemProjectionBuildPhase::Parsing(_))
        {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        let item = required::<CanonicalItemsFamily>(reader, &build.item_id())?;
        let source_is_immutable =
            super::lifecycle::validate_projection_source(reader, &item, build.source())?;
        if item.revision() != build.source_item_revision()
            || item.projection_source() != Some(build.source())
        {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        let ItemProjectionBuildPhase::Parsing(checkpoint) = build.phase() else {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        };
        let resume_checkpoint = checkpoint.clone();
        let piece = range::load_piece(reader, &build, checkpoint)?;
        let step = parser::advance(checkpoint, piece)?;
        let outputs_are_stable = !step.finished || source_is_immutable;
        let mut records = AdvanceBuildRecords {
            build: build.clone(),
            next_build: None,
            set: None,
            head: None,
            projections: Vec::with_capacity(step.outputs.len()),
            stable_projection_indexes: Vec::with_capacity(step.outputs.len()),
            projection_indexes: Vec::with_capacity(step.outputs.len()),
            resources: Vec::new(),
            resource_indexes: Vec::new(),
        };
        let mut projection_count = build.projection_count();
        let mut resource_count = build.resource_count();
        let mut output_digest = build.output_digest();
        for output in step.outputs {
            projection_count = projection_count
                .checked_add(1)
                .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
            let ordinal = crate::ProjectionOrdinal::new(projection_count)?;
            let materialized = materialize::materialize_output(
                reader,
                &item,
                build.source(),
                build.format(),
                ordinal,
                output,
            )?;
            output_digest = crate::projection::advance_item_set_digest(
                output_digest,
                materialized.projection.id(),
                materialized.projection.revision(),
            );
            if outputs_are_stable {
                records.stable_projection_indexes.push(
                    crate::StableItemProjectionIndexRecord::new(
                        item.id(),
                        ordinal,
                        materialized.projection.id(),
                        materialized.projection.revision(),
                    ),
                );
            } else {
                records
                    .projection_indexes
                    .push(crate::ItemProjectionIndexRecord::new(
                        item.id(),
                        build.generation(),
                        ordinal,
                        materialized.projection.id(),
                        materialized.projection.revision(),
                    ));
            }
            if let Some((resource, index)) = materialized.resource {
                resource_count = resource_count
                    .checked_add(1)
                    .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
                output_digest = crate::projection::advance_item_set_resource_digest(
                    output_digest,
                    resource.id(),
                    resource.revision(),
                    *resource
                        .digest()
                        .ok_or(SyndicMutationError::ProjectionBuildConflict)?,
                );
                records.resources.push(resource);
                records.resource_indexes.push(index);
            }
            records.projections.push(materialized.projection);
        }
        records.finish(
            reader,
            &item,
            source_is_immutable,
            resume_checkpoint,
            step.checkpoint,
            step.finished,
            projection_count,
            resource_count,
            output_digest,
        )?;
        records.validate_collisions(reader)?;
        Ok(records)
    }
}

impl DomainMutation<SyndicDomain> for AdvanceBuildMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ProjectionsCodec>(64)?;
        reservation.reserve_records::<StableItemProjectionsCodec>(64)?;
        reservation.reserve_records::<ItemProjectionsCodec>(64)?;
        reservation.reserve_records::<ResourcesCodec>(64)?;
        reservation.reserve_records::<ProjectionResourcesCodec>(64)?;
        reservation.reserve_records::<ItemProjectionBuildsCodec>(1)?;
        reservation.reserve_records::<ItemProjectionSetsCodec>(1)?;
        reservation.reserve_records::<ItemProjectionHeadsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let records = self.records(reader)?;
        for projection in &records.projections {
            if point::<ProjectionsFamily>(reader, &projection.id())?.is_none() {
                mutations.put::<ProjectionsCodec>(&projection.id(), projection)?;
            }
        }
        for index in &records.stable_projection_indexes {
            mutations.put::<StableItemProjectionsCodec>(
                &StableItemProjectionKey {
                    item: index.item_id(),
                    ordinal: index.ordinal(),
                },
                index,
            )?;
        }
        for index in &records.projection_indexes {
            mutations.put::<ItemProjectionsCodec>(
                &ItemProjectionKey {
                    item: index.item_id(),
                    generation: index.generation(),
                    ordinal: index.ordinal(),
                },
                index,
            )?;
        }
        for (resource, index) in records.resources.iter().zip(&records.resource_indexes) {
            if point::<ResourcesFamily>(reader, &resource.id())?.is_none() {
                mutations.put::<ResourcesCodec>(&resource.id(), resource)?;
            }
            let key = ProjectionResourceKey {
                owner: index.projection_id(),
                ordinal: index.ordinal(),
            };
            if point::<ProjectionResourcesFamily>(reader, &key)?.is_none() {
                mutations.put::<ProjectionResourcesCodec>(&key, index)?;
            }
        }
        let build_key = ItemProjectionSetKey {
            item: records.build.item_id(),
            generation: records.build.generation(),
        };
        if let Some(build) = &records.next_build {
            mutations.put::<ItemProjectionBuildsCodec>(&build_key, build)?;
        } else {
            mutations.delete::<ItemProjectionBuildsCodec>(&build_key)?;
            let set = records
                .set
                .as_ref()
                .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
            let head = records
                .head
                .as_ref()
                .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
            mutations.put::<ItemProjectionSetsCodec>(&build_key, set)?;
            mutations.put::<ItemProjectionHeadsCodec>(&head.item_id(), head)?;
        }
        Ok(())
    }
}
