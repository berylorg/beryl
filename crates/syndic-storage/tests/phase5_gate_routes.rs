use beryl_model::SyndicTurnId;
use syndic_storage::{CompactionOperationNonce, InputGateState, StopOperationNonce};

fn turn(value: u8) -> SyndicTurnId {
    SyndicTurnId::from_bytes([value; 16])
}

#[test]
fn input_gate_retains_only_coarse_turn_mode() {
    let active = turn(1);
    assert_eq!(InputGateState::Idle.blocking_turn_id(), None);
    assert_eq!(
        InputGateState::PendingTurn(active).blocking_turn_id(),
        Some(active)
    );
    assert_eq!(
        InputGateState::AwaitingSteering(active).blocking_turn_id(),
        Some(active)
    );
    assert_eq!(
        InputGateState::Steerable(active).blocking_turn_id(),
        Some(active)
    );
    assert_eq!(
        InputGateState::Compacting {
            turn_id: active,
            operation_nonce: CompactionOperationNonce::from_bytes([8; 16]),
        }
        .blocking_turn_id(),
        Some(active)
    );
    assert_eq!(
        InputGateState::stopping(active, StopOperationNonce::from_bytes([9; 16]))
            .blocking_turn_id(),
        Some(active)
    );
}
