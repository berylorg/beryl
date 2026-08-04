use std::{
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

use beryl_model::{
    BindingRevision, CasThreadId, CasTurnId, SyndicExecutionSnapshotId, SyndicThreadId,
    SyndicTurnId,
};
use syndic_storage::{
    PendingSteeringTargetProof, SteeringTargetProof, SyndicTimestamp, TurnEndStatus,
};

use super::{live_command, register, router};
use crate::cas_projection::connection::router::{
    ActiveSteeringAttemptAcquireError, ProvenTerminalOutcome, StopElectionAcquireError,
};

fn exact_target(
    generation: u64,
    thread_id: &'static str,
    turn_id: &'static str,
) -> (
    Arc<super::EventRouter>,
    super::TargetRegistration,
    SteeringTargetProof,
    CasThreadId,
    CasTurnId,
) {
    let router = router(generation);
    let owner = generation as u8;
    let registration = register(&router, thread_id, owner, generation, None);
    let cas_thread_id = CasThreadId::new(thread_id).unwrap();
    let cas_turn_id = CasTurnId::new(turn_id).unwrap();
    router.authorize_turn_start(&registration.proof()).unwrap();
    router
        .acquire_source_publication(&cas_thread_id, &cas_turn_id)
        .unwrap()
        .finish()
        .unwrap();
    let steering_target = SteeringTargetProof::new(
        PendingSteeringTargetProof::new(
            BindingRevision::new(1).unwrap(),
            SyndicExecutionSnapshotId::from_bytes([owner.wrapping_add(2); 16]),
            SyndicTurnId::from_bytes([owner.wrapping_add(1); 16]),
            cas_thread_id.clone(),
        ),
        cas_turn_id.clone(),
    );
    (
        router,
        registration,
        steering_target,
        cas_thread_id,
        cas_turn_id,
    )
}

#[test]
fn stop_waits_for_an_earlier_active_steering_attempt_to_settle() {
    let (router, registration, steering_target, thread_id, turn_id) =
        exact_target(101, "cas-stop-after-steering", "turn-stop-after-steering");
    let steering = router
        .acquire_active_steering_attempt(
            &registration.proof(),
            &steering_target,
            registration.loaded_generation(),
        )
        .unwrap();
    let stop_target = router
        .stop_target(registration.owner(), &thread_id, &turn_id)
        .unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(0);
    let (result_tx, result_rx) = mpsc::sync_channel(0);
    let waiter_router = Arc::clone(&router);
    let waiter = thread::spawn(move || {
        started_tx.send(()).unwrap();
        let command = live_command(&waiter_router);
        result_tx
            .send(waiter_router.acquire_stop_election(command, &stop_target))
            .unwrap();
    });

    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(
        result_rx.recv_timeout(Duration::from_millis(25)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    steering.finish().unwrap();
    result_rx
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap()
        .finish();
    waiter.join().unwrap();
}

#[test]
fn stop_winner_rejects_a_later_steering_acquisition() {
    let (router, registration, steering_target, thread_id, turn_id) =
        exact_target(102, "cas-steering-after-stop", "turn-steering-after-stop");
    let stop_target = router
        .stop_target(registration.owner(), &thread_id, &turn_id)
        .unwrap();
    let stop = router
        .acquire_stop_election(live_command(&router), &stop_target)
        .unwrap();

    assert!(matches!(
        router.acquire_active_steering_attempt(
            &registration.proof(),
            &steering_target,
            registration.loaded_generation(),
        ),
        Err(ActiveSteeringAttemptAcquireError::Busy)
    ));

    stop.finish();
    router
        .acquire_active_steering_attempt(
            &registration.proof(),
            &steering_target,
            registration.loaded_generation(),
        )
        .unwrap()
        .finish()
        .unwrap();
}

#[test]
fn exact_terminal_publication_waits_for_stop_and_then_proceeds() {
    let (router, registration, _steering_target, thread_id, turn_id) =
        exact_target(103, "cas-terminal-after-stop", "turn-terminal-after-stop");
    let stop_target = router
        .stop_target(registration.owner(), &thread_id, &turn_id)
        .unwrap();
    let stop = router
        .acquire_stop_election(live_command(&router), &stop_target)
        .unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(0);
    let (result_tx, result_rx) = mpsc::sync_channel(0);
    let terminal_router = Arc::clone(&router);
    let terminal_thread_id = thread_id.clone();
    let terminal_turn_id = turn_id.clone();
    let waiter = thread::spawn(move || {
        started_tx.send(()).unwrap();
        let published = terminal_router
            .acquire_terminal_source_publication(&terminal_thread_id, &terminal_turn_id)
            .is_ok_and(|permit| {
                permit
                    .finish_terminal(ProvenTerminalOutcome::new(
                        TurnEndStatus::complete(),
                        SyndicTimestamp::from_unix_millis(103),
                    ))
                    .is_ok()
            });
        result_tx.send(published).unwrap();
    });

    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(
        result_rx.recv_timeout(Duration::from_millis(25)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));

    stop.finish();
    assert!(result_rx.recv_timeout(Duration::from_secs(1)).unwrap());
    waiter.join().unwrap();
    assert!(registration.proven_terminal().is_some());
}

#[test]
fn stale_and_mismatched_stop_targets_are_rejected() {
    let (router, registration, _steering_target, thread_id, turn_id) =
        exact_target(104, "cas-stop-exactness", "turn-stop-exactness");

    assert!(matches!(
        router.stop_target(SyndicThreadId::from_bytes([105; 16]), &thread_id, &turn_id,),
        Err(StopElectionAcquireError::TargetMismatch)
    ));
    assert!(matches!(
        router.stop_target(
            registration.owner(),
            &thread_id,
            &CasTurnId::new("turn-stop-other").unwrap(),
        ),
        Err(StopElectionAcquireError::TargetMismatch)
    ));

    let stale = router
        .stop_target(registration.owner(), &thread_id, &turn_id)
        .unwrap();
    router.unregister(
        &registration,
        crate::cas_projection::connection::router::LiveEventTargetCloseReason::ThreadClosed,
    );
    assert!(matches!(
        router.acquire_stop_election(live_command(&router), &stale),
        Err(StopElectionAcquireError::TargetMismatch)
    ));
}

#[test]
fn dropping_a_stop_permit_releases_the_election() {
    let (router, registration, _steering_target, thread_id, turn_id) =
        exact_target(105, "cas-stop-drop", "turn-stop-drop");
    let stop_target = router
        .stop_target(registration.owner(), &thread_id, &turn_id)
        .unwrap();

    let first = router
        .acquire_stop_election(live_command(&router), &stop_target)
        .unwrap();
    drop(first);
    router
        .acquire_stop_election(live_command(&router), &stop_target)
        .unwrap()
        .finish();
}
