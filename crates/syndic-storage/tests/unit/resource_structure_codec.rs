use super::{codec::*, *};
use crate::codec::{
    Family,
    parts::{Decoder, Encoder},
};
use crate::{
    ContentEncoding, ContentReference, ContentSummary, ProjectionSourceRange, ProjectionTextSource,
    ResourceBacking, ResourceKind, ResourceMetadataRecord, ResourceOrdinal, ResourceStructure,
};
use beryl_model::*;
use sha2::{Digest, Sha256};

#[path = "resource_structure_fixture_support.rs"]
mod fixture_support;

#[path = "resource_structure_advances.rs"]
mod advances;

#[path = "resource_structure_table_advances.rs"]
mod table_advances;

#[path = "resource_structure_queries.rs"]
mod queries;

#[path = "resource_structure_durable.rs"]
mod durable;

fn key() -> BuildKey {
    BuildKey {
        resource: ResourceKey {
            id: SyndicResourceId::from_bytes([0x11; 16]),
            revision: ProjectionRevision::new(7).unwrap(),
        },
        operation: [0x22; 16],
    }
}

fn range(start: u64, end: u64) -> ByteRange {
    ByteRange { start, end }
}

fn binding() -> Binding {
    let source = ProjectionTextSource::composer(ContentReference::new(
        SyndicContentId::from_bytes([0x33; 16]),
        ContentRevision::new(2).unwrap(),
        ContentEncoding::ComposerV1,
        ContentSummary::new(
            1,
            1,
            100,
            100,
            1,
            0,
            [0; 32],
            None,
            SyndicContentDigest::from_bytes([0x44; 32]),
        )
        .unwrap(),
    ));
    let metadata = ResourceMetadataRecord::new(
        key().resource.id,
        key().resource.revision,
        SyndicProjectionId::from_bytes([0x55; 16]),
        SyndicItemId::from_bytes([0x66; 16]),
        ResourceOrdinal::FIRST,
        ResourceKind::Code,
        "text/plain",
        ResourceBacking::TextRange {
            source,
            range: ProjectionSourceRange::new(5, 15).unwrap(),
        },
        [0x77; 32],
        None,
        ResourceStructure::code(Some("rust"), 2).unwrap(),
    )
    .unwrap();
    Binding {
        resource: key().resource,
        metadata,
        source,
        range: range(5, 15),
    }
}

fn build() -> Build {
    let binding = binding();
    let binding_digest = binding_digest(&binding).unwrap();
    Build {
        key: key(),
        binding,
        binding_digest,
        revision: 1,
        lifecycle: Lifecycle::Building,
        transition: 0,
        progress: None,
        next_page: 1,
        chain: empty_chain(key(), &binding_digest),
        counts: [0; 4],
        checkpoint: Checkpoint {
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
        },
    }
}

fn page() -> Page {
    let mut page = Page {
        key: SequenceKey {
            build: key(),
            ordinal: 1,
        },
        binding_digest: [0x88; 32],
        prior_chain: [0x99; 32],
        descriptors: vec![Descriptor::CodeLine {
            ordinal: 0,
            raw: range(0, 4),
            content: range(0, 2),
            terminator: range(2, 4),
        }],
        digest: [0; 32],
    };
    page.digest = page_digest(&page).unwrap();
    page
}

fn root() -> Root {
    let mut root = Root {
        key: key(),
        binding_digest: [0x88; 32],
        pages: 1,
        chain: [0x99; 32],
        counts: [2, 0, 0, 0],
        code_lines: 2,
        body_rows: 0,
        columns: 0,
        header: None,
        language: Some("rust".into()),
        progress: SequenceKey {
            build: key(),
            ordinal: 3,
        },
        digest: [0; 32],
    };
    root.digest = root_digest(&root).unwrap();
    root
}

fn independent_hash(domain: &[u8], fields: &[&[u8]]) -> [u8; 32] {
    let mut bytes = Vec::new();
    for field in std::iter::once(domain).chain(fields.iter().copied()) {
        bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
        bytes.extend_from_slice(field);
    }
    Sha256::digest(bytes).into()
}

#[test]
fn code_page_matches_independently_framed_canonical_bytes() {
    let page = page();
    let mut page_key = vec![0x11; 16];
    page_key.extend_from_slice(&7_u64.to_be_bytes());
    page_key.extend_from_slice(&[0x22; 16]);
    page_key.extend_from_slice(&1_u64.to_be_bytes());
    let mut descriptors = 1_u64.to_be_bytes().to_vec();
    descriptors.push(0);
    for scalar in [0_u64, 0, 4, 0, 2, 2, 4] {
        descriptors.extend_from_slice(&scalar.to_be_bytes());
    }
    let digest = independent_hash(
        b"syndic/resource-structure-page/v1",
        &[&page_key, &[0x88; 32], &[0x99; 32], &descriptors],
    );
    let mut expected = page_key;
    expected.extend_from_slice(&[0x88; 32]);
    expected.extend_from_slice(&[0x99; 32]);
    expected.extend_from_slice(&descriptors);
    expected.extend_from_slice(&digest);
    assert_eq!(encode_page(&page).unwrap(), expected);
    assert_eq!(decode_page(&expected).unwrap(), page);
    for end in 0..expected.len() {
        assert!(
            decode_page(&expected[..end]).is_err(),
            "accepted truncated page at {end}"
        );
    }
    let mut trailing = expected.clone();
    trailing.push(0);
    assert!(decode_page(&trailing).is_err());
    for position in [
        0,
        16,
        24,
        40,
        48,
        80,
        112,
        120,
        121,
        128,
        expected.len() - 1,
    ] {
        let mut altered = expected.clone();
        altered[position] ^= 0x80;
        assert!(
            decode_page(&altered).is_err(),
            "accepted altered page at {position}"
        );
    }
}

#[test]
fn coordinate_keys_are_numeric_big_endian_and_family_specific() {
    let first = CoordinateKey::TableCell(key(), RowRole::Body, 255, 65_535);
    let second = CoordinateKey::TableCell(key(), RowRole::Body, 256, 0);
    assert!(coordinate_key(first).unwrap() < coordinate_key(second).unwrap());
    assert_eq!(coordinate_key(first).unwrap().len(), 57);
    assert_eq!(
        &coordinate_key(first).unwrap()[40..49],
        &[1, 0, 0, 0, 0, 0, 0, 0, 255]
    );
    assert!(CodeLinesFamily::encode_key(&first).is_err());
    assert!(TableRowsFamily::decode_key(&coordinate_key(first).unwrap()).is_err());
    assert!(
        TableCellsFamily::encode_key(&CoordinateKey::TableCell(key(), RowRole::Header, 1, 0))
            .is_err()
    );
    assert_eq!(
        TableCellsFamily::decode_key(&coordinate_key(first).unwrap()).unwrap(),
        first
    );
}

#[test]
fn authored_empty_cells_and_nonempty_terminated_empty_lines_are_distinct() {
    for descriptor in [
        Descriptor::TableCell {
            role: RowRole::Body,
            row: 3,
            column: 9,
            raw: range(20, 20),
            trimmed: range(20, 20),
            presentation: CellPresentation::Literal,
        },
        Descriptor::CodeLine {
            ordinal: 2,
            raw: range(6, 7),
            content: range(6, 6),
            terminator: range(6, 7),
        },
        Descriptor::CodeLine {
            ordinal: 2,
            raw: range(6, 8),
            content: range(6, 6),
            terminator: range(6, 8),
        },
    ] {
        let mut e = Encoder::new();
        enc_descriptor(&mut e, &descriptor).unwrap();
        let bytes = e.finish();
        let mut d = Decoder::new(&bytes);
        assert_eq!(dec_descriptor(&mut d).unwrap(), descriptor);
        d.finish().unwrap();
    }
    assert!(
        validate_descriptor(&Descriptor::CodeLine {
            ordinal: 2,
            raw: range(6, 6),
            content: range(6, 6),
            terminator: range(6, 6)
        })
        .is_err()
    );
    assert!(
        validate_descriptor(&Descriptor::TableCell {
            role: RowRole::Body,
            row: 0,
            column: 0,
            raw: range(1, 4),
            trimmed: range(0, 4),
            presentation: CellPresentation::Literal
        })
        .is_err()
    );
}

#[test]
fn descriptor_tags_ranges_order_and_page_capacity_are_closed() {
    let mut value = page();
    value.descriptors.push(value.descriptors[0].clone());
    assert!(page_digest(&value).is_err());
    value.descriptors = (0..257)
        .map(|ordinal| Descriptor::TableColumn {
            ordinal,
            alignment: Alignment::Left,
        })
        .collect();
    assert!(page_digest(&value).is_err());
    assert!(dec_descriptor(&mut Decoder::new(&[4])).is_err());
    let mut column = vec![3];
    column.extend_from_slice(&0_u64.to_be_bytes());
    column.push(4);
    assert!(dec_descriptor(&mut Decoder::new(&column)).is_err());
    value = page();
    value.key.ordinal = 0;
    assert!(encode_page(&value).is_err());
    assert!(
        validate_descriptor(&Descriptor::CodeLine {
            ordinal: 0,
            raw: range(0, 10),
            content: range(0, 7),
            terminator: range(7, 10)
        })
        .is_err()
    );
}

#[test]
fn source_anchor_fixture_has_closed_coordinate_and_empty_interval_rules() {
    let value = AnchorLocator {
        key: AnchorKey {
            build: key(),
            start: 42,
            tag: 2,
            role: RowRole::Body,
            row: 256,
            column: 7,
        },
        end: 42,
        page: 1,
        slot: 255,
        digest: [0x88; 32],
    };
    let bytes = encode_anchor(&value).unwrap();
    assert_eq!(bytes.len(), 122);
    assert_eq!(&bytes[40..50], &[0, 0, 0, 0, 0, 0, 0, 42, 2, 1]);
    assert_eq!(decode_anchor(&bytes).unwrap(), value);
    let mut invalid = value;
    invalid.key.tag = 1;
    invalid.key.column = 0;
    assert!(encode_anchor(&invalid).is_err());
    invalid = value;
    invalid.slot = 256;
    assert!(encode_anchor(&invalid).is_err());
    invalid = value;
    invalid.key.tag = 3;
    assert!(encode_anchor(&invalid).is_err());
}

#[test]
fn source_binding_retains_metadata_and_rejects_substitution_and_format_changes() {
    let value = binding();
    let bytes = binding_bytes(&value).unwrap();
    assert_eq!(decode_binding(&bytes).unwrap(), value);
    assert_eq!(
        u64::from_be_bytes(bytes[24..32].try_into().unwrap()) as usize,
        crate::codec::ResourcesFamily::encode_value(&value.metadata)
            .unwrap()
            .len()
    );
    let mut changed = value.clone();
    changed.range.end += 1;
    assert!(binding_bytes(&changed).is_err());
    changed = value.clone();
    changed.resource.revision = ProjectionRevision::new(8).unwrap();
    assert!(binding_bytes(&changed).is_err());
    let mut changed_bytes = bytes.clone();
    *changed_bytes.last_mut().unwrap() = 2;
    assert!(decode_binding(&changed_bytes).is_err());
    let mut oversized = bytes;
    oversized[24..32].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(decode_binding(&oversized).is_err());
}

#[path = "resource_structure_codec/control.rs"]
mod control;

#[path = "resource_structure_construction.rs"]
mod construction;

#[path = "resource_structure_transitions.rs"]
mod transitions;
