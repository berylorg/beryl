use super::*;
use crate::{
    cas_projection::{
        ProjectionServiceGeneration,
        accepted_input_scheduler::AcceptedInputSchedulerSignal,
        connection::router::{
            ActiveSteeringAttemptFinishOutcome, ProvenTerminalOutcome, TargetAuthorizationFailure,
        },
        persistent_failure::MasterCommandGate,
    },
    process_admission::{ProcessAdmissionError, ProcessAdmissionGate},
};

fn fixture() -> (Arc<EventRouter>, MasterCommandGate, ProcessAdmissionGate) {
    let process_gate = ProcessAdmissionGate::new();
    let service = MasterCommandGate::new(
        process_gate.clone(),
        ProjectionServiceGeneration::allocate().unwrap(),
        None,
    );
    let router = Arc::new(
        EventRouter::new_with_scheduler(
            RuntimeId::from_bytes([91; 16]),
            process(),
            91,
            AcceptedInputSchedulerSignal::new(),
            service.authorizer(),
            None,
        )
        .unwrap(),
    );
    (router, service, process_gate)
}

#[test]
fn runtime_work_router_contention_refuses_under_process_admission() {
    use crate::cas_projection::runtime_work::RuntimeWorkError;
    let (router, _service, process_gate) = fixture();
    let registration = register(&router, "observation-contention", 91, 91, None);
    let command = live_command(&router);
    let before = router.work_stamp().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let held = router.state.lock().unwrap();
        let reader = scope.spawn(|| {
            send.send(process_gate.admit(|| router.try_work_stamp()))
                .unwrap();
        });
        let result = receive.recv_timeout(Duration::from_secs(2));
        drop(held);
        reader.join().unwrap();
        assert_eq!(result.unwrap().unwrap(), Err(RuntimeWorkError::Busy));
    });
    assert_eq!(router.try_work_stamp().unwrap(), before);
    router
        .authorize_turn_start(&command, &registration.proof())
        .unwrap();
}

#[test]
fn runtime_work_router_unavailable_revision_and_poison_preserve_requests() {
    use crate::cas_projection::runtime_work::RuntimeWorkError;
    let (router, _service, _process) = fixture();
    router.state.lock().unwrap().work_revision = None;
    assert_eq!(router.try_work_stamp(), Err(RuntimeWorkError::Unavailable));
    let before = router.state.lock().unwrap().work_requests.len();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = router.state.lock().unwrap();
            panic!("inject router observation poison");
        }))
        .is_err()
    );
    assert_eq!(router.try_work_stamp(), Err(RuntimeWorkError::Unavailable));
    let state = router.state.lock().err().unwrap().into_inner();
    assert_eq!(state.work_requests.len(), before);
    assert_eq!(state.work_revision, None);
}

#[test]
fn pending_start_keeps_queued_epoch_and_requires_exact_undispatched_handoff() {
    let (router, _service, process_gate) = fixture();
    let registration = register(&router, "queued-start", 91, 91, None);
    let queued = live_command(&router);
    let fence = process_gate.fence().unwrap();
    assert_eq!(
        router.authorize_turn_start(&queued, &registration.proof()),
        Err(TargetAuthorizationFailure::ExecutionFenced(
            ProcessAdmissionError::Fenced
        )),
    );
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Quiet
    ));
    assert!(matches!(
        router.handoff_target(&registration, TargetHandoffRequirement::NotStarted),
        Err(LiveEventTargetHandoffError::TargetMayHaveStarted),
    ));
    queued.reopen_process_admission_for_test(&fence).unwrap();
    assert_eq!(
        router.authorize_turn_start(&queued, &registration.proof()),
        Err(TargetAuthorizationFailure::ExecutionFenced(
            ProcessAdmissionError::Stale
        )),
    );
    router
        .handoff_target(&registration, TargetHandoffRequirement::StartNotAuthorized)
        .unwrap();
}

#[test]
fn dispatched_start_cannot_gain_nondispatch_proof_from_a_later_fence() {
    let (router, _service, process_gate) = fixture();
    let registration = register(&router, "already-started", 92, 92, None);
    let queued = live_command(&router);
    router
        .authorize_turn_start(&queued, &registration.proof())
        .unwrap();
    process_gate.fence().unwrap();
    assert!(matches!(
        router.handoff_target(&registration, TargetHandoffRequirement::StartNotAuthorized),
        Err(LiveEventTargetHandoffError::TargetMayHaveStarted),
    ));
    assert!(matches!(
        router.authorize_turn_start(&queued, &registration.proof()),
        Err(TargetAuthorizationFailure::Target(_)),
    ));
}

#[test]
fn queued_compaction_stays_undispatched_across_fence_and_reopen() {
    let (router, _service, process_gate) = fixture();
    let (registration, _, _) = compaction::register_compaction(&router, "queued", 93);
    let queued = live_command(&router);
    let fence = process_gate.fence().unwrap();
    assert_eq!(
        router.authorize_context_compaction_command(&queued, &registration.proof()),
        Err(TargetAuthorizationFailure::ExecutionFenced(
            ProcessAdmissionError::Fenced
        )),
    );
    queued.reopen_process_admission_for_test(&fence).unwrap();
    assert_eq!(
        router.authorize_context_compaction_command(&queued, &registration.proof()),
        Err(TargetAuthorizationFailure::ExecutionFenced(
            ProcessAdmissionError::Stale
        )),
    );
    assert!(matches!(
        router.handoff_target(
            &registration,
            TargetHandoffRequirement::CompactionNotDispatched
        ),
        Err(LiveEventTargetHandoffError::TargetMayHaveStarted),
    ));
    router
        .handoff_target(
            &registration,
            TargetHandoffRequirement::CompactionNotAuthorized,
        )
        .unwrap();
}

#[test]
fn queued_steering_is_retryable_and_keeps_the_active_target_after_a_fence() {
    let (router, _service, process_gate) = fixture();
    let registration = register(&router, "queued-steering", 94, 94, None);
    let target = active_steering::activate(&router, &registration, "active-turn", 94);
    let attempt = router
        .acquire_active_steering_attempt(
            &registration.proof(),
            &target,
            registration.loaded_generation(),
        )
        .unwrap();
    let queued = live_command(&router);
    let authorization = attempt.command_authorization();
    let fence = process_gate.fence().unwrap();
    assert_eq!(
        router.authorize_active_steering_command(&queued, &authorization),
        Err(TargetAuthorizationFailure::ExecutionFenced(
            ProcessAdmissionError::Fenced
        )),
    );
    queued.reopen_process_admission_for_test(&fence).unwrap();
    assert_eq!(
        router.authorize_active_steering_command(&queued, &authorization),
        Err(TargetAuthorizationFailure::ExecutionFenced(
            ProcessAdmissionError::Stale
        )),
    );
    assert_eq!(
        attempt.finish().unwrap(),
        ActiveSteeringAttemptFinishOutcome::Settled
    );
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Quiet
    ));
}

#[test]
fn process_fence_keeps_exact_stop_and_terminal_publication_available() {
    let (router, _service, process_gate) = fixture();
    let registration = register(&router, "fenced-active", 96, 96, None);
    let target = active_steering::activate(&router, &registration, "active-turn", 96);
    let command = live_command(&router);
    process_gate.fence().unwrap();
    let stop_target = router
        .stop_target(
            registration.owner(),
            target.pending().cas_thread_id(),
            target.cas_turn_id(),
        )
        .unwrap();
    router
        .acquire_stop_election(command, &stop_target)
        .unwrap()
        .finish();
    router
        .acquire_source_publication(target.pending().cas_thread_id(), target.cas_turn_id())
        .unwrap()
        .finish_terminal(ProvenTerminalOutcome::new(
            syndic_storage::TurnEndStatus::complete(),
            syndic_storage::SyndicTimestamp::from_unix_millis(1),
        ))
        .unwrap();
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::ProvenTerminal(_)
    ));
    router
        .handoff_target(&registration, TargetHandoffRequirement::ProvenTerminal)
        .unwrap();
}

#[test]
fn failed_service_takes_precedence_over_process_nondispatch() {
    let (router, service, process_gate) = fixture();
    let registration = register(&router, "failed-service", 95, 95, None);
    let queued = live_command(&router);
    process_gate.fence().unwrap();
    service.close_for_local_failure();
    assert_eq!(
        router.authorize_turn_start(&queued, &registration.proof()),
        Err(TargetAuthorizationFailure::Router),
    );
}
