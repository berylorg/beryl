use beryl_home_store::DomainReader;

use crate::{
    ItemProjectionBuildPhase, ItemProjectionBuildRecord, MarkdownParserCheckpoint,
    SyndicMutationError, codec::*, domain::SyndicDomain,
};

use super::AdvanceBuildRecords;
use crate::mutation::point;

impl AdvanceBuildRecords {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn finish(
        &mut self,
        reader: &DomainReader<'_, SyndicDomain>,
        item: &crate::CanonicalItemRecord,
        source_is_immutable: bool,
        resume_checkpoint: MarkdownParserCheckpoint,
        checkpoint: MarkdownParserCheckpoint,
        finished: bool,
        projection_count: u64,
        resource_count: u64,
        output_digest: [u8; 32],
    ) -> Result<(), SyndicMutationError> {
        let source = item
            .projection_source()
            .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
        if finished {
            if checkpoint.consumed_source_bytes() != self.build.source_bytes()
                || checkpoint.closed_source_bytes() != self.build.source_bytes()
                || !checkpoint.line_carry().is_empty()
                || checkpoint.open_block().is_some()
                || projection_count == 0
            {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
            let (stable_projection_count, stable_resource_count, stable_digest, resume_checkpoint) =
                if source_is_immutable {
                    (
                        projection_count,
                        resource_count,
                        output_digest,
                        checkpoint.clone(),
                    )
                } else {
                    (
                        self.build.projection_count(),
                        self.build.resource_count(),
                        self.build.output_digest(),
                        resume_checkpoint,
                    )
                };
            let set = crate::ItemProjectionSetRecord::new(
                item.id(),
                self.build.generation(),
                self.build.format(),
                item.revision(),
                source,
                self.build.source_bytes(),
                stable_projection_count,
                stable_resource_count,
                stable_digest,
                projection_count,
                resource_count,
                output_digest,
                resume_checkpoint,
                source_is_immutable,
            );
            if point::<ItemProjectionSetsFamily>(
                reader,
                &ItemProjectionSetKey {
                    item: item.id(),
                    generation: self.build.generation(),
                },
            )?
            .is_some()
            {
                return Err(SyndicMutationError::ProjectionIdentityCollision);
            }
            let revision = match point::<ItemProjectionHeadsFamily>(reader, &item.id())? {
                Some(head) => head.revision().checked_next()?,
                None => beryl_model::ProjectionRevision::new(1)
                    .expect("initial projection-head revision"),
            };
            self.head = Some(crate::ItemProjectionHeadRecord::new(
                item.id(),
                revision,
                item.revision(),
                self.build.generation(),
                crate::ProjectionLifecycle::Current,
            ));
            self.set = Some(set);
        } else {
            self.next_build = Some(ItemProjectionBuildRecord::new(
                self.build.item_id(),
                self.build.generation(),
                self.build.revision().checked_next()?,
                self.build.format(),
                self.build.source_item_revision(),
                self.build.source(),
                self.build.source_bytes(),
                projection_count,
                resource_count,
                output_digest,
                ItemProjectionBuildPhase::Parsing(checkpoint),
            ));
        }
        Ok(())
    }
}
