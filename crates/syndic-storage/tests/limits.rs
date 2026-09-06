use beryl_model::{InputGateRevision, SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    AcceptedRouteGeneration, AcceptedRouteHeadProof, AcceptedRouteRevision, InputGateRecord,
    InputGateState, SyndicRecordError,
};

#[test]
fn input_gate_has_no_logical_live_route_cap() {
    let gate = InputGateRecord::new(
        SyndicThreadId::from_bytes([1; 16]),
        InputGateRevision::new(1).unwrap(),
        InputGateState::PendingTurn(SyndicTurnId::from_bytes([2; 16])),
        10_000,
        None,
        None,
        9_000,
        1_000,
        u64::MAX,
    )
    .unwrap();
    assert_eq!(gate.live_count(), 10_000);
}

#[test]
fn input_gate_rejects_only_checked_aggregate_overflow() {
    assert!(matches!(
        InputGateRecord::new(
            SyndicThreadId::from_bytes([1; 16]),
            InputGateRevision::new(1).unwrap(),
            InputGateState::PendingTurn(SyndicTurnId::from_bytes([2; 16])),
            u64::MAX,
            None,
            None,
            u64::MAX,
            1,
            0,
        ),
        Err(SyndicRecordError::LengthOverflow { .. })
    ));
}

#[test]
fn input_gate_rejects_a_selected_route_beyond_allocator_authority() {
    let thread = SyndicThreadId::from_bytes([1; 16]);
    let turn = SyndicTurnId::from_bytes([2; 16]);
    let selected =
        AcceptedRouteHeadProof::new(AcceptedRouteGeneration::FIRST, AcceptedRouteRevision::FIRST);
    assert!(matches!(
        InputGateRecord::new(
            thread,
            InputGateRevision::new(1).unwrap(),
            InputGateState::PendingTurn(turn),
            0,
            None,
            Some(selected),
            0,
            0,
            0,
        ),
        Err(SyndicRecordError::InvalidAcceptedRouteSelection)
    ));
    assert!(matches!(
        InputGateRecord::new(
            thread,
            InputGateRevision::new(1).unwrap(),
            InputGateState::PendingTurn(turn),
            0,
            Some(AcceptedRouteGeneration::FIRST),
            Some(AcceptedRouteHeadProof::new(
                AcceptedRouteGeneration::new(2).unwrap(),
                AcceptedRouteRevision::FIRST,
            )),
            0,
            0,
            0,
        ),
        Err(SyndicRecordError::InvalidAcceptedRouteSelection)
    ));
}
