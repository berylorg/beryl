use super::*;
use crate::resource_structure::construction::initial_build;

pub(super) fn code_build(source_bytes: &[u8], lines: u64) -> Build {
    let original = binding();
    let length = source_bytes.len() as u64 + 5;
    let source = ProjectionTextSource::composer(ContentReference::new(
        SyndicContentId::from_bytes([0x33; 16]),
        ContentRevision::new(2).unwrap(),
        ContentEncoding::ComposerV1,
        ContentSummary::new(
            1,
            1,
            length,
            length,
            1,
            0,
            [0; 32],
            None,
            SyndicContentDigest::from_bytes([0x44; 32]),
        )
        .unwrap(),
    ));
    let metadata = ResourceMetadataRecord::new(
        key().resource.id,
        key().resource.revision,
        original.metadata.projection_id().unwrap(),
        original.metadata.item_id(),
        ResourceOrdinal::FIRST,
        ResourceKind::Code,
        "text/plain",
        ResourceBacking::TextRange {
            source,
            range: ProjectionSourceRange::new(5, length).unwrap(),
        },
        [0x77; 32],
        None,
        ResourceStructure::code(Some("rust"), lines).unwrap(),
    )
    .unwrap();
    initial_build(key(), metadata).unwrap()
}

pub(super) fn table_build(source_bytes: &[u8], body_rows: u64, columns: u64) -> Build {
    let original = binding();
    let length = source_bytes.len() as u64 + 5;
    let source = ProjectionTextSource::composer(ContentReference::new(
        SyndicContentId::from_bytes([0x33; 16]),
        ContentRevision::new(2).unwrap(),
        ContentEncoding::ComposerV1,
        ContentSummary::new(
            1,
            1,
            length,
            length,
            1,
            0,
            [0; 32],
            None,
            SyndicContentDigest::from_bytes([0x44; 32]),
        )
        .unwrap(),
    ));
    let header_end = source_bytes.iter().position(|byte| *byte == b'\n').unwrap() as u64 + 1;
    let metadata = ResourceMetadataRecord::new(
        key().resource.id,
        key().resource.revision,
        original.metadata.projection_id().unwrap(),
        original.metadata.item_id(),
        ResourceOrdinal::FIRST,
        ResourceKind::Table,
        "text/plain",
        ResourceBacking::TextRange {
            source,
            range: ProjectionSourceRange::new(5, length).unwrap(),
        },
        [0x77; 32],
        None,
        ResourceStructure::table(
            body_rows,
            columns,
            ProjectionSourceRange::new(0, header_end).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    initial_build(key(), metadata).unwrap()
}
