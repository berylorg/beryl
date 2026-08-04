use std::{
    cell::Cell,
    sync::{Arc, TryLockError},
    thread,
    time::{Duration, Instant},
};

use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    BindingRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasThreadId, CasTurnId,
    SyndicExecutionSnapshotId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{PendingSteeringTargetProof, SteeringTargetProof};

use super::{register, router};
use crate::cas_projection::connection::router::{
    DelayedSteeringLifecycleFinishError, DelayedSteeringLifecyclePermitError, LiveEventPoll,
    LiveEventTargetCloseReason, SourcePublicationFinishError, TargetLossAcquisition, TargetTurn,
};

fn exact_target(
    generation: u64,
    thread: &'static str,
    turn: &'static str,
) -> (
    Arc<super::EventRouter>,
    super::TargetRegistration,
    CasThreadId,
    CasTurnId,
) {
    let router = router(generation);
    let registration = register(&router, thread, generation as u8, generation, Some(turn));
    (
        router,
        registration,
        CasThreadId::new(thread).unwrap(),
        CasTurnId::new(turn).unwrap(),
    )
}

fn steering_target(thread: &CasThreadId, turn: &CasTurnId, seed: u8) -> SteeringTargetProof {
    SteeringTargetProof::new(
        PendingSteeringTargetProof::new(
            BindingRevision::new(1).unwrap(),
            SyndicExecutionSnapshotId::from_bytes([seed; 16]),
            SyndicTurnId::from_bytes([seed.wrapping_add(1); 16]),
            thread.clone(),
        ),
        turn.clone(),
    )
}

#[test]
fn acquisition_requires_an_already_exact_durably_activated_target() {
    let pending_router = router(61);
    let registration = register(&pending_router, "cas-steering-pending", 61, 61, None);
    let thread = CasThreadId::new("cas-steering-pending").unwrap();
    let turn = CasTurnId::new("turn-steering-pending").unwrap();

    assert!(matches!(
        pending_router.acquire_delayed_steering_lifecycle(
            &super::live_command(&pending_router),
            &thread,
            &turn,
        ),
        Err(DelayedSteeringLifecyclePermitError::Target(ref invalidation))
            if invalidation.reason == LiveEventTargetCloseReason::EventBeforeTurnStart
    ));
    assert_eq!(registration.bound_turn(), None);
    assert_eq!(
        pending_router.snapshot().unwrap().routed_operation_count(),
        0
    );

    let undurable_router = router(62);
    let registration = register(&undurable_router, "cas-steering-undurable", 62, 62, None);
    undurable_router
        .authorize_turn_start(&registration.proof())
        .unwrap();
    let thread = CasThreadId::new("cas-steering-undurable").unwrap();
    let turn = CasTurnId::new("turn-steering-undurable").unwrap();

    assert!(matches!(
        undurable_router.acquire_delayed_steering_lifecycle(
            &super::live_command(&undurable_router),
            &thread,
            &turn,
        ),
        Err(DelayedSteeringLifecyclePermitError::Target(ref invalidation))
            if invalidation.reason
                == LiveEventTargetCloseReason::TurnActivationPublicationFailed
    ));
    assert_eq!(registration.bound_turn(), None);
    let state = undurable_router.state.lock().unwrap();
    let target = state.targets.get(&thread).unwrap();
    assert_eq!(target.turn_state, TargetTurn::AwaitingStart);
    assert!(target.start_dispatched);
    assert!(!target.activation_durable);
}

#[test]
fn wrong_exact_turn_fails_closed_without_acquiring_a_permit() {
    let (router, registration, thread, _turn) =
        exact_target(63, "cas-steering-wrong-turn", "turn-steering-exact");
    let wrong_turn = CasTurnId::new("turn-steering-other").unwrap();

    assert!(matches!(
        router.acquire_delayed_steering_lifecycle(
            &super::live_command(&router),
            &thread,
            &wrong_turn,
        ),
        Err(DelayedSteeringLifecyclePermitError::Target(ref invalidation))
            if invalidation.reason == LiveEventTargetCloseReason::ConflictingTurnIdentity
    ));
    assert_eq!(
        registration.terminal_reason(),
        Some(LiveEventTargetCloseReason::ConflictingTurnIdentity)
    );
    assert_eq!(router.snapshot().unwrap().target_count(), 0);
}

#[test]
fn acquisition_rejects_an_ordinary_publication_or_target_loss_owner() {
    let (source_router, registration, thread, turn) =
        exact_target(71, "cas-steering-source", "turn-steering-source");
    let source = source_router
        .acquire_source_publication(&thread, &turn)
        .unwrap();
    assert!(matches!(
        source_router.acquire_delayed_steering_lifecycle(
            &super::live_command(&source_router),
            &thread,
            &turn,
        ),
        Err(DelayedSteeringLifecyclePermitError::Target(ref invalidation))
            if invalidation.reason
                == LiveEventTargetCloseReason::SourcePublicationRouteUnavailable
    ));
    assert!(matches!(
        source.finish(),
        Err(SourcePublicationFinishError::Target(ref invalidation))
            if invalidation.reason
                == LiveEventTargetCloseReason::SourcePublicationRouteUnavailable
    ));
    assert_eq!(
        registration.terminal_reason(),
        Some(LiveEventTargetCloseReason::SourcePublicationRouteUnavailable)
    );

    let loss_router = router(72);
    let registration = register(&loss_router, "cas-steering-loss", 72, 72, None);
    loss_router
        .authorize_turn_start(&registration.proof())
        .unwrap();
    let thread = CasThreadId::new("cas-steering-loss").unwrap();
    let turn = CasTurnId::new("turn-steering-loss").unwrap();
    loss_router
        .acquire_source_publication(&thread, &turn)
        .unwrap()
        .finish()
        .unwrap();
    let TargetLossAcquisition::Authority(loss) = loss_router
        .acquire_target_loss(super::live_command(&loss_router), &registration.proof())
        .unwrap()
    else {
        panic!("active target loss must acquire its publication authority");
    };
    assert!(matches!(
        loss_router.acquire_delayed_steering_lifecycle(
            &super::live_command(&loss_router),
            &thread,
            &turn,
        ),
        Err(DelayedSteeringLifecyclePermitError::Unmatched)
    ));
    drop(loss);
}

#[test]
fn loss_requested_during_the_permit_suppresses_commit_and_wakes_the_waiter() {
    let router = router(73);
    let registration = register(&router, "cas-steering-loss-race", 73, 73, None);
    router.authorize_turn_start(&registration.proof()).unwrap();
    let thread_id = CasThreadId::new("cas-steering-loss-race").unwrap();
    let turn_id = CasTurnId::new("turn-steering-loss-race").unwrap();
    router
        .acquire_source_publication(&thread_id, &turn_id)
        .unwrap()
        .finish()
        .unwrap();
    let permit = router
        .acquire_delayed_steering_lifecycle(&super::live_command(&router), &thread_id, &turn_id)
        .unwrap();

    let loss_router = Arc::clone(&router);
    let proof = registration.proof();
    let waiter = thread::spawn(move || {
        let command = super::live_command(&loss_router);
        loss_router.acquire_target_loss(command, &proof)
    });
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let loss_requested = router
            .state
            .lock()
            .unwrap()
            .targets
            .get(&thread_id)
            .is_some_and(|target| target.loss_requested);
        if loss_requested {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "target-loss waiter did not claim the target"
        );
        thread::yield_now();
    }

    let calls = Cell::new(0);
    assert!(matches!(
        permit.finish_with(|| calls.set(calls.get() + 1)),
        Err(DelayedSteeringLifecycleFinishError::Target(ref invalidation))
            if invalidation.reason
                == LiveEventTargetCloseReason::SourcePublicationRouteUnavailable
    ));
    assert_eq!(calls.get(), 0);
    let TargetLossAcquisition::Authority(authority) = waiter.join().unwrap().unwrap() else {
        panic!("waiting loss request must wake with its publication authority");
    };
    authority.fail().unwrap();
}

#[test]
fn close_before_or_during_the_permit_never_invokes_the_callback() {
    let (router, registration, thread, turn) = exact_target(
        64,
        "cas-steering-close-before",
        "turn-steering-close-before",
    );
    assert!(!router.unregister(&registration, LiveEventTargetCloseReason::ThreadClosed));
    assert!(matches!(
        router.acquire_delayed_steering_lifecycle(&super::live_command(&router), &thread, &turn,),
        Err(DelayedSteeringLifecyclePermitError::Unmatched)
    ));

    let (router, registration, thread, turn) = exact_target(
        65,
        "cas-steering-close-during",
        "turn-steering-close-during",
    );
    let permit = router
        .acquire_delayed_steering_lifecycle(&super::live_command(&router), &thread, &turn)
        .unwrap();
    assert!(!router.unregister(&registration, LiveEventTargetCloseReason::ThreadClosed));
    let calls = Cell::new(0);
    assert!(matches!(
        permit.finish_with(|| calls.set(calls.get() + 1)),
        Err(DelayedSteeringLifecycleFinishError::Target(ref invalidation))
            if invalidation.reason == LiveEventTargetCloseReason::ThreadClosed
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(router.snapshot().unwrap().target_count(), 0);
}

#[test]
fn stable_finish_invokes_the_callback_once_under_the_router_lock() {
    let (router, registration, thread, turn) =
        exact_target(66, "cas-steering-finish", "turn-steering-finish");
    let before = router.snapshot().unwrap();
    let permit = router
        .acquire_delayed_steering_lifecycle(&super::live_command(&router), &thread, &turn)
        .unwrap();
    let callback_router = Arc::clone(&router);
    let calls = Cell::new(0);

    permit
        .finish_with(|| {
            calls.set(calls.get() + 1);
            assert!(matches!(
                callback_router.state.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
        })
        .unwrap();

    assert_eq!(calls.get(), 1);
    let after = router.snapshot().unwrap();
    assert_eq!(
        after.routed_operation_count(),
        before.routed_operation_count()
    );
    assert_eq!(
        after.queued_operation_count(),
        before.queued_operation_count()
    );
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Quiet
    ));
    assert_eq!(registration.terminal_reason(), None);
}

#[test]
fn permit_accessors_validate_the_exact_durable_target_and_home_generation() {
    let (router, registration, thread, turn) =
        exact_target(67, "cas-steering-proof", "turn-steering-proof");
    let permit = router
        .acquire_delayed_steering_lifecycle(&super::live_command(&router), &thread, &turn)
        .unwrap();
    let target = steering_target(&thread, &turn, 67);

    assert_eq!(permit.syndic_thread_id(), registration.owner());
    assert_eq!(permit.cas_thread_id(), &thread);
    assert_eq!(permit.cas_turn_id(), &turn);
    assert_eq!(permit.loaded_generation(), registration.loaded_generation());
    assert!(permit.matches_target(
        registration.owner(),
        &target,
        registration.loaded_generation(),
    ));
    assert!(!permit.matches_target(
        SyndicThreadId::from_bytes([68; 16]),
        &target,
        registration.loaded_generation(),
    ));
    let wrong_loaded_generation = CasLoadedSessionGeneration::new(
        super::process(),
        CasLoadedThreadGeneration::new(68).unwrap(),
    );
    assert!(!permit.matches_target(registration.owner(), &target, wrong_loaded_generation,));
    let wrong_target = steering_target(&CasThreadId::new("cas-steering-other").unwrap(), &turn, 67);
    assert!(!permit.matches_target(
        registration.owner(),
        &wrong_target,
        registration.loaded_generation(),
    ));

    let directory = tempfile::tempdir().unwrap();
    let home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    assert_eq!(permit.home_generation(&home), home.health().generation());
    permit.finish_with(|| {}).unwrap();
}

#[test]
fn delayed_lifecycle_has_no_activation_source_terminal_or_provider_side_effects() {
    let (router, registration, thread, turn) =
        exact_target(69, "cas-steering-isolated", "turn-steering-isolated");
    let before = router.snapshot().unwrap();
    let permit = router
        .acquire_delayed_steering_lifecycle(&super::live_command(&router), &thread, &turn)
        .unwrap();
    {
        let state = router.state.lock().unwrap();
        let target = state.targets.get(&thread).unwrap();
        assert_eq!(
            target.publication_in_flight,
            Some(super::super::TargetPublication::DelayedSteering)
        );
        assert!(target.start_dispatched);
        assert!(target.activation_durable);
        assert_eq!(target.turn_state, TargetTurn::Exact);
    }
    permit.finish_with(|| {}).unwrap();

    let after = router.snapshot().unwrap();
    assert_eq!(
        after.routed_operation_count(),
        before.routed_operation_count()
    );
    assert_eq!(
        after.queued_operation_count(),
        before.queued_operation_count()
    );
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Quiet
    ));
    assert_eq!(registration.proven_terminal(), None);

    let source = router.acquire_source_publication(&thread, &turn).unwrap();
    assert!(source.active_turn_request().is_none());
    source.finish().unwrap();
    assert_eq!(
        router.snapshot().unwrap().routed_operation_count(),
        before.routed_operation_count() + 1
    );
}

#[test]
fn dropping_an_unfinished_permit_fails_closed() {
    let (router, registration, thread, turn) =
        exact_target(70, "cas-steering-drop", "turn-steering-drop");
    let permit = router
        .acquire_delayed_steering_lifecycle(&super::live_command(&router), &thread, &turn)
        .unwrap();

    drop(permit);

    assert_eq!(
        registration.terminal_reason(),
        Some(LiveEventTargetCloseReason::SourcePublicationFailed)
    );
    assert_eq!(router.snapshot().unwrap().target_count(), 0);
    assert_eq!(router.snapshot().unwrap().retired_thread_lane_count(), 1);
}
