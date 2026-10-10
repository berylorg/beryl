use super::*;
use crate::resource_structure::query_display::{DisplayRequest, Target};

#[test]
fn long_code_fragments_resume_under_exact_binding_and_payload_ceiling() {
    let source = format!("{}€tail\r\n", "x".repeat(20_000));
    let (mut fixture, binding) = Fixture::code(source.as_bytes(), 1);
    fixture.source(&binding, source.as_bytes());
    let request = DisplayRequest {
        target: Target::Code(0),
        offset: 19_999,
    };
    let mut query = open(&fixture, &binding);
    let first = query
        .authenticated_display(&fixture, binding.range.end, request, None, 65_536)
        .unwrap();
    assert_eq!(first.fragment.as_ref().unwrap().text, "x€tail");
    assert!(first.complete);
    assert_eq!(
        first.fragment.as_ref().unwrap().source,
        range(20_004, 20_012)
    );
    assert_eq!(first.cursor.next_offset, 20_007);
    assert_eq!(first.encode().unwrap().len(), first.encoded_bytes);
    let budget = first.encoded_bytes - 7;
    let mut query = open(&fixture, &binding);
    let first = query
        .authenticated_display(&fixture, binding.range.end, request, None, budget)
        .unwrap();
    assert_eq!(first.fragment.as_ref().unwrap().text, "x");
    assert!(!first.complete);
    assert_eq!(first.encoded_bytes, budget);
    let mut query = open(&fixture, &binding);
    let next = query
        .authenticated_display(
            &fixture,
            binding.range.end,
            request,
            Some(first.cursor),
            65_536,
        )
        .unwrap();
    assert_eq!(next.fragment.as_ref().unwrap().text, "€tail");
    assert!(next.complete);
    fixture.reads.borrow_mut().clear();
    let end = query
        .authenticated_display(
            &fixture,
            binding.range.end,
            request,
            Some(next.cursor),
            65_536,
        )
        .unwrap();
    assert!(end.fragment.unwrap().text.is_empty());
    assert!(
        fixture
            .reads
            .borrow()
            .iter()
            .all(|(name, _)| !name.starts_with("content-"))
    );
}

#[test]
fn display_cursor_substitution_fails_before_any_record_acquisition() {
    let (mut fixture, binding) = Fixture::code(b"abc\n", 1);
    fixture.source(&binding, b"abc\n");
    let request = DisplayRequest {
        target: Target::Code(0),
        offset: 0,
    };
    let mut query = open(&fixture, &binding);
    let cursor = query
        .authenticated_display(&fixture, binding.range.end, request, None, 65_536)
        .unwrap()
        .cursor;
    for changed in [
        {
            let mut c = cursor;
            c.root[0] ^= 1;
            c
        },
        {
            let mut c = cursor;
            c.build.operation[0] ^= 1;
            c
        },
        {
            let mut c = cursor;
            c.request.offset = 1;
            c
        },
        {
            let mut c = cursor;
            c.request.target = Target::Code(1);
            c
        },
    ] {
        fixture.reads.borrow_mut().clear();
        let before = query.ledger;
        assert!(matches!(
            query.authenticated_display(
                &fixture,
                binding.range.end,
                request,
                Some(changed),
                65_536
            ),
            Err(ConstructionError::Collision)
        ));
        assert!(fixture.reads.borrow().is_empty());
        assert_eq!(query.ledger, before);
    }
}

#[test]
fn cell_display_retains_alignment_empty_missing_and_literal_fallback() {
    let source = b"a|b|c\n:---|---:|:---:\nx\\|y||z\nu|v\none | two | three | surplus\n";
    let (mut fixture, binding) = Fixture::table(source, 3, 3);
    fixture.source(&binding, source);
    let mut query = open(&fixture, &binding);
    let request = |row, column| DisplayRequest {
        target: Target::Cell {
            role: RowRole::Body,
            row,
            column,
        },
        offset: 0,
    };
    let escaped = query
        .authenticated_display(&fixture, binding.range.end, request(0, 0), None, 65_536)
        .unwrap();
    assert_eq!(escaped.fragment.unwrap().text, "x\\|y");
    assert!(escaped.literal_fallback);
    assert_eq!(escaped.alignment, Some(Alignment::Left));
    let mut query = open(&fixture, &binding);
    let empty = query
        .authenticated_display(&fixture, binding.range.end, request(0, 1), None, 65_536)
        .unwrap();
    assert_eq!(empty.fragment.unwrap().text, "");
    assert!(empty.descriptor.is_some());
    let mut query = open(&fixture, &binding);
    let missing = query
        .authenticated_display(&fixture, binding.range.end, request(1, 2), None, 65_536)
        .unwrap();
    assert!(missing.fragment.is_none());
    assert!(missing.descriptor.is_none());
    assert!(missing.complete);
    let mut query = open(&fixture, &binding);
    let fallback = query
        .authenticated_display(&fixture, binding.range.end, request(2, 2), None, 65_536)
        .unwrap();
    assert_eq!(
        fallback.fragment.unwrap().text,
        "one | two | three | surplus\n"
    );
    assert!(fallback.literal_fallback);
    assert!(matches!(
        fallback.descriptor.unwrap().descriptor,
        Descriptor::TableRow {
            disposition: RowDisposition::WholeRowFallback,
            ..
        }
    ));
}

#[test]
fn oversized_cell_is_bounded_and_fallback_row_supports_direct_late_demand() {
    let source = format!(
        "a|b\n---|---\n{}|z\nu|v|{}\n",
        "x".repeat(10_000),
        "q".repeat(10_000)
    );
    let (mut fixture, binding) = Fixture::table(source.as_bytes(), 2, 2);
    fixture.source(&binding, source.as_bytes());
    let request = DisplayRequest {
        target: Target::Cell {
            role: RowRole::Body,
            row: 0,
            column: 0,
        },
        offset: 0,
    };
    let mut query = open(&fixture, &binding);
    let first = query
        .authenticated_display(&fixture, binding.range.end, request, None, 65_536)
        .unwrap();
    assert_eq!(first.fragment.as_ref().unwrap().text.len(), 8_192);
    assert!(!first.complete);
    assert_eq!(first.cursor.next_offset, 8_192);
    let mut query = open(&fixture, &binding);
    let next = query
        .authenticated_display(
            &fixture,
            binding.range.end,
            request,
            Some(first.cursor),
            65_536,
        )
        .unwrap();
    assert_eq!(next.fragment.unwrap().text.len(), 1_808);
    assert!(next.complete);
    let mut query = open(&fixture, &binding);
    fixture.reads.borrow_mut().clear();
    let fallback = query
        .authenticated_display(
            &fixture,
            binding.range.end,
            DisplayRequest {
                target: Target::Cell {
                    role: RowRole::Body,
                    row: 1,
                    column: 1,
                },
                offset: 9_999,
            },
            None,
            65_536,
        )
        .unwrap();
    assert_eq!(fallback.fragment.unwrap().text, "qqqqq\n");
    assert!(fallback.literal_fallback);
    assert!(
        fixture
            .reads
            .borrow()
            .iter()
            .all(|(name, _)| *name != TableCellsFamily::NAME)
    );
}

#[test]
fn display_refuses_small_payload_before_source_and_ledger_exhaustion_before_lookup() {
    let (mut fixture, binding) = Fixture::code("€\n".as_bytes(), 1);
    fixture.source(&binding, "€\n".as_bytes());
    let request = DisplayRequest {
        target: Target::Code(0),
        offset: 0,
    };
    let mut query = open(&fixture, &binding);
    let response = query
        .authenticated_display(&fixture, binding.range.end, request, None, 65_536)
        .unwrap();
    let mut query = open(&fixture, &binding);
    fixture.reads.borrow_mut().clear();
    assert!(matches!(
        query.authenticated_display(
            &fixture,
            binding.range.end,
            request,
            None,
            response.encoded_bytes - 3
        ),
        Err(ConstructionError::Limit)
    ));
    assert!(
        fixture
            .reads
            .borrow()
            .iter()
            .all(|(name, _)| !name.starts_with("content-"))
    );
    let mut query = open(&fixture, &binding);
    query.ledger.retained = 524_288;
    let before = query.ledger;
    fixture.reads.borrow_mut().clear();
    assert!(matches!(
        query.authenticated_display(&fixture, binding.range.end, request, None, 65_536),
        Err(ConstructionError::Limit)
    ));
    assert!(fixture.reads.borrow().is_empty());
    assert_eq!(query.ledger, before);
}
