#![cfg(feature = "test-faults")]

use syndic_storage::test_faults::{
    RetiredWorkerCapacityCodecField, retired_worker_capacity_codec_rejection,
};

#[test]
fn retired_worker_capacity_next_turn_reason_tag_is_rejected() {
    assert_eq!(
        retired_worker_capacity_codec_rejection(RetiredWorkerCapacityCodecField::NextTurnReason,),
        Some(("next-turn reason", 4)),
    );
}

#[test]
fn retired_worker_capacity_leaf_transition_tag_is_rejected() {
    assert_eq!(
        retired_worker_capacity_codec_rejection(
            RetiredWorkerCapacityCodecField::AcceptedRouteLeafTransition,
        ),
        Some(("accepted-route leaf transition", 4)),
    );
}
