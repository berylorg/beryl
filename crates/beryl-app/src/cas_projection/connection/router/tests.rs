use std::{sync::Arc, time::Duration};

use beryl_backend::{
    ApprovalInterruption, ApprovalRequestKind, ApprovalResponseDisposition, ExactForegroundTurn,
    StopAttemptCorrelation, StopAttemptDisposition, StopOperationCorrelation,
    lifecycle_test_support::approval_request,
};
use beryl_model::{
    BindingRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration,
    CasThreadId, CasTurnId, InputGateRevision, RuntimeId, SyndicExecutionSnapshotId,
    SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{SyndicTimestamp, TurnEndStatus, TurnStateRevision};

use super::{
    EventRouter, LiveEventPoll, LiveEventTargetHandoffError, LiveEventTargetRegistrationError,
    PreparedApprovalInterruption, RETIRED_THREAD_LANE_LIMIT, TargetHandoffRequirement,
    TargetRegistration, TargetTurnRegistration,
};
use crate::cas_projection::{
    PendingTurnActivation,
    connection::{registry::LoadedThreadKey, router::ApprovalRouteOutcome},
};

mod active_steering;
mod compaction;
mod delayed_steering;
mod persistent_failure;
mod provider_publication;
mod queue_capacity;
mod retirement;
mod stop_election;

fn process() -> CasProcessGeneration {
    CasProcessGeneration::new(7).unwrap()
}

fn router(connection_generation: u64) -> Arc<EventRouter> {
    router_for(connection_generation as u8, connection_generation)
}

fn router_for(runtime: u8, connection_generation: u64) -> Arc<EventRouter> {
    Arc::new(
        EventRouter::new(
            RuntimeId::from_bytes([runtime; 16]),
            process(),
            connection_generation,
        )
        .unwrap(),
    )
}

fn router_with_gate_for(
    runtime: u8,
    connection_generation: u64,
) -> (
    Arc<EventRouter>,
    crate::cas_projection::persistent_failure::MasterCommandGate,
) {
    let gate = crate::cas_projection::persistent_failure::MasterCommandGate::new(
        crate::cas_projection::ProjectionServiceGeneration::allocate()
            .expect("test service generation is available"),
        None,
    );
    let router = Arc::new(
        EventRouter::new_with_scheduler(
            RuntimeId::from_bytes([runtime; 16]),
            process(),
            connection_generation,
            crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(),
            gate.authorizer(),
            None,
        )
        .unwrap(),
    );
    (router, gate)
}

fn live_command(router: &EventRouter) -> crate::cas_projection::LiveCommandPermit {
    router
        .commands
        .authorize()
        .expect("test router admits one live command")
}

fn register(
    router: &EventRouter,
    cas_thread_id: &str,
    owner: u8,
    loaded_thread_generation: u64,
    turn_id: Option<&str>,
) -> TargetRegistration {
    let command = live_command(router);
    router
        .register(
            &command,
            LoadedThreadKey {
                runtime_id: router.runtime_id,
                process_generation: router.process_generation,
                cas_thread_id: CasThreadId::new(cas_thread_id).unwrap(),
            },
            SyndicThreadId::from_bytes([owner; 16]),
            CasLoadedSessionGeneration::new(
                router.process_generation,
                CasLoadedThreadGeneration::new(loaded_thread_generation).unwrap(),
            ),
            1,
            Duration::from_secs(1),
            turn_id.map_or_else(
                || TargetTurnRegistration::Pending(pending_activation(owner)),
                |turn_id| TargetTurnRegistration::Active(CasTurnId::new(turn_id).unwrap()),
            ),
        )
        .unwrap()
}

fn pending_activation(owner: u8) -> PendingTurnActivation {
    PendingTurnActivation::new(
        SyndicThreadId::from_bytes([owner; 16]),
        SyndicTurnId::from_bytes([owner.wrapping_add(1); 16]),
        BindingRevision::new(1).unwrap(),
        InputGateRevision::new(1).unwrap(),
        TurnStateRevision::FIRST,
        SyndicExecutionSnapshotId::from_bytes([owner.wrapping_add(2); 16]),
        SyndicTimestamp::from_unix_millis(u64::from(owner) + 1),
    )
}

#[test]
fn targets_are_exact_within_and_across_connection_routers() {
    let first_router = router_for(101, 1);
    let second_router = router_for(101, 2);
    let first = register(&first_router, "cas-thread-one", 1, 1, None);
    let second = register(&first_router, "cas-thread-two", 2, 2, None);
    let same_remote_id_elsewhere = register(&second_router, "cas-thread-one", 3, 3, None);

    assert_eq!(first_router.snapshot().unwrap().target_count(), 2);
    assert_eq!(second_router.snapshot().unwrap().target_count(), 1);
    assert_ne!(first.registration(), second.registration());
    assert_ne!(
        first.loaded_generation(),
        same_remote_id_elsewhere.loaded_generation()
    );

    let command = live_command(&first_router);
    let duplicate = first_router.register(
        &command,
        LoadedThreadKey {
            runtime_id: first_router.runtime_id,
            process_generation: first_router.process_generation,
            cas_thread_id: CasThreadId::new("cas-thread-one").unwrap(),
        },
        SyndicThreadId::from_bytes([4; 16]),
        CasLoadedSessionGeneration::new(
            first_router.process_generation,
            CasLoadedThreadGeneration::new(4).unwrap(),
        ),
        1,
        Duration::from_secs(1),
        TargetTurnRegistration::Pending(pending_activation(4)),
    );
    assert!(matches!(
        duplicate,
        Err(LiveEventTargetRegistrationError::TargetAlreadyRegistered { thread_id })
            if thread_id.as_str() == "cas-thread-one"
    ));
}

#[test]
fn approval_ownership_survives_queue_and_poll_until_consumer_release() {
    let router = router(5);
    let registration = register(
        &router,
        "cas-approval-retained",
        5,
        5,
        Some("turn-approval-retained"),
    );
    let thread_id = CasThreadId::new("cas-approval-retained").unwrap();
    let turn_id = CasTurnId::new("turn-approval-retained").unwrap();
    let interruption = ApprovalInterruption::DurableStopOwned {
        operation: StopOperationCorrelation::from_bytes([0x31; 16]),
        target: ExactForegroundTurn::new(
            router.runtime_id,
            registration.loaded_generation(),
            thread_id.clone(),
            turn_id.clone(),
        ),
        attempt_disposition: StopAttemptDisposition::ClaimedNotDispatched(
            StopAttemptCorrelation::from_bytes([0x41; 16]),
        ),
    };
    let request = approval_request(
        ApprovalRequestKind::Permissions,
        ApprovalResponseDisposition::ResponseRequired,
        Some(thread_id),
        Some(turn_id),
        None,
    );
    assert!(matches!(
        router.route_approval(
            &live_command(&router),
            request,
            Some(PreparedApprovalInterruption::new(
                interruption.clone(),
                None,
            )),
        ),
        ApprovalRouteOutcome::Routed {
            interruption: routed,
            obligation: Some(_),
        } if routed == interruption
    ));
    assert_eq!(router.snapshot().unwrap().queued_operation_count(), 1);

    let LiveEventPoll::Approval(approval) = registration.poll(Duration::ZERO) else {
        panic!("expected the exact routed approval")
    };
    assert_eq!(router.snapshot().unwrap().queued_operation_count(), 0);

    let (_request, polled_interruption) = approval.into_parts();
    assert_eq!(polled_interruption, interruption);
}

#[test]
fn proven_terminal_handoff_waits_for_earlier_feature_operation() {
    let router = router(11);
    let registration = register(
        &router,
        "cas-terminal-order",
        11,
        11,
        Some("turn-terminal-order"),
    );
    let request = approval_request(
        ApprovalRequestKind::CommandExecution,
        ApprovalResponseDisposition::ResponseRequired,
        Some(CasThreadId::new("cas-terminal-order").unwrap()),
        Some(CasTurnId::new("turn-terminal-order").unwrap()),
        None,
    );
    assert!(matches!(
        router.route_approval(&live_command(&router), request, None),
        ApprovalRouteOutcome::Routed { .. }
    ));
    router
        .acquire_source_publication(
            &CasThreadId::new("cas-terminal-order").unwrap(),
            &CasTurnId::new("turn-terminal-order").unwrap(),
        )
        .unwrap()
        .finish_terminal(super::ProvenTerminalOutcome::new(
            TurnEndStatus::complete(),
            SyndicTimestamp::from_unix_millis(50),
        ))
        .unwrap();

    assert!(matches!(
        router.handoff_target(&registration, TargetHandoffRequirement::ProvenTerminal),
        Err(LiveEventTargetHandoffError::QueuedOperations { count: 1 })
    ));
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Approval(_)
    ));
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::ProvenTerminal(_)
    ));
    router
        .handoff_target(&registration, TargetHandoffRequirement::ProvenTerminal)
        .unwrap();
}

#[test]
fn not_started_handoff_requires_dispatch_authorization() {
    let router = router(12);
    let registration = register(&router, "cas-premature", 12, 12, None);

    assert!(matches!(
        router.handoff_target(&registration, TargetHandoffRequirement::NotStarted),
        Err(LiveEventTargetHandoffError::TargetMayHaveStarted)
    ));
    router.authorize_turn_start(&registration.proof()).unwrap();
    router
        .handoff_target(&registration, TargetHandoffRequirement::NotStarted)
        .unwrap();
}
