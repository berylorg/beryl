use beryl_home_store::DomainReader;
use beryl_model::SyndicItemId;

use crate::{
    ItemProjectionGeneration, ProjectionResourceIndexRecord, ResourceMetadataRecord, codec::*,
    domain::SyndicDomain, error::SyndicValidationError,
};

use super::super::invariant;
use crate::validation::scan::require;

#[derive(Clone, Copy)]
pub(super) enum Membership {
    Stable,
    Suffix(ItemProjectionGeneration),
}

pub(super) fn validate_projection_membership(
    reader: &DomainReader<'_, SyndicDomain>,
    item: SyndicItemId,
    generation: ItemProjectionGeneration,
    membership: Membership,
    projection: &crate::ProjectionRecord,
) -> Result<(), SyndicValidationError> {
    let stored = require::<ProjectionsFamily>(
        reader,
        &projection.id(),
        "replayed projection record is missing",
    )?;
    if stored != *projection {
        return invariant("replayed projection record disagrees");
    }
    match membership {
        Membership::Stable => {
            let index = require::<StableItemProjectionsFamily>(
                reader,
                &StableItemProjectionKey {
                    item,
                    ordinal: projection.ordinal(),
                },
                "replayed stable projection membership is missing",
            )?;
            if index.projection_id() != projection.id()
                || index.projection_revision() != projection.revision()
            {
                return invariant("replayed stable projection membership disagrees");
            }
        }
        Membership::Suffix(expected_generation) => {
            if expected_generation != generation {
                return invariant("replayed projection suffix generation disagrees");
            }
            let index = require::<ItemProjectionsFamily>(
                reader,
                &ItemProjectionKey {
                    item,
                    generation,
                    ordinal: projection.ordinal(),
                },
                "replayed projection suffix membership is missing",
            )?;
            if index.projection_id() != projection.id()
                || index.projection_revision() != projection.revision()
            {
                return invariant("replayed projection suffix membership disagrees");
            }
        }
    }
    Ok(())
}

pub(super) fn validate_resource_replay(
    reader: &DomainReader<'_, SyndicDomain>,
    resource: &ResourceMetadataRecord,
    index: &ProjectionResourceIndexRecord,
) -> Result<(), SyndicValidationError> {
    if require::<ResourcesFamily>(
        reader,
        &resource.id(),
        "replayed projection resource is missing",
    )? != *resource
    {
        return invariant("replayed projection resource disagrees");
    }
    let key = ProjectionResourceKey {
        owner: index.projection_id(),
        ordinal: index.ordinal(),
    };
    if require::<ProjectionResourcesFamily>(
        reader,
        &key,
        "replayed projection resource index is missing",
    )? != *index
    {
        return invariant("replayed projection resource index disagrees");
    }
    Ok(())
}
