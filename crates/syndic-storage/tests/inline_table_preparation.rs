use std::mem::size_of;

use syndic_storage::{
    MarkdownBlockId, MarkdownBlockKind, ProjectionPayload, ProjectionSourceRange,
    indexed_resource::{Alignment, CellPresentation, RowDisposition, RowRole},
    inline_table::{
        InlineTableCell, InlineTablePreparationErrorKind, InlineTableRow, PreparedInlineTable,
        prepare_inline_table,
    },
};

fn payload(source: &str) -> ProjectionPayload {
    ProjectionPayload::InlineMarkdown {
        block_id: MarkdownBlockId::from_bytes([7; 32]),
        block_kind: MarkdownBlockKind::Table,
        span_ordinal: 1,
        source_range: ProjectionSourceRange::new(91, 91 + source.len() as u64).unwrap(),
        source: source.into(),
    }
}

fn prepare(source: &str) -> PreparedInlineTable {
    prepare_inline_table(payload(source), usize::MAX).unwrap()
}

#[test]
fn crlf_escaping_code_utf8_and_alignment_preserve_exact_source_ranges() {
    let source =
        " | café | code | last |\r\n | :--- | :---: | ---: |\r\n | a\\|b | `x|y` | **z** |\r\n";
    let table = prepare(source);
    assert_eq!(table.source(), source);
    assert_eq!(
        table.source_range(),
        ProjectionSourceRange::new(91, 91 + source.len() as u64).unwrap()
    );
    assert_eq!(table.block_id(), MarkdownBlockId::from_bytes([7; 32]));
    assert_eq!(
        table.alignments(),
        &[Alignment::Left, Alignment::Center, Alignment::Right]
    );
    assert_eq!(table.body_rows(), 1);
    assert_eq!(table.columns(), 3);
    for (column, raw, displayed) in [
        (0, " a\\|b ", "a\\|b"),
        (1, " `x|y` ", "`x|y`"),
        (2, " **z** ", "**z**"),
    ] {
        let cell = table.cell(RowRole::Body, 0, column).unwrap();
        let raw_range = cell.raw.unwrap();
        assert_eq!(
            &source[raw_range.start as usize..raw_range.end as usize],
            raw
        );
        assert_eq!(table.cell_display(cell), Some(displayed));
        assert_eq!(cell.display, cell.trimmed);
        assert_eq!(cell.presentation, CellPresentation::LiteralFallback);
        assert!(!cell.is_missing());
        assert!(!cell.is_empty());
    }
    for row in table.rows() {
        assert!(source[row.raw.start as usize..row.raw.end as usize].ends_with("\r\n"));
    }
    let header = table.cell(RowRole::Header, 0, 0).unwrap();
    assert_eq!(table.cell_display(header), Some("café"));
    assert_eq!(header.presentation, CellPresentation::Literal);
    assert_eq!(
        &source[table.delimiter().start as usize..table.delimiter().end as usize],
        " | :--- | :---: | ---: |\r\n"
    );
}

#[test]
fn authored_empty_missing_and_surplus_fallback_are_distinct() {
    let source = "a|b|c\n---|---|---\n|x| |\nx|y|z|surplus\nq|r|s";
    let table = prepare(source);
    let empty = table.cell(RowRole::Body, 0, 1).unwrap();
    assert!(empty.is_empty());
    assert!(!empty.is_missing());
    assert_eq!(table.cell_display(empty), Some(""));
    let missing = table.cell(RowRole::Body, 0, 2).unwrap();
    assert!(missing.is_missing());
    assert!(!missing.is_empty());
    assert_eq!(table.cell_display(missing), None);
    let fallback = table.rows()[2];
    assert_eq!(fallback.ordinal, 1);
    assert_eq!(fallback.authored_cells, 4);
    assert_eq!(fallback.disposition, RowDisposition::WholeRowFallback);
    assert_eq!(
        &source[fallback.raw.start as usize..fallback.raw.end as usize],
        "x|y|z|surplus\n"
    );
    assert!(table.cell(RowRole::Body, 1, 0).is_none());
    assert_eq!(
        table.cell_display(table.cell(RowRole::Body, 2, 2).unwrap()),
        Some("s")
    );
    assert_eq!(table.cells().len(), 9);
}

#[test]
fn maximum_inline_shape_has_complete_offscreen_positions() {
    let row = vec!["x"; 12].join("|");
    let delimiter = vec!["---"; 12].join("|");
    let source = format!("{row}\n{delimiter}\n{}", format!("{row}\n").repeat(32));
    let table = prepare(&source);
    assert_eq!(table.rows().len(), 33);
    assert_eq!(table.cells().len(), 396);
    assert_eq!(table.alignments().len(), 12);
    assert_eq!(
        table.cell_display(table.cell(RowRole::Body, 31, 11).unwrap()),
        Some("x")
    );
    assert!(table.cell(RowRole::Body, 32, 0).is_none());
}

#[test]
fn rejects_threshold_crossing_and_noncanonical_structure_without_losing_source() {
    let header = vec!["h"; 13].join("|");
    let delimiter = vec!["---"; 13].join("|");
    for source in [
        "a|b\n---|---\n".to_owned() + &"x|y\n".repeat(33),
        format!("{header}\n{delimiter}\n"),
        "a|b\n---|--\n".to_owned(),
        "a|b\n---|---|---\n".to_owned(),
        "a|b\n---|---\nno separator\n".to_owned(),
        "a|b\n---|---\n\n".to_owned(),
        "a|b\n---|---\n".to_owned() + &"x".repeat(8_192) + "|y",
        "a|b".to_owned(),
    ] {
        let original = payload(&source);
        let error = prepare_inline_table(original.clone(), usize::MAX).unwrap_err();
        assert_eq!(
            error.kind(),
            InlineTablePreparationErrorKind::InvalidStructure
        );
        assert_eq!(error.into_payload(), original);
    }
}

#[test]
fn rejects_wrong_kind_span_and_source_binding() {
    let source = "a|b\n---|---\nx|y";
    for alteration in 0..3 {
        let mut invalid = payload(source);
        if let ProjectionPayload::InlineMarkdown {
            block_kind,
            span_ordinal,
            source_range,
            ..
        } = &mut invalid
        {
            match alteration {
                0 => *block_kind = MarkdownBlockKind::Paragraph,
                1 => *span_ordinal = 2,
                _ => *source_range = ProjectionSourceRange::new(91, 92).unwrap(),
            }
        }
        let error = prepare_inline_table(invalid.clone(), usize::MAX).unwrap_err();
        assert_eq!(
            error.kind(),
            InlineTablePreparationErrorKind::InvalidStructure
        );
        assert_eq!(error.into_payload(), invalid);
    }
    let error = prepare_inline_table(ProjectionPayload::Empty, usize::MAX).unwrap_err();
    assert_eq!(
        error.kind(),
        InlineTablePreparationErrorKind::InvalidStructure
    );
}

#[test]
fn exact_byte_boundary_and_allocation_admission_move_the_single_source_allocation() {
    let prefix = "a|b\n---|---\n";
    let source = prefix.to_owned() + &"x".repeat(8_192 - prefix.len() - 2) + "|y";
    let original = payload(&source);
    let source_pointer = original.inline_source().unwrap().as_ptr();
    let table = prepare_inline_table(original, usize::MAX).unwrap();
    assert_eq!(table.source().as_ptr(), source_pointer);
    assert_eq!(table.source().len(), 8_192);
    let minimum = size_of::<PreparedInlineTable>()
        + source.len()
        + table.rows().len() * size_of::<InlineTableRow>()
        + table.cells().len() * size_of::<InlineTableCell>()
        + table.alignments().len() * size_of::<Alignment>();
    assert!(table.retained_bytes() >= minimum);
    let admitted = prepare_inline_table(payload(&source), table.retained_bytes()).unwrap();
    assert_eq!(admitted.retained_bytes(), table.retained_bytes());
    for allowance in [0, table.retained_bytes() - 1] {
        let original = payload(&source);
        let pointer = original.inline_source().unwrap().as_ptr();
        let error = prepare_inline_table(original, allowance).unwrap_err();
        assert_eq!(error.kind(), InlineTablePreparationErrorKind::Capacity);
        assert_eq!(error.payload().inline_source().unwrap().as_ptr(), pointer);
        assert_eq!(error.into_payload().inline_source(), Some(source.as_str()));
    }
}
