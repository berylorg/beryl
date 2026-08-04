use std::{sync::atomic::Ordering, time::Duration};

use beryl_backend::{
    ApprovalInterruption, ApprovalRequest, ApprovalRequestKind, ApprovalResponseDisposition,
    OrderedTurnStreamSubmitCause, lifecycle_test_support::approval_request,
};
use beryl_model::{CasItemId, CasThreadId, CasTurnId};

use super::*;
use crate::cas_projection::LiveEventTargetCloseReason;

fn command_approval(thread_id: &str, turn_id: &str, item_id: Option<CasItemId>) -> ApprovalRequest {
    approval_request(
        ApprovalRequestKind::CommandExecution,
        ApprovalResponseDisposition::ResponseRequired,
        Some(CasThreadId::new(thread_id).unwrap()),
        Some(CasTurnId::new(turn_id).unwrap()),
        item_id,
    )
}

#[test]
fn target_operation_queue_returns_one_over_and_reuses_capacity_without_accumulation() {
    let router = router(51);
    let registration = register(
        &router,
        "cas-operation-capacity",
        51,
        51,
        Some("turn-operation-capacity"),
    );

    for _ in 0..super::super::TARGET_OPERATION_QUEUE_CAPACITY {
        assert!(matches!(
            router.route_approval(
                &live_command(&router),
                command_approval("cas-operation-capacity", "turn-operation-capacity", None,),
                None
            ),
            ApprovalRouteOutcome::Routed {
                interruption: ApprovalInterruption::NotRequired,
                obligation: None,
            }
        ));
    }
    let full = router.snapshot().unwrap();
    assert_eq!(
        full.queued_operation_count(),
        super::super::TARGET_OPERATION_QUEUE_CAPACITY
    );
    assert_eq!(
        registration.queued_operations.load(Ordering::Acquire),
        super::super::TARGET_OPERATION_QUEUE_CAPACITY
    );

    let overflow_item = CasItemId::new("approval-overflow-marker").unwrap();
    let overflow = router.route_approval(
        &live_command(&router),
        command_approval(
            "cas-operation-capacity",
            "turn-operation-capacity",
            Some(overflow_item.clone()),
        ),
        None,
    );
    let ApprovalRouteOutcome::TargetFailed {
        request,
        cause,
        target,
        obligation,
    } = overflow
    else {
        panic!("the one-over approval must retain its request in a target-local failure")
    };
    assert_eq!(request.kind(), ApprovalRequestKind::CommandExecution);
    assert_eq!(request.item_id(), Some(&overflow_item));
    assert_eq!(cause, OrderedTurnStreamSubmitCause::CapacityFull);
    assert_eq!(target.reason, LiveEventTargetCloseReason::QueueOverflow);
    assert!(obligation.is_none());
    assert_eq!(
        registration.queued_operations.load(Ordering::Acquire),
        super::super::TARGET_OPERATION_QUEUE_CAPACITY
    );
    drop(request);

    for _ in 0..super::super::TARGET_OPERATION_QUEUE_CAPACITY {
        let LiveEventPoll::Approval(approval) = registration.poll(Duration::ZERO) else {
            panic!("the retained registration must drain every accepted approval")
        };
        let (request, interruption) = approval.into_parts();
        assert_eq!(request.kind(), ApprovalRequestKind::CommandExecution);
        assert_eq!(interruption, ApprovalInterruption::NotRequired);
    }
    assert_eq!(registration.queued_operations.load(Ordering::Acquire), 0);
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Closed(LiveEventTargetCloseReason::QueueOverflow)
    ));
    drop(registration);

    let later = register(
        &router,
        "cas-operation-capacity-later",
        52,
        52,
        Some("turn-operation-capacity-later"),
    );
    for _ in 0..super::super::TARGET_OPERATION_QUEUE_CAPACITY {
        assert!(matches!(
            router.route_approval(
                &live_command(&router),
                command_approval(
                    "cas-operation-capacity-later",
                    "turn-operation-capacity-later",
                    None,
                ),
                None
            ),
            ApprovalRouteOutcome::Routed { .. }
        ));
    }
    assert_eq!(
        router.snapshot().unwrap().queued_operation_count(),
        super::super::TARGET_OPERATION_QUEUE_CAPACITY
    );
    for _ in 0..super::super::TARGET_OPERATION_QUEUE_CAPACITY {
        assert!(matches!(
            later.poll(Duration::ZERO),
            LiveEventPoll::Approval(_)
        ));
    }
    assert_eq!(later.queued_operations.load(Ordering::Acquire), 0);
    assert!(!router.unregister(&later, LiveEventTargetCloseReason::ReceiverAbandoned));
    drop(later);

    let released = router.snapshot().unwrap();
    assert_eq!(released.target_count(), 0);
    assert_eq!(released.queued_operation_count(), 0);
    assert_eq!(released.queue_pressure_count(), 1);
}
