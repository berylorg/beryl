use super::*;
use beryl_app::cas_projection::{
    NativeLineageHistoryRecovery, NativeLineageRecoveryDenial, ProjectionExecutionError,
};
use syndic_storage::{RecoveryBudgetKind, RecoveryProjectionError};

#[test]
fn exact_history_preflight_denial_is_bounded_and_never_grants_admission() {
    let cases = [
        (
            RecoveryProjectionError::MissingModelContextWindow,
            NativeLineageRecoveryDenial::ModelContextUnavailable,
        ),
        (
            RecoveryProjectionError::ZeroModelContextWindow,
            NativeLineageRecoveryDenial::ModelContextUnavailable,
        ),
        (
            RecoveryProjectionError::StaleSelectedPath,
            NativeLineageRecoveryDenial::SelectedPathChanged,
        ),
        (
            RecoveryProjectionError::CurrentTailNotPendingOrdinaryUser,
            NativeLineageRecoveryDenial::PendingTurnRequired,
        ),
        (
            RecoveryProjectionError::MissingHistory {
                record: "private record identity",
            },
            NativeLineageRecoveryDenial::MissingHistory,
        ),
        (
            RecoveryProjectionError::IncompleteHistory {
                reason: "private incomplete history detail",
            },
            NativeLineageRecoveryDenial::CompleteHistoryRequired,
        ),
        (
            RecoveryProjectionError::UnsupportedHistory {
                reason: "private shape detail",
            },
            NativeLineageRecoveryDenial::UnsupportedHistory,
        ),
        (
            RecoveryProjectionError::MediaHistory {
                reason: "private media detail",
            },
            NativeLineageRecoveryDenial::MediaHistory,
        ),
        (
            RecoveryProjectionError::EmptyHistoryItem,
            NativeLineageRecoveryDenial::EmptyHistoryItem,
        ),
        (
            RecoveryProjectionError::BudgetOverflow {
                kind: RecoveryBudgetKind::ItemCount,
                maximum: 1,
                actual: u64::MAX,
            },
            NativeLineageRecoveryDenial::ItemCountLimit,
        ),
        (
            RecoveryProjectionError::BudgetOverflow {
                kind: RecoveryBudgetKind::Utf8Bytes,
                maximum: 1,
                actual: u64::MAX,
            },
            NativeLineageRecoveryDenial::Utf8BytesLimit,
        ),
        (
            RecoveryProjectionError::ConcurrentChange,
            NativeLineageRecoveryDenial::SelectedPathChanged,
        ),
        (
            RecoveryProjectionError::CursorTerminal,
            NativeLineageRecoveryDenial::InvalidHistory,
        ),
        (
            RecoveryProjectionError::InvalidCursorPageLimit { actual: usize::MAX },
            NativeLineageRecoveryDenial::InvalidHistory,
        ),
        (
            RecoveryProjectionError::CursorPageLimitTooSmall {
                offset: u64::MAX,
                actual: usize::MAX,
            },
            NativeLineageRecoveryDenial::InvalidHistory,
        ),
        (
            RecoveryProjectionError::CursorMismatch {
                reason: "private cursor detail",
            },
            NativeLineageRecoveryDenial::InvalidHistory,
        ),
        (
            RecoveryProjectionError::Invariant("private invariant detail"),
            NativeLineageRecoveryDenial::InvalidHistory,
        ),
        (
            RecoveryProjectionError::Read(beryl_home_store::ReadError::ForeignDomain {
                domain: "private read detail",
            }),
            NativeLineageRecoveryDenial::HistoryReadFailed,
        ),
    ];
    let control = NativeLineageRecoveryControl::for_test(NonZeroUsize::new(1).unwrap());
    let thread = SyndicThreadId::from_bytes([216; 16]);
    for (error, expected) in cases {
        let history = NativeLineageHistoryRecovery::from_preflight(Err(
            ProjectionExecutionError::RecoveryProjection(error),
        ));
        assert_eq!(history, NativeLineageHistoryRecovery::Denied(expected));
        assert!(!history.is_available());
        assert!(history.disabled_explanation().len() <= 256);
        assert!(!format!("{history:?}").contains("private"));
        assert!(!history.disabled_explanation().contains("private"));
        let key = control
            .install_route_for_test(
                thread,
                thread,
                BindingRevision::new(7).unwrap(),
                NativeLineageOperation::Resume,
                3,
                history,
            )
            .unwrap();
        assert_eq!(
            control.submit(key, NativeLineageRecoveryCommand::RecoverFromSyndic),
            Err(NativeLineageRecoveryCommandError::NotActionable)
        );
        assert_eq!(control.take_command_for_test(key), None);
        control
            .submit(key, NativeLineageRecoveryCommand::Retry)
            .unwrap();
        assert_eq!(
            control.take_command_for_test(key),
            Some(NativeLineageRecoveryCommand::Retry)
        );
        control.cancel(key).unwrap();
    }
    assert_eq!(
        NativeLineageHistoryRecovery::from_preflight(Ok(())),
        NativeLineageHistoryRecovery::Available
    );
    assert!(std::mem::size_of::<NativeLineageHistoryRecovery>() <= 2);
}

#[test]
fn scheduler_retains_actual_model_context_denial_across_retry_failure() {
    prove_scheduler_denial(false, None, 217);
}

#[test]
fn recovered_pending_retains_actual_zero_context_denial_across_retry_failure() {
    prove_scheduler_denial(true, Some(0), 218);
}

fn prove_scheduler_denial(recovered_pending: bool, model_context: Option<u64>, seed: u8) {
    let (mut fixture, faults, slot, cas_thread_id) =
        scheduler_fixture_with_context(seed, 4, model_context);
    let control = fixture.store.native_lineage_recovery_control();
    let mut occupied = Vec::new();
    if recovered_pending {
        for byte in 190..194 {
            occupied.push(
                control
                    .install_route_for_test(
                        SyndicThreadId::from_bytes([byte; 16]),
                        SyndicThreadId::from_bytes([189; 16]),
                        BindingRevision::new(1).unwrap(),
                        NativeLineageOperation::Resume,
                        0,
                        NativeLineageHistoryRecovery::Available,
                    )
                    .unwrap(),
            );
        }
    }
    let server = NativeLineageServer::spawn_retry_failure_then_park(cas_thread_id);
    attach_session(&fixture, &slot, &server, u64::from(seed) * 1_000 + 1);
    let ids = admit_pending_turn(&mut fixture, &faults, seed);
    if recovered_pending {
        wait_until("durable pending route awaiting capacity", || {
            let home = fixture.store.live_home_command().ok()?;
            let promoted = accepted_route_state(home.home(), fixture.storage.clone(), &ids)
                == AcceptedRouteEffectiveState::Promoted;
            (promoted
                && fixture
                    .store
                    .accepted_input_scheduler_diagnostics()
                    .workers_active()
                    == 0
                && slot.is_ready())
            .then_some(())
        });
        assert_eq!(server.resume_request_count(), 0);
        control.cancel(occupied.pop().unwrap()).unwrap();
    }
    server.wait_for_first_resume();
    let pending = promoted_pending_turn(&fixture.store, &fixture.storage, &ids);
    server.release_first_resume();
    server.wait_for_initial_retries();
    let initial = wait_for_parked_route(&fixture);
    let history =
        NativeLineageHistoryRecovery::Denied(NativeLineageRecoveryDenial::ModelContextUnavailable);
    assert_eq!(
        initial.status(),
        NativeLineageRecoveryStatus::Ready {
            history_recovery: history
        }
    );
    assert_eq!(
        control.submit(
            initial.key(),
            NativeLineageRecoveryCommand::RecoverFromSyndic
        ),
        Err(NativeLineageRecoveryCommandError::NotActionable)
    );
    control
        .submit(initial.key(), NativeLineageRecoveryCommand::Retry)
        .unwrap();
    server.wait_for_command_retries();
    let failed = wait_until("exact history denial after failed retry", || {
        let snapshot = control.snapshot_for_thread(fixture.thread)?;
        (snapshot.status()
            == NativeLineageRecoveryStatus::Failed {
                command: NativeLineageRecoveryCommand::Retry,
                history_recovery: history,
            })
        .then_some(snapshot)
    });
    assert_eq!(failed.key(), initial.key());
    assert_eq!(failed.source_thread_id(), initial.source_thread_id());
    assert_eq!(
        failed.source_binding_revision(),
        initial.source_binding_revision()
    );
    assert_eq!(failed.operation(), initial.operation());
    assert_eq!(failed.failed_attempts(), initial.failed_attempts());
    assert_eq!(
        control.submit(
            failed.key(),
            NativeLineageRecoveryCommand::RecoverFromSyndic
        ),
        Err(NativeLineageRecoveryCommandError::NotActionable)
    );
    assert_eq!(
        promoted_pending_turn(&fixture.store, &fixture.storage, &ids),
        pending
    );
    let command = fixture.store.live_home_command().unwrap();
    let state = fixture
        .storage
        .turn_state(command.home(), pending, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Pending);
    assert_eq!(state.source_event_count(), 0);
    drop(command);
    for key in occupied {
        control.cancel(key).unwrap();
    }
    close_fixture(fixture, &slot, server);
}
