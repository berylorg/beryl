use super::*;
use crate::resource_structure::{
    construction::ConstructionError,
    ledger::Ledger,
    query::{CellLookup, IndexedDescriptor, IndexedQuery, QueryRecords},
};

#[path = "resource_structure_query_support.rs"]
mod support;
use support::Fixture;

#[path = "resource_structure_responses.rs"]
mod responses;

#[path = "resource_structure_display_responses.rs"]
mod display_responses;

#[path = "resource_structure_range_display.rs"]
mod range_display;

#[path = "resource_structure_anchors.rs"]
mod anchors;

fn open(fixture: &Fixture, binding: &Binding) -> IndexedQuery {
    IndexedQuery::authenticate(fixture, binding, None, Ledger::default())
        .unwrap()
        .unwrap()
}

#[test]
fn late_code_lookup_reads_exact_closure_one_locator_and_one_page() {
    let source = "x\n".repeat(4_096);
    let (fixture, binding) = Fixture::code(source.as_bytes(), 4_096);
    let mut query = open(&fixture, &binding);
    assert_eq!(fixture.reads.borrow().len(), 5);
    let line = query.line(&fixture, 4_095).unwrap();
    assert_eq!(line.absolute_raw, Some(range(8_195, 8_197)));
    assert_eq!(line.descriptor.coordinate(), (0, RowRole::Header, 4_095, 0));
    assert_eq!(fixture.reads.borrow().len(), 7);
    assert_eq!(query.root.code_lines, 4_096);
    assert_eq!(query.root.language.as_deref(), Some("rust"));
    query.line(&fixture, 4_094).unwrap();
    assert_eq!(fixture.reads.borrow().len(), 8);
    let before = query.ledger;
    assert!(query.line(&fixture, 4_096).is_err());
    assert_eq!(query.ledger, before);
    assert!(
        fixture
            .reads
            .borrow()
            .iter()
            .all(|(family, _)| !family.starts_with("content-"))
    );
}

#[test]
fn table_lookup_preserves_empty_missing_and_whole_row_fallback() {
    let (fixture, binding) =
        Fixture::table(b"a|b|c\n---|:---:|---:\nx||z\nu|v\nx|y|z|surplus\n", 3, 3);
    let mut query = open(&fixture, &binding);
    let cell = query.cell(&fixture, RowRole::Body, 0, 1).unwrap();
    assert!(
        matches!(cell, CellLookup::Authored(IndexedDescriptor { descriptor:
        Descriptor::TableCell { raw, trimmed, .. }, .. }) if raw.start == raw.end && trimmed == raw)
    );
    fixture.reads.borrow_mut().clear();
    let missing = query.cell(&fixture, RowRole::Body, 1, 2).unwrap();
    assert_eq!(
        missing,
        CellLookup::Missing {
            role: RowRole::Body,
            row: 1,
            column: 2
        }
    );
    assert!(
        !fixture
            .reads
            .borrow()
            .iter()
            .any(|(name, _)| *name == TableCellsFamily::NAME)
    );
    fixture.reads.borrow_mut().clear();
    let fallback = query.cell(&fixture, RowRole::Body, 2, 2).unwrap();
    assert!(matches!(
        fallback,
        CellLookup::RowFallback(IndexedDescriptor {
            descriptor: Descriptor::TableRow {
                disposition: RowDisposition::WholeRowFallback,
                ..
            },
            ..
        })
    ));
    assert!(
        !fixture
            .reads
            .borrow()
            .iter()
            .any(|(name, _)| *name == TableCellsFamily::NAME)
    );
    assert!(matches!(
        query.column(&fixture, 1).unwrap().descriptor,
        Descriptor::TableColumn {
            ordinal: 1,
            alignment: Alignment::Center
        }
    ));
    assert!(query.cell(&fixture, RowRole::Header, 1, 0).is_err());
    assert!(query.cell(&fixture, RowRole::Body, 3, 0).is_err());
    assert!(query.cell(&fixture, RowRole::Body, 0, 3).is_err());
}

#[test]
fn wide_table_last_cell_uses_only_requested_coordinate() {
    let header = vec!["h"; 128].join("|");
    let delimiter = vec!["---"; 128].join("|");
    let body = vec!["v"; 128].join("|");
    let source = format!("{header}\n{delimiter}\n{body}\n");
    let (fixture, binding) = Fixture::table(source.as_bytes(), 1, 128);
    let mut query = open(&fixture, &binding);
    fixture.reads.borrow_mut().clear();
    assert!(matches!(
        query.cell(&fixture, RowRole::Body, 0, 127).unwrap(),
        CellLookup::Authored(_)
    ));
    let reads = fixture.reads.borrow();
    assert_eq!(reads.len(), 4);
    let cell_keys: Vec<_> = reads
        .iter()
        .filter(|(name, _)| *name == TableCellsFamily::NAME)
        .collect();
    assert_eq!(cell_keys.len(), 1);
    assert_eq!(
        cell_keys[0].1,
        TableCellsFamily::encode_key(&CoordinateKey::TableCell(
            query.root.key,
            RowRole::Body,
            0,
            127
        ))
        .unwrap()
    );
    drop(reads);
    fixture.reads.borrow_mut().clear();
    query.cell(&fixture, RowRole::Body, 0, 126).unwrap();
    let reads = fixture.reads.borrow();
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].0, TableCellsFamily::NAME);
}

#[test]
fn pending_is_distinct_from_stale_root_and_incomplete_selected_closure() {
    let (mut fixture, binding) = Fixture::code(b"one\nthree\n", 2);
    let query = open(&fixture, &binding);
    assert!(matches!(
        IndexedQuery::authenticate(&fixture, &binding, Some([0x19; 32]), Ledger::default()),
        Err(ConstructionError::Collision)
    ));
    fixture.remove::<ProgressFamily>(&query.root.progress);
    assert!(matches!(
        IndexedQuery::authenticate(&fixture, &binding, None, Ledger::default()),
        Err(ConstructionError::Corrupt)
    ));
    fixture.remove::<HeadsFamily>(&binding.resource);
    assert!(
        IndexedQuery::authenticate(&fixture, &binding, None, Ledger::default())
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        IndexedQuery::authenticate(
            &fixture,
            &binding,
            Some(query.root.digest),
            Ledger::default()
        ),
        Err(ConstructionError::Unavailable)
    ));
    let mut wrong = binding.clone();
    wrong.metadata = super::fixture_support::table_build(b"a|b\n---|---\n", 0, 2)
        .binding
        .metadata;
    assert!(matches!(
        IndexedQuery::authenticate(&fixture, &wrong, None, Ledger::default()),
        Err(ConstructionError::Collision)
    ));
}

#[test]
fn locator_page_digest_slot_and_binding_substitution_are_rejected() {
    for change in 0..5 {
        let (mut fixture, binding) = Fixture::code(b"one\nthree\n", 2);
        let mut query = open(&fixture, &binding);
        let key = CoordinateKey::CodeLine(query.root.key, 1);
        let mut locator = fixture.point::<CodeLinesFamily>(&key, 96).unwrap().unwrap();
        match change {
            0 => locator.digest = [0x91; 32],
            1 => locator.slot = 0,
            2 => locator.page = query.root.pages + 1,
            3 => {
                let page_key = SequenceKey {
                    build: query.root.key,
                    ordinal: locator.page,
                };
                fixture.remove::<PagesFamily>(&page_key);
            }
            4 => {
                let page_key = SequenceKey {
                    build: query.root.key,
                    ordinal: locator.page,
                };
                let mut page = fixture
                    .point::<PagesFamily>(&page_key, 65_536)
                    .unwrap()
                    .unwrap();
                page.binding_digest = [0x92; 32];
                page.digest = page_digest(&page).unwrap();
                locator.digest = page.digest;
                fixture.put::<PagesFamily>(&page.key, &page);
            }
            _ => unreachable!(),
        }
        fixture.put::<CodeLinesFamily>(&key, &locator);
        assert!(matches!(
            query.line(&fixture, 1),
            Err(ConstructionError::Corrupt)
        ));
    }
}

#[test]
fn query_read_allowance_refuses_before_any_acquisition() {
    let (fixture, binding) = Fixture::code(b"one\nthree\n", 2);
    let mut query = open(&fixture, &binding);
    query.ledger.records = 520;
    fixture.reads.borrow_mut().clear();
    let before = query.ledger;
    assert!(matches!(
        query.line(&fixture, 1),
        Err(ConstructionError::Limit)
    ));
    assert!(fixture.reads.borrow().is_empty());
    assert_eq!(query.ledger, before);
}
