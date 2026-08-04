use super::{register, router};
use crate::cas_projection::connection::router::{
    LiveEventConnectionState, LiveEventTargetCloseReason,
};
use beryl_model::CasThreadId;

#[test]
fn retired_thread_lane_fence_is_bounded_by_connection_retirement() {
    let router = router(10);
    for index in 0..=super::RETIRED_THREAD_LANE_LIMIT {
        let registration = register(
            &router,
            &format!("cas-retired-{index}"),
            index as u8,
            index as u64 + 1,
            Some(&format!("turn-retired-{index}")),
        );
        let retired =
            router.unregister(&registration, LiveEventTargetCloseReason::ReceiverAbandoned);
        assert_eq!(retired, index == super::RETIRED_THREAD_LANE_LIMIT);
    }

    let snapshot = router.snapshot().unwrap();
    assert_eq!(
        snapshot.state(),
        LiveEventConnectionState::Retired(LiveEventTargetCloseReason::RetiredThreadLaneCapacity)
    );
    assert_eq!(
        snapshot.retired_thread_lane_count(),
        super::RETIRED_THREAD_LANE_LIMIT
    );
    assert_eq!(snapshot.target_count(), 0);
}

#[test]
fn absent_thread_close_overflow_fails_the_connection_closed() {
    let router = router(11);
    for index in 0..=super::RETIRED_THREAD_LANE_LIMIT {
        let thread_id = CasThreadId::new(format!("cas-closed-absent-{index}")).unwrap();
        let retired = router.record_thread_closed(&thread_id).unwrap();
        assert_eq!(retired, index == super::RETIRED_THREAD_LANE_LIMIT);
    }

    let snapshot = router.snapshot().unwrap();
    assert_eq!(
        snapshot.state(),
        LiveEventConnectionState::Retired(LiveEventTargetCloseReason::RetiredThreadLaneCapacity)
    );
    assert_eq!(
        snapshot.retired_thread_lane_count(),
        super::RETIRED_THREAD_LANE_LIMIT
    );
    assert_eq!(snapshot.target_count(), 0);
}
