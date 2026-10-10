use super::*;
use crate::table_syntax;

fn initial() -> Checkpoint {
    Checkpoint {
        pass: Pass::TableRows,
        next_byte: 0,
        line_start: 0,
        role: RowRole::Body,
        next_row: 0,
        next_column: 0,
        phase: ParsePhase::Consuming,
        pending_cr: false,
        utf8_carry: Vec::new(),
        syntax_start: 0,
        syntax: Vec::new(),
        output_position: 0,
        escaped: false,
        in_code: false,
    }
}

fn row(source: &str, quantum: usize, columns: u64) -> Result<Descriptor, DerivationError> {
    let mut checkpoint = initial();
    loop {
        let start = checkpoint.next_byte as usize;
        let end = (start + quantum).min(source.len());
        let step = advance_row(
            &checkpoint,
            &source.as_bytes()[start..end],
            source.len() as u64,
            columns,
        )?;
        assert_eq!(
            step.checkpoint.next_byte,
            checkpoint.next_byte + step.consumed as u64
        );
        assert!(step.checkpoint.syntax.len() <= 1);
        assert!(step.checkpoint.utf8_carry.len() <= 3);
        assert!(step.descriptors.len() <= 1);
        if let Some(descriptor) = step.descriptors.into_iter().next() {
            return Ok(descriptor);
        }
        assert_ne!(step.consumed, 0);
        checkpoint = step.checkpoint;
    }
}

#[test]
fn streamed_row_matches_shared_grammar_across_every_short_syntax_and_byte_join() {
    let alphabet = ['|', '\\', '`', ' ', '\t', 'a', '\u{a0}'];
    for length in 0..=5 {
        for mut encoded in 0..alphabet.len().pow(length) {
            let mut source = String::new();
            for _ in 0..length {
                source.push(alphabet[encoded % alphabet.len()]);
                encoded /= alphabet.len();
            }
            for ending in ["", "\n", "\r\n"] {
                let source = format!("{source}{ending}");
                let expected = table_syntax::columns(&source);
                let actual = row(&source, 1, u64::MAX);
                match (expected, actual) {
                    (None, Err(DerivationError::InvalidSyntax)) => {}
                    (
                        Some(expected),
                        Ok(Descriptor::TableRow {
                            authored_cells,
                            raw,
                            disposition,
                            ..
                        }),
                    ) => {
                        assert_eq!(expected, authored_cells, "{source:?}");
                        assert_eq!(
                            raw,
                            ByteRange {
                                start: 0,
                                end: source.len() as u64
                            }
                        );
                        assert_eq!(disposition, RowDisposition::Ordinary);
                    }
                    mismatch => panic!("{source:?}: {mismatch:?}"),
                }
            }
        }
    }
}

#[test]
fn oversized_unicode_row_keeps_offsets_and_one_pending_pipe() {
    let source = format!("\u{2003}|🦀{}|`a|b`|\\| |\u{a0}\r\n", "🦀".repeat(70_000));
    for quantum in [1, 3, 8_192, 65_536] {
        let descriptor = row(&source, quantum, 4).unwrap();
        assert_eq!(
            descriptor,
            Descriptor::TableRow {
                role: RowRole::Body,
                ordinal: 0,
                raw: ByteRange {
                    start: 0,
                    end: source.len() as u64
                },
                authored_cells: table_syntax::columns(&source).unwrap(),
                disposition: RowDisposition::WholeRowFallback,
            }
        );
    }
}

#[test]
fn row_stops_at_lf_and_preserves_role_ordinals_and_fallback_threshold() {
    let source = "a|b\r\nc|d\n";
    let mut checkpoint = initial();
    checkpoint.role = RowRole::Header;
    let first = advance_row(&checkpoint, source.as_bytes(), source.len() as u64, 2).unwrap();
    assert_eq!(first.consumed, 5);
    assert_eq!(first.checkpoint.role, RowRole::Body);
    assert_eq!(first.checkpoint.next_row, 0);
    assert_eq!(first.checkpoint.line_start, 5);
    let second = advance_row(
        &first.checkpoint,
        &source.as_bytes()[5..],
        source.len() as u64,
        2,
    )
    .unwrap();
    assert_eq!(second.checkpoint.next_row, 1);
    assert_eq!(second.checkpoint.phase, ParsePhase::Eof);
    assert_eq!(second.checkpoint.line_start, 0);
    for (length, disposition) in [
        (16_384, RowDisposition::Ordinary),
        (16_385, RowDisposition::WholeRowFallback),
    ] {
        let source = format!("a|{}", "x".repeat(length - 2));
        assert!(
            matches!(row(&source, 127, 2).unwrap(), Descriptor::TableRow { disposition: actual, .. } if actual == disposition)
        );
    }
    assert!(matches!(
        row("a|b|c", 1, 2).unwrap(),
        Descriptor::TableRow {
            disposition: RowDisposition::WholeRowFallback,
            authored_cells: 3,
            ..
        }
    ));
    assert!(matches!(
        row("a|b", 1, 3).unwrap(),
        Descriptor::TableRow {
            disposition: RowDisposition::Ordinary,
            authored_cells: 2,
            ..
        }
    ));
}

#[test]
fn malformed_utf8_invalid_frontiers_header_mismatch_and_overflow_are_rejected() {
    for bytes in [&[0xff, b'|', b'a'][..], &[b'a', b'|', 0xf0][..]] {
        assert_eq!(
            advance_row(&initial(), bytes, bytes.len() as u64, 2).err(),
            Some(DerivationError::InvalidUtf8)
        );
    }
    let mut checkpoint = initial();
    checkpoint.role = RowRole::Header;
    assert_eq!(
        advance_row(&checkpoint, b"a|b", 3, 3).err(),
        Some(DerivationError::InvalidSyntax)
    );
    checkpoint.role = RowRole::Body;
    checkpoint.next_row = u64::MAX;
    assert_eq!(
        advance_row(&checkpoint, b"a|b", 3, 2).err(),
        Some(DerivationError::Overflow)
    );
    checkpoint = initial();
    checkpoint.next_byte = u64::MAX;
    checkpoint.line_start = u64::MAX;
    assert_eq!(
        advance_row(&checkpoint, b"a", u64::MAX, 2).err(),
        Some(DerivationError::Overflow)
    );
    assert_eq!(
        advance_row(&initial(), &vec![b'a'; 65_537], 65_537, 2).err(),
        Some(DerivationError::InvalidAllowance)
    );
    assert_eq!(
        advance_row(&initial(), b"", 1, 2).err(),
        Some(DerivationError::InvalidFrontier)
    );
}

#[test]
fn row_pass_covers_exact_header_delimiter_and_body_without_delimiter_descriptors() {
    let source = "|α|🦀|\r\n|:---|---:|\r\n|a|b|\n|a|b|c|\n";
    let header = ByteRange {
        start: 0,
        end: "|α|🦀|\r\n".len() as u64,
    };
    for quantum in 1..=source.len() {
        let mut checkpoint = initial();
        checkpoint.role = RowRole::Header;
        let mut descriptors = Vec::new();
        while checkpoint.phase != ParsePhase::Eof {
            let start = checkpoint.next_byte as usize;
            let end = (start + quantum).min(source.len());
            let step = advance_rows(
                &checkpoint,
                &source.as_bytes()[start..end],
                source.len() as u64,
                header,
                2,
            )
            .unwrap();
            assert!(step.consumed > 0);
            assert!(step.descriptors.len() <= 1);
            checkpoint = step.checkpoint;
            descriptors.extend(step.descriptors);
        }
        let body_start = source.find("|a|b|").unwrap() as u64;
        assert_eq!(
            descriptors,
            vec![
                Descriptor::TableRow {
                    role: RowRole::Header,
                    ordinal: 0,
                    raw: header,
                    authored_cells: 2,
                    disposition: RowDisposition::Ordinary
                },
                Descriptor::TableRow {
                    role: RowRole::Body,
                    ordinal: 0,
                    raw: ByteRange {
                        start: body_start,
                        end: body_start + 6
                    },
                    authored_cells: 2,
                    disposition: RowDisposition::Ordinary
                },
                Descriptor::TableRow {
                    role: RowRole::Body,
                    ordinal: 1,
                    raw: ByteRange {
                        start: body_start + 6,
                        end: source.len() as u64
                    },
                    authored_cells: 3,
                    disposition: RowDisposition::WholeRowFallback
                },
            ]
        );
        assert_eq!(checkpoint.next_row, 2);
        assert!(checkpoint.syntax.is_empty());
        assert!(checkpoint.utf8_carry.is_empty());
    }
}

#[test]
fn row_pass_rejects_wrong_header_and_delimiter_and_accepts_zero_body_rows() {
    let mut checkpoint = initial();
    checkpoint.role = RowRole::Header;
    assert_eq!(
        advance_rows(
            &checkpoint,
            b"a|b\nc|d\n",
            8,
            ByteRange { start: 0, end: 5 },
            2
        )
        .err(),
        Some(DerivationError::InvalidSyntax)
    );
    let header = ByteRange { start: 0, end: 4 };
    for delimiter in ["---|bad\n", "---|---|---\n", "", "🦀|---\n"] {
        let source = format!("a|b\n{delimiter}");
        if source.len() == 4 {
            assert!(advance_rows(&checkpoint, source.as_bytes(), 4, header, 2).is_err());
            continue;
        }
        let first = advance_rows(
            &checkpoint,
            source.as_bytes(),
            source.len() as u64,
            header,
            2,
        )
        .unwrap();
        assert_eq!(
            advance_rows(
                &first.checkpoint,
                &source.as_bytes()[4..],
                source.len() as u64,
                header,
                2
            )
            .err(),
            Some(DerivationError::InvalidSyntax)
        );
    }
    for delimiter in ["---|---", "---|---\r\n"] {
        let source = format!("a|b\n{delimiter}");
        let first = advance_rows(
            &checkpoint,
            source.as_bytes(),
            source.len() as u64,
            header,
            2,
        )
        .unwrap();
        let second = advance_rows(
            &first.checkpoint,
            &source.as_bytes()[4..],
            source.len() as u64,
            header,
            2,
        )
        .unwrap();
        assert!(second.descriptors.is_empty());
        assert_eq!(second.checkpoint.phase, ParsePhase::Eof);
        assert_eq!(second.checkpoint.next_row, 0);
    }
}
