use std::{
    cell::Cell,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use beryl_model::{
    BindingRevision, CasThreadId, CasTurnId, SyndicExecutionSnapshotId, SyndicTurnId,
};
use syndic_storage::{
    PendingSteeringTargetProof, SteeringTargetProof, SyndicTimestamp, TurnEndStatus,
};

use super::{register, router};
use crate::cas_projection::connection::router::{
    ActiveSteeringAttemptAcquireError, ActiveSteeringAttemptFinishOutcome,
    ActiveSteeringAttemptStatus, LiveEventPoll, LiveEventTargetCloseReason, ProvenTerminalOutcome,
    TargetLossAcquisition,
};

fn active_target(
    generation: u64,
    thread_id: &'static str,
    turn_id: &'static str,
) -> (
    Arc<super::EventRouter>,
    super::TargetRegistration,
    SteeringTargetProof,
) {
    let router = router(generation);
    let owner = generation as u8;
    let registration = register(&router, thread_id, owner, generation, None);
    router.authorize_turn_start(&registration.proof()).unwrap();
    let cas_thread_id = CasThreadId::new(thread_id).unwrap();
    let cas_turn_id = CasTurnId::new(turn_id).unwrap();
    router
        .acquire_source_publication(&cas_thread_id, &cas_turn_id)
        .unwrap()
        .finish()
        .unwrap();
    let target = SteeringTargetProof::new(
        PendingSteeringTargetProof::new(
            BindingRevision::new(1).unwrap(),
            SyndicExecutionSnapshotId::from_bytes([owner.wrapping_add(2); 16]),
            SyndicTurnId::from_bytes([owner.wrapping_add(1); 16]),
            cas_thread_id,
        ),
        cas_turn_id,
    );
    (router, registration, target)
}

fn activate(
    router: &Arc<super::EventRouter>,
    registration: &super::TargetRegistration,
    turn_id: &'static str,
    owner: u8,
) -> SteeringTargetProof {
    router.authorize_turn_start(&registration.proof()).unwrap();
    let cas_thread_id = registration.key().cas_thread_id.clone();
    let cas_turn_id = CasTurnId::new(turn_id).unwrap();
    router
        .acquire_source_publication(&cas_thread_id, &cas_turn_id)
        .unwrap()
        .finish()
        .unwrap();
    SteeringTargetProof::new(
        PendingSteeringTargetProof::new(
            BindingRevision::new(1).unwrap(),
            SyndicExecutionSnapshotId::from_bytes([owner.wrapping_add(2); 16]),
            SyndicTurnId::from_bytes([owner.wrapping_add(1); 16]),
            cas_thread_id,
        ),
        cas_turn_id,
    )
}

#[test]
fn active_steering_attempt_is_connection_wide_and_reusable_after_finish() {
    let router = router(80);
    let first = register(&router, "cas-active-first", 80, 80, None);
    let second = register(&router, "cas-active-second", 81, 81, None);
    let first_target = activate(&router, &first, "turn-active-first", 80);
    let second_target = activate(&router, &second, "turn-active-second", 81);

    let first_attempt = router
        .acquire_active_steering_attempt(&first.proof(), &first_target, first.loaded_generation())
        .unwrap();
    assert!(matches!(
        router.acquire_active_steering_attempt(
            &second.proof(),
            &second_target,
            second.loaded_generation(),
        ),
        Err(ActiveSteeringAttemptAcquireError::Busy)
    ));
    first_attempt.finish().unwrap();
    router
        .acquire_active_steering_attempt(
            &second.proof(),
            &second_target,
            second.loaded_generation(),
        )
        .unwrap()
        .finish()
        .unwrap();
}

#[test]
fn requested_close_stays_hidden_until_the_exact_steering_attempt_finishes() {
    let (router, registration, target) = active_target(81, "cas-active-close", "turn-active-close");
    let attempt = router
        .acquire_active_steering_attempt(
            &registration.proof(),
            &target,
            registration.loaded_generation(),
        )
        .unwrap();

    assert!(!router.unregister(&registration, LiveEventTargetCloseReason::ReceiverAbandoned,));
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Quiet
    ));
    assert_eq!(
        attempt.status(),
        ActiveSteeringAttemptStatus::Closed(LiveEventTargetCloseReason::ReceiverAbandoned)
    );

    assert_eq!(
        attempt.finish().unwrap(),
        ActiveSteeringAttemptFinishOutcome::Settled
    );
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Closed(LiveEventTargetCloseReason::ReceiverAbandoned)
    ));
    assert_eq!(router.snapshot().unwrap().target_count(), 0);
}

#[test]
fn transferred_loss_keeps_global_admission_until_finish_and_leaves_a_receipt() {
    let router = router(82);
    let first = register(&router, "cas-transfer-first", 82, 82, None);
    let second = register(&router, "cas-transfer-second", 83, 83, None);
    let first_target = activate(&router, &first, "turn-transfer-first", 82);
    let second_target = activate(&router, &second, "turn-transfer-second", 83);
    let attempt = router
        .acquire_active_steering_attempt(&first.proof(), &first_target, first.loaded_generation())
        .unwrap();
    let loss = attempt.transfer_to_target_loss().unwrap();

    assert!(matches!(first.poll(Duration::ZERO), LiveEventPoll::Quiet));
    assert!(matches!(
        router.acquire_active_steering_attempt(
            &second.proof(),
            &second_target,
            second.loaded_generation(),
        ),
        Err(ActiveSteeringAttemptAcquireError::Busy)
    ));
    loss.finish().unwrap();
    assert!(matches!(
        first.poll(Duration::ZERO),
        LiveEventPoll::Closed(LiveEventTargetCloseReason::ReceiverAbandoned)
    ));
    assert!(matches!(
        router
            .acquire_target_loss(super::live_command(&router), &first.proof())
            .unwrap(),
        TargetLossAcquisition::Incomplete
    ));
    router
        .acquire_active_steering_attempt(
            &second.proof(),
            &second_target,
            second.loaded_generation(),
        )
        .unwrap()
        .finish()
        .unwrap();
}

#[test]
fn router_retirement_stays_hidden_until_the_steering_attempt_finishes() {
    let (router, registration, target) =
        active_target(83, "cas-retired-steering", "turn-retired-steering");
    let attempt = router
        .acquire_active_steering_attempt(
            &registration.proof(),
            &target,
            registration.loaded_generation(),
        )
        .unwrap();

    router.retire(LiveEventTargetCloseReason::StreamFailure);
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Quiet
    ));
    assert_eq!(
        attempt.status(),
        ActiveSteeringAttemptStatus::Closed(LiveEventTargetCloseReason::StreamFailure)
    );

    assert_eq!(
        attempt.finish().unwrap(),
        ActiveSteeringAttemptFinishOutcome::TargetLossPending
    );
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Closed(LiveEventTargetCloseReason::StreamFailure)
    ));
}

#[test]
fn ordinary_loss_for_another_target_can_finish_during_an_active_attempt() {
    let router = router(84);
    let first = register(&router, "cas-cross-first", 84, 84, None);
    let second = register(&router, "cas-cross-second", 85, 85, None);
    let first_target = activate(&router, &first, "turn-cross-first", 84);
    activate(&router, &second, "turn-cross-second", 85);
    let attempt = router
        .acquire_active_steering_attempt(&first.proof(), &first_target, first.loaded_generation())
        .unwrap();

    let TargetLossAcquisition::Authority(loss) = router
        .acquire_target_loss(super::live_command(&router), &second.proof())
        .unwrap()
    else {
        panic!("the unrelated target must acquire ordinary loss authority");
    };
    loss.finish().unwrap();
    assert_eq!(attempt.status(), ActiveSteeringAttemptStatus::Active);
    attempt.finish().unwrap();
}

#[test]
fn checked_delayed_publication_wins_after_loss_is_requested_under_the_attempt() {
    let (router, registration, target) =
        active_target(86, "cas-active-loss-race", "turn-active-loss-race");
    let attempt = router
        .acquire_active_steering_attempt(
            &registration.proof(),
            &target,
            registration.loaded_generation(),
        )
        .unwrap();
    let delayed = router
        .acquire_delayed_steering_lifecycle(
            &super::live_command(&router),
            target.pending().cas_thread_id(),
            target.cas_turn_id(),
        )
        .unwrap();
    let loss_router = Arc::clone(&router);
    let proof = registration.proof();
    let waiter = thread::spawn(move || {
        let command = super::live_command(&loss_router);
        loss_router.acquire_target_loss(command, &proof)
    });
    wait_until_loss_requested(&router, target.pending().cas_thread_id());

    assert_eq!(attempt.status(), ActiveSteeringAttemptStatus::Active);
    let published = Cell::new(false);
    delayed
        .finish_with(|| published.set(true))
        .expect("the already-acquired checked publication wins");
    assert!(published.get());
    assert!(matches!(
        attempt.status(),
        ActiveSteeringAttemptStatus::Closed(_)
    ));

    attempt.finish().unwrap();
    let TargetLossAcquisition::Authority(loss) = waiter.join().unwrap().unwrap() else {
        panic!("loss proceeds only after the exact attempt disposition");
    };
    loss.fail().unwrap();
}

#[test]
fn exact_steering_finish_preserves_a_concurrent_proven_terminal_for_loss() {
    let (router, registration, target) =
        active_target(87, "cas-exact-terminal-race", "turn-exact-terminal-race");
    let attempt = router
        .acquire_active_steering_attempt(
            &registration.proof(),
            &target,
            registration.loaded_generation(),
        )
        .unwrap();
    let terminal = router
        .acquire_source_publication(target.pending().cas_thread_id(), target.cas_turn_id())
        .unwrap();
    let loss_router = Arc::clone(&router);
    let proof = registration.proof();
    let waiter = thread::spawn(move || {
        let command = super::live_command(&loss_router);
        loss_router.acquire_target_loss(command, &proof)
    });
    wait_until_loss_requested(&router, target.pending().cas_thread_id());

    let outcome = ProvenTerminalOutcome::new(
        TurnEndStatus::complete(),
        SyndicTimestamp::from_unix_millis(87),
    );
    terminal.finish_terminal(outcome).unwrap();
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::ProvenTerminal(observed) if observed == outcome
    ));

    assert_eq!(
        attempt.finish().unwrap(),
        ActiveSteeringAttemptFinishOutcome::ProvenTerminal
    );
    assert!(matches!(
        waiter.join().unwrap().unwrap(),
        TargetLossAcquisition::ProvenTerminal(observed) if observed == outcome
    ));
    assert_eq!(router.snapshot().unwrap().target_count(), 1);
}

#[test]
fn abandoned_steering_cleanup_does_not_overwrite_a_proven_terminal() {
    let (router, registration, target) =
        active_target(88, "cas-drop-terminal-race", "turn-drop-terminal-race");
    let attempt = router
        .acquire_active_steering_attempt(
            &registration.proof(),
            &target,
            registration.loaded_generation(),
        )
        .unwrap();
    let outcome = ProvenTerminalOutcome::new(
        TurnEndStatus::complete(),
        SyndicTimestamp::from_unix_millis(88),
    );
    router
        .acquire_source_publication(target.pending().cas_thread_id(), target.cas_turn_id())
        .unwrap()
        .finish_terminal(outcome)
        .unwrap();

    drop(attempt);
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::ProvenTerminal(observed) if observed == outcome
    ));
    assert_eq!(router.snapshot().unwrap().target_count(), 1);
}

fn wait_until_loss_requested(router: &Arc<super::EventRouter>, thread_id: &CasThreadId) {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if router
            .state
            .lock()
            .unwrap()
            .targets
            .get(thread_id)
            .is_some_and(|target| target.loss_requested)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "target-loss request did not reach the router"
        );
        thread::yield_now();
    }
}
