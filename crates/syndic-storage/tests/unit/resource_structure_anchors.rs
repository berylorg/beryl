use super::*;
use crate::resource_structure::query_anchor::SourceAnchor;

fn anchor(binding: &Binding, position: u64) -> SourceAnchor {
    SourceAnchor {
        source: binding.source,
        block_start: binding.range.start,
        kind: binding.metadata.kind(),
        position,
        visual_offset: -17,
    }
}

#[test]
fn late_code_anchor_seeks_only_neighbors_and_preserves_position_and_visual_offset() {
    let (fixture, binding) = Fixture::code("x\n".repeat(4_096).as_bytes(), 4_096);
    let mut query = open(&fixture, &binding);
    fixture.reads.borrow_mut().clear();
    let result = query
        .resolve_anchor(&fixture, anchor(&binding, 8_196))
        .unwrap()
        .unwrap();
    assert_eq!(
        result.descriptor.descriptor.coordinate(),
        (0, RowRole::Header, 4_095, 0)
    );
    assert_eq!(result.anchor, anchor(&binding, 8_196));
    assert_eq!(fixture.reads.borrow().len(), 3);
    assert_eq!(fixture.reads.borrow()[0].0, SourceAnchorsFamily::NAME);
    assert_eq!(fixture.reads.borrow()[2].0, SourceAnchorsFamily::NAME);
}

#[test]
fn table_delimiter_gap_chooses_leading_cell_on_equal_distance() {
    let (fixture, binding) = Fixture::table(b"a|b\n---|---:\nc|d\n", 1, 2);
    let mut query = open(&fixture, &binding);
    let position = binding.range.start + 8;
    let result = query
        .resolve_anchor(&fixture, anchor(&binding, position))
        .unwrap()
        .unwrap();
    assert_eq!(
        result.descriptor.descriptor.coordinate(),
        (2, RowRole::Header, 0, 1)
    );
    assert_eq!(result.anchor.position, binding.range.start + 3);
    let mut query = open(&fixture, &binding);
    let result = query
        .resolve_anchor(&fixture, anchor(&binding, position + 1))
        .unwrap()
        .unwrap();
    assert_eq!(
        result.descriptor.descriptor.coordinate(),
        (2, RowRole::Body, 0, 0)
    );
    assert_eq!(result.anchor.position, binding.range.start + 13);
}

#[test]
fn table_anchor_resolves_authored_empty_and_whole_row_fallback() {
    let (fixture, binding) = Fixture::table(b"|a|b|\n|---|---|\n||z|\n|x|y|extra|\n", 2, 2);
    let mut query = open(&fixture, &binding);
    let result = query
        .resolve_anchor(&fixture, anchor(&binding, binding.range.start + 17))
        .unwrap()
        .unwrap();
    assert_eq!(
        result.descriptor.descriptor.coordinate(),
        (2, RowRole::Body, 0, 0)
    );
    assert_eq!(
        result.descriptor.absolute_raw,
        Some(range(binding.range.start + 17, binding.range.start + 17))
    );
    let mut query = open(&fixture, &binding);
    let result = query
        .resolve_anchor(&fixture, anchor(&binding, binding.range.start + 28))
        .unwrap()
        .unwrap();
    assert_eq!(
        result.descriptor.descriptor.coordinate(),
        (1, RowRole::Body, 1, 0)
    );
    assert_eq!(result.anchor.position, binding.range.start + 28);
}

#[test]
fn replaced_source_or_block_invalidates_without_acquisition() {
    let (fixture, binding) = Fixture::code(b"a\n", 1);
    let mut query = open(&fixture, &binding);
    fixture.reads.borrow_mut().clear();
    let before = query.ledger;
    let mut old = anchor(&binding, binding.range.start);
    old.block_start += 1;
    assert_eq!(query.resolve_anchor(&fixture, old).unwrap(), None);
    old = anchor(&binding, binding.range.start);
    old.source = super::super::binding().source;
    assert_eq!(query.resolve_anchor(&fixture, old).unwrap(), None);
    assert_eq!(query.ledger, before);
    assert!(fixture.reads.borrow().is_empty());
}

#[test]
fn anchor_locator_substitution_is_corrupt_and_capacity_refusal_precedes_seek() {
    let (mut fixture, binding) = Fixture::code(b"a\nb\n", 2);
    let key = AnchorKey {
        build: key(),
        start: binding.range.start,
        tag: 0,
        role: RowRole::Header,
        row: 0,
        column: 0,
    };
    let mut locator = fixture
        .point::<SourceAnchorsFamily>(&key, 122)
        .unwrap()
        .unwrap();
    locator.digest = [0xff; 32];
    fixture.put::<SourceAnchorsFamily>(&key, &locator);
    let mut query = open(&fixture, &binding);
    assert!(matches!(
        query.resolve_anchor(&fixture, anchor(&binding, binding.range.start + 1)),
        Err(ConstructionError::Corrupt)
    ));
    let mut query = open(&fixture, &binding);
    query.ledger.records = 520;
    fixture.reads.borrow_mut().clear();
    assert!(matches!(
        query.resolve_anchor(&fixture, anchor(&binding, binding.range.start)),
        Err(ConstructionError::Limit)
    ));
    assert!(fixture.reads.borrow().is_empty());
}

#[test]
fn missing_required_anchor_index_is_corrupt() {
    let (mut fixture, binding) = Fixture::code(b"a\n", 1);
    fixture.remove::<SourceAnchorsFamily>(&AnchorKey {
        build: key(),
        start: binding.range.start,
        tag: 0,
        role: RowRole::Header,
        row: 0,
        column: 0,
    });
    let mut query = open(&fixture, &binding);
    assert!(matches!(
        query.resolve_anchor(&fixture, anchor(&binding, binding.range.start)),
        Err(ConstructionError::Corrupt)
    ));
}
