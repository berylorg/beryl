#![cfg(feature = "test-faults")]

use beryl_model::{AssetId, SyndicItemId, SyndicResourceId, SyndicThreadId, SyndicTurnId};
use beryl_state::{
    AssetDimensions, AssetMediaType, AssetState, GeneratedMediaFacts, GeneratedMediaLifecycle,
    GeneratedMediaTarget, RecordRevision,
};
use std::num::NonZeroU64;

fn facts(dimensions: Option<AssetDimensions>) -> GeneratedMediaFacts {
    GeneratedMediaFacts::new(
        AssetId::sha256_v1([6; 32], NonZeroU64::new(0x0102030405060708).unwrap()),
        AssetMediaType::new("image/png").unwrap(),
        dimensions,
    )
    .unwrap()
}
fn target() -> GeneratedMediaTarget {
    GeneratedMediaTarget::new(
        SyndicResourceId::from_bytes([1; 16]),
        SyndicThreadId::from_bytes([2; 16]),
        SyndicTurnId::from_bytes([3; 16]),
        SyndicItemId::from_bytes([4; 16]),
        [5; 32],
    )
}
fn prepared() -> Vec<u8> {
    AssetState::generated_media_record_bytes_for_test(
        target(),
        facts(Some(AssetDimensions::new(
            NonZeroU64::new(256).unwrap(),
            NonZeroU64::new(512).unwrap(),
        ))),
        GeneratedMediaLifecycle::Prepared,
        RecordRevision::INITIAL,
    )
}
fn valid(bytes: &[u8]) -> bool {
    AssetState::generated_media_record_valid_for_test(&[1; 16], bytes)
}

#[test]
fn generated_admission_has_exact_little_endian_wire_shape() {
    let bytes = prepared();
    let mut expected = Vec::new();
    expected.extend_from_slice(&[2; 16]);
    expected.extend_from_slice(&[3; 16]);
    expected.extend_from_slice(&[4; 16]);
    expected.extend_from_slice(&[5; 32]);
    expected.push(1);
    expected.extend_from_slice(&[6; 32]);
    expected.extend_from_slice(&0x0102030405060708u64.to_le_bytes());
    expected.push(9);
    expected.extend_from_slice(b"image/png");
    expected.push(1);
    expected.extend_from_slice(&256u32.to_le_bytes());
    expected.extend_from_slice(&512u32.to_le_bytes());
    expected.push(1);
    expected.extend_from_slice(&1u64.to_le_bytes());
    assert_eq!(bytes, expected);
    assert!(valid(&bytes));
    assert!(bytes.len() <= 512);
}

#[test]
fn generated_admission_rejects_every_truncation_and_trailing_byte() {
    let bytes = prepared();
    for end in 0..bytes.len() {
        assert!(!valid(&bytes[..end]), "accepted prefix {end}");
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(!valid(&trailing));
    assert!(!AssetState::generated_media_record_valid_for_test(
        &[1; 15],
        &prepared()
    ));
    assert!(!AssetState::generated_media_record_valid_for_test(
        &[1; 17],
        &prepared()
    ));
}

#[test]
fn generated_admission_rejects_invalid_tags_lengths_dimensions_and_revision() {
    let base = prepared();
    for (offset, replacement) in [
        (80, 0),
        (80, 2),
        (121, 0),
        (121, 128),
        (122, b' '),
        (131, 2),
        (140, 0),
        (140, 3),
        (141, 0),
        (141, 2),
    ] {
        let mut bytes = base.clone();
        bytes[offset] = replacement;
        assert!(!valid(&bytes), "offset {offset} value {replacement}");
    }
    for range in [113..121, 132..136, 136..140] {
        let mut bytes = base.clone();
        bytes[range.clone()].fill(0);
        assert!(!valid(&bytes), "zero range {range:?}");
    }
}

#[test]
fn published_requires_the_prepared_successor_and_absent_dimensions_roundtrip() {
    let bytes = AssetState::generated_media_record_bytes_for_test(
        target(),
        facts(None),
        GeneratedMediaLifecycle::Published,
        RecordRevision::new(2).unwrap(),
    );
    assert!(valid(&bytes));
    for revision in [1, 3, u64::MAX] {
        let bytes = AssetState::generated_media_record_bytes_for_test(
            target(),
            facts(None),
            GeneratedMediaLifecycle::Published,
            RecordRevision::new(revision).unwrap(),
        );
        assert!(!valid(&bytes));
    }
}

#[test]
fn generated_dimensions_must_fit_nonzero_u32() {
    let dimensions = AssetDimensions::new(
        NonZeroU64::new(u64::from(u32::MAX) + 1).unwrap(),
        NonZeroU64::MIN,
    );
    assert!(
        GeneratedMediaFacts::new(
            facts(None).asset(),
            AssetMediaType::new("image/png").unwrap(),
            Some(dimensions)
        )
        .is_err()
    );
    let dimensions = AssetDimensions::new(
        NonZeroU64::new(u64::from(u32::MAX)).unwrap(),
        NonZeroU64::MIN,
    );
    assert!(valid(&AssetState::generated_media_record_bytes_for_test(
        target(),
        facts(Some(dimensions)),
        GeneratedMediaLifecycle::Prepared,
        RecordRevision::INITIAL
    )));
}
