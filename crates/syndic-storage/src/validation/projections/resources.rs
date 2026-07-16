use beryl_home_store::DomainReader;

use crate::validation::scan::{point, require, scan};
use crate::{
    CanonicalItemPayload, ProjectionPayload, ProjectionResourceIndexRecord, ResourceBacking,
    codec::*, domain::SyndicDomain, error::SyndicValidationError,
};

use super::invariant;

pub(super) fn validate_metadata(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ResourcesFamily>(reader, |key, resource| {
        if *key != resource.id() {
            return invariant("resource key and identity disagree");
        }
        let item = require::<CanonicalItemsFamily>(
            reader,
            &resource.item_id(),
            "resource owner item is missing",
        )?;
        if let ResourceBacking::GeneratedMedia(_) = resource.backing() {
            if resource.projection_id().is_some()
                || resource.ordinal().is_some()
                || !matches!(
                    item.payload(),
                    CanonicalItemPayload::GeneratedMedia(id) if *id == resource.id()
                )
            {
                return invariant("generated resource owner or projection boundary disagrees");
            }
            return Ok(());
        }
        let projection_id = resource
            .projection_id()
            .ok_or(SyndicValidationError::Invariant(
                "text resource omitted its projection owner",
            ))?;
        let ordinal = resource.ordinal().ok_or(SyndicValidationError::Invariant(
            "text resource omitted its projection ordinal",
        ))?;
        let digest = resource.digest().ok_or(SyndicValidationError::Invariant(
            "text resource omitted its digest",
        ))?;
        let projection = require::<ProjectionsFamily>(
            reader,
            &projection_id,
            "resource owner projection is missing",
        )?;
        if projection.item_id() != resource.item_id() {
            return invariant("resource source item disagrees");
        }
        if !matches!(
            projection.payload(),
            ProjectionPayload::ResourceReference {
                resource_id,
                source_range,
                ..
            } if *resource_id == resource.id()
                && Some(*source_range) == resource.backing().range()
        ) {
            return invariant("resource owner projection payload disagrees");
        }
        let index_key = ProjectionResourceKey {
            owner: projection_id,
            ordinal,
        };
        let expected = ProjectionResourceIndexRecord::new(
            projection_id,
            ordinal,
            resource.id(),
            resource.revision(),
            *digest,
        );
        if require::<ProjectionResourcesFamily>(
            reader,
            &index_key,
            "projection-resource index is missing",
        )? != expected
        {
            return invariant("projection-resource index disagrees");
        }
        Ok(())
    })
}

pub(super) fn validate_indexes(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ProjectionResourcesFamily>(reader, |key, index| {
        if key.owner != index.projection_id()
            || key.ordinal != index.ordinal()
            || index.ordinal() != crate::ResourceOrdinal::FIRST
        {
            return invariant("projection-resource key or V1 ordinal disagrees");
        }
        let resource = require::<ResourcesFamily>(
            reader,
            &index.resource_id(),
            "projection-resource target is missing",
        )?;
        if resource.projection_id() != Some(index.projection_id())
            || resource.ordinal() != Some(index.ordinal())
            || resource.revision() != index.resource_revision()
            || resource.digest().copied() != Some(index.resource_digest())
        {
            return invariant("projection-resource target disagrees");
        }
        Ok(())
    })?;
    scan::<ProjectionsFamily>(reader, |_, projection| {
        let key = ProjectionResourceKey {
            owner: projection.id(),
            ordinal: crate::ResourceOrdinal::FIRST,
        };
        let expects_resource = matches!(
            projection.payload(),
            ProjectionPayload::ResourceReference { .. }
        );
        if expects_resource != point::<ProjectionResourcesFamily>(reader, &key)?.is_some() {
            return invariant("projection resource zero frontier disagrees");
        }
        Ok(())
    })
}
