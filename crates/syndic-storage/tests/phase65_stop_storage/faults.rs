use beryl_home_store::{
    CommandError, CommandOutcome,
    HomeHealthState,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::{
    ClaimStopDispatch, JoinStopCause, LiveSourceEvent, SafelyReopenStopOperation,
    SourceEventPayload, SourceEventSequence, StopAttemptNonce, StopCause,
    StopOperationTransitionStatus, TurnEndStatus, TurnTerminalOutcome,
};

use super::stop_support::{active_stop_fixture_with_faults, point_limit};

fn cuts() -> [(
    &'static str,
    FaultPoint,
    Option<StopOperationTransitionStatus>,
); 3] {
    [
        (
            "before",
            FaultPoint::BeforeCommit,
            Some(StopOperationTransitionStatus::Prior),
        ),
        ("ambiguous", FaultPoint::AfterCommitBeforePersist, None),
        (
            "persisted",
            FaultPoint::AfterPersist,
            Some(StopOperationTransitionStatus::Exact),
        ),
    ]
}

fn verify_whole_state(
    fixture: &super::stop_support::ActiveStopFixture,
    status: StopOperationTransitionStatus,
    expected: Option<StopOperationTransitionStatus>,
) {
    assert_ne!(status, StopOperationTransitionStatus::Collision);
    if let Some(expected) = expected {
        assert_eq!(status, expected);
    }
    fixture.store.validate_registered_domains().unwrap();
}

fn assert_fault_outcome(point: FaultPoint, outcome: CommandOutcome) {
    match (point, outcome) {
        (FaultPoint::BeforeCommit, CommandOutcome::NotCommitted { evidence }) => {
            assert!(matches!(evidence, CommandError::Commit { .. }));
        }
        (
            FaultPoint::AfterCommitBeforePersist,
            outcome @ CommandOutcome::Indeterminate {
                failure: CommandError::Persistence { .. },
                reconciliation: _,
            },
        ) => {
            assert!(matches!(
                &outcome,
                CommandOutcome::Indeterminate {
                    failure: CommandError::Persistence { .. },
                    reconciliation: _,
                }
            ));
        }
        (
            FaultPoint::AfterPersist,
            CommandOutcome::Committed {
                later_failure: Some(CommandError::Persistence { .. }),
                ..
            },
        ) => {}
        (_, outcome) => panic!("expected exact fault-cut command outcome, got {outcome:?}"),
    }
}

#[test]
fn admission_fault_cuts_reconcile_to_one_whole_state() {
    for (name, point, expected) in cuts() {
        let faults = FaultController::new();
        let fixture =
            active_stop_fixture_with_faults(&format!("phase65-stop-admit-{name}"), faults.clone());
        faults.fail_next(point);
        assert_fault_outcome(point, fixture.store.execute_current(
            fixture
                .storage
                .current_admit_stop_operation(fixture.admission.clone()),
        ));
        assert_eq!(fixture.store.health().state(), HomeHealthState::Verifying);
        fixture.store.verify_health().unwrap();
        let status = fixture
            .storage
            .stop_admission_status(&fixture.store, &fixture.admission, point_limit())
            .unwrap();
        verify_whole_state(&fixture, status, expected);
        if status == StopOperationTransitionStatus::Exact {
            assert_eq!(
                fixture.stop().admission_causes(),
                fixture.admission.causes()
            );
        }
    }
}

#[test]
fn cause_join_fault_cuts_reconcile_to_one_whole_state() {
    for (name, point, expected) in cuts() {
        let faults = FaultController::new();
        let fixture =
            active_stop_fixture_with_faults(&format!("phase65-stop-join-{name}"), faults.clone());
        fixture.admit_stop();
        let request = JoinStopCause::new(
            fixture.operation_id,
            fixture.target.clone(),
            fixture.gate().revision(),
            fixture.stop().revision(),
            StopCause::DiagnosticControl,
        );
        faults.fail_next(point);
        assert_fault_outcome(point, fixture
            .store
            .execute_current(fixture.storage.current_join_stop_cause(request.clone())));
        fixture.store.verify_health().unwrap();
        let status = fixture
            .storage
            .stop_cause_join_status(&fixture.store, &request, point_limit())
            .unwrap();
        verify_whole_state(&fixture, status, expected);
        if status == StopOperationTransitionStatus::Exact {
            assert_eq!(
                fixture
                    .stop()
                    .cause_first_revisions()
                    .first_revision(request.cause()),
                request.expected_stop_revision().checked_next().ok()
            );
        }
    }
}

#[test]
fn dispatch_claim_fault_cuts_reconcile_to_one_whole_state() {
    for (name, point, expected) in cuts() {
        let faults = FaultController::new();
        let fixture =
            active_stop_fixture_with_faults(&format!("phase65-stop-claim-{name}"), faults.clone());
        fixture.admit_stop();
        let request = ClaimStopDispatch::new(
            fixture.operation_id,
            fixture.target.clone(),
            fixture.gate().revision(),
            fixture.stop().revision(),
            StopAttemptNonce::from_bytes([170; 16]),
        );
        faults.fail_next(point);
        assert_fault_outcome(point, fixture
            .store
            .execute_current(fixture.storage.current_claim_stop_dispatch(request.clone())));
        fixture.store.verify_health().unwrap();
        let status = fixture
            .storage
            .stop_dispatch_claim_status(&fixture.store, &request, point_limit())
            .unwrap();
        verify_whole_state(&fixture, status, expected);
        if status == StopOperationTransitionStatus::Exact {
            let claim = fixture.stop().dispatch_claim().unwrap();
            assert_eq!(claim.source_revision(), request.expected_stop_revision());
            assert_eq!(claim.attempt(), request.attempt());
        }
    }
}

#[test]
fn safe_reopen_fault_cuts_reconcile_to_one_whole_state() {
    for (name, point, expected) in cuts() {
        let faults = FaultController::new();
        let fixture =
            active_stop_fixture_with_faults(&format!("phase65-stop-reopen-{name}"), faults.clone());
        fixture.admit_stop();
        let request = SafelyReopenStopOperation::new(
            fixture.operation_id,
            fixture.target.clone(),
            fixture.gate().revision(),
            fixture.stop().revision(),
        );
        faults.fail_next(point);
        assert_fault_outcome(point, fixture.store.execute_current(
            fixture
                .storage
                .current_safely_reopen_stop_operation(request.clone()),
        ));
        fixture.store.verify_health().unwrap();
        let status = fixture
            .storage
            .safe_stop_reopen_status(&fixture.store, &request, point_limit())
            .unwrap();
        verify_whole_state(&fixture, status, expected);
    }
}

#[test]
fn abandonment_fault_cuts_reconcile_to_one_whole_state() {
    for (name, point, expected) in cuts() {
        let faults = FaultController::new();
        let fixture = active_stop_fixture_with_faults(
            &format!("phase65-stop-abandon-{name}"),
            faults.clone(),
        );
        fixture.admit_stop();
        let request = super::abandonment::startup_abandonment(&fixture).1;
        faults.fail_next(point);
        assert_fault_outcome(point, fixture.store.execute_current(
            fixture
                .storage
                .current_abandon_stop_operation(request.clone()),
        ));
        fixture.store.verify_health().unwrap();
        let status = fixture
            .storage
            .stop_abandonment_status(&fixture.store, &request, point_limit())
            .unwrap();
        verify_whole_state(&fixture, status, expected);
    }
}

#[test]
fn matching_terminal_fault_cuts_reconcile_to_one_whole_state() {
    for (name, point, expected) in cuts() {
        let faults = FaultController::new();
        let fixture = active_stop_fixture_with_faults(
            &format!("phase65-stop-terminal-{name}"),
            faults.clone(),
        );
        fixture.admit_stop();
        crate::support::exact_cas::correlate_user_item(
            &fixture.store,
            fixture.storage,
            fixture.thread,
            fixture.turn,
            beryl_model::SyndicItemId::from_bytes([104; 16]),
            &fixture.source,
            crate::support::timestamp(5),
        );
        let state = fixture
            .storage
            .turn_state(&fixture.store, fixture.turn, point_limit())
            .unwrap()
            .unwrap();
        let stop_revision = fixture.stop().revision();
        let event = LiveSourceEvent::new(
            fixture.thread,
            fixture.turn,
            state.revision(),
            fixture.gate().revision(),
            SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
            Some(fixture.source.clone()),
            SourceEventPayload::TurnEnded(
                TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
            ),
            crate::support::timestamp(6),
        )
        .unwrap();
        faults.fail_next(point);
        assert_fault_outcome(point, fixture.store.execute_current(
            fixture
                .storage
                .current_admit_live_source_event(event.clone()),
        ));
        fixture.store.verify_health().unwrap();
        let status = fixture
            .storage
            .stop_matching_terminal_status(
                &fixture.store,
                fixture.operation_id,
                &fixture.target,
                stop_revision,
                &event,
                point_limit(),
            )
            .unwrap();
        verify_whole_state(&fixture, status, expected);
    }
}
