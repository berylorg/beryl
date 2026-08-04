use std::sync::Arc;
use std::time::{Duration, Instant};

use beryl_model::{CasThreadId, CasTurnId};
use syndic_storage::{SyndicTimestamp, TurnEndStatus};

use super::{register, router};
use crate::cas_projection::connection::router::{
    LiveEventPoll, LiveEventTargetCloseReason, ProvenTerminalOutcome, SourcePublicationFinishError,
    SourcePublicationPermitError, TargetLossAcquisition,
};

fn exact_target(
    generation: u64,
    thread: &'static str,
    turn: &'static str,
) -> (
    Arc<crate::cas_projection::connection::router::EventRouter>,
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

fn finish_terminal(router: &Arc<super::EventRouter>, thread: &CasThreadId, turn: &CasTurnId) {
    router
        .acquire_source_publication(thread, turn)
        .unwrap()
        .finish_terminal(ProvenTerminalOutcome::new(
            TurnEndStatus::complete(),
            SyndicTimestamp::from_unix_millis(7),
        ))
        .unwrap();
}

#[test]
fn first_ordered_source_control_binds_and_carries_pending_activation() {
    let router = router(28);
    let registration = register(&router, "cas-source-first", 28, 28, None);
    router.authorize_turn_start(&registration.proof()).unwrap();
    let thread = CasThreadId::new("cas-source-first").unwrap();
    let turn = CasTurnId::new("turn-source-first").unwrap();

    let first = router.acquire_source_publication(&thread, &turn).unwrap();
    assert!(first.active_turn_request().is_some());
    assert_eq!(registration.bound_turn(), Some(turn.clone()));
    first.finish().unwrap();

    let second = router.acquire_source_publication(&thread, &turn).unwrap();
    assert!(second.active_turn_request().is_none());
    second.finish().unwrap();
}

#[test]
fn response_proof_waits_for_broker_source_activation() {
    let router = router(29);
    let registration = register(&router, "cas-response-first", 29, 29, None);
    router.authorize_turn_start(&registration.proof()).unwrap();
    let thread = CasThreadId::new("cas-response-first").unwrap();
    let turn = CasTurnId::new("turn-response-first").unwrap();

    let source = router.acquire_source_publication(&thread, &turn).unwrap();
    let proof_router = Arc::clone(&router);
    let registration_proof = registration.proof();
    let proof_turn = turn.clone();
    let waiter = std::thread::spawn(move || {
        proof_router.prove_response_activation(&registration_proof, &proof_turn)
    });

    source.finish().unwrap();
    assert_eq!(waiter.join().unwrap().unwrap().home_generation(), 1);
}

#[test]
fn terminal_permit_wins_a_concurrent_target_loss_request() {
    let router = router(30);
    let registration = register(&router, "cas-loss-terminal", 30, 30, None);
    router.authorize_turn_start(&registration.proof()).unwrap();
    let thread = CasThreadId::new("cas-loss-terminal").unwrap();
    let turn = CasTurnId::new("turn-loss-terminal").unwrap();
    let permit = router.acquire_source_publication(&thread, &turn).unwrap();
    let loss_router = Arc::clone(&router);
    let proof = registration.proof();
    let loss = std::thread::spawn(move || {
        let command = super::live_command(&loss_router);
        loss_router.acquire_target_loss(command, &proof)
    });

    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if router
            .state
            .lock()
            .unwrap()
            .targets
            .get(&thread)
            .is_some_and(|target| target.loss_requested)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "target-loss request did not reach the exact publication fence"
        );
        std::thread::yield_now();
    }
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Quiet
    ));

    permit
        .finish_terminal(ProvenTerminalOutcome::new(
            TurnEndStatus::complete(),
            SyndicTimestamp::from_unix_millis(8),
        ))
        .unwrap();

    assert!(matches!(
        loss.join().unwrap().unwrap(),
        TargetLossAcquisition::ProvenTerminal(outcome)
            if outcome.status() == TurnEndStatus::complete()
                && outcome.observed_at() == SyndicTimestamp::from_unix_millis(8)
    ));
    assert_eq!(router.snapshot().unwrap().target_count(), 1);
}

#[test]
fn router_retirement_stays_hidden_until_the_source_permit_finishes() {
    let (router, registration, thread, turn) =
        exact_target(31, "cas-retired-source", "turn-retired-source");
    let permit = router.acquire_source_publication(&thread, &turn).unwrap();

    router.retire(LiveEventTargetCloseReason::StreamFailure);
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Quiet
    ));
    assert!(matches!(
        permit.finish(),
        Err(SourcePublicationFinishError::Target(ref invalidation))
            if invalidation.reason == LiveEventTargetCloseReason::StreamFailure
    ));
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Closed(LiveEventTargetCloseReason::StreamFailure)
    ));
}

#[test]
fn terminal_first_rejects_later_source_publication_with_the_terminal_reason() {
    let (router, registration, thread, turn) =
        exact_target(21, "cas-close-first", "turn-close-first");
    finish_terminal(&router, &thread, &turn);

    let result = router.acquire_source_publication(&thread, &turn);
    assert!(matches!(
        result,
        Err(SourcePublicationPermitError::Target(ref invalidation))
            if invalidation.reason == LiveEventTargetCloseReason::EventAfterTurnCompletion
    ));
    assert_eq!(
        registration.terminal_reason(),
        Some(LiveEventTargetCloseReason::EventAfterTurnCompletion)
    );
    assert_eq!(router.snapshot().unwrap().target_count(), 0);
}

#[test]
fn permit_first_defers_close_until_the_publication_owner_finishes() {
    let (router, _registration, thread, turn) =
        exact_target(22, "cas-permit-first", "turn-permit-first");
    let permit = router.acquire_source_publication(&thread, &turn).unwrap();

    assert!(!router.record_thread_closed(&thread).unwrap());
    assert_eq!(router.snapshot().unwrap().target_count(), 1);
    assert!(!router.permits_reacquisition_thread(&thread).unwrap());
    assert!(matches!(
        permit.finish(),
        Err(SourcePublicationFinishError::Target(ref invalidation))
            if invalidation.reason == LiveEventTargetCloseReason::ThreadClosed
    ));
    assert_eq!(router.snapshot().unwrap().target_count(), 0);
    assert_eq!(router.snapshot().unwrap().retired_thread_lane_count(), 1);
}

#[test]
fn requested_close_becomes_observable_only_after_the_exact_permit_resolves() {
    let (router, registration, thread, turn) =
        exact_target(27, "cas-close-fence", "turn-close-fence");
    let permit = router.acquire_source_publication(&thread, &turn).unwrap();

    assert!(!router.unregister(&registration, LiveEventTargetCloseReason::ReceiverAbandoned));
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Quiet
    ));
    assert!(matches!(
        permit.finish(),
        Err(SourcePublicationFinishError::Target(ref invalidation))
            if invalidation.reason == LiveEventTargetCloseReason::ReceiverAbandoned
    ));
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Closed(LiveEventTargetCloseReason::ReceiverAbandoned)
    ));
}

#[test]
fn competing_publication_marks_the_target_for_fail_close() {
    let (router, registration, thread, turn) = exact_target(23, "cas-competing", "turn-competing");
    let first = router.acquire_source_publication(&thread, &turn).unwrap();

    assert!(matches!(
        router.acquire_source_publication(&thread, &turn),
        Err(SourcePublicationPermitError::Target(ref invalidation))
            if invalidation.reason
                == LiveEventTargetCloseReason::SourcePublicationRouteUnavailable
    ));
    assert!(matches!(
        first.finish(),
        Err(SourcePublicationFinishError::Target(ref invalidation))
            if invalidation.reason
                == LiveEventTargetCloseReason::SourcePublicationRouteUnavailable
    ));
    assert_eq!(
        registration.terminal_reason(),
        Some(LiveEventTargetCloseReason::SourcePublicationRouteUnavailable)
    );
    assert_eq!(router.snapshot().unwrap().target_count(), 0);
}

#[test]
fn failed_source_ownership_closes_and_fences_the_exact_target() {
    let (router, registration, thread, turn) =
        exact_target(24, "cas-consumer-failure", "turn-consumer-failure");
    let permit = router.acquire_source_publication(&thread, &turn).unwrap();

    let invalidation = permit.fail().unwrap();
    assert_eq!(
        invalidation.reason,
        LiveEventTargetCloseReason::SourcePublicationFailed
    );
    assert_eq!(
        registration.terminal_reason(),
        Some(LiveEventTargetCloseReason::SourcePublicationFailed)
    );
    assert_eq!(router.snapshot().unwrap().target_count(), 0);
    assert_eq!(router.snapshot().unwrap().retired_thread_lane_count(), 1);
}

#[test]
fn authority_lost_source_ownership_releases_without_failing_the_target() {
    let (router, registration, thread, turn) =
        exact_target(33, "cas-authority-lost", "turn-authority-lost");
    let permit = router.acquire_source_publication(&thread, &turn).unwrap();
    assert_eq!(router.commands.active_command_count_for_test(), 1);

    permit.settle_authority_lost();

    assert_eq!(router.commands.active_command_count_for_test(), 0);
    assert_eq!(registration.terminal_reason(), None);
    assert_eq!(router.snapshot().unwrap().target_count(), 1);
    assert!(
        router
            .state
            .lock()
            .unwrap()
            .targets
            .get(&thread)
            .is_some_and(|target| target.publication_in_flight.is_none())
    );
    router
        .acquire_source_publication(&thread, &turn)
        .unwrap()
        .finish()
        .unwrap();
}

#[test]
fn post_commit_hold_keeps_stop_election_behind_activity_publication() {
    let (router, registration, thread, turn) =
        exact_target(32, "cas-activity-fence", "turn-activity-fence");
    let proof = router
        .stop_target(registration.owner(), &thread, &turn)
        .unwrap();
    let post_commit = router
        .acquire_source_publication(&thread, &turn)
        .unwrap()
        .finish_held()
        .unwrap();
    assert!(
        router
            .state
            .lock()
            .unwrap()
            .targets
            .get(&thread)
            .is_some_and(|target| target.publication_in_flight.is_some())
    );

    let waiter_router = Arc::clone(&router);
    let (started_sender, started_receiver) = std::sync::mpsc::sync_channel(1);
    let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);
    let waiter = std::thread::spawn(move || {
        started_sender.send(()).unwrap();
        let command = super::live_command(&waiter_router);
        result_sender
            .send(waiter_router.acquire_stop_election(command, &proof))
            .unwrap();
    });
    started_receiver.recv().unwrap();
    assert!(matches!(
        result_receiver.recv_timeout(Duration::from_millis(25)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));

    post_commit.release();
    let stop = result_receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    stop.finish();
    waiter.join().unwrap();
    assert!(
        router
            .state
            .lock()
            .unwrap()
            .targets
            .get(&thread)
            .is_some_and(|target| target.publication_in_flight.is_none())
    );
}
