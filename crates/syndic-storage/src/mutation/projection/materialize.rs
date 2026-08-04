use beryl_home_store::DomainReader;
use beryl_model::ProjectionRevision;

use crate::{ProjectionFormatVersion, SyndicMutationError, domain::SyndicDomain};

use super::{parser, range};

pub(crate) struct MaterializedOutput {
    pub(crate) projection: crate::ProjectionRecord,
    pub(crate) resource: Option<(
        crate::ResourceMetadataRecord,
        crate::ProjectionResourceIndexRecord,
    )>,
}

type ResourceDraft = (
    beryl_model::SyndicResourceId,
    ProjectionRevision,
    crate::ResourceOrdinal,
    crate::ResourceKind,
    Box<str>,
    crate::ResourceStructure,
    [u8; 32],
);

pub(crate) fn materialize_output(
    reader: &DomainReader<'_, SyndicDomain>,
    item: &crate::CanonicalItemRecord,
    source: crate::ProjectionTextSource,
    format: ProjectionFormatVersion,
    ordinal: crate::ProjectionOrdinal,
    output: parser::ParserOutput,
) -> Result<MaterializedOutput, SyndicMutationError> {
    let (block_start, payload, resource_draft) = match output {
        parser::ParserOutput::Inline {
            block_start,
            kind,
            span_ordinal,
            range,
        } => {
            let text = range::read_source_range(reader, source, range)?;
            let block = crate::projection::markdown_block_id(format, item.id(), block_start, kind);
            (
                block_start,
                crate::ProjectionPayload::inline_markdown(block, kind, span_ordinal, range, text)?,
                None,
            )
        }
        parser::ParserOutput::Resource {
            block_start,
            kind,
            range,
            preview,
            structure,
            digest,
        } => {
            let resource_kind = match kind {
                crate::MarkdownBlockKind::FencedCode => crate::ResourceKind::Code,
                crate::MarkdownBlockKind::Table => crate::ResourceKind::Table,
                _ => return Err(SyndicMutationError::ProjectionBuildConflict),
            };
            let resource_ordinal = crate::ResourceOrdinal::FIRST;
            let (resource_id, resource_revision) = crate::projection::resource_identity(
                format,
                item.id(),
                resource_ordinal,
                resource_kind,
                source,
                range,
            );
            let block = crate::projection::markdown_block_id(format, item.id(), block_start, kind);
            let payload = crate::ProjectionPayload::resource_reference(
                block,
                kind,
                range,
                resource_id,
                &preview,
            )?;
            (
                block_start,
                payload,
                Some((
                    resource_id,
                    resource_revision,
                    resource_ordinal,
                    resource_kind,
                    preview,
                    structure,
                    digest,
                )),
            )
        }
        parser::ParserOutput::ImageMarker {
            atom_ordinal,
            marker_ordinal,
            source_offset,
            marker_id,
            label,
        } => (
            source_offset,
            crate::ProjectionPayload::image_marker(
                atom_ordinal,
                marker_ordinal,
                source_offset,
                crate::ComposerImageMarker::new(marker_id, label),
            ),
            None,
        ),
        parser::ParserOutput::Empty => (0, crate::ProjectionPayload::empty(), None),
    };
    let (projection_id, projection_revision) =
        crate::projection::projection_identity(format, item.id(), block_start, ordinal, &payload);
    let projection = crate::ProjectionRecord::new(
        projection_id,
        projection_revision,
        item.id(),
        item.turn_id(),
        ordinal,
        payload,
    );
    let resource = resource_draft
        .map(|draft| materialize_resource(item, source, &projection, draft))
        .transpose()?;
    Ok(MaterializedOutput {
        projection,
        resource,
    })
}

fn materialize_resource(
    item: &crate::CanonicalItemRecord,
    source: crate::ProjectionTextSource,
    projection: &crate::ProjectionRecord,
    draft: ResourceDraft,
) -> Result<
    (
        crate::ResourceMetadataRecord,
        crate::ProjectionResourceIndexRecord,
    ),
    SyndicMutationError,
> {
    let (id, revision, ordinal, kind, preview, structure, digest) = draft;
    let range = projection
        .payload()
        .source_range()
        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
    let preview_range = if preview.is_empty() {
        None
    } else {
        Some(crate::ProjectionSourceRange::new(0, preview.len() as u64)?)
    };
    let media_type = match kind {
        crate::ResourceKind::Code => "text/plain; charset=utf-8",
        crate::ResourceKind::Table => "text/markdown; charset=utf-8",
        _ => return Err(SyndicMutationError::ProjectionBuildConflict),
    };
    let resource = crate::ResourceMetadataRecord::new(
        id,
        revision,
        projection.id(),
        item.id(),
        ordinal,
        kind,
        media_type,
        crate::ResourceBacking::TextRange { source, range },
        digest,
        preview_range,
        structure,
    )?;
    let index = crate::ProjectionResourceIndexRecord::new(
        projection.id(),
        ordinal,
        resource.id(),
        resource.revision(),
        *resource
            .digest()
            .ok_or(SyndicMutationError::ProjectionBuildConflict)?,
    );
    Ok((resource, index))
}
