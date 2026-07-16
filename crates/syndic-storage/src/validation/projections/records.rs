use beryl_home_store::DomainReader;

use crate::validation::scan::{require, scan};
use crate::{
    CanonicalItemKind, ProjectionPayload, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};

use super::invariant;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ProjectionsFamily>(reader, |key, projection| {
        if *key != projection.id() {
            return invariant("projection key and identity disagree");
        }
        let item = require::<CanonicalItemsFamily>(
            reader,
            &projection.item_id(),
            "projection source item is missing",
        )?;
        if item.turn_id() != projection.turn_id() {
            return invariant("projection turn disagrees with source item");
        }
        let content = item
            .payload()
            .content()
            .ok_or(SyndicValidationError::Invariant(
                "projection source item omitted canonical content",
            ))?;
        let manifest = require::<ContentManifestsFamily>(
            reader,
            &content.id(),
            "projection source content is missing",
        )?;
        match projection.payload() {
            ProjectionPayload::Empty => {}
            ProjectionPayload::InlineMarkdown {
                source_range,
                source,
                ..
            } => {
                if source_range.end() > manifest.expected().logical_utf8_bytes()
                    || source_range.len() != source.len() as u64
                {
                    return invariant("inline projection source range is invalid");
                }
            }
            ProjectionPayload::ResourceReference {
                source_range,
                resource_id,
                ..
            } => {
                if source_range.end() > manifest.expected().logical_utf8_bytes() {
                    return invariant("resource projection source range is invalid");
                }
                let resource = require::<ResourcesFamily>(
                    reader,
                    resource_id,
                    "projection resource metadata is missing",
                )?;
                if resource.projection_id() != Some(projection.id())
                    || resource.item_id() != projection.item_id()
                    || resource.backing().content_id() != Some(content.id())
                    || resource.backing().range() != Some(*source_range)
                {
                    return invariant("projection resource metadata disagrees");
                }
            }
            ProjectionPayload::ImageMarker { source_offset, .. } => {
                if !matches!(item.kind(), CanonicalItemKind::UserInput)
                    || *source_offset > manifest.expected().logical_utf8_bytes()
                {
                    return invariant("image-marker projection source is invalid");
                }
            }
        }
        Ok(())
    })
}
