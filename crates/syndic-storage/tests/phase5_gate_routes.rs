use beryl_model::{
    BindingRevision, CasThreadId, CasTurnId, SyndicExecutionSnapshotId, SyndicTurnId,
};
use syndic_storage::{
    AcceptedInputDisposition, InputGateState, NextTurnReason, PendingSteeringTargetProof,
    SteeringTargetProof,
};

#[test]
fn every_non_idle_gate_state_has_one_exact_admission_route() {
    let active_turn = SyndicTurnId::from_bytes([1; 16]);
    let pending = PendingSteeringTargetProof::new(
        BindingRevision::new(2).unwrap(),
        SyndicExecutionSnapshotId::from_bytes([3; 16]),
        active_turn,
        CasThreadId::new("gate-thread").unwrap(),
    );
    let target = SteeringTargetProof::new(pending.clone(), CasTurnId::new("gate-turn").unwrap());

    assert_eq!(InputGateState::Idle.admitted_disposition(), None);
    assert_eq!(
        InputGateState::PendingTurn(active_turn).admitted_disposition(),
        Some(AcceptedInputDisposition::NextTurn(
            NextTurnReason::PendingTurn,
        )),
    );
    assert_eq!(
        InputGateState::AwaitingSteering(pending.clone()).admitted_disposition(),
        Some(AcceptedInputDisposition::AwaitingSteering(pending)),
    );
    assert_eq!(
        InputGateState::Steerable(target.clone()).admitted_disposition(),
        Some(AcceptedInputDisposition::SteerActiveTurn(target.clone())),
    );
    assert_eq!(
        InputGateState::Compacting(active_turn).admitted_disposition(),
        Some(AcceptedInputDisposition::NextTurn(
            NextTurnReason::Compaction,
        )),
    );
    assert_eq!(
        InputGateState::Stopping(target).admitted_disposition(),
        Some(AcceptedInputDisposition::NextTurn(NextTurnReason::Stop)),
    );
}
