use super::*;
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{
    draft_piece_build_encoding_for_test, inject_draft_piece_build_older_version_for_test,
};

fn hash_parts(parts: &[&[u8]]) -> [u8; 32] {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part);
    }
    digest.finalize().into()
}

fn expected_inserted_mapping(edit: &Transaction) -> Vec<u8> {
    let draft = edit.prepared.header().draft_id();
    let owner = [
        draft.as_bytes().as_slice(),
        edit.session.as_bytes().as_slice(),
        edit.operation.as_bytes().as_slice(),
    ]
    .concat();
    let entries = [vec![2], 2_u128.to_be_bytes().to_vec()].concat();
    let node_digest = hash_parts(&[
        b"syndic/draft-piece-build-mapping-node/v1",
        &owner,
        &[1],
        &1_u64.to_be_bytes(),
        &entries,
    ]);
    let id = hash_parts(&[
        b"syndic/draft-piece-build-mapping-record-id/v1",
        draft.as_bytes(),
        edit.session.as_bytes(),
        edit.operation.as_bytes(),
        &1_u64.to_be_bytes(),
        &node_digest,
    ]);
    [
        vec![1, 2],
        id[..16].to_vec(),
        node_digest.to_vec(),
        vec![1],
        0_u128.to_be_bytes().to_vec(),
        2_u128.to_be_bytes().to_vec(),
        0_u128.to_be_bytes().to_vec(),
        vec![0, 0],
    ]
    .concat()
}

#[test]
fn sequence_build_families_have_canonical_v6_vectors_and_reject_older_envelopes() {
    for (family, version) in (0..3).flat_map(|family| [3, 4, 5].map(|version| (family, version))) {
        let (_home, store, storage, thread) = fixture("sequence-format", 200);
        let edit = transaction(
            &storage,
            &store,
            &current(&storage, &store, thread),
            201,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                vec![DraftPieceV1::Text("α".to_owned())],
            )],
            point(2),
        );
        run_transaction(&storage, &store, &edit, 202);
        let key = syndic_storage::DraftPieceSettlementKeyV1::new(
            edit.prepared.header().draft_id(),
            edit.session,
            edit.operation,
        );
        let encodings = draft_piece_build_encoding_for_test(&store, &storage, key);
        assert_eq!(encodings.versions, [6, 6, 6]);
        assert_eq!(encodings.encoded, encodings.reencoded);
        let mapping = expected_inserted_mapping(&edit);
        assert_eq!(mapping.len(), 101);
        for encoded in &encodings.encoded[..2] {
            assert_eq!(
                encoded
                    .windows(mapping.len())
                    .filter(|window| *window == mapping.as_slice())
                    .count(),
                1
            );
        }
        let settlement = &encodings.encoded[2];
        let (payload, digest) = settlement.split_at(settlement.len() - 32);
        assert_eq!(
            digest,
            hash_parts(&[b"syndic/draft-piece-settlement/v6", payload])
        );
        let hashes = std::array::from_fn::<_, 3, _>(|index| {
            let mut digest = Sha256::new();
            digest.update(&encodings.keys[index]);
            digest.update(&encodings.encoded[index]);
            format!("{:x}", digest.finalize())
        });
        assert_eq!(
            hashes,
            [
                "a4709b97f3e962e0b2f24f2dbbc9379e8840caea14ceb7795b5446413855aa43",
                "2e3fff438967632b25287fc22123e48f6492071740329f4565f416ed4886ff6b",
                "a1599a05b122d1d47c461f90f2d90681d58ec2207993235f96f7f87523b36229",
            ]
        );
        inject_draft_piece_build_older_version_for_test(&store, &storage, key, family, version);
        assert!(
            store
                .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
                .is_err()
        );
    }
}
