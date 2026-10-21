#![cfg(feature = "test-faults")]

use syndic_storage::test_faults::{
    generated_media_pending_codec_roundtrip_for_test as pending,
    generated_media_source_codec_roundtrip_for_test as source,
};

fn resource_key() -> Vec<u8> {
    let mut key = vec![1];
    key.extend_from_slice(&[9; 16]);
    key
}

#[test]
fn generated_projection_set_and_payload_have_closed_canonical_shapes() {
    use beryl_model::{
        ProjectionRevision, SyndicItemId, SyndicProjectionId, SyndicResourceId, SyndicTurnId,
    };
    use syndic_storage::test_faults::{
        decode_generated_projection_for_test as decode_projection,
        decode_generated_projection_set_for_test as decode_set,
        generated_projection_codec_for_test as encode_projection,
        generated_projection_set_codec_for_test as encode_set,
        generated_projection_set_record_version_for_test,
    };
    use syndic_storage::{
        ItemProjectionGeneration, ItemProjectionSetRecord, ProjectionFormatVersion,
        ProjectionOrdinal, ProjectionPayload, ProjectionRecord,
    };
    let item = SyndicItemId::from_bytes([1; 16]);
    let resource = SyndicResourceId::from_bytes([2; 16]);
    let set = ItemProjectionSetRecord::generated_resource(
        item,
        ItemProjectionGeneration::FIRST,
        ProjectionFormatVersion::V1,
        ProjectionRevision::new(3).unwrap(),
        resource,
        [4; 32],
    );
    assert_eq!(generated_projection_set_record_version_for_test(), 2);
    let bytes = encode_set(&set);
    assert_eq!(bytes.len(), 98);
    assert_eq!(&bytes[..16], item.as_bytes());
    assert_eq!(&bytes[16..24], &1u64.to_be_bytes());
    assert_eq!(bytes[24], 1);
    assert_eq!(&bytes[25..33], &3u64.to_be_bytes());
    assert_eq!(bytes[33], 1);
    assert_eq!(&bytes[34..50], resource.as_bytes());
    assert_eq!(&bytes[50..58], &1u64.to_be_bytes());
    assert_eq!(&bytes[58..66], &0u64.to_be_bytes());
    assert_eq!(&bytes[66..], &[4; 32]);
    assert_eq!(decode_set(&bytes), Some(set));
    for end in 0..bytes.len() {
        assert!(decode_set(&bytes[..end]).is_none());
    }
    for offset in [33, 57, 65] {
        let mut invalid = bytes.clone();
        invalid[offset] = 2;
        assert!(decode_set(&invalid).is_none());
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode_set(&trailing).is_none());
    let projection = ProjectionRecord::new(
        SyndicProjectionId::from_bytes([5; 16]),
        ProjectionRevision::new(1).unwrap(),
        item,
        SyndicTurnId::from_bytes([6; 16]),
        ProjectionOrdinal::new(1).unwrap(),
        ProjectionPayload::GeneratedResource {
            resource_id: resource,
        },
    );
    let bytes = encode_projection(&projection);
    assert_eq!(bytes.len(), 81);
    assert_eq!(bytes[64], 4);
    assert_eq!(&bytes[65..], resource.as_bytes());
    assert_eq!(decode_projection(&bytes), Some(projection));
    for end in 0..bytes.len() {
        assert!(decode_projection(&bytes[..end]).is_none());
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode_projection(&trailing).is_none());
}
fn awaiting() -> Vec<u8> {
    let mut bytes = 0x0102030405060708u64.to_le_bytes().to_vec();
    for byte in 1..=4 {
        bytes.extend_from_slice(&[byte; 16]);
    }
    bytes.push(1);
    bytes.extend_from_slice(&5u64.to_le_bytes());
    bytes.push(0);
    bytes.push(0);
    bytes
}

#[test]
fn source_allocator_awaiting_and_completed_have_canonical_payloads() {
    let allocator = 0x0102030405060708u64.to_le_bytes().to_vec();
    assert_eq!(source(&[0], &allocator), Some((vec![0], allocator)));
    let key = resource_key();
    let initial = awaiting();
    assert_eq!(initial.len(), 83);
    assert_eq!(source(&key, &initial), Some((key.clone(), initial.clone())));
    let mut completed = initial;
    completed[82] = 1;
    completed.extend_from_slice(&6u64.to_le_bytes());
    completed.extend_from_slice(&[7; 32]);
    assert_eq!(completed.len(), 123);
    assert_eq!(
        source(&key, &completed),
        Some((key.clone(), completed.clone()))
    );
    for end in 0..completed.len() {
        assert!(
            source(&key, &completed[..end]).is_none(),
            "accepted truncated {end}"
        );
    }
    completed.push(0);
    assert!(source(&key, &completed).is_none());
}

#[test]
fn source_rejects_wrong_key_kind_tags_zero_and_regressed_event_sequences() {
    let key = resource_key();
    let initial = awaiting();
    for malformed_key in [vec![], vec![2], vec![0, 0], vec![1], vec![1; 18]] {
        assert!(source(&malformed_key, &initial).is_none());
    }
    assert!(source(&[0], &initial).is_none());
    assert!(source(&key, &5u64.to_le_bytes()).is_none());
    for offset in [72, 81, 82] {
        let mut bytes = initial.clone();
        bytes[offset] = 2;
        assert!(source(&key, &bytes).is_none());
    }
    for range in [0..8, 73..81] {
        let mut bytes = initial.clone();
        bytes[range].fill(0);
        assert!(source(&key, &bytes).is_none());
    }
    let mut completed = initial;
    completed[82] = 1;
    completed.extend_from_slice(&4u64.to_le_bytes());
    completed.extend_from_slice(&[7; 32]);
    assert!(source(&key, &completed).is_none());
}

#[test]
fn source_incarnation_and_snapshot_owner_have_exact_canonical_shapes() {
    let resource = resource_key();
    let mut owner_key = vec![2];
    owner_key.extend_from_slice(&[4; 16]);
    let owner = vec![0x47; 32];
    assert_eq!(
        source(&owner_key, &owner),
        Some((owner_key.clone(), owner.clone()))
    );
    for size in [0, 8, 31, 33] {
        assert!(source(&owner_key, &vec![0x47; size]).is_none());
    }
    assert!(source(&resource, &owner).is_none());
    assert!(source(&[0], &owner).is_none());
    let mut owned = awaiting();
    owned[81] = 1;
    owned.splice(82..82, owner);
    owned[114] = 1;
    owned.extend_from_slice(&6u64.to_le_bytes());
    owned.extend_from_slice(&[7; 32]);
    assert_eq!(owned.len(), 155);
    assert_eq!(
        source(&resource, &owned),
        Some((resource.clone(), owned.clone()))
    );
    for end in 0..owned.len() {
        assert!(
            source(&resource, &owned[..end]).is_none(),
            "accepted truncated owned {end}"
        );
    }
    owned.push(0);
    assert!(source(&resource, &owned).is_none());
}

#[test]
fn pending_index_orders_by_big_endian_nonzero_sequence_and_has_exact_resource_value() {
    for sequence in [1u64, 255, 256, u64::MAX] {
        let key = sequence.to_be_bytes().to_vec();
        let value = vec![8; 16];
        assert_eq!(pending(&key, &value), Some((key, value)));
    }
    assert!(255u64.to_be_bytes() < 256u64.to_be_bytes());
    assert!(pending(&0u64.to_be_bytes(), &[8; 16]).is_none());
    assert!(pending(&[1; 7], &[8; 16]).is_none());
    assert!(pending(&[1; 9], &[8; 16]).is_none());
    assert!(pending(&1u64.to_be_bytes(), &[8; 15]).is_none());
    assert!(pending(&1u64.to_be_bytes(), &[8; 17]).is_none());
}

#[test]
fn unavailable_resource_preserves_all_eight_closed_reason_codes() {
    use beryl_model::{
        GeneratedMediaUnavailableReason, ProjectionRevision, SyndicItemId, SyndicResourceId,
    };
    use syndic_storage::test_faults::{
        decode_generated_media_resource_for_test as decode,
        generated_media_resource_codec_for_test as encode,
    };
    use syndic_storage::{GeneratedMediaResourceDisposition, ResourceMetadataRecord};
    for code in 0..8 {
        let record = ResourceMetadataRecord::generated_media(
            SyndicResourceId::from_bytes([1; 16]),
            ProjectionRevision::new(2).unwrap(),
            SyndicItemId::from_bytes([3; 16]),
            GeneratedMediaResourceDisposition::Unavailable(
                GeneratedMediaUnavailableReason::from_code(code).unwrap(),
            ),
        );
        let mut bytes = encode(&record);
        assert_eq!(*bytes.last().unwrap(), code);
        assert_eq!(decode(&bytes), Some(record));
        *bytes.last_mut().unwrap() = 8;
        assert!(decode(&bytes).is_none());
        bytes.pop();
        assert!(decode(&bytes).is_none());
    }
}
