#![cfg(feature = "test-faults")]

#[path = "phase72_context_compaction/support.rs"]
mod support;
#[path = "phase10_projection/syndic.rs"]
mod syndic;

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

use std::{
    thread,
    time::{Duration, Instant},
};

use beryl_app::cas_projection::{
    ContextCompactionError, ContextCompactionOutcome, ContextCompactionRequest,
    ContextCompactionTerminalResponseTestOutcome, ContextCompactionWaitTestHarness,
    MinimumTurnCaptureReserve, ProjectionConnectionService, ProjectionServiceConfig,
    ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
};
use beryl_home_store::{CommandOutcome, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasTurnId,
    SyndicThreadId,
};
use beryl_state::BerylState;
use syndic_storage::{
    ClaimCompactionDispatch, CompactionAbandonmentReason, CompactionAdmissionRead,
    CompactionAttemptNonce, CompactionOperationId, CompactionOperationNonce,
    CompactionOperationState, CompactionProviderEvent, CompactionProviderSequence,
    CompactionRequestDisposition, CompactionSettlement, ContentLifecycle,
    PublishCompactionProviderEvent, StopAdmissionRead, StopCause, StopCauseSet, StopOperationNonce,
    StopOperationState, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    prepare_lifecycle_continuation_content,
};

use support::{LifecycleFixture, point_limit};

struct UnavailableProvider;

impl ScheduledOrdinaryExecutionProvider for UnavailableProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

fn service() -> (
    tempfile::TempDir,
    SyndicStorage,
    ProjectionConnectionService,
) {
    let directory = tempfile::tempdir().unwrap();
    let mut home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(128, 8, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap(),
        Box::new(UnavailableProvider),
    )
    .unwrap();
    (directory, storage, service)
}

fn wait_until(description: &str, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        thread::yield_now();
    }
}

fn publish_provider_event(
    home: &HomeStore,
    storage: SyndicStorage,
    operation_id: CompactionOperationId,
    event: CompactionProviderEvent,
    observed_at: u64,
) {
    let operation = storage
        .compaction_operation(home, operation_id, point_limit())
        .unwrap()
        .unwrap();
    let sequence = operation
        .provider_frontier()
        .map_or(CompactionProviderSequence::FIRST, |frontier| {
            frontier.checked_next().unwrap()
        });
    let outcome = home.execute_current(storage.current_publish_compaction_provider_event(
        PublishCompactionProviderEvent::new(
            operation_id,
            operation.revision(),
            sequence,
            event,
            SyndicTimestamp::from_unix_millis(observed_at),
        ),
    ));
    match outcome {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::NotCommitted { .. }
        | outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        }
        | outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("expected committed provider event, got {outcome:?}")
        }
    }
}

#[test]
fn completion_timeout_is_strict_and_timeout_does_not_consume_late_success() {
    let thread_id = SyndicThreadId::from_bytes([72; 16]);
    for timeout in [
        Duration::ZERO,
        Duration::from_millis(1_001),
        Duration::from_secs(86_401),
    ] {
        assert!(matches!(
            ContextCompactionRequest::new(thread_id, timeout).validate(),
            Err(ContextCompactionError::InvalidTimeout)
        ));
    }
    ContextCompactionRequest::new(thread_id, Duration::from_secs(1))
        .validate()
        .unwrap();
    ContextCompactionRequest::new(thread_id, Duration::from_secs(86_400))
        .validate()
        .unwrap();

    let wait = ContextCompactionWaitTestHarness::new(Duration::from_millis(1));
    wait.mark_accepted();
    assert_eq!(wait.wait(), ContextCompactionOutcome::StillRunning);
    wait.succeed();
    assert_eq!(wait.wait(), ContextCompactionOutcome::Succeeded);
}

#[test]
fn compaction_queue_and_workers_stay_at_their_fixed_capacity() {
    let (directory, _, service) = service();
    let initial = service.context_compaction_diagnostics();
    assert_eq!(initial.queue_capacity(), 64);
    assert_eq!(initial.worker_capacity(), 8);
    assert_eq!(initial.queued_current(), 0);
    assert_eq!(initial.workers_current(), 0);
    assert_eq!(initial.retained_operations(), 0);

    let guard = service
        .saturate_context_compaction_capacity_for_test()
        .unwrap();
    let saturated = service.context_compaction_diagnostics();
    assert_eq!(saturated.queued_current(), saturated.queue_capacity());
    assert_eq!(saturated.workers_current(), saturated.worker_capacity());
    assert_eq!(saturated.queued_high_water(), saturated.queue_capacity());
    assert_eq!(saturated.workers_high_water(), saturated.worker_capacity());
    assert_eq!(saturated.retained_operations(), 0);

    assert!(service.deny_context_compaction_capacity_probe_for_test());
    let denied = service.context_compaction_diagnostics();
    assert_eq!(denied.denied_admissions(), 1);
    assert_eq!(denied.queued_current(), denied.queue_capacity());
    assert_eq!(denied.workers_current(), denied.worker_capacity());

    drop(guard);
    wait_until("compaction capacity release", || {
        let diagnostics = service.context_compaction_diagnostics();
        diagnostics.queued_current() == 0 && diagnostics.workers_current() == 0
    });
    let released = service.context_compaction_diagnostics();
    assert_eq!(released.retained_operations(), 0);
    service.close().unwrap();
    drop(directory);
}

#[test]
fn lifecycle_continuation_staging_is_fixed_ownerless_and_idempotent() {
    let (directory, storage, service) = service();
    let expected = prepare_lifecycle_continuation_content().unwrap();
    let first = service
        .stage_context_compaction_continuation_for_test()
        .unwrap();
    let second = service
        .stage_context_compaction_continuation_for_test()
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(first.id(), expected.id());
    assert_eq!(first.encoding(), expected.encoding());
    assert_eq!(first.summary(), expected.summary());
    let live_home = service.live_home_command().unwrap();
    let manifest = storage
        .content_manifest(
            live_home.home(),
            first.id(),
            SyndicPointReadLimit::new(1_000_000).unwrap(),
        )
        .unwrap()
        .expect("fixed lifecycle content remains durably staged");
    assert_eq!(manifest.lifecycle(), ContentLifecycle::Sealed);
    assert_eq!(manifest.owner(), None);
    assert_eq!(manifest.sealed_reference(), Some(first));

    service.close().unwrap();
    drop(directory);
}

#[test]
fn startup_consumes_provider_stop_without_ordinary_replay_and_preserves_accepted_next() {
    let mut source = syndic::Fixture::new(179);
    let submitted = source.submit_text("phase72 provider-stop base turn");
    source.complete_with_assistant(submitted, "phase72 provider-stop answer");
    let thread_id = source.thread;

    let CompactionAdmissionRead::Admissible(candidate) = source
        .storage
        .compaction_admission_read(&*source.home(), thread_id, point_limit())
        .unwrap()
    else {
        panic!("completed thread must admit context compaction")
    };
    let attempt = CompactionAttemptNonce::from_bytes([247; 16]);
    let admission = candidate.admission(
        CompactionOperationNonce::from_bytes([246; 16]),
        attempt,
        CasLoadedSessionGeneration::new(
            CasProcessGeneration::new(73).unwrap(),
            CasLoadedThreadGeneration::new(1).unwrap(),
        ),
        SyndicTimestamp::from_unix_millis(72_100),
    );
    let operation_id = admission.operation_id();
    let outcome = source
        .home()
        .execute_current(source.storage.current_admit_compaction_operation(admission));
    match outcome {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::NotCommitted { .. }
        | outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        }
        | outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("expected committed compaction admission, got {outcome:?}")
        }
    }
    let operation = source
        .storage
        .compaction_operation(&*source.home(), operation_id, point_limit())
        .unwrap()
        .unwrap();
    let outcome = source
        .home()
        .execute_current(source.storage.current_claim_compaction_dispatch(
            ClaimCompactionDispatch::new(operation_id, operation.revision(), attempt),
        ));
    match outcome {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::NotCommitted { .. }
        | outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        }
        | outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("expected committed dispatch claim, got {outcome:?}")
        }
    }
    publish_provider_event(
        &*source.home(),
        source.storage,
        operation_id,
        CompactionProviderEvent::ThreadStatus(syndic_storage::CompactionThreadStatus::Active),
        72_101,
    );
    publish_provider_event(
        &*source.home(),
        source.storage,
        operation_id,
        CompactionProviderEvent::TurnStarted(CasTurnId::new("phase72-provider-stop").unwrap()),
        72_102,
    );

    let StopAdmissionRead::Admissible(candidate) = source
        .storage
        .stop_admission_read(&*source.home(), thread_id, point_limit())
        .unwrap()
    else {
        panic!("live provider operation must admit stop")
    };
    assert_eq!(candidate.selected_route_option(), None);
    let stop = candidate.admission(
        StopOperationNonce::from_bytes([245; 16]),
        StopCauseSet::from(StopCause::SelectedOperationControl),
    );
    let stop_id = stop.operation_id();
    let outcome = source
        .home()
        .execute_current(source.storage.current_admit_stop_operation(stop));
    match outcome {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ CommandOutcome::NotCommitted { .. }
        | outcome @ CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        }
        | outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("expected committed stop admission, got {outcome:?}")
        }
    }
    assert!(matches!(
        source
            .storage
            .stop_admission_read(&*source.home(), thread_id, point_limit())
            .unwrap(),
        StopAdmissionRead::Stopping(_)
    ));
    source.accept_text("phase72 accepted during provider stop");

    let (directory, running) = source.into_service();
    running.close().unwrap();
    let mut home = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let reopened_storage = SyndicStorage::register(&mut home).unwrap();
    let state = BerylState::register(&mut home).unwrap();
    let service = ProjectionConnectionService::new(
        home,
        reopened_storage,
        ProjectionServiceConfig::try_new(128, 8, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap(),
        Box::new(UnavailableProvider),
    )
    .unwrap();

    let live_home = service.live_home_command().unwrap();
    let operation = reopened_storage
        .compaction_operation(live_home.home(), operation_id, point_limit())
        .unwrap()
        .unwrap();
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("startup did not consume provider compaction: {operation:?}")
    };
    assert_eq!(
        witness.settlement(),
        &CompactionSettlement::Abandoned(CompactionAbandonmentReason::StartupProcessGenerationLost)
    );
    assert!(matches!(
        reopened_storage
            .stop_operation(live_home.home(), stop_id, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        StopOperationState::Abandoned(_)
    ));
    let gate = reopened_storage
        .input_gate(live_home.home(), thread_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &syndic_storage::InputGateState::Idle);
    assert_eq!(gate.live_next_turn_count(), 1);
    drop(live_home);
    service
        .live_home_command()
        .unwrap()
        .home()
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_active_convergences(), 1);
    assert_eq!(diagnostics.startup_terminal_convergences(), 1);
    service.close().unwrap();
    drop(directory);
}

#[test]
fn shutdown_after_staging_consumes_manual_success_without_continuation() {
    let fixture = LifecycleFixture::new(172, 232);
    let original_tail = fixture.committed_tail();
    fixture.publish_success_prefix();
    let pause = fixture.harness.pause_after_lifecycle_staging().unwrap();
    let terminal_harness = fixture.harness.clone();
    let operation_id = fixture.operation_id;
    let terminal = thread::spawn(move || {
        terminal_harness
            .publish_provider_event(
                operation_id,
                syndic_storage::CompactionProviderEvent::Terminal(
                    syndic_storage::TurnEndStatus::complete(),
                ),
                syndic_storage::SyndicTimestamp::from_unix_millis(72_020),
            )
            .unwrap();
    });
    pause.wait_until_staged();

    let shutdown_harness = fixture.harness.clone();
    let shutdown = thread::spawn(move || shutdown_harness.request_shutdown().unwrap());
    wait_until("context-compaction shutdown fence", || {
        fixture.harness.shutdown_requested().unwrap()
    });
    terminal.join().unwrap();
    shutdown.join().unwrap();

    let operation = fixture.operation();
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("shutdown settlement did not consume compaction: {operation:?}")
    };
    assert_eq!(witness.settlement(), &CompactionSettlement::ManualSuccess);
    assert_eq!(fixture.committed_tail(), original_tail);
    assert_eq!(
        fixture
            .service
            .take_terminal_lifecycle_yield_outcome(fixture.thread_id, fixture.yielding_turn_id,)
            .unwrap(),
        None
    );
    assert_eq!(
        fixture
            .service
            .context_compaction_diagnostics()
            .retained_operations(),
        0
    );
    fixture.close();
}

#[test]
fn definitive_staging_failure_settles_success_and_cleans_local_authority() {
    let fixture = LifecycleFixture::new(173, 234);
    let original_tail = fixture.committed_tail();
    let wakes_before = fixture
        .service
        .accepted_input_scheduler_diagnostics()
        .wake_count();
    fixture.harness.fail_next_lifecycle_staging().unwrap();
    fixture.publish_success_prefix();
    fixture.publish_success_terminal();

    let operation = fixture.operation();
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("preparation-failure settlement did not consume compaction: {operation:?}")
    };
    assert_eq!(witness.settlement(), &CompactionSettlement::ManualSuccess);
    assert_eq!(fixture.committed_tail(), original_tail);
    let diagnostics = fixture.service.context_compaction_diagnostics();
    assert_eq!(diagnostics.lifecycle_continuation_failures(), 1);
    assert_eq!(diagnostics.retained_operations(), 0);
    assert!(
        fixture
            .service
            .accepted_input_scheduler_diagnostics()
            .wake_count()
            > wakes_before
    );
    assert_eq!(
        fixture
            .service
            .take_terminal_lifecycle_yield_outcome(fixture.thread_id, fixture.yielding_turn_id,)
            .unwrap(),
        None
    );
    fixture.close();
}

#[test]
fn target_loss_error_consumes_intent_and_removes_exact_local_operation() {
    let fixture = LifecycleFixture::new(174, 236);
    assert_eq!(
        fixture
            .service
            .context_compaction_diagnostics()
            .retained_operations(),
        1
    );
    fixture
        .harness
        .abandon_target_loss(fixture.operation_id)
        .unwrap();

    let operation = fixture.operation();
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("target-loss settlement did not consume compaction: {operation:?}")
    };
    assert_eq!(
        witness.settlement(),
        &CompactionSettlement::Abandoned(CompactionAbandonmentReason::TargetAuthorityLost)
    );
    assert_eq!(
        fixture
            .service
            .context_compaction_diagnostics()
            .retained_operations(),
        0
    );
    assert_eq!(
        fixture
            .service
            .take_terminal_lifecycle_yield_outcome(fixture.thread_id, fixture.yielding_turn_id,)
            .unwrap(),
        None
    );
    fixture.close();
}

#[test]
fn accepted_user_work_wins_lifecycle_settlement_and_wakes_readiness() {
    let fixture = LifecycleFixture::with_accepted_next(175, 238);
    let original_tail = fixture.committed_tail();
    let gate_before = fixture.input_gate();
    assert!(matches!(
        gate_before.state(),
        syndic_storage::InputGateState::Compacting { .. }
    ));
    assert_eq!(gate_before.live_count(), 1);
    let wakes_before = fixture
        .service
        .accepted_input_scheduler_diagnostics()
        .wake_count();

    fixture.publish_success_prefix();
    fixture.publish_success_terminal();

    let operation = fixture.operation();
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("user-work settlement did not consume compaction: {operation:?}")
    };
    assert_eq!(
        witness.settlement(),
        &CompactionSettlement::LifecycleUserWorkWon
    );
    let gate_after = fixture.input_gate();
    assert_eq!(gate_after.state(), &syndic_storage::InputGateState::Idle);
    assert_eq!(gate_after.live_count(), 1);
    assert_eq!(fixture.committed_tail(), original_tail);
    let diagnostics = fixture.service.context_compaction_diagnostics();
    assert_eq!(diagnostics.retained_operations(), 0);
    assert_eq!(diagnostics.lifecycle_continuation_failures(), 0);
    assert!(
        fixture
            .service
            .accepted_input_scheduler_diagnostics()
            .wake_count()
            > wakes_before
    );
    assert_eq!(
        fixture
            .service
            .take_terminal_lifecycle_yield_outcome(fixture.thread_id, fixture.yielding_turn_id,)
            .unwrap(),
        None
    );
    fixture.close();
}

#[test]
fn terminal_settled_before_ack_preserves_success_and_awaits_router_terminal() {
    let fixture = LifecycleFixture::new(176, 240);
    fixture.publish_success_prefix();
    fixture.publish_success_terminal();
    let before = fixture.operation();

    assert_eq!(
        fixture
            .harness
            .reconcile_settled_response(
                fixture.operation_id,
                before.attempt(),
                CompactionRequestDisposition::Accepted,
                false,
            )
            .unwrap(),
        ContextCompactionTerminalResponseTestOutcome::AwaitRouterTerminal
    );
    assert_eq!(fixture.operation(), before);
    fixture.close();
}

#[test]
fn terminal_settled_before_completion_unknown_retires_only_connection_authority() {
    let fixture = LifecycleFixture::new(177, 242);
    fixture.publish_success_prefix();
    fixture.publish_success_terminal();
    let before = fixture.operation();

    assert_eq!(
        fixture
            .harness
            .reconcile_settled_response(
                fixture.operation_id,
                before.attempt(),
                CompactionRequestDisposition::CompletionUnknown,
                false,
            )
            .unwrap(),
        ContextCompactionTerminalResponseTestOutcome::RetireConnection
    );
    assert_eq!(fixture.operation(), before);
    fixture.close();
}

#[test]
fn terminal_settled_response_contradictions_fail_closed() {
    let fixture = LifecycleFixture::new(178, 244);
    fixture.publish_success_prefix();
    fixture.publish_success_terminal();
    let before = fixture.operation();

    for disposition in [
        CompactionRequestDisposition::RejectedBeforeCore,
        CompactionRequestDisposition::ProvenLocalNondispatch,
    ] {
        assert_eq!(
            fixture
                .harness
                .reconcile_settled_response(
                    fixture.operation_id,
                    before.attempt(),
                    disposition,
                    false,
                )
                .unwrap(),
            ContextCompactionTerminalResponseTestOutcome::InvariantFailure
        );
    }
    assert_eq!(
        fixture
            .harness
            .reconcile_settled_response(
                fixture.operation_id,
                CompactionAttemptNonce::from_bytes([253; 16]),
                CompactionRequestDisposition::Accepted,
                false,
            )
            .unwrap(),
        ContextCompactionTerminalResponseTestOutcome::InvariantFailure
    );
    assert_eq!(fixture.operation(), before);
    fixture.close();
}
