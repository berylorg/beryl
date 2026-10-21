use super::invariant;
use crate::validation::scan::require;
use crate::{
    CanonicalItemKind, CanonicalItemRecord, ItemProjectionSetRecord, ItemProjectionSource,
    ProjectionPayload, ProjectionRecord, ResourceBacking, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};
use beryl_home_store::DomainReader;
use beryl_model::SyndicResourceId;

pub(super) fn validate_projection(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &CanonicalItemRecord,
    projection: &ProjectionRecord,
    resource_id: SyndicResourceId,
) -> Result<(), SyndicValidationError> {
    let resource = require::<ResourcesFamily>(
        reader,
        &resource_id,
        "generated projection resource is missing",
    )?;
    let (id, revision) = crate::projection::generated_resource_projection_identity(
        crate::ProjectionFormatVersion::V1,
        item.id(),
        resource_id,
    );
    if item.kind() != CanonicalItemKind::GeneratedMedia
        || item.projection_source() != Some(ItemProjectionSource::GeneratedResource(resource_id))
        || resource.item_id() != item.id()
        || resource.projection_id().is_some()
        || resource.ordinal().is_some()
        || !matches!(resource.backing(), ResourceBacking::GeneratedMedia(_))
        || projection.id() != id
        || projection.revision() != revision
        || projection.item_id() != item.id()
        || projection.turn_id() != item.turn_id()
        || projection.ordinal().get() != 1
        || projection.payload() != &(ProjectionPayload::GeneratedResource { resource_id })
    {
        return invariant("generated projection identity or canonical ownership disagrees");
    }
    Ok(())
}

pub(super) fn validate_set(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &CanonicalItemRecord,
    set: &ItemProjectionSetRecord,
) -> Result<(), SyndicValidationError> {
    let ItemProjectionSource::GeneratedResource(resource_id) = set.source() else {
        return invariant("generated projection set has text source");
    };
    if set.item_id() != item.id()
        || set.source_item_revision() > item.revision()
        || item.projection_source() != Some(set.source())
        || set.text_state().is_some()
        || set.projection_count() != 1
        || set.resource_count() != 0
    {
        return invariant("generated projection set source or frontiers disagree");
    }
    super::source::validate_generated_projection_snapshot(
        reader,
        item,
        set.source_item_revision(),
    )?;
    let ordinal = crate::ProjectionOrdinal::new(1).expect("initial ordinal");
    let member = crate::membership::point(reader, set, ordinal)?.ok_or(
        SyndicValidationError::Invariant("generated projection membership is missing"),
    )?;
    let projection = require::<ProjectionsFamily>(
        reader,
        &member.projection_id(),
        "generated projection is missing",
    )?;
    validate_projection(reader, item, &projection, resource_id)?;
    let digest = crate::projection::advance_item_set_digest(
        crate::projection::item_set_digest_seed(),
        projection.id(),
        projection.revision(),
    );
    if member.item_id() != item.id()
        || member.generation() != set.generation()
        || member.ordinal() != ordinal
        || member.projection_revision() != projection.revision()
        || set.digest() != digest
    {
        return invariant("generated projection set membership or digest disagrees");
    }
    Ok(())
}
