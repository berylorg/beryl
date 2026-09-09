use super::*;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_model::{CasTurnId, SyndicItemId};
use syndic_storage::{
    CompactionAdmissionRead, CompactionMarkerLifecycle, CompactionOperationState,
    CompactionProviderEvent, CompactionSettlement, InputGateState, SyndicTimestamp,
    test_faults::lifecycle_compaction_settlement_fault_scope,
};

#[derive(Clone, Copy, Debug)]
enum SettlementCase {
    Success,
    UserInput,
    PreparationFailure,
    CancelledPreparationFailure,
    Shutdown,
    NotCommitted,
    Indeterminate,
    CommittedThenFailure,
    IndeterminateThenHomeFailure,
    CommittedThenHomeFailure,
    LiveCompactionStop,
}

#[test]
fn real_yield_compaction_success_suppresses_failure_attention() {
    run_settlement(SettlementCase::Success);
}

#[test]
fn real_yield_compaction_user_input_wins_without_attention() {
    run_settlement(SettlementCase::UserInput);
}

#[test]
fn real_yield_compaction_preparation_failure_reports_attention() {
    run_settlement(SettlementCase::PreparationFailure);
}

#[test]
fn real_yield_compaction_soft_stop_suppresses_later_preparation_failure() {
    run_settlement(SettlementCase::CancelledPreparationFailure);
}

#[test]
fn real_yield_compaction_shutdown_suppresses_attention() {
    run_settlement(SettlementCase::Shutdown);
}

#[test]
fn real_yield_compaction_not_committed_settlement_reports_attention() {
    run_settlement(SettlementCase::NotCommitted);
}

#[test]
fn real_yield_compaction_indeterminate_settlement_preserves_exact_outcome() {
    run_settlement(SettlementCase::Indeterminate);
}

#[test]
fn real_yield_compaction_committed_settlement_suppresses_later_failure_attention() {
    run_settlement(SettlementCase::CommittedThenFailure);
}

#[test]
fn real_yield_compaction_unprovable_admission_reports_failure_after_home_loss() {
    run_settlement(SettlementCase::IndeterminateThenHomeFailure);
}

#[test]
fn real_yield_compaction_committed_admission_survives_home_loss_before_result_handling() {
    run_settlement(SettlementCase::CommittedThenHomeFailure);
}

#[test]
fn stopping_live_compaction_cancels_its_original_yield_without_attention() {
    run_settlement(SettlementCase::LiveCompactionStop);
}

fn run_settlement(case: SettlementCase) {
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(229, faults.clone());
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = YieldServer::spawn();
    let (session, projection) = support::obtain(&fixture, &server);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let mut lifecycle = fixture.store.lifecycle_yield_handler(&pool);
    let custody = fixture
        .store
        .context_compaction_lifecycle_test_harness()
        .unwrap();
    let pressure = custody.occupy_compaction_custody(71);
    let result = thread::scope(|scope| {
        let worker = scope.spawn(|| support::execute(&fixture, projection, &mut lifecycle));
        server.wait_started();
        assert_eq!(server.call("phase_continue")["result"]["success"], true);
        assert_eq!(pressure.in_use(), 72);
        if matches!(case, SettlementCase::LiveCompactionStop) {
            server.live_compaction();
        } else {
            server.compact();
        }
        worker.join().unwrap().unwrap()
    });
    assert!(matches!(
        result,
        OrdinaryTurnExecutionOutcome::LifecycleContinuationScheduled { .. }
    ));
    assert!(pool.snapshot().is_empty());
    assert_eq!(pressure.in_use(), 72);
    let operation = match fixture
        .storage
        .compaction_admission_read(&fixture.home(), fixture.thread, point_limit())
        .unwrap()
    {
        CompactionAdmissionRead::Existing(operation) => operation,
        other => panic!("expected admitted real compaction: {other:?}"),
    };
    let deadline = std::time::Instant::now() + server::TIMEOUT;
    loop {
        let current = fixture
            .storage
            .compaction_operation(&fixture.home(), operation.id(), point_limit())
            .unwrap()
            .unwrap();
        if current.request().is_some_and(|request| {
            request.disposition() == syndic_storage::CompactionRequestDisposition::Accepted
        }) && (!matches!(case, SettlementCase::LiveCompactionStop)
            || current.cas_turn().is_some())
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "compaction response was not durably observed"
        );
        thread::sleep(std::time::Duration::from_millis(10));
    }
    let harness = fixture
        .store
        .context_compaction_lifecycle_test_harness()
        .unwrap();
    if matches!(case, SettlementCase::LiveCompactionStop) {
        assert!(matches!(
            fixture
                .store
                .stop_selected_operation(fixture.thread)
                .unwrap(),
            beryl_app::cas_projection::StopCoordinationOutcome::Stopping { .. }
        ));
        session.invalidate_connection();
        drop(session);
        server.join();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while fixture
            .store
            .context_compaction_diagnostics()
            .retained_operations()
            != 0
        {
            assert!(
                std::time::Instant::now() < deadline,
                "stopped compaction did not retire after connection loss"
            );
            thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(pool.snapshot().is_empty());
        let (directory, service) = fixture.into_service();
        let _ = service.close().unwrap();
        assert_eq!(pressure.in_use(), 71);
        assert!(pool.snapshot().is_empty());
        drop(directory);
        return;
    }
    if matches!(case, SettlementCase::UserInput) {
        let latest = fixture
            .storage
            .thread_tail(&fixture.home(), fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .last_activity_at()
            .unix_millis();
        fixture.advance_clock_to(latest.checked_add(1).unwrap());
        fixture.accept_text(" accepted during automatic compaction");
    }
    if matches!(
        case,
        SettlementCase::PreparationFailure | SettlementCase::CancelledPreparationFailure
    ) {
        harness.fail_next_lifecycle_staging().unwrap();
    }
    if matches!(case, SettlementCase::CancelledPreparationFailure) {
        fixture
            .store
            .cancel_selected_continuation_for_window_close(fixture.thread)
            .unwrap();
    }
    if matches!(case, SettlementCase::Shutdown) {
        harness.request_shutdown().unwrap();
    }
    for (index, event) in [
        CompactionProviderEvent::ThreadStatus(syndic_storage::CompactionThreadStatus::Active),
        CompactionProviderEvent::TurnStarted(CasTurnId::new("continuation-compaction").unwrap()),
        CompactionProviderEvent::Marker {
            item_id: SyndicItemId::from_bytes([243; 16]),
            lifecycle: CompactionMarkerLifecycle::Started,
        },
        CompactionProviderEvent::Marker {
            item_id: SyndicItemId::from_bytes([243; 16]),
            lifecycle: CompactionMarkerLifecycle::Completed,
        },
        CompactionProviderEvent::ThreadStatus(syndic_storage::CompactionThreadStatus::Idle),
    ]
    .into_iter()
    .enumerate()
    {
        if matches!(case, SettlementCase::LiveCompactionStop) && index < 2 {
            continue;
        }
        let result = harness.publish_provider_event(
            operation.id(),
            event,
            SyndicTimestamp::from_unix_millis(98_000 + index as u64),
        );
        if !matches!(case, SettlementCase::Shutdown) {
            result.unwrap();
        }
    }
    match case {
        SettlementCase::NotCommitted => faults.fail_next_in_scope(
            FaultPoint::BeforeCommit,
            lifecycle_compaction_settlement_fault_scope(),
        ),
        SettlementCase::Indeterminate | SettlementCase::IndeterminateThenHomeFailure => faults
            .fail_next_in_scope(
                FaultPoint::AfterCommitBeforePersist,
                lifecycle_compaction_settlement_fault_scope(),
            ),
        SettlementCase::CommittedThenFailure => faults.fail_next_in_scope(
            FaultPoint::AfterPersist,
            lifecycle_compaction_settlement_fault_scope(),
        ),
        _ => {}
    }
    let publish_terminal = || {
        harness.publish_provider_event(
            operation.id(),
            CompactionProviderEvent::Terminal(syndic_storage::TurnEndStatus::complete()),
            SyndicTimestamp::from_unix_millis(98_010),
        )
    };
    let terminal = if matches!(
        case,
        SettlementCase::IndeterminateThenHomeFailure | SettlementCase::CommittedThenHomeFailure
    ) {
        let pause = harness.pause_after_lifecycle_settlement().unwrap();
        thread::scope(|scope| {
            let worker = scope.spawn(publish_terminal);
            pause.wait_until_settled();
            force_home_failure(&fixture, &faults);
            assert!(
                pool.snapshot().is_empty(),
                "final settlement owns its exact attempt through the cut"
            );
            pause.release();
            worker.join().unwrap()
        })
    } else {
        publish_terminal()
    };
    let expected_failure = matches!(
        case,
        SettlementCase::PreparationFailure
            | SettlementCase::NotCommitted
            | SettlementCase::IndeterminateThenHomeFailure
    );
    if matches!(
        case,
        SettlementCase::Success
            | SettlementCase::UserInput
            | SettlementCase::PreparationFailure
            | SettlementCase::CancelledPreparationFailure
            | SettlementCase::Indeterminate
            | SettlementCase::LiveCompactionStop
    ) {
        terminal.unwrap();
    } else {
        assert!(terminal.is_err(), "fault was not exercised: {case:?}");
    }
    let records = pool.snapshot();
    assert_eq!(records.len(), usize::from(expected_failure), "{case:?}");
    if expected_failure {
        assert_eq!(
            records[0].kind(),
            LifecycleAttentionKind::ContinuationFailed
        );
        assert_eq!(records[0].turn_id(), submitted.turn);
        assert!(pool.acknowledge(records[0].token()));
    }
    if matches!(
        case,
        SettlementCase::Success | SettlementCase::UserInput | SettlementCase::Indeterminate
    ) {
        let settled = fixture
            .storage
            .compaction_operation(&fixture.home(), operation.id(), point_limit())
            .unwrap()
            .unwrap();
        let CompactionOperationState::Consumed(witness) = settled.state() else {
            panic!("unsettled successful compaction");
        };
        let gate = fixture
            .storage
            .input_gate(&fixture.home(), fixture.thread, point_limit())
            .unwrap()
            .unwrap();
        if matches!(case, SettlementCase::UserInput) {
            assert_eq!(
                witness.settlement(),
                &CompactionSettlement::LifecycleUserWorkWon
            );
            assert_eq!(gate.live_count(), 1);
            assert_eq!(gate.state(), &InputGateState::Idle);
        } else {
            assert!(matches!(
                witness.settlement(),
                CompactionSettlement::LifecycleContinuation { .. }
            ));
            assert!(matches!(gate.state(), InputGateState::PendingTurn(_)));
        }
    }
    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    assert!(pool.snapshot().is_empty(), "retirement duplicated {case:?}");
    assert_eq!(pressure.in_use(), 71);
    drop(directory);
}

fn force_home_failure(fixture: &Fixture, faults: &FaultController) {
    use beryl_state::{
        ApplySettings, ExpectedSettingRevision, SettingKey, SettingUpdate, SettingValue,
    };
    let home = fixture.store.home_for_shutdown_test();
    let update = SettingUpdate::new(
        SettingKey::DeveloperInstructions,
        ExpectedSettingRevision::Absent,
        SettingValue::developer_instructions("continuation failure evidence").unwrap(),
    );
    let contribution = fixture.state.settings().apply(
        fixture.state.settings().revision(home).unwrap(),
        ApplySettings::new(vec![update]).unwrap(),
    );
    let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
    command.add(contribution).unwrap();
    faults.panic_next(FaultPoint::BeforeCommit);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| home.execute(command))).is_err()
    );
    assert_eq!(
        home.health().state(),
        beryl_home_store::HomeHealthState::Failed
    );
}

#[test]
fn persistent_failure_retains_continuation_preparation_capacity_until_disposal() {
    run_failed_preparation_custody(false);
}

#[test]
fn persistent_failure_retains_continuation_preparation_capacity_through_unwind() {
    run_failed_preparation_custody(true);
}

fn run_failed_preparation_custody(unwind: bool) {
    use beryl_app::cas_projection::CompactionCustodyTestStage;

    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(242, faults.clone());
    fixture.submit_text(SUBMITTED_TEXT);
    let server = YieldServer::spawn();
    let (session, projection) = support::obtain(&fixture, &server);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let mut lifecycle = fixture.store.lifecycle_yield_handler(&pool);
    let custody = fixture
        .store
        .context_compaction_lifecycle_test_harness()
        .unwrap();
    let pressure = custody.occupy_compaction_custody(71);
    thread::scope(|scope| {
        let pause =
            custody.pause_compaction_custody(CompactionCustodyTestStage::LifecyclePreparation);
        let worker = scope.spawn(|| support::execute(&fixture, projection, &mut lifecycle));
        server.wait_started();
        assert_eq!(server.call("phase_continue")["result"]["success"], true);
        server.finish(false);
        pause.wait_until_paused();
        assert_eq!(pressure.in_use(), 72);
        assert!(!custody.has_local_compaction(fixture.thread));
        force_home_failure(&fixture, &faults);
        let deadline = std::time::Instant::now() + server::TIMEOUT;
        while pool.snapshot().is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "accepted intent was not removed by failure freeze"
            );
            thread::yield_now();
        }
        assert_eq!(pressure.in_use(), 72);
        assert_ne!(
            fixture.store.persistent_failure_cut_snapshot().state(),
            beryl_app::cas_projection::PersistentFailureCutState::Finished
        );
        if unwind {
            pause.unwind();
            assert!(worker.join().is_err());
        } else {
            pause.release();
            assert!(worker.join().unwrap().is_err());
        }
        assert_eq!(pressure.in_use(), 71);
    });
    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    assert_eq!(pressure.in_use(), 71);
    assert_eq!(pool.snapshot().len(), 1);
    drop(directory);
}

#[test]
fn home_failure_reports_pending_yield_and_preserves_prior_cancellation() {
    for cancelled in [false, true] {
        let faults = FaultController::new();
        let mut fixture = Fixture::with_faults(231, faults.clone());
        fixture.submit_text(SUBMITTED_TEXT);
        let server = YieldServer::spawn();
        let (session, projection) = support::obtain(&fixture, &server);
        let pool = Arc::new(ProcessLifecycleAttentionPool::new());
        let mut lifecycle = fixture.store.lifecycle_yield_handler(&pool);
        thread::scope(|scope| {
            let worker = scope.spawn(|| support::execute(&fixture, projection, &mut lifecycle));
            server.wait_started();
            assert_eq!(server.call("phase_continue")["result"]["success"], true);
            if cancelled {
                fixture
                    .store
                    .cancel_selected_continuation_for_window_close(fixture.thread)
                    .unwrap();
            }
            faults.panic_next(FaultPoint::BeforeCommit);
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = fixture
                        .store
                        .stage_context_compaction_continuation_for_test();
                }))
                .is_err()
            );
            server.finish(true);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while fixture.store.persistent_failure_cut_snapshot().state()
                != beryl_app::cas_projection::PersistentFailureCutState::Finished
            {
                assert!(
                    std::time::Instant::now() < deadline,
                    "failure cut did not finish: {:?}",
                    fixture.store.persistent_failure_cut_snapshot()
                );
                thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(session.connection_retirement_handle_for_test().shutdown());
            assert!(worker.join().unwrap().is_err());
        });
        session.invalidate_connection();
        drop(session);
        server.join();
        let (directory, service) = fixture.into_service();
        let _ = service.close().unwrap();
        assert_eq!(pool.snapshot().len(), usize::from(!cancelled));
        drop(directory);
    }
}

#[test]
fn shutdown_cancels_accepted_yield_before_compaction_installation() {
    let mut fixture = Fixture::new(232);
    fixture.submit_text(SUBMITTED_TEXT);
    let server = YieldServer::spawn();
    let (session, projection) = support::obtain(&fixture, &server);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let mut lifecycle = fixture.store.lifecycle_yield_handler(&pool);
    thread::scope(|scope| {
        let worker = scope.spawn(|| support::execute(&fixture, projection, &mut lifecycle));
        server.wait_started();
        assert_eq!(server.call("phase_continue")["result"]["success"], true);
        fixture
            .store
            .context_compaction_lifecycle_test_harness()
            .unwrap()
            .request_shutdown()
            .unwrap();
        server.finish(false);
        assert!(worker.join().unwrap().is_err());
    });
    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    let _ = service.close().unwrap();
    assert!(pool.snapshot().is_empty());
    drop(directory);
}

#[test]
fn backend_loss_reports_pending_continuation_but_preserves_prior_soft_stop() {
    for cancelled in [false, true] {
        let mut fixture = Fixture::new(230);
        fixture.submit_text(SUBMITTED_TEXT);
        let server = YieldServer::spawn();
        let (session, projection) = support::obtain(&fixture, &server);
        let pool = Arc::new(ProcessLifecycleAttentionPool::new());
        let mut lifecycle = fixture.store.lifecycle_yield_handler(&pool);
        let result = thread::scope(|scope| {
            let worker = scope.spawn(|| support::execute(&fixture, projection, &mut lifecycle));
            server.wait_started();
            assert_eq!(server.call("phase_continue")["result"]["success"], true);
            if cancelled {
                fixture
                    .store
                    .cancel_selected_continuation_for_window_close(fixture.thread)
                    .unwrap();
            }
            server.finish(true);
            worker.join().unwrap().unwrap()
        });
        assert!(matches!(
            result,
            OrdinaryTurnExecutionOutcome::Incomplete { .. }
        ));
        assert_eq!(pool.snapshot().len(), usize::from(!cancelled));
        session.invalidate_connection();
        drop(session);
        server.join();
        let (directory, service) = fixture.into_service();
        let _ = service.close().unwrap();
        assert_eq!(pool.snapshot().len(), usize::from(!cancelled));
        drop(directory);
    }
}
