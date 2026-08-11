use std::{sync::Arc, time::Duration};

use beryl_backend::{
    ApprovalRequestKind, ApprovalResponseDisposition,
    lifecycle_test_support::{approval_request, building_dynamic_tool_call},
};
use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasThreadId, CasTurnId,
    DynamicToolCallId, RuntimeId, SyndicThreadId,
};
use syndic_storage::{CompactionOperationId, CompactionOperationNonce};

use super::{live_command, router_for, router_with_gate_for};
use crate::cas_projection::{
    ProjectionServiceGeneration,
    connection::{
        registry::LoadedThreadKey,
        router::{
            EventRouter, LIVE_EVENT_TARGET_CAPACITY, LiveEventConnectionState, LiveEventPoll,
            LiveEventTargetCloseReason, LiveEventTargetRegistrationError,
            PersistentFailureTargetIneligibility, StopElectionAdmission, TargetRegistration,
            TargetTurnRegistration,
        },
    },
    context_compaction::ContextCompactionTargetAuthority,
    persistent_failure::{PersistentFailureCutIdentity, PersistentFailureGeneration},
};

use super::super::{ActiveSteeringAttemptKey, TargetPublication, TargetTurn};

#[derive(Clone, Copy)]
enum EligibilityCase {
    Eligible,
    ActiveOperation,
    ActiveOnlyRegistration,
    ContextCompaction,
    AwaitingActivation,
    Terminal,
    Closing,
    Lost,
    PublicationInFlight,
    IdentityMismatch,
    PriorPrimaryAmbiguous,
    GenerationMismatch,
}

#[test]
fn approval_route_cannot_enqueue_after_the_exact_failure_cut() {
    let (router, gate) = router_with_gate_for(0xec, 8_898);
    let registration = register_target(&router, 0, 1).expect("register approval race target");
    let request = approval_request(
        ApprovalRequestKind::CommandExecution,
        ApprovalResponseDisposition::ResponseRequired,
        Some(registration.key().cas_thread_id.clone()),
        Some(CasTurnId::new("cas-turn-000").unwrap()),
        None,
    );
    let command = gate.authorizer().authorize().expect("admit approval route");
    let router_state = router.state.lock().unwrap();
    let blocked_router = Arc::clone(&router);
    let route = std::thread::spawn(move || blocked_router.route_approval(&command, request, None));

    assert!(
        gate.elect_persistent_failure_for_test(PersistentFailureGeneration::FIRST)
            .expect("elect exact test failure")
    );
    drop(router_state);
    assert!(matches!(
        route.join().unwrap(),
        super::super::ApprovalRouteOutcome::Rejected { .. }
    ));
    let snapshot = router.snapshot().unwrap();
    assert_eq!(snapshot.queued_operation_count(), 0);
    assert_eq!(snapshot.routed_operation_count(), 0);
    assert_eq!(snapshot.target_count(), 1);
}

#[test]
fn dynamic_tool_builder_liveness_cannot_authorize_after_the_exact_failure_cut() {
    let (router, gate) = router_with_gate_for(0xeb, 8_897);
    let registration = register_target(&router, 0, 1).expect("register dynamic-tool race target");
    let call = building_dynamic_tool_call(
        registration.key().cas_thread_id.clone(),
        CasTurnId::new("cas-turn-000").unwrap(),
        DynamicToolCallId::new("dynamic-call-000").unwrap(),
    );
    let reserve_command = gate
        .authorizer()
        .authorize()
        .expect("admit dynamic-tool reservation");
    let permit = match router.reserve_dynamic_tool(&reserve_command, &call) {
        Ok(permit) => permit,
        Err(_) => panic!("reserve exact dynamic-tool target"),
    };
    drop(reserve_command);

    let stale_command = gate
        .authorizer()
        .authorize()
        .expect("admit dynamic-tool builder operation before the cut");
    let router_state = router.state.lock().unwrap();
    let blocked_router = Arc::clone(&router);
    let liveness =
        std::thread::spawn(move || blocked_router.dynamic_tool_is_live(&stale_command, &permit));

    assert!(
        gate.elect_persistent_failure_for_test(PersistentFailureGeneration::FIRST)
            .expect("elect exact test failure")
    );
    drop(router_state);
    assert!(!liveness.join().unwrap());
}

#[test]
fn stop_wait_releases_its_command_before_failure_drain_and_freeze_wake() {
    let home_directory = tempfile::tempdir().expect("router wait failure home");
    let home = HomeStore::open(HomeOpenOptions::new(
        home_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .expect("open router wait failure home");
    let home_generation = home.health().generation().unwrap();
    let (router, gate) = router_with_gate_for(0xed, 8_899);
    let registration = {
        let command = live_command(&router);
        router
            .register(
                &command,
                LoadedThreadKey {
                    runtime_id: router.runtime_id,
                    process_generation: router.process_generation,
                    cas_thread_id: target_id(0),
                },
                SyndicThreadId::from_bytes([1; 16]),
                CasLoadedSessionGeneration::new(
                    router.process_generation,
                    CasLoadedThreadGeneration::new(1).unwrap(),
                ),
                home_generation.get(),
                Duration::from_secs(1),
                TargetTurnRegistration::Pending(super::pending_activation(1)),
            )
            .expect("register stop wait failure target")
    };
    let thread_id = registration.key().cas_thread_id.clone();
    let turn_id = CasTurnId::new("cas-turn-000").unwrap();
    router.authorize_turn_start(&registration.proof()).unwrap();
    router
        .acquire_source_publication(&thread_id, &turn_id)
        .unwrap()
        .finish()
        .unwrap();
    let proof = router
        .stop_target(registration.owner(), &thread_id, &turn_id)
        .expect("build exact stop proof");
    router
        .state
        .lock()
        .unwrap()
        .targets
        .get_mut(&thread_id)
        .unwrap()
        .publication_in_flight = Some(TargetPublication::Source);
    let (waiting_tx, waiting_rx) = std::sync::mpsc::sync_channel(0);
    router.observe_next_stop_election_wait_for_test(waiting_tx);
    let waiter_router = Arc::clone(&router);
    let waiter = std::thread::spawn(move || {
        let command = live_command(&waiter_router);
        waiter_router.acquire_stop_election(command, &proof)
    });
    waiting_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("stop election reached its permit-free wait");

    assert!(
        gate.elect_persistent_failure_for_test(PersistentFailureGeneration::FIRST)
            .expect("elect exact test failure")
    );
    let (drained_tx, drained_rx) = std::sync::mpsc::sync_channel(0);
    let drain_gate = gate.clone();
    let drainer = std::thread::spawn(move || {
        let result = drain_gate.wait_until_drained();
        drained_tx.send(result).unwrap();
    });
    drained_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("router waiter does not retain a drain-counted permit")
        .expect("failure gate drains cleanly");
    drainer.join().unwrap();

    let identity = PersistentFailureCutIdentity::new(
        home.home_id(),
        home_generation,
        gate.service_generation(),
        PersistentFailureGeneration::FIRST,
    );
    router
        .freeze_persistent_failure_targets(identity)
        .expect("failure freeze wakes the permit-free router wait");
    assert!(matches!(
        waiter.join().unwrap(),
        Err(super::super::StopElectionAcquireError::Router)
    ));
}

#[test]
fn unregister_and_failure_election_have_one_deterministic_cut_order() {
    let (failure_first, failure_gate) = router_with_gate_for(0xee, 8_900);
    let registration =
        register_target(&failure_first, 0, 1).expect("register failure-first target");
    let stale_command = failure_gate
        .authorizer()
        .authorize()
        .expect("admit unregister before the cut");
    let quiet_command = failure_gate
        .authorizer()
        .authorize()
        .expect("admit quiet-poll accounting before the cut");
    let router_state = failure_first.state.lock().unwrap();
    let blocked_router = Arc::clone(&failure_first);
    let blocked_unregister = std::thread::spawn(move || {
        blocked_router.unregister_admitted(
            &stale_command,
            &registration,
            LiveEventTargetCloseReason::ThreadClosed,
        )
    });
    assert!(
        failure_gate
            .elect_persistent_failure_for_test(PersistentFailureGeneration::FIRST)
            .expect("elect exact test failure")
    );
    drop(router_state);
    assert!(!blocked_unregister.join().unwrap());
    failure_first.record_quiet_poll(&quiet_command);
    failure_first.retire(LiveEventTargetCloseReason::StreamFailure);
    let snapshot = failure_first.snapshot().unwrap();
    assert_eq!(snapshot.target_count(), 1);
    assert_eq!(snapshot.quiet_poll_count(), 0);
    assert_eq!(snapshot.state(), LiveEventConnectionState::Active);

    let (unregister_first, later_failure_gate) = router_with_gate_for(0xef, 8_901);
    let registration =
        register_target(&unregister_first, 0, 1).expect("register unregister-first target");
    let command = later_failure_gate
        .authorizer()
        .authorize()
        .expect("admit winning unregister");
    assert!(!unregister_first.unregister_admitted(
        &command,
        &registration,
        LiveEventTargetCloseReason::ThreadClosed,
    ));
    assert_eq!(unregister_first.snapshot().unwrap().target_count(), 0);
    assert!(
        later_failure_gate
            .elect_persistent_failure_for_test(PersistentFailureGeneration::FIRST)
            .expect("elect later exact test failure")
    );
}

#[test]
fn live_target_capacity_rejects_the_sixty_fifth_target_and_reopens_after_removal() {
    let router = router_for(0xf0, 9_000);
    let registrations = register_targets(&router, 0..LIVE_EVENT_TARGET_CAPACITY, 1);

    let rejected = register_target(&router, LIVE_EVENT_TARGET_CAPACITY, 1)
        .expect_err("the sixty-fifth exact target exceeds router capacity");
    assert!(matches!(
        rejected,
        LiveEventTargetRegistrationError::TargetCapacityFull {
            capacity: LIVE_EVENT_TARGET_CAPACITY,
        }
    ));

    let mismatched_owner = SyndicThreadId::from_bytes([0xa1; 16]);
    let command = live_command(&router);
    let mismatched = router
        .register(
            &command,
            LoadedThreadKey {
                runtime_id: router.runtime_id,
                process_generation: router.process_generation,
                cas_thread_id: target_id(LIVE_EVENT_TARGET_CAPACITY + 1),
            },
            mismatched_owner,
            CasLoadedSessionGeneration::new(
                router.process_generation,
                CasLoadedThreadGeneration::new((LIVE_EVENT_TARGET_CAPACITY + 2) as u64).unwrap(),
            ),
            1,
            Duration::from_secs(1),
            TargetTurnRegistration::Pending(super::pending_activation(0xa2)),
        )
        .expect_err("pending authority is validated before capacity admission");
    assert!(matches!(
        mismatched,
        LiveEventTargetRegistrationError::ActivationAuthorityMismatch
    ));

    assert!(!router.unregister(&registrations[0], LiveEventTargetCloseReason::ThreadClosed));
    let _replacement = register_target(&router, LIVE_EVENT_TARGET_CAPACITY, 1)
        .expect("removing one exact target reopens one capacity slot");
}

#[test]
fn failure_cut_snapshots_all_admitted_targets_in_deterministic_order() {
    let home_directory = tempfile::tempdir().expect("router failure batch home");
    let home = HomeStore::open(HomeOpenOptions::new(
        home_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .expect("open router failure batch home");
    let home_generation = home
        .health()
        .generation()
        .expect("open home has one exact generation");
    let identity = PersistentFailureCutIdentity::new(
        home.home_id(),
        home_generation,
        ProjectionServiceGeneration::allocate().expect("test service generation"),
        PersistentFailureGeneration::FIRST,
    );

    let ascending_router = router_for(0xf1, 9_001);
    let descending_router = router_for(0xf2, 9_002);
    let _ascending = register_targets(
        &ascending_router,
        0..LIVE_EVENT_TARGET_CAPACITY,
        home_generation.get(),
    );
    let _descending = register_targets(
        &descending_router,
        (0..LIVE_EVENT_TARGET_CAPACITY).rev(),
        home_generation.get(),
    );

    let ascending_identity = identity_for_router(&ascending_router, identity);
    let descending_identity = identity_for_router(&descending_router, identity);
    let ascending = ascending_router
        .freeze_persistent_failure_targets(ascending_identity)
        .expect("active router freezes all of its bounded targets");
    let descending = descending_router
        .freeze_persistent_failure_targets(descending_identity)
        .expect("active router freezes all of its bounded targets");

    let ascending = ascending.into_candidates();
    let descending = descending.into_candidates();
    assert_eq!(ascending.len(), LIVE_EVENT_TARGET_CAPACITY);
    assert_eq!(descending.len(), LIVE_EVENT_TARGET_CAPACITY);

    let ascending_ids = candidate_ids(ascending);
    let descending_ids = candidate_ids(descending);
    let expected = (0..LIVE_EVENT_TARGET_CAPACITY)
        .map(target_id)
        .collect::<Vec<_>>();
    assert_eq!(ascending_ids, expected);
    assert_eq!(descending_ids, expected);
}

#[test]
fn failure_target_freeze_classifies_every_eligibility_exclusion() {
    let home_directory = tempfile::tempdir().expect("router eligibility home");
    let home = HomeStore::open(HomeOpenOptions::new(
        home_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .expect("open router eligibility home");
    let identity = PersistentFailureCutIdentity::new(
        home.home_id(),
        home.health().generation().unwrap(),
        ProjectionServiceGeneration::allocate().unwrap(),
        PersistentFailureGeneration::FIRST,
    );
    let cases = [
        (
            EligibilityCase::ActiveOperation,
            PersistentFailureTargetIneligibility::ActiveOperation,
        ),
        (
            EligibilityCase::ActiveOnlyRegistration,
            PersistentFailureTargetIneligibility::ActiveOnlyRegistration,
        ),
        (
            EligibilityCase::ContextCompaction,
            PersistentFailureTargetIneligibility::ContextCompaction,
        ),
        (
            EligibilityCase::AwaitingActivation,
            PersistentFailureTargetIneligibility::AwaitingActivation,
        ),
        (
            EligibilityCase::Terminal,
            PersistentFailureTargetIneligibility::Terminal,
        ),
        (
            EligibilityCase::Closing,
            PersistentFailureTargetIneligibility::Closing,
        ),
        (
            EligibilityCase::Lost,
            PersistentFailureTargetIneligibility::Lost,
        ),
        (
            EligibilityCase::PublicationInFlight,
            PersistentFailureTargetIneligibility::PublicationInFlight,
        ),
        (
            EligibilityCase::IdentityMismatch,
            PersistentFailureTargetIneligibility::IdentityMismatch,
        ),
        (
            EligibilityCase::PriorPrimaryAmbiguous,
            PersistentFailureTargetIneligibility::PriorPrimaryAmbiguous,
        ),
        (
            EligibilityCase::GenerationMismatch,
            PersistentFailureTargetIneligibility::GenerationMismatch,
        ),
    ];

    assert!(frozen_candidate(EligibilityCase::Eligible, identity).is_ok());
    for (case, expected) in cases {
        assert_eq!(frozen_candidate(case, identity).err(), Some(expected));
    }
}

#[test]
fn router_failure_freeze_is_single_flight_after_dispatch_authorization() {
    let home_directory = tempfile::tempdir().expect("router guard home");
    let home = HomeStore::open(HomeOpenOptions::new(
        home_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .expect("open router guard home");
    let identity = PersistentFailureCutIdentity::new(
        home.home_id(),
        home.health().generation().unwrap(),
        ProjectionServiceGeneration::allocate().unwrap(),
        PersistentFailureGeneration::FIRST,
    );
    let (router, proof) = frozen_eligible_target(identity);
    let _authorization = router
        .authorize_persistent_failure_dispatch(proof)
        .expect("the exact frozen guard is initially spendable");
    assert_eq!(
        router.freeze_persistent_failure_targets(identity).err(),
        Some(PersistentFailureTargetIneligibility::RouterUnavailable)
    );
}

#[test]
fn thread_close_before_failure_freeze_marks_the_exact_target_ineligible() {
    let home_directory = tempfile::tempdir().expect("router close-before-freeze home");
    let home = HomeStore::open(HomeOpenOptions::new(
        home_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .expect("open router close-before-freeze home");
    let identity = PersistentFailureCutIdentity::new(
        home.home_id(),
        home.health().generation().unwrap(),
        ProjectionServiceGeneration::allocate().unwrap(),
        PersistentFailureGeneration::FIRST,
    );
    let (router, registration, identity) = eligibility_target(EligibilityCase::Eligible, identity);
    let thread_id = registration.key().cas_thread_id.clone();

    assert!(!router.record_thread_closed(&thread_id).unwrap());
    let mut candidates = router
        .freeze_persistent_failure_targets(identity)
        .expect("the router records the exact failure cut")
        .into_candidates();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates.pop().unwrap().into_proof().err(),
        Some(PersistentFailureTargetIneligibility::Closing)
    );
    let snapshot = router.snapshot().unwrap();
    assert_eq!(snapshot.target_count(), 1);
    assert_eq!(snapshot.retired_thread_lane_count(), 1);
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Closed(LiveEventTargetCloseReason::ThreadClosed)
    ));
}

#[test]
fn thread_close_after_failure_freeze_closes_routing_and_invalidates_dispatch() {
    let home_directory = tempfile::tempdir().expect("router close-after-freeze home");
    let home = HomeStore::open(HomeOpenOptions::new(
        home_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .expect("open router close-after-freeze home");
    let identity = PersistentFailureCutIdentity::new(
        home.home_id(),
        home.health().generation().unwrap(),
        ProjectionServiceGeneration::allocate().unwrap(),
        PersistentFailureGeneration::FIRST,
    );
    let (router, registration, identity) = eligibility_target(EligibilityCase::Eligible, identity);
    let mut candidates = router
        .freeze_persistent_failure_targets(identity)
        .unwrap()
        .into_candidates();
    let proof = candidates.pop().unwrap().into_proof().unwrap();
    let thread_id = proof.cas_thread_id().clone();

    assert!(!router.record_thread_closed(&thread_id).unwrap());
    let snapshot = router.snapshot().unwrap();
    assert_eq!(snapshot.target_count(), 1);
    assert_eq!(snapshot.retired_thread_lane_count(), 1);
    assert!(matches!(
        registration.poll(Duration::ZERO),
        LiveEventPoll::Closed(LiveEventTargetCloseReason::ThreadClosed)
    ));
    assert_eq!(
        router.authorize_persistent_failure_dispatch(proof).err(),
        Some(PersistentFailureTargetIneligibility::IdentityMismatch)
    );
}

fn frozen_candidate(
    case: EligibilityCase,
    identity: PersistentFailureCutIdentity,
) -> Result<super::super::PersistentFailureTargetProof, PersistentFailureTargetIneligibility> {
    let (router, _registration, identity) = eligibility_target(case, identity);
    let mut candidates = router
        .freeze_persistent_failure_targets(identity)
        .expect("active test router freezes once")
        .into_candidates();
    assert_eq!(candidates.len(), 1);
    candidates.pop().unwrap().into_proof()
}

fn frozen_eligible_target(
    identity: PersistentFailureCutIdentity,
) -> (Arc<EventRouter>, super::super::PersistentFailureTargetProof) {
    let (router, _registration, identity) = eligibility_target(EligibilityCase::Eligible, identity);
    let mut candidates = router
        .freeze_persistent_failure_targets(identity)
        .unwrap()
        .into_candidates();
    (router, candidates.pop().unwrap().into_proof().unwrap())
}

fn eligibility_target(
    case: EligibilityCase,
    identity: PersistentFailureCutIdentity,
) -> (
    Arc<EventRouter>,
    TargetRegistration,
    PersistentFailureCutIdentity,
) {
    let (router, gate) = router_with_gate_for(0xe1, 8_001);
    let identity = identity_for_router(&router, identity);
    let owner = SyndicThreadId::from_bytes([0xe1; 16]);
    let cas_thread_id = CasThreadId::new("eligibility-target").unwrap();
    let command = live_command(&router);
    let registration = router
        .register(
            &command,
            LoadedThreadKey {
                runtime_id: router.runtime_id,
                process_generation: router.process_generation,
                cas_thread_id: cas_thread_id.clone(),
            },
            owner,
            CasLoadedSessionGeneration::new(
                router.process_generation,
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
            identity.home_generation.get(),
            Duration::from_secs(1),
            TargetTurnRegistration::Pending(super::pending_activation(0xe1)),
        )
        .unwrap();
    drop(command);
    let mut state = router.state.lock().unwrap();
    let target = state.targets.get_mut(&cas_thread_id).unwrap();
    let cas_turn_id = CasTurnId::new("eligible-turn").unwrap();
    target.turn_state = TargetTurn::Exact;
    target.turn_id = Some(cas_turn_id.clone());
    target.start_dispatched = true;
    target.activation_durable = true;
    drop(state);

    if !matches!(case, EligibilityCase::PriorPrimaryAmbiguous) {
        let stop_proof = router
            .stop_target(owner, &cas_thread_id, &cas_turn_id)
            .expect("eligible target produces an exact stop proof");
        let permit = router
            .acquire_stop_election(
                gate.authorizer()
                    .authorize()
                    .expect("admit exact stop election"),
                &stop_proof,
            )
            .expect("elect exact stop target");
        assert!(
            gate.elect_persistent_failure_for_test(PersistentFailureGeneration::FIRST)
                .expect("elect exact persistent failure")
        );
        let StopElectionAdmission::PersistentFailure(admission) = permit
            .admission(super::pending_activation(0xe1).turn_id())
            .expect("classify exact failed admission")
        else {
            panic!("the exact stop admission transfers to persistent failure");
        };
        admission
            .preserve()
            .expect("preserve exact failed-admission proof");
        let state = router.state.lock().unwrap();
        assert!(state.active_stop_election.is_none());
        assert!(state.volatile_stop_admission.is_some());
    }

    let mut state = router.state.lock().unwrap();
    if matches!(case, EligibilityCase::ActiveOperation) {
        state.active_steering_attempt = Some(ActiveSteeringAttemptKey {
            token: 1,
            thread_id: cas_thread_id.clone(),
            registration: registration.registration(),
            command_dispatched: false,
            loss_transferred: false,
        });
    }
    let target = state.targets.get_mut(&cas_thread_id).unwrap();
    match case {
        EligibilityCase::ContextCompaction => {
            let operation =
                CompactionOperationId::new(owner, CompactionOperationNonce::from_bytes([0xe2; 16]));
            target.compaction = Some(ContextCompactionTargetAuthority::new(
                operation,
                operation.provider_turn_id(),
            ));
        }
        EligibilityCase::Terminal => target.turn_state = TargetTurn::Terminal,
        EligibilityCase::Closing => {
            target.publication_closing = Some(LiveEventTargetCloseReason::ThreadClosed);
        }
        EligibilityCase::Lost => target.loss_requested = true,
        EligibilityCase::PublicationInFlight => {
            target.publication_in_flight = Some(TargetPublication::Source);
        }
        EligibilityCase::IdentityMismatch => {
            target.key.runtime_id = RuntimeId::from_bytes([0xe3; 16]);
        }
        EligibilityCase::GenerationMismatch => {
            target.home_generation = target.home_generation.checked_add(1).unwrap();
        }
        EligibilityCase::ActiveOnlyRegistration => target.pending_activation = None,
        EligibilityCase::AwaitingActivation => target.activation_durable = false,
        EligibilityCase::Eligible
        | EligibilityCase::ActiveOperation
        | EligibilityCase::PriorPrimaryAmbiguous => {}
    }
    drop(state);
    (router, registration, identity)
}

fn identity_for_router(
    router: &EventRouter,
    identity: PersistentFailureCutIdentity,
) -> PersistentFailureCutIdentity {
    PersistentFailureCutIdentity::new(
        identity.home_id,
        identity.home_generation,
        router.commands.service_generation(),
        identity.failure_generation,
    )
}

fn register_targets(
    router: &Arc<EventRouter>,
    indices: impl IntoIterator<Item = usize>,
    home_generation: u64,
) -> Vec<TargetRegistration> {
    indices
        .into_iter()
        .map(|index| {
            register_target(router, index, home_generation)
                .expect("register bounded failure target")
        })
        .collect()
}

fn register_target(
    router: &Arc<EventRouter>,
    index: usize,
    home_generation: u64,
) -> Result<TargetRegistration, LiveEventTargetRegistrationError> {
    let seed = u8::try_from(index + 1).expect("test target count fits u8");
    let command = live_command(router);
    router.register(
        &command,
        LoadedThreadKey {
            runtime_id: router.runtime_id,
            process_generation: router.process_generation,
            cas_thread_id: target_id(index),
        },
        SyndicThreadId::from_bytes([seed; 16]),
        CasLoadedSessionGeneration::new(
            router.process_generation,
            CasLoadedThreadGeneration::new((index + 1) as u64).unwrap(),
        ),
        home_generation,
        Duration::from_secs(1),
        TargetTurnRegistration::Active(CasTurnId::new(format!("cas-turn-{index:03}")).unwrap()),
    )
}

fn candidate_ids(
    candidates: Vec<super::super::PersistentFailureTargetCandidate>,
) -> Vec<CasThreadId> {
    candidates
        .into_iter()
        .map(|candidate| candidate.cas_thread_id().clone())
        .collect()
}

fn target_id(index: usize) -> CasThreadId {
    CasThreadId::new(format!("cas-thread-{index:03}")).unwrap()
}
