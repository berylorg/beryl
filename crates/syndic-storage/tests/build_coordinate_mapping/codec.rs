use super::*;
use sha2::{Digest, Sha256};

fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_be_bytes());
        h.update(p);
    }
    h.finalize().into()
}
fn sign(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = hash(&[
        b"syndic/draft-piece-build-mapping-node/v1",
        &bytes[..48],
        &bytes[64..65],
        &bytes[65..73],
        &bytes[73..end],
    ]);
    bytes[end..].copy_from_slice(&digest);
}

#[test]
fn exact_key_digest_record_identity_and_all_truncations() {
    let (_home, store, storage) = Home::open("codec");
    let mut map = BuildMappingForTest::new(7);
    map.insert(&storage, &store, 3, 2).unwrap();
    let bytes = map.root_node_bytes(&storage, &store);
    assert_eq!(bytes.len(), 64 + 1 + 8 + 3 * 17 + 32);
    assert_eq!(&bytes[..48], &[71; 48]);
    assert_eq!(bytes[64], 1);
    assert_eq!(&bytes[65..73], &3u64.to_be_bytes());
    let mut expected_runs = Vec::new();
    for (tag, n) in [(0, 3u128), (2, 2), (0, 4)] {
        expected_runs.push(tag);
        expected_runs.extend(n.to_be_bytes());
    }
    assert_eq!(&bytes[73..124], expected_runs);
    let digest = hash(&[
        b"syndic/draft-piece-build-mapping-node/v1",
        &[71; 48],
        &[1],
        &3u64.to_be_bytes(),
        &expected_runs,
    ]);
    assert_eq!(&bytes[124..], digest);
    let id = hash(&[
        b"syndic/draft-piece-build-mapping-record-id/v1",
        &[71; 16],
        &[71; 16],
        &[71; 16],
        &1u64.to_be_bytes(),
        &digest,
    ]);
    assert_eq!(&bytes[48..64], &id[..16]);
    assert_eq!(mapping_node_codec_roundtrip(&bytes), Some(bytes.clone()));
    for n in 0..bytes.len() {
        assert!(
            mapping_node_codec_roundtrip(&bytes[..n]).is_none(),
            "node truncation {n}"
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(mapping_node_codec_roundtrip(&trailing).is_none());
    for root in [
        vec![0],
        {
            let mut b = vec![1];
            b.extend(7u128.to_be_bytes());
            b
        },
        map.root_bytes(),
    ] {
        assert_eq!(mapping_root_codec_roundtrip(&root), Some(root.clone()));
        for n in 0..root.len() {
            assert!(mapping_root_codec_roundtrip(&root[..n]).is_none());
        }
        let mut trailing = root;
        trailing.push(0);
        assert!(mapping_root_codec_roundtrip(&trailing).is_none());
    }
}

#[test]
fn malformed_canonical_nodes_and_roots_fail_closed() {
    let (_home, store, storage) = Home::open("malformed");
    let mut map = BuildMappingForTest::new(7);
    map.insert(&storage, &store, 3, 2).unwrap();
    let bytes = map.root_node_bytes(&storage, &store);
    for (offset, value) in [(64, 0), (64, 23), (72, 0), (72, 17), (73, 3), (89, 0)] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        sign(&mut bad);
        assert!(
            mapping_node_codec_roundtrip(&bad).is_none(),
            "offset {offset}"
        );
    }
    let mut huge_count = bytes.clone();
    huge_count[65..73].fill(255);
    sign(&mut huge_count);
    assert!(mapping_node_codec_roundtrip(&huge_count).is_none());
    let mut huge_run = bytes.clone();
    huge_run[74..90].fill(255);
    sign(&mut huge_run);
    assert!(mapping_node_codec_roundtrip(&huge_run).is_none());
    let mut digest = bytes.clone();
    *digest.last_mut().unwrap() ^= 1;
    assert!(mapping_node_codec_roundtrip(&digest).is_none());
    assert!(mapping_root_codec_roundtrip(&[3]).is_none());
    assert!(mapping_root_codec_roundtrip(&[1; 17]).is_none());
    let mut zero = vec![1];
    zero.extend([0; 16]);
    assert!(mapping_root_codec_roundtrip(&zero).is_none());
    let mut wrong = map.root_bytes();
    wrong[49] = 23;
    assert!(mapping_root_codec_roundtrip(&wrong).is_none());
    let mut wrong = map.root_bytes();
    wrong[50..].fill(0);
    assert!(mapping_root_codec_roundtrip(&wrong).is_none());
    assert!(!map.authenticate(&storage, &store, 7, 8));
    let mut substituted = map.root_bytes();
    substituted[17] ^= 1;
    assert!(map.set_root_bytes(&substituted));
    assert!(!map.authenticate(&storage, &store, 7, 9));
}

#[test]
fn internal_maximum_width_and_invalid_summaries() {
    let (_home, store, storage) = Home::open("internal-codec");
    let mut map = BuildMappingForTest::new(0);
    map.seed_runs(&storage, &store, &vec![(2, 1); 256]);
    let bytes = map.root_node_bytes(&storage, &store);
    assert_eq!(bytes.len(), 1385);
    assert_eq!(mapping_node_codec_roundtrip(&bytes), Some(bytes.clone()));
    let mut zero_child = bytes.clone();
    zero_child[121..153].fill(0);
    sign(&mut zero_child);
    assert!(mapping_node_codec_roundtrip(&zero_child).is_none());
    let mut overflow = bytes.clone();
    overflow[121..137].copy_from_slice(&(2 * u64::MAX as u128).to_be_bytes());
    overflow[201..217].copy_from_slice(&1u128.to_be_bytes());
    sign(&mut overflow);
    assert!(mapping_node_codec_roundtrip(&overflow).is_none());
    let mut unary = bytes[..153].to_vec();
    unary[65..73].copy_from_slice(&1u64.to_be_bytes());
    unary.extend([0; 32]);
    sign(&mut unary);
    assert!(mapping_node_codec_roundtrip(&unary).is_none());
    map.set_owner([72; 48]);
    assert!(!map.authenticate(&storage, &store, 0, 256));
}
