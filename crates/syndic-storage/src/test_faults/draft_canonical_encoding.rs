use crate::{SyndicPointReadLimit, SyndicStorage, codec::Family, draft_piece::*};
use beryl_home_store::HomeStore;

pub struct DraftCanonicalEncodingForTest {
    pub versions: [u32; 5],
    pub keys: [Vec<u8>; 5],
    pub payloads: [Option<Vec<u8>>; 5],
}

pub fn draft_canonical_encoding_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    key: DraftPieceSettlementKeyV1,
    head: &DraftEditorCandidateSessionV1,
) -> DraftCanonicalEncodingForTest {
    let build = storage
        .point::<DraftPieceBuildsFamily>(store, key, SyndicPointReadLimit::new(65_540).unwrap())
        .unwrap()
        .unwrap();
    let progress_key = build.progress_receipt().key();
    let history_key = head.newest_history().key();
    let session_key =
        DraftEditorCandidateSessionRecordKeyV1::head(head.draft_id(), head.session_id());
    DraftCanonicalEncodingForTest {
        versions: [
            DraftPieceBuildsFamily::RECORD_VERSION.get(),
            DraftPieceBuildProgressFamily::RECORD_VERSION.get(),
            DraftPieceSettlementsFamily::RECORD_VERSION.get(),
            DraftEditHistoryFrontiersFamily::RECORD_VERSION.get(),
            DraftEditorCandidateSessionsFamily::RECORD_VERSION.get(),
        ],
        keys: [
            DraftPieceBuildsFamily::encode_key(&key).unwrap(),
            DraftPieceBuildProgressFamily::encode_key(&progress_key).unwrap(),
            DraftPieceSettlementsFamily::encode_key(&key).unwrap(),
            DraftEditHistoryFrontiersFamily::encode_key(&history_key).unwrap(),
            DraftEditorCandidateSessionsFamily::encode_key(&session_key).unwrap(),
        ],
        payloads: [
            encoded::<DraftPieceBuildsFamily>(store, storage, key),
            encoded::<DraftPieceBuildProgressFamily>(store, storage, progress_key),
            encoded::<DraftPieceSettlementsFamily>(store, storage, key),
            encoded::<DraftEditHistoryFrontiersFamily>(store, storage, history_key),
            encoded::<DraftEditorCandidateSessionsFamily>(store, storage, session_key),
        ],
    }
}

fn encoded<F: Family>(store: &HomeStore, storage: &SyndicStorage, key: F::Key) -> Option<Vec<u8>> {
    storage
        .point::<F>(
            store,
            key,
            SyndicPointReadLimit::new(F::MAX_VALUE_BYTES + 4).unwrap(),
        )
        .unwrap()
        .map(|value| F::encode_value(&value).unwrap())
}

pub fn draft_canonical_payload_reencode_for_test(family: usize, payload: &[u8]) -> Option<Vec<u8>> {
    fn roundtrip<F: Family>(payload: &[u8]) -> Option<Vec<u8>> {
        F::encode_value(&F::decode_value(payload).ok()?).ok()
    }
    match family {
        0 => roundtrip::<DraftPieceBuildsFamily>(payload),
        1 => roundtrip::<DraftPieceBuildProgressFamily>(payload),
        2 => roundtrip::<DraftPieceSettlementsFamily>(payload),
        3 => roundtrip::<DraftEditHistoryFrontiersFamily>(payload),
        4 => roundtrip::<DraftEditorCandidateSessionsFamily>(payload),
        _ => None,
    }
}
