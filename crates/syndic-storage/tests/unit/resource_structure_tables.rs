use super::*;
use crate::table_syntax;

fn window(source: &str, pass: Pass, start: u64) -> Checkpoint {
    Checkpoint {
        pass,
        next_byte: start + source.len() as u64,
        line_start: 0,
        role: RowRole::Header,
        next_row: 0,
        next_column: 0,
        phase: ParsePhase::DrainingSyntax,
        pending_cr: false,
        utf8_carry: Vec::new(),
        syntax_start: start,
        syntax: source.as_bytes().to_vec(),
        output_position: 0,
        escaped: false,
        in_code: false,
    }
}

fn cells(source: &str, start: u64, output: usize) -> Vec<Descriptor> {
    let mut checkpoint = window(source, Pass::TableCells, start);
    let row = table_row(
        source,
        ByteRange {
            start,
            end: checkpoint.next_byte,
        },
        RowRole::Header,
        0,
        table_syntax::columns(source).unwrap(),
    )
    .unwrap();
    let mut descriptors = Vec::new();
    while checkpoint.phase == ParsePhase::DrainingSyntax {
        let step = drain_cells(&checkpoint, &row, checkpoint.next_byte, output).unwrap();
        assert_eq!(step.consumed, 0);
        assert!(!step.descriptors.is_empty());
        assert!(step.descriptors.len() <= output);
        descriptors.extend(step.descriptors);
        checkpoint = step.checkpoint;
    }
    assert!(checkpoint.syntax.is_empty());
    assert_eq!(checkpoint.role, RowRole::Body);
    assert_eq!(checkpoint.next_row, 0);
    descriptors
}

#[test]
fn cells_preserve_escaped_code_empty_unicode_and_ascii_trim_ranges() {
    let source = "  | a\\|b | `c|d` |\t  |\u{a0}🦀\u{a0}| |\r\n";
    let expected = cells(source, 41, 256);
    assert_eq!(expected.len(), 5);
    for output in 1..=5 {
        assert_eq!(cells(source, 41, output), expected);
    }
    let values: Vec<_> = expected
        .iter()
        .map(|descriptor| {
            let Descriptor::TableCell {
                raw,
                trimmed,
                presentation,
                ..
            } = descriptor
            else {
                panic!()
            };
            let text = &source[(trimmed.start - 41) as usize..(trimmed.end - 41) as usize];
            assert!(raw.contains(*trimmed));
            (text, *presentation)
        })
        .collect();
    assert_eq!(
        values,
        vec![
            ("a\\|b", CellPresentation::LiteralFallback),
            ("`c|d`", CellPresentation::LiteralFallback),
            ("", CellPresentation::Literal),
            ("\u{a0}🦀\u{a0}", CellPresentation::Literal),
            ("", CellPresentation::Literal),
        ]
    );
    let Descriptor::TableCell { raw, trimmed, .. } = expected[2] else {
        panic!()
    };
    assert_eq!(trimmed.start, raw.end);
    assert_eq!(trimmed.end, raw.end);
}

#[test]
fn optional_pipes_do_not_discard_authored_unicode_whitespace() {
    for source in ["\u{a0}a | b\u{a0}\n", "|\u{a0}a | b\u{a0}|\n"] {
        let actual = cells(source, 0, 1);
        let Descriptor::TableCell { trimmed, .. } = actual[0] else {
            panic!()
        };
        assert_eq!(
            &source[trimmed.start as usize..trimmed.end as usize],
            "\u{a0}a"
        );
        let Descriptor::TableCell { trimmed, .. } = actual[1] else {
            panic!()
        };
        assert_eq!(
            &source[trimmed.start as usize..trimmed.end as usize],
            "b\u{a0}"
        );
    }
}

#[test]
fn missing_cells_are_absent_and_surplus_rows_are_whole_row_fallback() {
    let source = "|a|b|\n";
    let raw = ByteRange {
        start: 7,
        end: 7 + source.len() as u64,
    };
    let ordinary = table_row(source, raw, RowRole::Body, 8, 4).unwrap();
    assert!(matches!(
        ordinary,
        Descriptor::TableRow {
            authored_cells: 2,
            disposition: RowDisposition::Ordinary,
            ..
        }
    ));
    let mut checkpoint = window(source, Pass::TableCells, 7);
    checkpoint.role = RowRole::Body;
    checkpoint.next_row = 8;
    let step = drain_cells(&checkpoint, &ordinary, checkpoint.next_byte, 256).unwrap();
    assert_eq!(step.descriptors.len(), 2);
    assert_eq!(step.checkpoint.next_row, 9);
    let fallback = table_row(source, raw, RowRole::Body, 8, 1).unwrap();
    assert!(
        matches!(fallback, Descriptor::TableRow { raw: found, authored_cells: 2, disposition: RowDisposition::WholeRowFallback, .. } if found == raw)
    );
    assert!(drain_cells(&checkpoint, &fallback, checkpoint.next_byte, 1).is_err());
    assert!(table_row(source, raw, RowRole::Header, 0, 4).is_err());
}

#[test]
fn alignment_window_drains_across_output_only_steps() {
    let source = " | --- | :--- | ---: | :---: |\r\n";
    for output in 1..=4 {
        let mut checkpoint = window(source, Pass::Columns, 127);
        let length = checkpoint.next_byte + 100;
        let mut descriptors = Vec::new();
        while checkpoint.phase == ParsePhase::DrainingSyntax {
            let step = drain_columns(&checkpoint, 4, length, output).unwrap();
            assert_eq!(step.consumed, 0);
            assert_eq!(step.checkpoint.next_byte, checkpoint.next_byte);
            descriptors.extend(step.descriptors);
            checkpoint = step.checkpoint;
        }
        assert_eq!(
            descriptors,
            vec![
                Descriptor::TableColumn {
                    ordinal: 0,
                    alignment: Alignment::Unspecified
                },
                Descriptor::TableColumn {
                    ordinal: 1,
                    alignment: Alignment::Left
                },
                Descriptor::TableColumn {
                    ordinal: 2,
                    alignment: Alignment::Right
                },
                Descriptor::TableColumn {
                    ordinal: 3,
                    alignment: Alignment::Center
                },
            ]
        );
        assert!(checkpoint.syntax.is_empty());
        assert_eq!(checkpoint.next_column, 0);
    }
}

#[test]
fn wide_table_output_stays_bounded_and_advances_without_source_consumption() {
    let row = format!("|{}\n", "x|".repeat(1500));
    assert_eq!(cells(&row, 0, 1).len(), 1500);
    let delimiter = format!("|{}\n", "---|".repeat(1500));
    let mut checkpoint = window(&delimiter, Pass::Columns, row.len() as u64);
    let mut total = 0;
    while checkpoint.phase == ParsePhase::DrainingSyntax {
        let step = drain_columns(&checkpoint, 1500, checkpoint.next_byte, 256).unwrap();
        assert!(step.descriptors.len() <= 256);
        total += step.descriptors.len();
        checkpoint = step.checkpoint;
    }
    assert_eq!(total, 1500);
}

#[test]
fn invalid_windows_counts_delimiters_and_overflow_are_rejected() {
    let source = "a|b\n";
    let mut checkpoint = window(source, Pass::TableCells, 0);
    let row = table_row(
        source,
        ByteRange {
            start: 0,
            end: source.len() as u64,
        },
        RowRole::Header,
        0,
        2,
    )
    .unwrap();
    assert!(drain_cells(&checkpoint, &row, checkpoint.next_byte, 0).is_err());
    checkpoint.output_position = 1;
    assert!(drain_cells(&checkpoint, &row, checkpoint.next_byte, 1).is_err());
    checkpoint = window(source, Pass::TableCells, 0);
    checkpoint.next_byte += 1;
    assert!(drain_cells(&checkpoint, &row, checkpoint.next_byte, 1).is_err());
    checkpoint = window(source, Pass::TableCells, 0);
    checkpoint.role = RowRole::Body;
    checkpoint.next_row = u64::MAX;
    let row = table_row(
        source,
        ByteRange {
            start: 0,
            end: source.len() as u64,
        },
        RowRole::Body,
        u64::MAX,
        2,
    )
    .unwrap();
    assert!(matches!(
        drain_cells(&checkpoint, &row, checkpoint.next_byte, 256),
        Err(DerivationError::Overflow)
    ));
    for source in ["--|---\n", ":--:|---\n", "---|abc\n", "---|---\n---|---\n"] {
        let checkpoint = window(source, Pass::Columns, 0);
        assert!(drain_columns(&checkpoint, 2, checkpoint.next_byte, 1).is_err());
    }
    let source = "---|---\n";
    let checkpoint = window(source, Pass::Columns, 0);
    assert!(drain_columns(&checkpoint, 3, checkpoint.next_byte, 1).is_err());
}

#[test]
fn shared_recognition_preserves_existing_projection_grammar() {
    fn previous(line: &str) -> Option<u64> {
        let line = line
            .strip_suffix('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .unwrap_or(line);
        let source = line.trim();
        let source = source.strip_prefix('|').unwrap_or(source);
        let source = source.strip_suffix('|').unwrap_or(source);
        let mut escaped = false;
        let mut in_code = false;
        let mut separators = 0;
        for byte in source.bytes() {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'`' {
                in_code = !in_code;
            } else if byte == b'|' && !in_code {
                separators += 1;
            }
        }
        (separators != 0).then_some(separators + 1)
    }
    let alphabet = ['|', '\\', '`', ' ', '\t', 'a', '\u{a0}'];
    for length in 0..=5_u32 {
        for mut encoded in 0..alphabet.len().pow(length) {
            let mut source = String::new();
            for _ in 0..length {
                source.push(alphabet[encoded % alphabet.len()]);
                encoded /= alphabet.len();
            }
            for ending in ["", "\n", "\r\n"] {
                let source = format!("{source}{ending}");
                assert_eq!(
                    table_syntax::columns(&source),
                    previous(&source),
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn cell_source_admission_resumes_at_every_utf8_and_output_boundary() {
    let source = "|α|🦀\\|b|`c|d`|\r\n";
    let row = table_row(
        source,
        ByteRange {
            start: 13,
            end: 13 + source.len() as u64,
        },
        RowRole::Header,
        0,
        3,
    )
    .unwrap();
    let expected = cells(source, 13, 256);
    for quantum in 1..=source.len() {
        for output in 1..=3 {
            let mut checkpoint = window("", Pass::TableCells, 13);
            checkpoint.phase = ParsePhase::Consuming;
            let mut descriptors = Vec::new();
            while checkpoint.role == RowRole::Header {
                let start = (checkpoint.next_byte - 13) as usize;
                let end = (start + quantum).min(source.len());
                let offered = if checkpoint.phase == ParsePhase::DrainingSyntax {
                    &[][..]
                } else {
                    &source.as_bytes()[start..end]
                };
                let step =
                    advance_cells(&checkpoint, &row, offered, 13 + source.len() as u64, output)
                        .unwrap();
                assert!(step.consumed != 0 || !step.descriptors.is_empty());
                assert_eq!(
                    step.checkpoint.next_byte,
                    checkpoint.next_byte + step.consumed as u64
                );
                assert!(step.checkpoint.utf8_carry.len() <= 3);
                assert!(std::str::from_utf8(&step.checkpoint.syntax).is_ok());
                descriptors.extend(step.descriptors);
                checkpoint = step.checkpoint;
            }
            assert_eq!(descriptors, expected, "quantum {quantum} output {output}");
        }
    }
}

#[test]
fn column_admission_stops_at_delimiter_and_drains_before_reading_body() {
    let source = ":---|---:\r\nbody|row\n";
    let delimiter_length = source.find('\n').unwrap() + 1;
    for quantum in 1..=source.len() {
        let mut checkpoint = window("", Pass::Columns, 100);
        checkpoint.phase = ParsePhase::Consuming;
        let mut descriptors = Vec::new();
        while checkpoint.phase != ParsePhase::Eof {
            let start = (checkpoint.next_byte - 100) as usize;
            let end = (start + quantum).min(source.len());
            let offered = if checkpoint.phase == ParsePhase::DrainingSyntax {
                &[][..]
            } else {
                &source.as_bytes()[start..end]
            };
            let step =
                advance_columns(&checkpoint, offered, 2, 100 + source.len() as u64, 1).unwrap();
            assert!(step.consumed != 0 || !step.descriptors.is_empty());
            assert!(step.checkpoint.next_byte <= 100 + delimiter_length as u64);
            descriptors.extend(step.descriptors);
            checkpoint = step.checkpoint;
        }
        assert_eq!(checkpoint.next_byte, 100 + delimiter_length as u64);
        assert_eq!(
            descriptors,
            vec![
                Descriptor::TableColumn {
                    ordinal: 0,
                    alignment: Alignment::Left
                },
                Descriptor::TableColumn {
                    ordinal: 1,
                    alignment: Alignment::Right
                },
            ]
        );
    }
}

#[test]
fn closed_fallback_rows_skip_source_and_ordinary_rows_enforce_syntax_ceiling() {
    let row = Descriptor::TableRow {
        role: RowRole::Body,
        ordinal: 5,
        raw: ByteRange {
            start: 80,
            end: 1_000_080,
        },
        authored_cells: 2000,
        disposition: RowDisposition::WholeRowFallback,
    };
    let mut checkpoint = window("", Pass::TableCells, 80);
    checkpoint.phase = ParsePhase::Consuming;
    checkpoint.role = RowRole::Body;
    checkpoint.next_row = 5;
    let step = advance_cells(&checkpoint, &row, &[], 1_000_080, 1).unwrap();
    assert_eq!(step.consumed, 0);
    assert!(step.descriptors.is_empty());
    assert_eq!(step.checkpoint.next_byte, 1_000_080);
    assert_eq!(step.checkpoint.next_row, 6);
    assert!(step.checkpoint.syntax.is_empty());
    assert!(advance_cells(&checkpoint, &row, b"x", 1_000_080, 1).is_err());
    let row = match row {
        Descriptor::TableRow {
            role,
            ordinal,
            raw,
            authored_cells,
            ..
        } => Descriptor::TableRow {
            role,
            ordinal,
            raw,
            authored_cells,
            disposition: RowDisposition::Ordinary,
        },
        _ => unreachable!(),
    };
    let source = vec![b'x'; 16_385];
    assert!(matches!(
        advance_cells(&checkpoint, &row, &source, 1_000_080, 1),
        Err(DerivationError::InvalidAllowance)
    ));
}

#[test]
fn source_windows_reject_malformed_utf8_incomplete_eof_and_oversized_input() {
    for source in [&b"a|\xff"[..], &b"a|\xf0\x9f"[..]] {
        let row = Descriptor::TableRow {
            role: RowRole::Header,
            ordinal: 0,
            raw: ByteRange {
                start: 0,
                end: source.len() as u64,
            },
            authored_cells: 2,
            disposition: RowDisposition::Ordinary,
        };
        let mut checkpoint = window("", Pass::TableCells, 0);
        checkpoint.phase = ParsePhase::Consuming;
        assert!(matches!(
            advance_cells(&checkpoint, &row, source, source.len() as u64, 1),
            Err(DerivationError::InvalidUtf8)
        ));
    }
    let mut checkpoint = window("", Pass::Columns, 19);
    checkpoint.phase = ParsePhase::Consuming;
    let step = advance_columns(&checkpoint, b":---|---:", 2, 28, 256).unwrap();
    assert_eq!(step.checkpoint.phase, ParsePhase::Eof);
    assert_eq!(step.descriptors.len(), 2);
    let oversized = vec![b'-'; 65_537];
    assert!(matches!(
        advance_columns(&checkpoint, &oversized, 2, 100_000, 1),
        Err(DerivationError::InvalidAllowance)
    ));
    let oversized = vec![b'-'; 16_385];
    assert!(matches!(
        advance_columns(&checkpoint, &oversized, 2, 100_000, 1),
        Err(DerivationError::InvalidAllowance)
    ));
}

#[test]
fn shared_delimiter_recognition_preserves_previous_grammar() {
    fn previous(line: &str) -> Option<u64> {
        let source = table_syntax::content(line).trim();
        let source = source.strip_prefix('|').unwrap_or(source);
        let source = source.strip_suffix('|').unwrap_or(source);
        let cells: Vec<_> = source.split('|').collect();
        for cell in &cells {
            let source = cell.trim();
            let source = source.strip_prefix(':').unwrap_or(source);
            let source = source.strip_suffix(':').unwrap_or(source);
            if source.len() < 3 || !source.bytes().all(|byte| byte == b'-') {
                return None;
            }
        }
        Some(cells.len() as u64)
    }
    let alphabet = ['|', ':', '-', ' ', '\t', 'a', '\u{a0}'];
    for length in 0..=5_u32 {
        for mut encoded in 0..alphabet.len().pow(length) {
            let mut source = String::new();
            for _ in 0..length {
                source.push(alphabet[encoded % alphabet.len()]);
                encoded /= alphabet.len();
            }
            assert_eq!(
                table_syntax::delimiter_columns(&source),
                previous(&source),
                "{source:?}"
            );
        }
    }
}
