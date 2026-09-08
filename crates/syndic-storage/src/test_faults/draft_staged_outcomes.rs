use crate::{SyndicPointReadLimit, SyndicStorage, codec::Family, draft_piece::*};
use beryl_home_store::{HomeStore, RecordCodec};

pub fn staged_outcome_build_for_test(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
) -> DraftPieceBuildRecordV1 {
    storage
        .point::<DraftPieceBuildsFamily>(
            store,
            DraftPieceSettlementKeyV1::new(
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            ),
            SyndicPointReadLimit::new(DraftPieceBuildsFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap()
}

pub fn corrupt_staged_outcome_pending_sequence_for_test(
    storage: &SyndicStorage,
    store: &HomeStore,
    build: &DraftPieceBuildRecordV1,
) {
    let active = build.marker_effect_continuation().active().unwrap();
    let descriptor = match active.pending() {
        DraftPieceMarkerPendingV1::InsertIdentity {
            sequence_target, ..
        }
        | DraftPieceMarkerPendingV1::InsertOrder {
            sequence_target, ..
        }
        | DraftPieceMarkerPendingV1::RemoveIdentity { sequence_target }
        | DraftPieceMarkerPendingV1::RemoveOrder {
            sequence_target, ..
        } => sequence_target,
        _ => panic!("fixture requires an independently pending sequence root"),
    };
    let key = DraftPieceRecordKeyV1::new(build.draft_id(), descriptor.root_node_id.unwrap());
    let record = storage
        .point::<DraftPieceNodesFamily>(
            store,
            key,
            SyndicPointReadLimit::new(DraftPieceNodesFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    let mut encoded = DraftPieceNodesFamily::encode_value(&record).unwrap();
    *encoded.last_mut().unwrap() ^= 1;
    let mut stored = DraftPieceNodesFamily::RECORD_VERSION
        .get()
        .to_be_bytes()
        .to_vec();
    stored.extend_from_slice(&encoded);
    let key = <crate::codec::ExactCodec<DraftPieceNodesFamily> as RecordCodec<
        crate::domain::SyndicDomain,
    >>::encode_key(&key)
    .unwrap();
    store.inject_persisted_corrupt_record::<crate::domain::SyndicDomain, crate::codec::ExactCodec<DraftPieceNodesFamily>>(
        &storage.handle, &key, &stored).unwrap();
}
