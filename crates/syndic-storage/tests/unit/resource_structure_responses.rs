use super::*;
use crate::resource_structure::query_response::{
    Item, OrdinalRange, Position, Request, ResponseLimits,
};

fn ordinals(start: u64, end: u64) -> OrdinalRange {
    OrdinalRange { start, end }
}

#[test]
fn maximum_descriptor_page_stops_at_exact_256_and_can_resume() {
    let (fixture, binding) = Fixture::code("x\n".repeat(300).as_bytes(), 300);
    let request = Request::Code {
        lines: ordinals(0, u64::MAX),
    };
    let first = open(&fixture, &binding)
        .range(&fixture, request, None, ResponseLimits::default())
        .unwrap();
    assert_eq!(first.items.len(), 256);
    assert_eq!(first.cursor.next, Position::Code(256));
    assert!(!first.request_complete && !first.resource_complete);
    assert_eq!(first.encoded_bytes, first.encode().unwrap().len());
    assert!(first.encoded_bytes <= 65_536);
    let rest = open(&fixture, &binding)
        .range(
            &fixture,
            request,
            Some(first.cursor),
            ResponseLimits::default(),
        )
        .unwrap();
    assert_eq!(rest.items.len(), 44);
    assert!(rest.request_complete && rest.resource_complete);
}

#[test]
fn rectangle_resume_keeps_exact_column_and_header_completion_is_not_body_eof() {
    let (fixture, binding) = Fixture::table(b"a|b|c\n---|---|---\nx|y|z\n", 1, 3);
    let request = Request::Table {
        role: RowRole::Body,
        rows: ordinals(0, 1),
        columns: ordinals(1, 3),
    };
    let limits = ResponseLimits {
        descriptors: 1,
        ..ResponseLimits::default()
    };
    let first = open(&fixture, &binding)
        .range(&fixture, request, None, limits)
        .unwrap();
    assert_eq!(first.cursor.next, Position::Table { row: 0, column: 2 });
    let second = open(&fixture, &binding)
        .range(&fixture, request, Some(first.cursor), limits)
        .unwrap();
    assert_eq!(second.cursor.next, Position::Table { row: 1, column: 1 });
    assert!(second.request_complete && second.resource_complete);
    let header = open(&fixture, &binding)
        .range(
            &fixture,
            Request::Table {
                role: RowRole::Header,
                rows: ordinals(0, 1),
                columns: ordinals(0, 3),
            },
            None,
            ResponseLimits::default(),
        )
        .unwrap();
    assert!(header.request_complete);
    assert!(!header.resource_complete);
    let partial = open(&fixture, &binding)
        .range(
            &fixture,
            Request::Table {
                role: RowRole::Body,
                rows: ordinals(0, 1),
                columns: ordinals(0, 2),
            },
            None,
            ResponseLimits::default(),
        )
        .unwrap();
    assert!(partial.request_complete);
    assert!(!partial.resource_complete);
}

#[test]
fn code_pages_preserve_original_range_and_exact_next_ordinal() {
    let (fixture, binding) = Fixture::code(b"a\nb\nc\nd\n", 4);
    let request = Request::Code {
        lines: ordinals(1, 3),
    };
    let limits = ResponseLimits {
        descriptors: 1,
        ..ResponseLimits::default()
    };
    let mut query = open(&fixture, &binding);
    let first = query.range(&fixture, request, None, limits).unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.cursor.next, Position::Code(2));
    assert!(!first.request_complete);
    assert!(!first.resource_complete);
    assert_eq!(first.encode().unwrap().len(), first.encoded_bytes);
    let mut resumed = open(&fixture, &binding);
    let second = resumed
        .range(&fixture, request, Some(first.cursor), limits)
        .unwrap();
    assert_eq!(second.cursor.request, request);
    assert_eq!(second.cursor.next, Position::Code(3));
    assert!(second.request_complete);
    assert!(!second.resource_complete);
    assert_eq!(second.encode().unwrap().len(), second.encoded_bytes);
    let empty = resumed
        .range(&fixture, request, Some(second.cursor), limits)
        .unwrap();
    assert!(empty.items.is_empty());
    assert!(empty.request_complete);
    assert!(!empty.resource_complete);
    let eof = resumed
        .range(
            &fixture,
            Request::Code {
                lines: ordinals(4, u64::MAX),
            },
            None,
            limits,
        )
        .unwrap();
    assert!(eof.items.is_empty());
    assert!(eof.request_complete && eof.resource_complete);
}

#[test]
fn rectangle_pages_return_fallback_once_and_missing_without_inventing_source() {
    let source = b"a|b|c\n---|---|---\nx||z\nu|v\nx|y|z|extra\n";
    let (fixture, binding) = Fixture::table(source, 3, 3);
    let request = Request::Table {
        role: RowRole::Body,
        rows: ordinals(0, 3),
        columns: ordinals(1, 3),
    };
    let limits = ResponseLimits {
        descriptors: 2,
        ..ResponseLimits::default()
    };
    let mut cursor = None;
    let mut items = Vec::new();
    for expected_next in [
        Position::Table { row: 1, column: 1 },
        Position::Table { row: 2, column: 1 },
        Position::Table { row: 3, column: 1 },
    ] {
        let mut query = open(&fixture, &binding);
        let response = query.range(&fixture, request, cursor, limits).unwrap();
        assert_eq!(response.cursor.next, expected_next);
        assert_eq!(response.encode().unwrap().len(), response.encoded_bytes);
        assert_eq!(
            response.request_complete,
            expected_next == Position::Table { row: 3, column: 1 }
        );
        assert_eq!(response.resource_complete, response.request_complete);
        cursor = Some(response.cursor);
        items.extend(response.items);
    }
    assert_eq!(items.len(), 5);
    assert!(matches!(&items[0], Item::Descriptor(IndexedDescriptor {
        descriptor: Descriptor::TableCell { raw, .. }, ..
    }) if raw.start == raw.end));
    assert!(matches!(
        &items[3],
        Item::Missing {
            role: RowRole::Body,
            row: 1,
            column: 2
        }
    ));
    assert!(matches!(
        &items[4],
        Item::Descriptor(IndexedDescriptor {
            descriptor: Descriptor::TableRow {
                ordinal: 2,
                disposition: RowDisposition::WholeRowFallback,
                ..
            },
            ..
        })
    ));
}

#[test]
fn cursor_substitution_and_invalid_limits_fail_before_demanded_reads() {
    let (fixture, binding) = Fixture::code(b"a\nb\n", 2);
    let request = Request::Code {
        lines: ordinals(0, 2),
    };
    let mut query = open(&fixture, &binding);
    let first = query
        .range(
            &fixture,
            request,
            None,
            ResponseLimits {
                descriptors: 1,
                ..ResponseLimits::default()
            },
        )
        .unwrap();
    let mut stale = first.cursor;
    stale.root_digest[0] ^= 1;
    let mut wrong_operation = first.cursor;
    wrong_operation.build.operation[0] ^= 1;
    let mut invalid_position = first.cursor;
    invalid_position.next = Position::Code(3);
    for (request, cursor) in [
        (request, stale),
        (request, wrong_operation),
        (
            Request::Code {
                lines: ordinals(1, 2),
            },
            first.cursor,
        ),
        (request, invalid_position),
    ] {
        let before = fixture.reads.borrow().len();
        let ledger = query.ledger.clone();
        assert!(
            query
                .range(&fixture, request, Some(cursor), ResponseLimits::default())
                .is_err()
        );
        assert_eq!(fixture.reads.borrow().len(), before);
        assert_eq!(query.ledger, ledger);
    }
    for limits in [
        ResponseLimits {
            descriptors: 0,
            bytes: 65_536,
        },
        ResponseLimits {
            descriptors: 257,
            bytes: 65_536,
        },
        ResponseLimits {
            descriptors: 1,
            bytes: 65_537,
        },
    ] {
        let ledger = query.ledger.clone();
        assert!(matches!(
            query.range(&fixture, request, None, limits),
            Err(ConstructionError::Limit)
        ));
        assert_eq!(query.ledger, ledger);
    }
}

#[test]
fn payload_limit_counts_binding_cursor_and_absolute_source_ranges() {
    let (fixture, binding) = Fixture::code(b"a\nb\nc\n", 3);
    let request = Request::Code {
        lines: ordinals(0, 3),
    };
    let first = open(&fixture, &binding)
        .range(
            &fixture,
            request,
            None,
            ResponseLimits {
                descriptors: 1,
                ..ResponseLimits::default()
            },
        )
        .unwrap();
    let exact = ResponseLimits {
        descriptors: 256,
        bytes: first.encoded_bytes,
    };
    let bounded = open(&fixture, &binding)
        .range(&fixture, request, None, exact)
        .unwrap();
    assert_eq!(bounded.items.len(), 1);
    assert_eq!(bounded.cursor.next, Position::Code(1));
    assert_eq!(bounded.encode().unwrap().len(), exact.bytes);
    assert!(matches!(
        open(&fixture, &binding).range(
            &fixture,
            request,
            None,
            ResponseLimits {
                bytes: exact.bytes - 1,
                ..exact
            }
        ),
        Err(ConstructionError::Limit)
    ));
    let before = fixture.reads.borrow().len();
    assert!(matches!(
        open(&fixture, &binding).range(
            &fixture,
            request,
            None,
            ResponseLimits { bytes: 1, ..exact }
        ),
        Err(ConstructionError::Limit)
    ));
    assert_eq!(fixture.reads.borrow().len(), before + 5);
}

#[test]
fn empty_rectangle_and_late_narrow_rectangle_do_not_scan_previous_cells() {
    let source = b"a|b|c\n---|---|---\nx|y|z\n";
    let (fixture, binding) = Fixture::table(source, 1, 3);
    let mut query = open(&fixture, &binding);
    let before = fixture.reads.borrow().len();
    let empty = query
        .range(
            &fixture,
            Request::Table {
                role: RowRole::Body,
                rows: ordinals(0, 1),
                columns: ordinals(1, 1),
            },
            None,
            ResponseLimits::default(),
        )
        .unwrap();
    assert!(empty.items.is_empty() && empty.request_complete);
    assert!(!empty.resource_complete);
    assert_eq!(fixture.reads.borrow().len(), before);
    let response = query
        .range(
            &fixture,
            Request::Table {
                role: RowRole::Body,
                rows: ordinals(0, 1),
                columns: ordinals(2, 3),
            },
            None,
            ResponseLimits::default(),
        )
        .unwrap();
    assert_eq!(response.items.len(), 1);
    assert!(response.request_complete && response.resource_complete);
    let reads = fixture.reads.borrow();
    let cell_reads: Vec<_> = reads
        .iter()
        .filter(|(family, _)| *family == TableCellsFamily::NAME)
        .collect();
    assert_eq!(cell_reads.len(), 1);
    assert_eq!(
        cell_reads[0].1,
        TableCellsFamily::encode_key(&CoordinateKey::TableCell(
            query.root.key,
            RowRole::Body,
            0,
            2
        ))
        .unwrap()
    );
}
