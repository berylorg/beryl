use super::super::super::{
    copy_keyboard::CopyMethod,
    table_export::{COPY_BYTES, TableExport},
    table_viewport::TableCellId,
};
use super::*;
use crate::syndic_transcript::{IndexedResourceResponse, IndexedResourceValue};
use std::sync::Arc;
use syndic_storage::indexed_resource as index;

pub(super) struct Fixture {
    pub binding: index::Binding,
    pub source: Vec<u8>,
    pub cells: Vec<(TableCellId, index::Item)>,
    pub page: Arc<IndexedResourceResponse>,
}

pub(super) fn fixture(rows: &[Vec<Option<&str>>], delimiter: &str) -> Fixture {
    let mut binding = super::code_anchors::binding(1);
    let mut source = String::new();
    let mut cells = Vec::new();
    let columns = rows[0].len() as u64;
    let mut header_end = 0;
    for (row, values) in rows.iter().enumerate() {
        source.push('|');
        for (column, value) in values.iter().enumerate() {
            let id = TableCellId {
                role: if row == 0 {
                    index::RowRole::Header
                } else {
                    index::RowRole::Body
                },
                row: row.saturating_sub(1) as u64,
                column: column as u64,
            };
            let item = if let Some(value) = value {
                let start = source.len() as u64;
                source.push_str(value);
                let end = source.len() as u64;
                source.push('|');
                index::Item::Descriptor(index::IndexedDescriptor {
                    descriptor: index::Descriptor::TableCell {
                        role: id.role,
                        row: id.row,
                        column: id.column,
                        raw: index::ByteRange { start, end },
                        trimmed: index::ByteRange { start, end },
                        presentation: index::CellPresentation::Literal,
                    },
                    absolute_raw: Some(index::ByteRange {
                        start: start + 137,
                        end: end + 137,
                    }),
                })
            } else {
                index::Item::Missing {
                    role: id.role,
                    row: id.row,
                    column: id.column,
                }
            };
            cells.push((id, item));
        }
        source.push_str("\r\n");
        if row == 0 {
            header_end = source.len() as u64;
            source.push_str(delimiter);
            source.push_str("\r\n");
        }
    }
    binding.range = index::ByteRange {
        start: 137,
        end: 137 + source.len() as u64,
    };
    binding.source = syndic_storage::ProjectionTextSource::provider_narrative(
        syndic_storage::ProviderNarrativeReference::new(
            beryl_model::SyndicContentId::from_bytes([33; 16]),
            syndic_storage::ProviderNarrativeGeneration::new(1).unwrap(),
            1,
            binding.range.end,
            [44; 32],
        )
        .unwrap(),
    );
    binding.metadata = syndic_storage::ResourceMetadataRecord::new(
        binding.resource.id,
        binding.resource.revision,
        binding.metadata.projection_id().unwrap(),
        binding.metadata.item_id(),
        binding.metadata.ordinal().unwrap(),
        syndic_storage::ResourceKind::Table,
        "text/markdown",
        syndic_storage::ResourceBacking::TextRange {
            source: binding.source,
            range: syndic_storage::ProjectionSourceRange::new(
                binding.range.start,
                binding.range.end,
            )
            .unwrap(),
        },
        [77; 32],
        None,
        syndic_storage::ResourceStructure::table(
            rows.len().saturating_sub(1) as u64,
            columns,
            syndic_storage::ProjectionSourceRange::new(0, header_end).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let build = index::BuildKey {
        resource: binding.resource,
        operation: [22; 16],
    };
    let root = index::Root {
        key: build,
        binding_digest: [88; 32],
        pages: 1,
        chain: [99; 32],
        counts: [0, rows.len() as u64, cells.len() as u64, columns],
        code_lines: 0,
        body_rows: rows.len().saturating_sub(1) as u64,
        columns,
        header: Some(index::ByteRange {
            start: 0,
            end: header_end,
        }),
        language: None,
        progress: index::SequenceKey { build, ordinal: 1 },
        digest: [55; 32],
    };
    let page = Arc::new(IndexedResourceResponse {
        root: Some(root),
        encoded_bytes: 4096,
        value: IndexedResourceValue::Range(index::Response {
            binding: binding.clone(),
            cursor: index::Cursor {
                build,
                root_digest: [55; 32],
                request: index::Request::Table {
                    role: index::RowRole::Body,
                    rows: index::OrdinalRange {
                        start: 0,
                        end: rows.len().saturating_sub(1) as u64,
                    },
                    columns: index::OrdinalRange {
                        start: 0,
                        end: columns,
                    },
                },
                next: index::Position::Table {
                    row: rows.len() as u64,
                    column: 0,
                },
                fragment_offset: 0,
            },
            items: cells.iter().map(|(_, item)| item.clone()).collect(),
            request_complete: true,
            resource_complete: true,
            encoded_bytes: 4096,
            presentations: None,
        }),
    });
    Fixture {
        binding,
        source: source.into_bytes(),
        cells,
        page,
    }
}

fn bytes(fixture: &Fixture, export: &TableExport) -> Vec<u8> {
    export
        .ranges
        .iter()
        .flat_map(|range| {
            fixture.source[(range.start - 137) as usize..(range.end - 137) as usize]
                .iter()
                .copied()
        })
        .collect()
}

#[test]
fn table_whole_markdown_preserves_original_delimiter_empty_cells_and_crlf() {
    let fixture = fixture(
        &[
            vec![Some(" a "), Some(" b ")],
            vec![Some("\\| "), None],
            vec![Some(""), Some(" ")],
        ],
        "| :---- | -----: |",
    );
    let export = TableExport::selected(
        fixture.cells.clone(),
        &[fixture.page.clone()],
        &fixture.binding,
        true,
    )
    .unwrap();
    assert_eq!(export.ranges, vec![fixture.binding.range]);
    assert_eq!(
        export
            .render(
                bytes(&fixture, &TableExport::raw(fixture.binding.range).unwrap()),
                CopyMethod::Markdown
            )
            .unwrap()
            .as_bytes(),
        fixture.source
    );
}

#[test]
fn table_partial_markdown_uses_exact_selected_cells_without_unselected_columns() {
    let fixture = fixture(
        &[
            vec![
                Some(" hidden "),
                Some(" **selected\\|** "),
                Some(" untouched "),
            ],
            vec![Some(" no "), Some(" `literal*` "), None],
        ],
        "|---|---|---|",
    );
    let selected = fixture
        .cells
        .iter()
        .filter(|(id, _)| id.column == 1)
        .cloned()
        .collect();
    let export =
        TableExport::selected(selected, &[fixture.page.clone()], &fixture.binding, false).unwrap();
    assert!(export.ranges.iter().all(|range| range.start >= 137));
    let source = bytes(&fixture, &export);
    assert_eq!(
        export.render(source, CopyMethod::Markdown).unwrap(),
        "| **selected\\|** |\n|---|\n| `literal*` |"
    );
    let selected = fixture
        .cells
        .iter()
        .filter(|(id, _)| id.role == index::RowRole::Body && id.column == 1)
        .cloned()
        .collect();
    let export = TableExport::selected(selected, &[], &fixture.binding, false).unwrap();
    let source = bytes(&fixture, &export);
    assert_eq!(
        export.render(source, CopyMethod::Markdown).unwrap(),
        "| `literal*` |"
    );
}

#[test]
fn table_text_omits_empty_rows_and_tails_but_preserves_interior_gaps_and_literal_code() {
    let fixture = fixture(
        &[
            vec![Some(" h "), Some(""), Some(" tail "), None],
            vec![Some(" **bold** "), Some(" "), Some(" `literal*` "), None],
            vec![Some(""), None, None, None],
        ],
        "|---|---|---|---|",
    );
    let export =
        TableExport::selected(fixture.cells.clone(), &[], &fixture.binding, false).unwrap();
    let source = bytes(&fixture, &export);
    assert_eq!(
        export.render(source, CopyMethod::Stripped).unwrap(),
        "h\t\ttail\nbold\t\tliteral*"
    );
}

#[test]
fn table_export_checks_source_output_and_utf8_independently() {
    assert!(
        TableExport::raw(index::ByteRange {
            start: 137,
            end: 138 + COPY_BYTES as u64
        })
        .is_err()
    );
    let big = "x".repeat(COPY_BYTES);
    let fixture = fixture(&[vec![Some(&big)]], "|---|");
    let export =
        TableExport::selected(fixture.cells.clone(), &[], &fixture.binding, false).unwrap();
    let source = bytes(&fixture, &export);
    assert!(export.render(source, CopyMethod::Markdown).is_err());
    let export = TableExport::raw(index::ByteRange {
        start: 137,
        end: 139,
    })
    .unwrap();
    assert!(
        export
            .render(vec![0xf0, 0x9f], CopyMethod::Markdown)
            .is_err()
    );
}

#[test]
fn table_partial_header_exports_only_the_selected_column_alignment() {
    let mut fixture = fixture(
        &[vec![Some(" left "), Some(" selected "), Some(" right ")]],
        "|:----|------:|:---:|",
    );
    let page = Arc::get_mut(&mut fixture.page).unwrap();
    let IndexedResourceValue::Range(range) = &mut page.value else {
        panic!();
    };
    range
        .items
        .push(index::Item::Descriptor(index::IndexedDescriptor {
            descriptor: index::Descriptor::TableColumn {
                ordinal: 1,
                alignment: index::Alignment::Right,
            },
            absolute_raw: None,
        }));
    let selected = fixture
        .cells
        .iter()
        .filter(|(id, _)| id.column == 1)
        .cloned()
        .collect();
    let export =
        TableExport::selected(selected, &[fixture.page.clone()], &fixture.binding, false).unwrap();
    let source = bytes(&fixture, &export);
    assert_eq!(
        export.render(source, CopyMethod::Markdown).unwrap(),
        "| selected |\n|---:|"
    );
}
