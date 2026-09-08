use crate::{
    SyndicStorage,
    codec::{ExactCodec, Family},
    domain::SyndicDomain,
    draft_piece::*,
};
use beryl_home_store::{HomeStore, RecordCodec};

pub struct DraftPieceBuildEncodingForTest {
    pub versions: [u32; 3],
    pub keys: [Vec<u8>; 3],
    pub encoded: [Vec<u8>; 3],
    pub reencoded: [Vec<u8>; 3],
}

pub fn draft_piece_build_encoding_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    key: DraftPieceSettlementKeyV1,
) -> DraftPieceBuildEncodingForTest {
    let build = storage
        .point::<DraftPieceBuildsFamily>(
            store,
            key,
            crate::SyndicPointReadLimit::new(DraftPieceBuildsFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    let receipt = storage
        .point::<DraftPieceBuildProgressFamily>(
            store,
            build.progress_receipt().key(),
            crate::SyndicPointReadLimit::new(DraftPieceBuildProgressFamily::MAX_VALUE_BYTES)
                .unwrap(),
        )
        .unwrap()
        .unwrap();
    let settlement = storage
        .point::<DraftPieceSettlementsFamily>(
            store,
            key,
            crate::SyndicPointReadLimit::new(DraftPieceSettlementsFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    let keys = [
        DraftPieceBuildsFamily::encode_key(&key).unwrap(),
        DraftPieceBuildProgressFamily::encode_key(&build.progress_receipt().key()).unwrap(),
        DraftPieceSettlementsFamily::encode_key(&key).unwrap(),
    ];
    let (build, rebuilt) = roundtrip::<DraftPieceBuildsFamily>(&build);
    let (receipt, rereceipt) = roundtrip::<DraftPieceBuildProgressFamily>(&receipt);
    let (settlement, resettlement) = roundtrip::<DraftPieceSettlementsFamily>(&settlement);
    DraftPieceBuildEncodingForTest {
        keys,
        versions: [
            DraftPieceBuildsFamily::RECORD_VERSION.get(),
            DraftPieceBuildProgressFamily::RECORD_VERSION.get(),
            DraftPieceSettlementsFamily::RECORD_VERSION.get(),
        ],
        encoded: [build, receipt, settlement],
        reencoded: [rebuilt, rereceipt, resettlement],
    }
}

fn roundtrip<F: Family>(value: &F::Value) -> (Vec<u8>, Vec<u8>) {
    let encoded = F::encode_value(value).unwrap();
    let decoded = F::decode_value(&encoded).unwrap();
    (encoded, F::encode_value(&decoded).unwrap())
}

pub fn inject_draft_piece_build_older_version_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    key: DraftPieceSettlementKeyV1,
    family: usize,
    version: u32,
) {
    let build = storage
        .point::<DraftPieceBuildsFamily>(
            store,
            key,
            crate::SyndicPointReadLimit::new(DraftPieceBuildsFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    let encodings = draft_piece_build_encoding_for_test(store, storage, key);
    assert!(matches!(version, 3 | 4 | 5));
    let mut stored = version.to_be_bytes().to_vec();
    stored.extend_from_slice(&encodings.encoded[family]);
    match family {
        0 => inject::<DraftPieceBuildsFamily>(store, storage, &key, &stored),
        1 => inject::<DraftPieceBuildProgressFamily>(
            store,
            storage,
            &build.progress_receipt().key(),
            &stored,
        ),
        2 => inject::<DraftPieceSettlementsFamily>(store, storage, &key, &stored),
        _ => panic!("unknown draft piece build family"),
    }
}

fn inject<F: Family>(store: &HomeStore, storage: &SyndicStorage, key: &F::Key, encoded: &[u8]) {
    let key = <ExactCodec<F> as RecordCodec<SyndicDomain>>::encode_key(key).unwrap();
    store
        .inject_persisted_corrupt_record::<SyndicDomain, ExactCodec<F>>(
            &storage.handle,
            &key,
            encoded,
        )
        .unwrap();
}
