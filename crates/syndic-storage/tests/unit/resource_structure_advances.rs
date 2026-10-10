use super::*;
use crate::resource_structure::{
    advance::prepare_code,
    construction::{ConstructionError, initial_build},
    transition::{prepare_control, validate_receipt},
};

#[test]
fn code_advance_publishes_byte_equal_page_locators_and_absolute_raw_anchors() {
    let before = initial_build(key(), binding().metadata).unwrap();
    let prepared = prepare_code(&before, b"one\nthree\n", 256).unwrap();
    let page = prepared.page.as_ref().unwrap();
    assert_eq!(page.key.ordinal, 1);
    assert_eq!(page.prior_chain, before.chain);
    assert_eq!(
        prepared.build.chain,
        chain_link(&before.chain, &page.digest)
    );
    assert_eq!(prepared.build.counts, [2, 0, 0, 0]);
    assert_eq!(prepared.build.checkpoint.phase, ParsePhase::Eof);
    assert_eq!(
        prepared.receipt.page,
        Some(Reference {
            key: page.key,
            digest: page.digest
        })
    );
    validate_receipt(&prepared.build, &prepared.receipt).unwrap();
    assert_eq!(decode_page(&encode_page(page).unwrap()).unwrap(), *page);
    for (slot, (line, anchor)) in prepared.lines.iter().zip(&prepared.anchors).enumerate() {
        assert_eq!(line.key, CoordinateKey::CodeLine(before.key, slot as u64));
        assert_eq!(
            (line.page, line.slot, line.digest),
            (page.key.ordinal, slot as u64, page.digest)
        );
        let raw = page.descriptors[slot].raw().unwrap();
        assert_eq!(anchor.key.start, before.binding.range.start + raw.start);
        assert_eq!(anchor.end, before.binding.range.start + raw.end);
        assert_eq!(
            (anchor.page, anchor.slot, anchor.digest),
            (line.page, line.slot, line.digest)
        );
        assert_eq!(
            CodeLinesFamily::decode_value(&CodeLinesFamily::encode_value(line).unwrap()).unwrap(),
            *line
        );
        assert_eq!(
            SourceAnchorsFamily::decode_value(&encode_anchor(anchor).unwrap()).unwrap(),
            *anchor
        );
    }
    let sealed = prepare_control(&prepared.build, Command::PassChange).unwrap();
    let completed = prepare_control(&sealed.build, Command::Complete).unwrap();
    let (root, _) = completed.selection.unwrap();
    assert_eq!(root.pages, 1);
    assert_eq!(root.chain, prepared.build.chain);
}

#[test]
fn source_only_advance_keeps_page_frontier_then_resumes_exact_line_ranges() {
    let before = initial_build(key(), binding().metadata).unwrap();
    let first = prepare_code(&before, b"on", 1).unwrap();
    assert!(first.page.is_none());
    assert!(first.receipt.page.is_none());
    assert!(first.lines.is_empty() && first.anchors.is_empty());
    assert_eq!(first.build.chain, before.chain);
    assert_eq!(first.build.next_page, before.next_page);
    assert_eq!(first.build.checkpoint.next_byte, 2);
    validate_receipt(&first.build, &first.receipt).unwrap();
    let resumed = decode_build(&encode_build(&first.build).unwrap()).unwrap();
    let second = prepare_code(&resumed, b"e\nthree\n", 1).unwrap();
    assert_eq!(second.build.checkpoint.next_byte, 4);
    assert_eq!(second.receipt.prior, first.build.progress);
    assert_eq!(
        second.page.unwrap().descriptors[0].raw().unwrap(),
        range(0, 4)
    );
    let third = prepare_code(&second.build, b"three\n", 1).unwrap();
    let page = third.page.unwrap();
    assert_eq!(page.key.ordinal, 2);
    assert_eq!(page.descriptors[0].coordinate().2, 1);
    assert_eq!(page.descriptors[0].raw().unwrap(), range(4, 10));
    assert_eq!(third.build.counts, [2, 0, 0, 0]);
    assert_eq!(third.build.checkpoint.phase, ParsePhase::Eof);
}

#[test]
fn code_advance_rejects_metadata_disagreement_bad_source_and_terminal_frontiers() {
    let before = initial_build(key(), binding().metadata).unwrap();
    for bytes in [&b"abcdefghij"[..], &b"\n\n\n\n\n\n\n\n\n\n"[..]] {
        assert!(matches!(
            prepare_code(&before, bytes, 256),
            Err(ConstructionError::Unavailable)
        ));
    }
    assert!(prepare_code(&before, b"\xff", 1).is_err());
    assert!(prepare_code(&before, b"", 1).is_err());
    assert!(prepare_code(&before, b"a", 0).is_err());
    let mut corrupt = prepare_code(&before, b"one\n", 1).unwrap().build;
    corrupt.checkpoint.next_row = 0;
    assert!(matches!(
        prepare_code(&corrupt, b"a", 1),
        Err(ConstructionError::Codec(_))
    ));
    let terminal = prepare_control(&before, Command::Cancel).unwrap();
    assert!(matches!(
        prepare_code(&terminal.build, b"a", 1),
        Err(ConstructionError::Collision)
    ));
    let eof = prepare_code(&before, b"one\nthree\n", 256).unwrap();
    assert!(prepare_code(&eof.build, b"", 1).is_err());
}
