use super::*;
use crate::resource_structure::{DerivationError, display::read_fragment};

fn fixture(bytes: &[u8]) -> (Records, Binding) {
    let mut records = Records::default();
    records.chunk_at(1, 0, bytes);
    records.text(0, bytes.len() as u64, 1, 0, 0);
    (
        records,
        binding(composer(bytes.len() as u64), 0, bytes.len() as u64),
    )
}

#[test]
fn late_fragment_adjusts_only_the_containing_scalar_and_preserves_mapping() {
    let mut bytes = vec![b'x'; 50_000];
    bytes.extend_from_slice("😀tail".as_bytes());
    let (records, binding) = fixture(&bytes);
    let content = ByteRange {
        start: 0,
        end: bytes.len() as u64,
    };
    for offset in 50_000..50_004 {
        records.calls.borrow_mut().clear();
        let fragment = read_fragment(
            &records,
            &mut Ledger::default(),
            &binding,
            content.end,
            content,
            offset,
            4,
        )
        .unwrap();
        assert_eq!(fragment.text, "😀");
        assert_eq!(
            fragment.source,
            ByteRange {
                start: 50_000,
                end: 50_004
            }
        );
        assert_eq!(fragment.next_offset, 50_004);
        assert_eq!(
            fragment.remaining,
            ByteRange {
                start: 50_004,
                end: content.end
            }
        );
        assert_eq!(records.calls.borrow().len(), 3);
    }
}

#[test]
fn fragment_cap_and_continuation_do_not_split_scalars() {
    let mut bytes = vec![b'x'; 8_191];
    bytes.extend_from_slice("€tail".as_bytes());
    let (records, binding) = fixture(&bytes);
    let content = ByteRange {
        start: 0,
        end: bytes.len() as u64,
    };
    let first = read_fragment(
        &records,
        &mut Ledger::default(),
        &binding,
        content.end,
        content,
        0,
        8_192,
    )
    .unwrap();
    assert_eq!(first.text.len(), 8_191);
    assert_eq!(first.next_offset, 8_191);
    let second = read_fragment(
        &records,
        &mut Ledger::default(),
        &binding,
        content.end,
        content,
        first.next_offset,
        8_192,
    )
    .unwrap();
    assert_eq!(second.text, "€tail");
    assert_eq!(second.source.start, first.source.end);
    assert_eq!(second.remaining.start, second.remaining.end);
    assert_eq!(second.next_offset, content.end);
}

#[test]
fn malformed_interior_and_terminal_incompleteness_are_never_lossy() {
    for bytes in [
        b"a\xffb".as_slice(),
        b"a\xe2\x82".as_slice(),
        b"\x80abc".as_slice(),
    ] {
        let (records, binding) = fixture(bytes);
        assert!(matches!(
            read_fragment(
                &records,
                &mut Ledger::default(),
                &binding,
                bytes.len() as u64,
                ByteRange {
                    start: 0,
                    end: bytes.len() as u64
                },
                0,
                8_192
            ),
            Err(ConstructionError::Derivation(DerivationError::InvalidUtf8))
        ));
    }
    let (records, binding) = fixture("€".as_bytes());
    assert!(matches!(
        read_fragment(
            &records,
            &mut Ledger::default(),
            &binding,
            3,
            ByteRange { start: 0, end: 3 },
            1,
            2
        ),
        Err(ConstructionError::Limit)
    ));
}

#[test]
fn split_chunks_join_exactly_and_resource_offsets_remain_absolute() {
    let mut records = Records::default();
    records.chunk_at(1, 0, b"xxA\xe2");
    records.chunk_at(2, 4, b"\x82\xacB");
    records.text(0, 4, 1, 0, 0);
    records.text(4, 7, 2, 4, 4);
    let binding = binding(composer(7), 2, 7);
    let fragment = read_fragment(
        &records,
        &mut Ledger::default(),
        &binding,
        7,
        ByteRange { start: 0, end: 5 },
        2,
        4,
    )
    .unwrap();
    assert_eq!(fragment.text, "€B");
    assert_eq!(fragment.source, ByteRange { start: 3, end: 7 });
    assert_eq!(fragment.next_offset, 5);
}

#[test]
fn invalid_limits_offsets_and_empty_completion_do_not_read_source() {
    let records = Records::default();
    let binding = binding(composer(8), 2, 8);
    let content = ByteRange { start: 1, end: 5 };
    for (offset, limit) in [(0, 0), (0, 8_193), (5, 8_192), (u64::MAX, 1)] {
        let mut ledger = Ledger::default();
        assert!(read_fragment(&records, &mut ledger, &binding, 8, content, offset, limit).is_err());
        assert_eq!(ledger, Ledger::default());
    }
    let fragment = read_fragment(
        &records,
        &mut Ledger::default(),
        &binding,
        8,
        content,
        4,
        8_192,
    )
    .unwrap();
    assert_eq!(fragment.text, "");
    assert_eq!(fragment.source, ByteRange { start: 7, end: 7 });
    assert!(records.calls.borrow().is_empty());
}
