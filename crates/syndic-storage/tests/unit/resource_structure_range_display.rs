use super::*;
use crate::resource_structure::{
    query_display::{DisplayRequest, Target},
    query_response::{Item, OrdinalRange, Position, Request, ResponseLimits},
};

fn code_request(end: u64) -> Request {
    Request::Code {
        lines: OrdinalRange { start: 0, end },
    }
}

fn rectangle_pages(
    fixture: &Fixture,
    binding: &Binding,
    request: Request,
) -> (
    Vec<Item>,
    Vec<crate::resource_structure::query_response::Presentation>,
) {
    let mut cursor = None;
    let mut items = Vec::new();
    let mut presentations = Vec::new();
    for _ in 0..10 {
        let mut query = open(fixture, binding);
        let response = query
            .authenticated_range_display(
                fixture,
                binding.range.end,
                request,
                cursor,
                ResponseLimits::default(),
            )
            .unwrap();
        assert_eq!(response.encode().unwrap().len(), response.encoded_bytes);
        assert!(response.encoded_bytes <= 65_536);
        assert!(query.ledger.retained <= 524_288 && query.ledger.records <= 520);
        assert!(!response.items.is_empty());
        cursor = Some(response.cursor);
        items.extend(response.items);
        presentations.extend(response.presentations.unwrap());
        if response.request_complete {
            assert!(response.resource_complete);
            return (items, presentations);
        }
    }
    panic!("rectangle did not finish within its bounded test pages");
}

#[test]
fn code_range_resumes_exact_fragment_then_advances_to_next_line() {
    let source = format!("{}€tail\r\nlast\n", "x".repeat(8_191));
    let (mut fixture, binding) = Fixture::code(source.as_bytes(), 2);
    fixture.source(&binding, source.as_bytes());
    let request = code_request(2);
    let first = open(&fixture, &binding)
        .authenticated_range_display(
            &fixture,
            binding.range.end,
            request,
            None,
            ResponseLimits::default(),
        )
        .unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.cursor.next, Position::Code(0));
    assert_eq!(first.cursor.fragment_offset, 8_191);
    assert!(!first.request_complete && !first.resource_complete);
    assert_eq!(
        first.presentations.unwrap()[0]
            .fragment
            .as_ref()
            .unwrap()
            .text
            .len(),
        8_191
    );
    let rest = open(&fixture, &binding)
        .authenticated_range_display(
            &fixture,
            binding.range.end,
            request,
            Some(first.cursor),
            ResponseLimits::default(),
        )
        .unwrap();
    let presentations = rest.presentations.as_ref().unwrap();
    assert_eq!(presentations[0].fragment.as_ref().unwrap().text, "€tail");
    assert_eq!(presentations[1].fragment.as_ref().unwrap().text, "last");
    assert_eq!(rest.cursor.next, Position::Code(2));
    assert_eq!(rest.cursor.fragment_offset, 0);
    assert!(rest.request_complete && rest.resource_complete);
    assert_eq!(rest.encoded_bytes, rest.encode().unwrap().len());
    fixture.reads.borrow_mut().clear();
    let empty = open(&fixture, &binding)
        .authenticated_range_display(
            &fixture,
            binding.range.end,
            request,
            Some(rest.cursor),
            ResponseLimits::default(),
        )
        .unwrap();
    assert!(empty.items.is_empty() && empty.request_complete);
    assert_eq!(fixture.reads.borrow().len(), 5);
}

#[test]
fn exact_payload_ceiling_retains_code_fragment_position() {
    let (mut fixture, binding) = Fixture::code(b"abc\n", 1);
    fixture.source(&binding, b"abc\n");
    let request = code_request(1);
    let full = open(&fixture, &binding)
        .authenticated_range_display(
            &fixture,
            binding.range.end,
            request,
            None,
            ResponseLimits::default(),
        )
        .unwrap();
    let bytes = full.encoded_bytes - 2;
    let first = open(&fixture, &binding)
        .authenticated_range_display(
            &fixture,
            binding.range.end,
            request,
            None,
            ResponseLimits {
                bytes,
                ..ResponseLimits::default()
            },
        )
        .unwrap();
    assert_eq!(first.encoded_bytes, bytes);
    assert_eq!(first.encode().unwrap().len(), bytes);
    assert_eq!(first.cursor.next, Position::Code(0));
    assert_eq!(first.cursor.fragment_offset, 1);
    assert_eq!(
        first.presentations.as_ref().unwrap()[0]
            .fragment
            .as_ref()
            .unwrap()
            .text,
        "a"
    );
    let rest = open(&fixture, &binding)
        .authenticated_range_display(
            &fixture,
            binding.range.end,
            request,
            Some(first.cursor),
            ResponseLimits::default(),
        )
        .unwrap();
    assert_eq!(
        rest.presentations.unwrap()[0]
            .fragment
            .as_ref()
            .unwrap()
            .text,
        "bc"
    );
    assert!(rest.request_complete);
}

#[test]
fn source_acquisition_capacity_returns_prefix_and_fresh_query_resumes() {
    let source = format!("{}{}", "x\n".repeat(30), "z".repeat(70_000));
    let (mut fixture, binding) = Fixture::code(source.as_bytes(), 31);
    fixture.source(&binding, source.as_bytes());
    let request = code_request(30);
    let mut cursor = None;
    let mut count = 0;
    for page in 0..30 {
        let mut query = open(&fixture, &binding);
        let response = query
            .authenticated_range_display(
                &fixture,
                binding.range.end,
                request,
                cursor,
                ResponseLimits::default(),
            )
            .unwrap();
        assert!(!response.items.is_empty());
        assert_eq!(response.encoded_bytes, response.encode().unwrap().len());
        assert!(query.ledger.retained <= 524_288 && query.ledger.read_bytes <= 524_288);
        if page == 0 {
            assert!(!response.request_complete);
        }
        for value in response.presentations.unwrap() {
            assert_eq!(value.fragment.unwrap().text, "x");
        }
        count += response.items.len();
        assert!(!response.resource_complete);
        cursor = Some(response.cursor);
        if response.request_complete {
            assert_eq!(count, 30);
            assert_eq!(response.cursor.next, Position::Code(30));
            return;
        }
    }
    panic!("source capacity continuation did not complete");
}

#[test]
fn rectangle_advances_past_truncated_cell_and_returns_fallback_once() {
    let source = format!(
        "a|b\n:---|---:\n{}|z\nu|v|{}\n",
        "x".repeat(10_000),
        "q".repeat(10_000)
    );
    let (mut fixture, binding) = Fixture::table(source.as_bytes(), 2, 2);
    fixture.source(&binding, source.as_bytes());
    let request = Request::Table {
        role: RowRole::Body,
        rows: OrdinalRange { start: 0, end: 2 },
        columns: OrdinalRange { start: 0, end: 2 },
    };
    let (items, presentations) = rectangle_pages(&fixture, &binding, request);
    assert_eq!(items.len(), 3);
    let cell = presentations[0].fragment.as_ref().unwrap();
    assert_eq!(cell.text.len(), 8_192);
    assert_eq!(cell.next_offset, 8_192);
    assert!(cell.remaining.start < cell.remaining.end);
    assert_eq!(presentations[0].alignment, Some(Alignment::Left));
    assert!(presentations[2].literal_fallback);
    assert!(matches!(
        &items[2],
        Item::Descriptor(IndexedDescriptor {
            descriptor: Descriptor::TableRow {
                disposition: RowDisposition::WholeRowFallback,
                ..
            },
            ..
        })
    ));
    let tail = open(&fixture, &binding)
        .authenticated_display(
            &fixture,
            binding.range.end,
            DisplayRequest {
                target: Target::Cell {
                    role: RowRole::Body,
                    row: 0,
                    column: 0,
                },
                offset: cell.next_offset,
            },
            None,
            65_536,
        )
        .unwrap();
    assert_eq!(tail.fragment.unwrap().text.len(), 1_808);
}

#[test]
fn display_cursor_rejects_stale_root_and_impossible_fragment_before_reads() {
    let (mut fixture, binding) = Fixture::code(b"abc\n", 1);
    fixture.source(&binding, b"abc\n");
    let request = code_request(1);
    let full = open(&fixture, &binding)
        .authenticated_range_display(
            &fixture,
            binding.range.end,
            request,
            None,
            ResponseLimits::default(),
        )
        .unwrap();
    let mut stale = full.cursor;
    stale.root_digest[0] ^= 1;
    let mut bad = full.cursor;
    bad.fragment_offset = 1;
    let mut query = open(&fixture, &binding);
    for cursor in [stale, bad] {
        let reads = fixture.reads.borrow().len();
        let ledger = query.ledger.clone();
        assert!(
            query
                .authenticated_range_display(
                    &fixture,
                    binding.range.end,
                    request,
                    Some(cursor),
                    ResponseLimits::default()
                )
                .is_err()
        );
        assert_eq!(fixture.reads.borrow().len(), reads);
        assert_eq!(query.ledger, ledger);
    }
}

#[test]
fn empty_and_missing_cell_presentations_preserve_source_distinction() {
    let source = b"a|b|c\n---|---|---\nx||z\nu|v\n";
    let (mut fixture, binding) = Fixture::table(source, 2, 3);
    fixture.source(&binding, source);
    let (items, presentations) = rectangle_pages(
        &fixture,
        &binding,
        Request::Table {
            role: RowRole::Body,
            rows: OrdinalRange { start: 0, end: 2 },
            columns: OrdinalRange { start: 1, end: 3 },
        },
    );
    assert_eq!(presentations[0].fragment.as_ref().unwrap().text, "");
    assert!(presentations[3].fragment.is_none());
    assert!(matches!(
        items[3],
        Item::Missing {
            row: 1,
            column: 2,
            ..
        }
    ));
}
