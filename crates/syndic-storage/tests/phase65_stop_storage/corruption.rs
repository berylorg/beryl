use beryl_home_store::CommandOutcome;
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};
use syndic_storage::{
    LiveSourceEvent, SafelyReopenStopOperation, SourceEventPayload, SourceEventSequence,
    StopOperationId, StopOperationNonce, StopOperationRecord, StopOperationState,
    StopOperationTransitionStatus, TurnEndStatus, TurnTerminalOutcome,
};

use super::stop_support::{active_stop_fixture, point_limit};

fn delete(fixture: &super::stop_support::ActiveStopFixture, record: FixtureDelete) {
    let mut mutation = FixtureBatch::new();
    mutation.delete(record).unwrap();
    crate::support::commit(&fixture.store, fixture.storage, mutation);
}

#[test]
fn stopping_gate_without_its_selected_record_is_rejected() {
    let fixture = active_stop_fixture("phase65-stop-missing-record");
    fixture.admit_stop();
    delete(&fixture, FixtureDelete::StopOperation(fixture.operation_id));

    let error = fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("current stopping gate has no stop-operation authority")
    );
}

#[test]
fn reconciliation_rejects_a_missing_selected_route_head_half() {
    let fixture = active_stop_fixture("phase66-stop-missing-route-head");
    fixture.admit_stop();
    delete(
        &fixture,
        FixtureDelete::AcceptedRouteGenerationHead(fixture.thread),
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
fn safe_reopen_reconciliation_rejects_a_missing_successor_route_half() {
    let fixture = active_stop_fixture("phase66-stop-missing-reopen-route");
    fixture.admit_stop();
    let request = SafelyReopenStopOperation::new(
        fixture.operation_id,
        fixture.target.clone(),
        fixture.gate().revision(),
        fixture.stop().revision(),
    );
    match fixture.store.execute_current(
        fixture
            .storage
            .current_safely_reopen_stop_operation(request.clone()),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean safe-reopen setup, got {outcome:?}"),
    }
    let StopOperationState::SafeReopened(witness) = fixture.stop().state() else {
        panic!("stop did not retain its safe-reopen witness");
    };
    delete(
        &fixture,
        FixtureDelete::AcceptedRouteGeneration {
            thread: fixture.thread,
            generation: witness.successor_route().generation(),
        },
    );
    assert_eq!(
        fixture
            .storage
            .safe_stop_reopen_status(&fixture.store, &request, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );
}

#[test]
fn matching_terminal_reconciliation_rejects_a_missing_event_half() {
    let fixture = active_stop_fixture("phase66-stop-missing-terminal-event");
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
    match fixture.store.execute_current(
        fixture
            .storage
            .current_admit_live_source_event(event.clone()),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean terminal setup, got {outcome:?}"),
    }
    delete(
        &fixture,
        FixtureDelete::SourceEvent {
            turn: fixture.turn,
            sequence: event.sequence(),
        },
    );
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
        StopOperationTransitionStatus::Collision
    );
}

#[test]
fn abandonment_reconciliation_rejects_a_missing_successor_binding_membership() {
    let fixture = active_stop_fixture("phase66-stop-missing-abandon-membership");
    fixture.admit_stop();
    let (_, request) = super::abandonment::startup_abandonment(&fixture);
    match fixture.store.execute_current(
        fixture
            .storage
            .current_abandon_stop_operation(request.clone()),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean abandonment setup, got {outcome:?}"),
    }
    let successor_revision = fixture.target.binding_revision().checked_next().unwrap();
    delete(
        &fixture,
        FixtureDelete::CasThreadBinding {
            thread: fixture.target.cas_thread_id().clone(),
            revision: successor_revision,
        },
    );
    assert_eq!(
        fixture
            .storage
            .stop_abandonment_status(&fixture.store, &request, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );
}

#[test]
fn admission_reconciliation_rejects_a_missing_immutable_cas_target_half() {
    let fixture = active_stop_fixture("phase66-stop-missing-cas-turn");
    fixture.admit_stop();
    delete(
        &fixture,
        FixtureDelete::CasTurn {
            thread: fixture.target.cas_thread_id().clone(),
            turn: fixture.target.cas_turn_id().clone(),
        },
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
fn unselected_live_stop_record_is_rejected() {
    let fixture = active_stop_fixture("phase65-stop-orphan-record");
    fixture.admit_stop();
    let selected = fixture.stop();
    let orphan_id = StopOperationId::new(fixture.thread, StopOperationNonce::from_bytes([140; 16]));
    let orphan = StopOperationRecord::admitted(
        orphan_id,
        selected.target().clone(),
        selected.admission(),
        selected.causes(),
    )
    .unwrap();
    crate::support::commit(
        &fixture.store,
        fixture.storage,
        crate::support::batch([FixtureRecord::StopOperation(orphan)]),
    );

    let error = fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("live stop operation and stopping gate disagree")
    );
}
