use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder};
use beryl_model::ProjectionRevision;

use crate::{
    CanonicalItemKind, ItemProjectionBuildPhase, ItemProjectionBuildRecord,
    ProjectionFormatVersion, SyndicMutationError, codec::*, domain::SyndicDomain,
};

use super::{StartBuildMutation, lifecycle};
use crate::mutation::{point, required};

struct StartBuildRecords {
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
            || !matches!(
                item.kind(),
                CanonicalItemKind::UserInput | CanonicalItemKind::AssistantMessage(_)
            )
        {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        let content = item
            .payload()
            .content()
            .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
        let manifest = required::<ContentManifestsFamily>(reader, &content.id())?;
        if matches!(manifest.lifecycle(), crate::ContentLifecycle::Building)
            || manifest.current_reference() != Some(content)
        {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
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
        let seed = lifecycle::projection_seed(latest_build.as_ref(), latest_set.as_ref(), content)?;
        let superseded = match latest_build {
            Some(build) => match build.phase() {
                ItemProjectionBuildPhase::Parsing(checkpoint) => {
                    if build.source_item_revision() == item.revision()
                        && build.source_content() == content
                    {
                        return Err(SyndicMutationError::ProjectionBuildConflict);
                    }
                    Some(ItemProjectionBuildRecord::new(
                        build.item_id(),
                        build.generation(),
                        build.revision().checked_next()?,
                        build.format(),
                        build.source_item_revision(),
                        build.source_content(),
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
            content,
            content.summary().logical_utf8_bytes(),
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

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let records = self.records(reader)?;
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
