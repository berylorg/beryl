use beryl_home_store::{RECORD_VERSION_BYTES, RecordCodec};
use beryl_model::{InputGateRevision, SyndicAcceptedInputId, SyndicItemId, SyndicTurnId};

use super::*;
use crate::{
    AcceptedInputPromotionProof, AcceptedOrderIndexRecord, AcceptedRouteHeadProof,
    AcceptedRouteLeafTransitionKind, AcceptedRouteLeafTransitionProof, SyndicTimestamp,
};

#[test]
fn maximal_v3_route_scan_row_matches_the_continuation_guard() {
    let thread = SyndicThreadId::from_bytes([1; 16]);
    let input = SyndicAcceptedInputId::from_bytes([2; 16]);
    let generation = AcceptedRouteGeneration::FIRST;
    let route = AcceptedRouteHeadProof::new(generation, AcceptedRouteRevision::FIRST);
    let transition = AcceptedRouteLeafTransitionProof::new(
        InputGateRevision::new(1).unwrap(),
        route,
        AcceptedInputRevision::new(1).unwrap(),
        AcceptedRouteLeafTransitionKind::SteeringRejected,
    );
    let promotion = AcceptedInputPromotionProof::new(
        InputGateRevision::new(2).unwrap(),
        route,
        AcceptedInputRevision::new(2).unwrap(),
        SyndicTurnId::from_bytes([3; 16]),
        SyndicItemId::from_bytes([4; 16]),
        SyndicTimestamp::from_unix_millis(5),
    );
    let leaf = AcceptedRouteLeafRecord::new(
        input,
        thread,
        generation,
        AcceptedInputOrdinal::FIRST,
        AcceptedInputRevision::new(3).unwrap(),
        AcceptedRouteLeafState::Routed,
        AcceptedInputLifecycle::Promoted,
    )
    .with_transition_proof(transition)
    .with_promotion_proof(promotion);
    let order =
        AcceptedOrderIndexRecord::new(thread, AcceptedInputOrdinal::FIRST, input, generation);
    let stored = AcceptedOrderCodec::encode_key(&ThreadAcceptedKey {
        owner: thread,
        ordinal: AcceptedInputOrdinal::FIRST,
    })
    .unwrap()
    .len()
        + RECORD_VERSION_BYTES
        + AcceptedOrderCodec::encode_value(&order).unwrap().len()
        + AcceptedRouteLeavesCodec::encode_key(&input).unwrap().len()
        + RECORD_VERSION_BYTES
        + AcceptedRouteLeavesCodec::encode_value(&leaf).unwrap().len();
    assert_eq!(stored, NEXT_CANDIDATE_MAX_ROW_BYTES);
}
