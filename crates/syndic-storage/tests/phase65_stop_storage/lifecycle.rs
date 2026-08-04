use beryl_model::CasTurnId;
use syndic_storage::{
    ClaimStopDispatch, InputGateState, JoinStopCause, LiveSourceEvent, SafelyReopenStopOperation,
    SourceEventPayload, SourceEventSequence, StopAttemptNonce, StopCause, StopCauseSet,
    StopOperationRevision, StopOperationState, StopOperationTransitionStatus, TurnEndStatus,
    TurnTerminalOutcome,
};

use super::stop_support::{active_stop_fixture, pending_stop_fixture, point_limit};

#[test]
fn admission_reconciles_and_retains_the_exact_stop_cut() {
    let fixture = active_stop_fixture("phase65-stop-admission");
    assert_eq!(
        fixture
            .storage
            .stop_admission_status(&fixture.store, &fixture.admission, point_limit(),)
            .unwrap(),
        StopOperationTransitionStatus::Prior
    );

    fixture.admit_stop();

    assert_eq!(
        fixture
            .storage
            .stop_admission_status(&fixture.store, &fixture.admission, point_limit(),)
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert_eq!(fixture.stop().state(), StopOperationState::Admitted);
    assert!(matches!(
        fixture.gate().state(),
        InputGateState::Stopping {
            turn_id,
            operation_nonce,
        } if *turn_id == fixture.turn
            && *operation_nonce == fixture.operation_id.nonce()
    ));
    fixture.store.validate_registered_domains().unwrap();

    assert!(
        fixture
            .store
            .execute_current(
                fixture
                    .storage
                    .current_admit_stop_operation(fixture.admission.clone()),
            )
            .is_err()
    );
}

#[test]
fn multiple_admission_causes_share_first_and_reconcile_as_the_exact_initial_set() {
    let fixture = active_stop_fixture("phase66-stop-multiple-admission-causes");
    let initial =
        StopCauseSet::from(StopCause::SelectedOperationControl).with(StopCause::DiagnosticControl);
    let admission = syndic_storage::AdmitStopOperation::new(
        fixture.operation_id,
        fixture.target.clone(),
        fixture.admission.expected_gate_revision(),
        fixture.admission.expected_route(),
        initial,
    );
    fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_admit_stop_operation(admission.clone()),
        )
        .unwrap();

    let stop = fixture.stop();
    assert_eq!(stop.admission_causes(), initial);
    assert_eq!(
        stop.cause_first_revisions()
            .first_revision(StopCause::SelectedOperationControl),
        Some(StopOperationRevision::FIRST)
    );
    assert_eq!(
        stop.cause_first_revisions()
            .first_revision(StopCause::DiagnosticControl),
        Some(StopOperationRevision::FIRST)
    );
    assert_eq!(
        fixture
            .storage
            .stop_admission_status(&fixture.store, &admission, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert_eq!(
        fixture
            .storage
            .stop_admission_status(&fixture.store, &fixture.admission, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );
}

#[test]
fn an_admission_cause_is_not_an_exact_later_join_after_an_unrelated_join() {
    let fixture = active_stop_fixture("phase66-stop-admission-cause-not-later-join");
    fixture.admit_stop();
    let gate_revision = fixture.gate().revision();
    let source_revision = fixture.stop().revision();
    let impossible = JoinStopCause::new(
        fixture.operation_id,
        fixture.target.clone(),
        gate_revision,
        source_revision,
        StopCause::SelectedOperationControl,
    );
    let unrelated = JoinStopCause::new(
        fixture.operation_id,
        fixture.target.clone(),
        gate_revision,
        source_revision,
        StopCause::DiagnosticControl,
    );
    fixture
        .store
        .execute_current(fixture.storage.current_join_stop_cause(unrelated.clone()))
        .unwrap();

    assert_eq!(
        fixture
            .storage
            .stop_cause_join_status(&fixture.store, &impossible, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );
    assert_eq!(
        fixture
            .storage
            .stop_cause_join_status(&fixture.store, &unrelated, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
}

#[test]
fn target_disagreement_is_a_reconciliation_collision() {
    let fixture = active_stop_fixture("phase65-stop-target-collision");
    fixture.admit_stop();
    let mut wrong_target = fixture.target.clone();
    let replacement_turn = CasTurnId::new("different-cas-turn").unwrap();
    wrong_target = syndic_storage::StopOperationTarget::new(
        wrong_target.thread_id(),
        wrong_target.turn_id(),
        wrong_target.turn_kind(),
        wrong_target.binding_revision(),
        wrong_target.snapshot_id(),
        wrong_target.runtime_id(),
        wrong_target.loaded_generation(),
        wrong_target.cas_thread_id().clone(),
        replacement_turn,
    );
    let wrong = syndic_storage::AdmitStopOperation::new(
        fixture.operation_id,
        wrong_target,
        fixture.admission.expected_gate_revision(),
        fixture.admission.expected_route(),
        fixture.admission.causes(),
    );
    assert_eq!(
        fixture
            .storage
            .stop_admission_status(&fixture.store, &wrong, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );
}

#[test]
fn pending_published_target_stops_and_consumes_a_matching_terminal_without_activation() {
    let fixture = pending_stop_fixture("phase66-stop-pending-terminal");
    let before = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(before.lifecycle(), syndic_storage::TurnLifecycle::Pending);
    assert_eq!(before.source_event_count(), 0);
    fixture.admit_stop();
    fixture.store.validate_registered_domains().unwrap();

    let stop_revision = fixture.stop().revision();
    let event = LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        before.revision(),
        fixture.gate().revision(),
        SourceEventSequence::FIRST,
        Some(fixture.source.clone()),
        SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
            syndic_storage::TurnIncompleteReason::StreamLost,
        )),
        crate::support::timestamp(4),
    )
    .unwrap();
    assert_eq!(
        fixture
            .storage
            .stop_matching_terminal_status(
                &fixture.store,
                fixture.operation_id,
                &fixture.target,
                stop_revision,
                &event,
                point_limit(),
            )
            .unwrap(),
        StopOperationTransitionStatus::Prior
    );
    fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_admit_live_source_event(event.clone()),
        )
        .unwrap();
    assert_eq!(
        fixture
            .storage
            .stop_matching_terminal_status(
                &fixture.store,
                fixture.operation_id,
                &fixture.target,
                stop_revision,
                &event,
                point_limit(),
            )
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn cause_claim_and_safe_reopen_are_exact_monotonic_transitions() {
    let fixture = active_stop_fixture("phase65-stop-live-transitions");
    fixture.admit_stop();
    let gate_revision = fixture.gate().revision();
    let initial = fixture.stop();
    let join = JoinStopCause::new(
        fixture.operation_id,
        fixture.target.clone(),
        gate_revision,
        initial.revision(),
        StopCause::DiagnosticControl,
    );
    assert_eq!(
        fixture
            .storage
            .stop_cause_join_status(&fixture.store, &join, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Prior
    );
    fixture
        .store
        .execute_current(fixture.storage.current_join_stop_cause(join.clone()))
        .unwrap();
    assert_eq!(
        fixture
            .storage
            .stop_cause_join_status(&fixture.store, &join, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );

    let joined = fixture.stop();
    let attempt = StopAttemptNonce::from_bytes([111; 16]);
    let claim = ClaimStopDispatch::new(
        fixture.operation_id,
        fixture.target.clone(),
        gate_revision,
        joined.revision(),
        attempt,
    );
    assert_eq!(
        fixture
            .storage
            .stop_dispatch_claim_status(&fixture.store, &claim, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Prior
    );
    fixture
        .store
        .execute_current(fixture.storage.current_claim_stop_dispatch(claim.clone()))
        .unwrap();
    assert_eq!(
        fixture
            .storage
            .stop_dispatch_claim_status(&fixture.store, &claim, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );

    let alternate = ClaimStopDispatch::new(
        fixture.operation_id,
        fixture.target.clone(),
        gate_revision,
        fixture.stop().revision(),
        StopAttemptNonce::from_bytes([112; 16]),
    );
    assert_eq!(
        fixture
            .storage
            .stop_dispatch_claim_status(&fixture.store, &alternate, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );
    assert!(
        fixture
            .store
            .execute_current(fixture.storage.current_claim_stop_dispatch(alternate))
            .is_err()
    );

    let claimed = fixture.stop();
    let stopped_route = fixture.gate().selected_route().unwrap();
    let reopen = SafelyReopenStopOperation::new(
        fixture.operation_id,
        fixture.target.clone(),
        gate_revision,
        claimed.revision(),
    );
    fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_safely_reopen_stop_operation(reopen.clone()),
        )
        .unwrap();
    assert_eq!(
        fixture
            .storage
            .safe_stop_reopen_status(&fixture.store, &reopen, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert!(matches!(
        fixture.stop().state(),
        StopOperationState::SafeReopened(_)
    ));
    let reopened_gate = fixture.gate();
    assert_eq!(
        reopened_gate.state(),
        &InputGateState::Steerable(fixture.turn)
    );
    assert!(reopened_gate.selected_route().unwrap().generation() > stopped_route.generation());
    assert_eq!(
        fixture
            .storage
            .stop_admission_status(&fixture.store, &fixture.admission, point_limit(),)
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert!(
        fixture
            .store
            .execute_current(
                fixture
                    .storage
                    .current_admit_stop_operation(fixture.admission.clone()),
            )
            .is_err()
    );
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn join_and_claim_orders_retain_distinct_exact_provenance_through_consumption() {
    let join_first = active_stop_fixture("phase66-stop-join-then-claim");
    join_first.admit_stop();
    let join_first_gate = join_first.gate().revision();
    let join = JoinStopCause::new(
        join_first.operation_id,
        join_first.target.clone(),
        join_first_gate,
        join_first.stop().revision(),
        StopCause::DiagnosticControl,
    );
    join_first
        .store
        .execute_current(join_first.storage.current_join_stop_cause(join.clone()))
        .unwrap();
    let join_first_attempt = StopAttemptNonce::from_bytes([121; 16]);
    let claim = ClaimStopDispatch::new(
        join_first.operation_id,
        join_first.target.clone(),
        join_first_gate,
        join_first.stop().revision(),
        join_first_attempt,
    );
    join_first
        .store
        .execute_current(
            join_first
                .storage
                .current_claim_stop_dispatch(claim.clone()),
        )
        .unwrap();
    let latest_join = JoinStopCause::new(
        join_first.operation_id,
        join_first.target.clone(),
        join_first_gate,
        join_first.stop().revision(),
        StopCause::HealthyHomeWindowClose,
    );
    join_first
        .store
        .execute_current(
            join_first
                .storage
                .current_join_stop_cause(latest_join.clone()),
        )
        .unwrap();
    let join_then_claim = join_first.stop();
    assert_eq!(
        join_then_claim
            .cause_first_revisions()
            .first_revision(StopCause::DiagnosticControl),
        Some(StopOperationRevision::new(2).unwrap())
    );
    assert_eq!(
        join_then_claim
            .cause_first_revisions()
            .first_revision(StopCause::HealthyHomeWindowClose),
        Some(StopOperationRevision::new(4).unwrap())
    );
    assert_eq!(
        join_then_claim.dispatch_claim().unwrap().source_revision(),
        StopOperationRevision::new(2).unwrap()
    );

    let claim_first = active_stop_fixture("phase66-stop-claim-then-join");
    claim_first.admit_stop();
    let claim_first_gate = claim_first.gate().revision();
    let claim_first_attempt = StopAttemptNonce::from_bytes([122; 16]);
    let early_claim = ClaimStopDispatch::new(
        claim_first.operation_id,
        claim_first.target.clone(),
        claim_first_gate,
        claim_first.stop().revision(),
        claim_first_attempt,
    );
    claim_first
        .store
        .execute_current(
            claim_first
                .storage
                .current_claim_stop_dispatch(early_claim.clone()),
        )
        .unwrap();
    let middle_join = JoinStopCause::new(
        claim_first.operation_id,
        claim_first.target.clone(),
        claim_first_gate,
        claim_first.stop().revision(),
        StopCause::HealthyHomeWindowClose,
    );
    claim_first
        .store
        .execute_current(
            claim_first
                .storage
                .current_join_stop_cause(middle_join.clone()),
        )
        .unwrap();
    let later_join = JoinStopCause::new(
        claim_first.operation_id,
        claim_first.target.clone(),
        claim_first_gate,
        claim_first.stop().revision(),
        StopCause::DiagnosticControl,
    );
    claim_first
        .store
        .execute_current(
            claim_first
                .storage
                .current_join_stop_cause(later_join.clone()),
        )
        .unwrap();
    let claim_then_join = claim_first.stop();
    assert_eq!(
        claim_then_join.dispatch_claim().unwrap().source_revision(),
        StopOperationRevision::FIRST
    );
    assert_eq!(
        claim_then_join
            .cause_first_revisions()
            .first_revision(StopCause::DiagnosticControl),
        Some(StopOperationRevision::new(4).unwrap())
    );
    assert_eq!(
        claim_then_join
            .cause_first_revisions()
            .first_revision(StopCause::HealthyHomeWindowClose),
        Some(StopOperationRevision::new(3).unwrap())
    );
    assert_ne!(
        join_then_claim.cause_first_revisions(),
        claim_then_join.cause_first_revisions()
    );

    let reopen = SafelyReopenStopOperation::new(
        join_first.operation_id,
        join_first.target.clone(),
        join_first_gate,
        join_first.stop().revision(),
    );
    join_first
        .store
        .execute_current(
            join_first
                .storage
                .current_safely_reopen_stop_operation(reopen),
        )
        .unwrap();
    assert_eq!(
        join_first
            .storage
            .stop_cause_join_status(&join_first.store, &join, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert_eq!(
        join_first
            .storage
            .stop_cause_join_status(&join_first.store, &latest_join, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert_eq!(
        join_first
            .storage
            .stop_dispatch_claim_status(&join_first.store, &claim, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert_eq!(
        claim_first
            .storage
            .stop_dispatch_claim_status(&claim_first.store, &early_claim, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert_eq!(
        claim_first
            .storage
            .stop_cause_join_status(&claim_first.store, &middle_join, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert_eq!(
        claim_first
            .storage
            .stop_cause_join_status(&claim_first.store, &later_join, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
}

#[test]
fn claim_before_an_intervening_cause_join_does_not_authenticate_the_later_claim() {
    let fixture = active_stop_fixture("phase66-stop-stale-claim-after-join");
    fixture.admit_stop();
    let gate_revision = fixture.gate().revision();
    let initial_revision = fixture.stop().revision();
    let attempt = StopAttemptNonce::from_bytes([113; 16]);
    let stale_claim = ClaimStopDispatch::new(
        fixture.operation_id,
        fixture.target.clone(),
        gate_revision,
        initial_revision,
        attempt,
    );
    let join = JoinStopCause::new(
        fixture.operation_id,
        fixture.target.clone(),
        gate_revision,
        initial_revision,
        StopCause::DiagnosticControl,
    );
    fixture
        .store
        .execute_current(fixture.storage.current_join_stop_cause(join))
        .unwrap();
    let current_claim = ClaimStopDispatch::new(
        fixture.operation_id,
        fixture.target.clone(),
        gate_revision,
        fixture.stop().revision(),
        attempt,
    );
    fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_claim_stop_dispatch(current_claim.clone()),
        )
        .unwrap();

    assert_eq!(
        fixture
            .storage
            .stop_dispatch_claim_status(&fixture.store, &stale_claim, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );
    assert_eq!(
        fixture
            .storage
            .stop_dispatch_claim_status(&fixture.store, &current_claim, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
}

#[test]
fn interrupting_approval_prevents_safe_reopen() {
    let fixture = active_stop_fixture("phase65-stop-approval-reopen");
    fixture.admit_stop();
    let gate_revision = fixture.gate().revision();
    let join = JoinStopCause::new(
        fixture.operation_id,
        fixture.target.clone(),
        gate_revision,
        fixture.stop().revision(),
        StopCause::InterruptingApproval,
    );
    fixture
        .store
        .execute_current(fixture.storage.current_join_stop_cause(join))
        .unwrap();
    let reopen = SafelyReopenStopOperation::new(
        fixture.operation_id,
        fixture.target.clone(),
        gate_revision,
        fixture.stop().revision(),
    );
    assert_eq!(
        fixture
            .storage
            .safe_stop_reopen_status(&fixture.store, &reopen, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );
    assert!(
        fixture
            .store
            .execute_current(fixture.storage.current_safely_reopen_stop_operation(reopen),)
            .is_err()
    );
    assert!(fixture.stop().state().is_live());
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn matching_terminal_atomically_consumes_the_live_stop() {
    let fixture = active_stop_fixture("phase65-stop-matching-terminal");
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
    assert_eq!(
        fixture
            .storage
            .stop_matching_terminal_status(
                &fixture.store,
                fixture.operation_id,
                &fixture.target,
                stop_revision,
                &event,
                point_limit(),
            )
            .unwrap(),
        StopOperationTransitionStatus::Prior
    );
    fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_admit_live_source_event(event.clone()),
        )
        .unwrap();

    assert!(matches!(
        fixture.stop().state(),
        StopOperationState::MatchingTerminal(_)
    ));
    assert_eq!(
        fixture
            .storage
            .stop_matching_terminal_status(
                &fixture.store,
                fixture.operation_id,
                &fixture.target,
                stop_revision,
                &event,
                point_limit(),
            )
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert_eq!(
        fixture.gate().state(),
        &InputGateState::FinalizingHistory(fixture.turn)
    );
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn consuming_transitions_win_cleanly_against_a_stale_cause_join() {
    let reopened = active_stop_fixture("phase65-stop-join-races-reopen");
    reopened.admit_stop();
    let reopen_join = JoinStopCause::new(
        reopened.operation_id,
        reopened.target.clone(),
        reopened.gate().revision(),
        reopened.stop().revision(),
        StopCause::HealthyHomeWindowClose,
    );
    let reopen = SafelyReopenStopOperation::new(
        reopened.operation_id,
        reopened.target.clone(),
        reopened.gate().revision(),
        reopened.stop().revision(),
    );
    reopened
        .store
        .execute_current(
            reopened
                .storage
                .current_safely_reopen_stop_operation(reopen),
        )
        .unwrap();
    assert_eq!(
        reopened
            .storage
            .stop_cause_join_status(&reopened.store, &reopen_join, point_limit(),)
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );

    let terminal = active_stop_fixture("phase65-stop-join-races-terminal");
    terminal.admit_stop();
    let terminal_join = JoinStopCause::new(
        terminal.operation_id,
        terminal.target.clone(),
        terminal.gate().revision(),
        terminal.stop().revision(),
        StopCause::HealthyHomeWindowClose,
    );
    crate::support::exact_cas::correlate_user_item(
        &terminal.store,
        terminal.storage,
        terminal.thread,
        terminal.turn,
        beryl_model::SyndicItemId::from_bytes([104; 16]),
        &terminal.source,
        crate::support::timestamp(5),
    );
    crate::support::exact_cas::admit_event(
        &terminal.store,
        terminal.storage,
        terminal.thread,
        terminal.turn,
        &terminal.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
        ),
        crate::support::timestamp(6),
    );
    assert_eq!(
        terminal
            .storage
            .stop_cause_join_status(&terminal.store, &terminal_join, point_limit(),)
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );
}
