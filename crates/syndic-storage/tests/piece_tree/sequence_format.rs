use super::*;
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{
    draft_piece_build_encoding_for_test, inject_draft_piece_build_v3_for_test,
};

#[test]
fn sequence_build_families_have_canonical_v4_vectors_and_reject_v3_envelopes() {
    for family in 0..3 {
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
        assert_eq!(encodings.versions, [4, 4, 4]);
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
                "ba55d95a5e84f57128bb2a8a8db30a7537069a7743581257e436adc821dd4c33",
                "b5ef2b55406c9f16b69307313daca75cf3db2c9941c9ad2e386f214889f5e7c3",
                "4939bf1159c0cab0683bac946521ffbd34cc272f26e687bf6c805732bfc4710d",
            ]
        );
        inject_draft_piece_build_v3_for_test(&store, &storage, key, family);
        assert!(
            store
                .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
                .is_err()
        );
    }
}
