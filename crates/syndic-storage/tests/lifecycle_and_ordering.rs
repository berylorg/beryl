use std::any::TypeId;

use beryl_model::{
    BindingRevision, CasThreadId, CasTurnId, SyndicExecutionSnapshotId, SyndicTurnId,
};
use syndic_storage::{
    AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedRouteTarget, BindingLifecycle,
    PendingSteeringTargetProof, SourceEventSequence, SteeringTargetProof, SyndicValueError,
    TranscriptPosition, TurnLifecycle,
};

#[test]
fn turn_lifecycle_exposes_only_proven_terminal_and_blocking_facts() {
    assert!(TurnLifecycle::Complete.is_proven_terminal());
    assert!(TurnLifecycle::Interrupted.is_proven_terminal());
    assert!(TurnLifecycle::Failed.is_proven_terminal());
    assert!(TurnLifecycle::Incomplete.is_proven_terminal());
    assert!(!TurnLifecycle::UnknownTerminal.is_proven_terminal());

    assert!(TurnLifecycle::Pending.blocks_same_thread_start());
    assert!(TurnLifecycle::Active.blocks_same_thread_start());
    assert!(TurnLifecycle::UnknownTerminal.blocks_same_thread_start());
    assert!(!TurnLifecycle::Complete.blocks_same_thread_start());
}

#[test]
fn accepted_input_and_binding_lifecycles_do_not_collapse() {
    assert!(AcceptedInputLifecycle::Delivered.is_terminal());
    assert!(AcceptedInputLifecycle::Failed.is_terminal());
    assert!(AcceptedInputLifecycle::DeliveryUnknown.is_terminal());
    assert!(AcceptedInputLifecycle::Promoted.is_terminal());
    assert!(!AcceptedInputLifecycle::Retryable.is_terminal());
    assert_ne!(BindingLifecycle::Unbound, BindingLifecycle::Stale);
    assert_ne!(BindingLifecycle::Valid, BindingLifecycle::Active);
}

#[test]
fn route_generation_target_carries_the_exact_active_turn() {
    let active_turn = SyndicTurnId::from_bytes([7; 16]);
    let other_turn = SyndicTurnId::from_bytes([8; 16]);
    let target = |turn| {
        SteeringTargetProof::new(
            PendingSteeringTargetProof::new(
                BindingRevision::new(3).unwrap(),
                SyndicExecutionSnapshotId::from_bytes([9; 16]),
                turn,
                CasThreadId::new("thread").unwrap(),
            ),
            CasTurnId::new("turn").unwrap(),
        )
    };
    let route_target = AcceptedRouteTarget::Steering(target(active_turn));

    assert!(matches!(
        &route_target,
        AcceptedRouteTarget::Steering(target)
            if target.pending().active_turn_id() == active_turn
    ));
    assert_ne!(
        route_target,
        AcceptedRouteTarget::Steering(target(other_turn))
    );
}

#[test]
fn one_based_values_reject_zero_and_exhaustion() {
    assert!(matches!(
        AcceptedInputOrdinal::new(0),
        Err(SyndicValueError::ZeroOrdinal { .. })
    ));
    assert_eq!(AcceptedInputOrdinal::FIRST.get(), 1);
    assert_eq!(SourceEventSequence::new(9).unwrap().get(), 9);
    assert_eq!(TranscriptPosition::new(11).unwrap().get(), 11);
    assert!(matches!(
        TranscriptPosition::new(u64::MAX).unwrap().checked_next(),
        Err(SyndicValueError::OrdinalExhausted { .. })
    ));
}

#[test]
fn ordering_domains_are_not_interchangeable_types() {
    assert_ne!(
        TypeId::of::<AcceptedInputOrdinal>(),
        TypeId::of::<SourceEventSequence>()
    );
    assert_ne!(
        TypeId::of::<SourceEventSequence>(),
        TypeId::of::<TranscriptPosition>()
    );
}
