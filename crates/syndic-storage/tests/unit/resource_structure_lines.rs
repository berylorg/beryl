use super::*;

fn initial() -> Checkpoint {
    Checkpoint {
        pass: Pass::CodeLines,
        next_byte: 0,
        line_start: 0,
        role: RowRole::Header,
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

fn lines(source: &[u8], quantum: usize, output: usize) -> (Vec<Descriptor>, Checkpoint) {
    let mut checkpoint = initial();
    let mut descriptors = Vec::new();
    while checkpoint.phase != ParsePhase::Eof {
        let start = checkpoint.next_byte as usize;
        let end = start.saturating_add(quantum).min(source.len());
        let step = advance_code(
            &checkpoint,
            &source[start..end],
            source.len() as u64,
            output,
        )
        .unwrap();
        assert!(
            step.consumed != 0
                || !step.descriptors.is_empty()
                || step.checkpoint.phase != checkpoint.phase
        );
        assert_eq!(
            step.checkpoint.next_byte,
            checkpoint.next_byte + step.consumed as u64
        );
        assert!(step.descriptors.len() <= output);
        checkpoint = step.checkpoint;
        descriptors.extend(step.descriptors);
    }
    (descriptors, checkpoint)
}

fn range(start: u64, end: u64) -> ByteRange {
    ByteRange { start, end }
}

#[test]
fn restart_at_every_byte_preserves_crlf_empty_lines_unicode_and_unterminated_tail() {
    let source = "\nα\r\n\r\n🦀\r".as_bytes();
    let expected = vec![
        Descriptor::CodeLine {
            ordinal: 0,
            raw: range(0, 1),
            content: range(0, 0),
            terminator: range(0, 1),
        },
        Descriptor::CodeLine {
            ordinal: 1,
            raw: range(1, 5),
            content: range(1, 3),
            terminator: range(3, 5),
        },
        Descriptor::CodeLine {
            ordinal: 2,
            raw: range(5, 7),
            content: range(5, 5),
            terminator: range(5, 7),
        },
        Descriptor::CodeLine {
            ordinal: 3,
            raw: range(7, 12),
            content: range(7, 12),
            terminator: range(12, 12),
        },
    ];
    for quantum in 1..=source.len() {
        for output in 1..=4 {
            let (actual, checkpoint) = lines(source, quantum, output);
            assert_eq!(actual, expected, "quantum {quantum}, output {output}");
            assert_eq!(checkpoint.next_row, 4);
            assert!(checkpoint.utf8_carry.is_empty());
        }
    }
}

#[test]
fn empty_source_and_trailing_terminators_do_not_synthesize_tail_lines() {
    for (source, count) in [
        (b"".as_slice(), 0),
        (b"x\n".as_slice(), 1),
        (b"\n\n".as_slice(), 2),
        (b"\r".as_slice(), 1),
    ] {
        assert_eq!(lines(source, 1, 1).0.len(), count);
    }
}

#[test]
fn long_line_retains_only_scalar_frontier_and_utf8_suffix_across_quanta() {
    let source = "🦀".repeat(100_000);
    let source = source.as_bytes();
    let mut checkpoint = initial();
    let mut descriptors = Vec::new();
    while checkpoint.phase != ParsePhase::Eof {
        let start = checkpoint.next_byte as usize;
        let end = (start + 65_535).min(source.len());
        let step =
            advance_code(&checkpoint, &source[start..end], source.len() as u64, 256).unwrap();
        assert!(step.checkpoint.syntax.is_empty());
        assert!(step.checkpoint.utf8_carry.len() <= 3);
        descriptors.extend(step.descriptors);
        checkpoint = step.checkpoint;
    }
    assert_eq!(
        descriptors,
        vec![Descriptor::CodeLine {
            ordinal: 0,
            raw: range(0, 400_000),
            content: range(0, 400_000),
            terminator: range(400_000, 400_000)
        }]
    );
}

#[test]
fn source_and_descriptor_quanta_stop_at_exact_consumed_frontier() {
    let source = b"a\nb\nc\n";
    let step = advance_code(&initial(), source, 6, 1).unwrap();
    assert_eq!(step.consumed, 2);
    assert_eq!(step.checkpoint.next_byte, 2);
    assert_eq!(step.descriptors.len(), 1);
    assert_eq!(step.checkpoint.phase, ParsePhase::Consuming);
    let (actual, _) = lines(&b"x\n".repeat(1_000), 65_536, 256);
    assert_eq!(actual.len(), 1_000);
    assert_eq!(actual.last().unwrap().coordinate().2, 999);
}

#[test]
fn malformed_utf8_incomplete_eof_and_overflow_refuse_without_advancing_input() {
    let initial = initial();
    assert!(matches!(
        advance_code(&initial, &[0xFF], 1, 1),
        Err(DerivationError::InvalidUtf8)
    ));
    assert!(matches!(
        advance_code(&initial, &[0xF0, 0x9F], 2, 1),
        Err(DerivationError::InvalidUtf8)
    ));
    let step = advance_code(&initial, &[0xF0, 0x9F], 4, 1).unwrap();
    assert!(matches!(
        advance_code(&step.checkpoint, b"x\n", 4, 1),
        Err(DerivationError::InvalidUtf8)
    ));
    assert!(matches!(
        advance_code(&initial, &[0; 65_537], 65_537, 1),
        Err(DerivationError::InvalidAllowance)
    ));
    assert!(matches!(
        advance_code(&initial, b"", 1, 1),
        Err(DerivationError::InvalidFrontier)
    ));
    let mut checkpoint = initial;
    checkpoint.next_byte = u64::MAX;
    checkpoint.line_start = u64::MAX;
    assert!(matches!(
        advance_code(&checkpoint, b"x", u64::MAX, 1),
        Err(DerivationError::Overflow)
    ));
    checkpoint.next_byte = 0;
    checkpoint.line_start = 0;
    checkpoint.next_row = u64::MAX;
    assert!(matches!(
        advance_code(&checkpoint, b"\n", 1, 1),
        Err(DerivationError::Overflow)
    ));
}
