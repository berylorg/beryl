use super::*;
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{
    draft_piece_build_encoding_for_test, inject_draft_piece_build_older_version_for_test,
};

#[test]
fn sequence_build_families_have_canonical_v5_vectors_and_reject_older_envelopes() {
    for (family, version) in (0..3).flat_map(|family| [3, 4].map(|version| (family, version))) {
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
        assert_eq!(encodings.versions, [5, 5, 5]);
        assert_eq!(encodings.encoded, encodings.reencoded);
        let hashes = std::array::from_fn::<_, 3, _>(|index| {
            let mut digest = Sha256::new();
            digest.update(&encodings.keys[index]);
            digest.update(&encodings.encoded[index]);
            format!("{:x}", digest.finalize())
        });
        assert_eq!(
            hashes,
            [
                "27d0420bc72f1e3c3eee62e0db524dc90fd9a7e4a0ccd95543299960edcb185c",
                "e2bb8ab6dabc90714935018cfadfa32f8895fe2325484ea9b56d342b42ff7ecc",
                "0842202cb4afdd58deb686436e7a0bdeb325cbf0581781b994aa598112e93008",
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
