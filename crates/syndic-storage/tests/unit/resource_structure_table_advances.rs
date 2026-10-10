use super::*;
use crate::resource_structure::{
    construction::ConstructionError,
    table_publication::{PreparedTableAdvance, prepare_table},
    transition::{prepare_control, validate_receipt},
};

use super::fixture_support::table_build;

fn assert_closure(before: &Build, prepared: &PreparedTableAdvance) {
    validate_receipt(&prepared.build, &prepared.receipt).unwrap();
    assert_eq!(prepared.receipt.prior, before.progress);
    assert_eq!(
        decode_build(&encode_build(&prepared.build).unwrap()).unwrap(),
        prepared.build
    );
    if let Some(page) = &prepared.page {
        assert_eq!(decode_page(&encode_page(page).unwrap()).unwrap(), *page);
        assert_eq!(page.prior_chain, before.chain);
        assert_eq!(
            prepared.build.chain,
            chain_link(&before.chain, &page.digest)
        );
        assert_eq!(prepared.locators.len(), page.descriptors.len());
        for (slot, locator) in prepared.locators.iter().enumerate() {
            assert_eq!(
                (locator.page, locator.slot, locator.digest),
                (page.key.ordinal, slot as u64, page.digest)
            );
            let (tag, role, row, column) = page.descriptors[slot].coordinate();
            let expected = match tag {
                1 => CoordinateKey::TableRow(before.key, role, row),
                2 => CoordinateKey::TableCell(before.key, role, row, column),
                3 => CoordinateKey::TableColumn(before.key, row),
                _ => panic!("unexpected descriptor"),
            };
            assert_eq!(locator.key, expected);
            let decoded = match tag {
                1 => {
                    TableRowsFamily::decode_value(&TableRowsFamily::encode_value(locator).unwrap())
                        .unwrap()
                }
                2 => TableCellsFamily::decode_value(
                    &TableCellsFamily::encode_value(locator).unwrap(),
                )
                .unwrap(),
                3 => TableColumnsFamily::decode_value(
                    &TableColumnsFamily::encode_value(locator).unwrap(),
                )
                .unwrap(),
                _ => unreachable!(),
            };
            assert_eq!(decoded, *locator);
        }
        for anchor in &prepared.anchors {
            let descriptor = &page.descriptors[anchor.slot as usize];
            let raw = descriptor.raw().unwrap();
            assert_eq!(anchor.key.start, before.binding.range.start + raw.start);
            assert_eq!(anchor.end, before.binding.range.start + raw.end);
            assert_eq!(
                (anchor.page, anchor.digest),
                (page.key.ordinal, page.digest)
            );
            assert_eq!(
                SourceAnchorsFamily::decode_value(&encode_anchor(anchor).unwrap()).unwrap(),
                *anchor
            );
        }
    } else {
        assert_eq!(prepared.build.chain, before.chain);
        assert_eq!(prepared.build.next_page, before.next_page);
        assert!(prepared.locators.is_empty() && prepared.anchors.is_empty());
        assert!(prepared.receipt.page.is_none());
    }
}

fn rows(mut build: Build, source: &[u8], quantum: usize) -> (Build, Vec<Descriptor>) {
    let mut output = Vec::new();
    while build.checkpoint.phase != ParsePhase::Eof {
        let start = build.checkpoint.next_byte as usize;
        let end = (start + quantum).min(source.len());
        let prepared = prepare_table(&build, None, &source[start..end], 1).unwrap();
        assert_closure(&build, &prepared);
        if let Some(page) = prepared.page {
            output.extend(page.descriptors);
        }
        build = prepared.build;
    }
    (build, output)
}

#[test]
fn complete_table_publishes_ordered_rows_cells_columns_and_raw_anchors() {
    let source = b"a|b|c\n:---|:---:|---:\nx| |z\nu|v\n";
    let before = table_build(source, 2, 3);
    let (closed_rows, descriptors) = rows(before, source, 2);
    assert_eq!(closed_rows.counts, [0, 3, 0, 0]);
    let mut cells = prepare_control(&closed_rows, Command::PassChange)
        .unwrap()
        .build;
    let mut emitted = Vec::new();
    for row in &descriptors {
        let (_, role, ordinal, _) = row.coordinate();
        while cells.checkpoint.role == role
            && cells.checkpoint.next_row == ordinal
            && cells.checkpoint.phase != ParsePhase::Eof
        {
            let raw = row.raw().unwrap();
            let start = cells.checkpoint.next_byte.max(raw.start) as usize;
            let source = if cells.checkpoint.phase == ParsePhase::DrainingSyntax {
                &[][..]
            } else {
                &source[start..raw.end as usize]
            };
            let prepared = prepare_table(&cells, Some(row), source, 1).unwrap();
            assert_closure(&cells, &prepared);
            assert_eq!(prepared.anchors.len(), prepared.locators.len());
            if let Some(page) = prepared.page {
                emitted.extend(page.descriptors);
            }
            cells = prepared.build;
        }
    }
    assert_eq!(cells.checkpoint.phase, ParsePhase::Eof);
    assert_eq!(cells.counts, [0, 3, 8, 0]);
    let empty = emitted
        .iter()
        .find(|cell| cell.coordinate() == (2, RowRole::Body, 0, 1))
        .unwrap();
    assert!(
        matches!(empty, Descriptor::TableCell { raw, trimmed, .. } if raw.end > raw.start && trimmed.start == raw.end && trimmed.end == raw.end)
    );
    assert!(
        !emitted
            .iter()
            .any(|cell| cell.coordinate() == (2, RowRole::Body, 1, 2))
    );
    let mut columns = prepare_control(&cells, Command::PassChange).unwrap().build;
    let mut alignments = Vec::new();
    while columns.checkpoint.phase != ParsePhase::Eof {
        let offered = if columns.checkpoint.phase == ParsePhase::DrainingSyntax {
            &[][..]
        } else {
            &source[columns.checkpoint.next_byte as usize..]
        };
        let prepared = prepare_table(&columns, None, offered, 1).unwrap();
        assert_closure(&columns, &prepared);
        assert!(prepared.anchors.is_empty());
        alignments.extend(prepared.page.unwrap().descriptors);
        columns = prepared.build;
    }
    assert_eq!(
        alignments,
        vec![
            Descriptor::TableColumn {
                ordinal: 0,
                alignment: Alignment::Left
            },
            Descriptor::TableColumn {
                ordinal: 1,
                alignment: Alignment::Center
            },
            Descriptor::TableColumn {
                ordinal: 2,
                alignment: Alignment::Right
            },
        ]
    );
    let sealed = prepare_control(&columns, Command::PassChange).unwrap();
    let complete = prepare_control(&sealed.build, Command::Complete).unwrap();
    let (root, _) = complete.selection.unwrap();
    assert_eq!(root.counts, [0, 3, 8, 3]);
    assert_eq!(root.chain, columns.chain);
    assert_eq!(root.pages, columns.next_page - 1);
}

#[test]
fn fallback_rows_publish_one_anchor_and_skip_cell_source() {
    for source in [
        b"a|b\n---|---\nx|y|z\n".to_vec(),
        [b"a|b\n---|---\n".as_slice(), &vec![b'x'; 16_400], b"|y\n"].concat(),
    ] {
        let before = table_build(&source, 1, 2);
        let (closed, descriptors) = rows(before, &source, 4096);
        assert!(matches!(
            descriptors[1],
            Descriptor::TableRow {
                disposition: RowDisposition::WholeRowFallback,
                ..
            }
        ));
        let mut cells = prepare_control(&closed, Command::PassChange).unwrap().build;
        let raw = descriptors[0].raw().unwrap();
        cells = prepare_table(
            &cells,
            Some(&descriptors[0]),
            &source[..raw.end as usize],
            256,
        )
        .unwrap()
        .build;
        let skipped = prepare_table(&cells, Some(&descriptors[1]), b"", 1).unwrap();
        assert_closure(&cells, &skipped);
        assert!(skipped.page.is_none());
        assert_eq!(skipped.build.checkpoint.phase, ParsePhase::Eof);
        assert_eq!(skipped.build.checkpoint.next_byte, source.len() as u64);
        assert_eq!(skipped.build.counts, [0, 2, 2, 0]);
        assert!(prepare_table(&cells, Some(&descriptors[1]), b"x", 1).is_err());
    }
    let before = table_build(b"a|b\n---|---\nx|y|z\n", 1, 2);
    let header = prepare_table(&before, None, b"a|b\n", 1).unwrap();
    let delimiter = prepare_table(&header.build, None, b"---|---\n", 1).unwrap();
    let fallback = prepare_table(&delimiter.build, None, b"x|y|z\n", 1).unwrap();
    assert_eq!(fallback.anchors.len(), 1);
    assert_eq!(fallback.anchors[0].key.tag, 1);
    assert_eq!(fallback.anchors[0].key.role, RowRole::Body);
}

#[test]
fn zero_body_table_cells_close_after_header_without_delimiter_cell_read() {
    let source = b"a|b\n---|---\n";
    let (closed, descriptors) = rows(table_build(source, 0, 2), source, 65_536);
    let cells = prepare_control(&closed, Command::PassChange).unwrap().build;
    let prepared = prepare_table(&cells, Some(&descriptors[0]), b"a|b\n", 256).unwrap();
    assert_closure(&cells, &prepared);
    assert_eq!(prepared.build.checkpoint.phase, ParsePhase::Eof);
    assert_eq!(prepared.build.checkpoint.next_byte, source.len() as u64);
    assert_eq!(prepared.build.counts, [0, 1, 2, 0]);
    assert!(prepare_control(&prepared.build, Command::PassChange).is_ok());
}

#[test]
fn table_preparation_rejects_wrong_counts_row_identity_and_no_progress() {
    let source = b"a|b\n---|---\nx|y\n";
    let before = table_build(source, 2, 2);
    let header = prepare_table(&before, None, b"a|b\n", 1).unwrap();
    let delimiter = prepare_table(&header.build, None, b"---|---\n", 1).unwrap();
    assert!(matches!(
        prepare_table(&delimiter.build, None, b"x|y\n", 1),
        Err(ConstructionError::Unavailable)
    ));
    let before = table_build(source, 1, 2);
    assert!(prepare_table(&before, None, b"", 1).is_err());
    assert!(prepare_table(&before, None, b"a", 0).is_err());
    let (closed, descriptors) = rows(before, source, 65_536);
    let cells = prepare_control(&closed, Command::PassChange).unwrap().build;
    assert!(prepare_table(&cells, Some(&descriptors[1]), b"x|y\n", 1).is_err());
    assert!(prepare_table(&cells, None, b"a|b\n", 1).is_err());
    assert!(matches!(
        prepare_table(&closed, None, b"", 1),
        Err(ConstructionError::Derivation(_))
    ));
    let terminal = prepare_control(&closed, Command::Cancel).unwrap();
    assert!(matches!(
        prepare_table(&terminal.build, None, b"a", 1),
        Err(ConstructionError::Collision)
    ));
}

#[test]
fn cell_publication_resumes_each_unicode_byte_and_restores_output_frontier() {
    let source = "€|𐐀|z\r\n---|---|---\r\nx|é||\r\n".as_bytes();
    let (closed, descriptors) = rows(table_build(source, 1, 3), source, 1);
    let mut cells = prepare_control(&closed, Command::PassChange).unwrap().build;
    let mut output = Vec::new();
    let mut output_only = 0;
    for row in &descriptors {
        let (_, role, ordinal, _) = row.coordinate();
        while cells.checkpoint.role == role
            && cells.checkpoint.next_row == ordinal
            && cells.checkpoint.phase != ParsePhase::Eof
        {
            let raw = row.raw().unwrap();
            let start = cells.checkpoint.next_byte.max(raw.start) as usize;
            let offered = if cells.checkpoint.phase == ParsePhase::DrainingSyntax {
                output_only += 1;
                &[][..]
            } else {
                &source[start..(start + 1).min(raw.end as usize)]
            };
            let prepared = prepare_table(&cells, Some(row), offered, 1).unwrap();
            assert_closure(&cells, &prepared);
            if let Some(page) = prepared.page {
                output.extend(page.descriptors);
            }
            cells = decode_build(&encode_build(&prepared.build).unwrap()).unwrap();
        }
    }
    assert_eq!(output_only, 4);
    assert_eq!(cells.counts, [0, 2, 6, 0]);
    assert_eq!(cells.checkpoint.phase, ParsePhase::Eof);
    let trimmed: Vec<_> = output
        .iter()
        .map(|cell| {
            let Descriptor::TableCell { trimmed, .. } = cell else {
                unreachable!()
            };
            std::str::from_utf8(&source[trimmed.start as usize..trimmed.end as usize]).unwrap()
        })
        .collect();
    assert_eq!(trimmed, ["€", "𐐀", "z", "x", "é", ""]);
}
