use beryl_app::cas_projection::{
    AdmittedProjectionSession, OrdinaryTurnCaptureLoss, OrdinaryTurnExecutionOutcome,
};
use beryl_backend::ManagedBackendError;
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    BindingState, InputGateState, SourceEventPayload, SourceEventSequence, TurnEndStatus,
    TurnIncompleteReason, TurnLifecycle,
};

use crate::{
    fixture::ExecutionResult,
    syndic::{Fixture, point_limit},
    verification::assert_connection_released,
};

pub fn start_completion_unknown(result: ExecutionResult) -> Box<ManagedBackendError> {
    let OrdinaryTurnExecutionOutcome::Incomplete {
        reason: OrdinaryTurnCaptureLoss::StartCompletionUnknown(error),
    } = result.unwrap()
    else {
        panic!("possibly dispatched start did not retain completion-unknown taxonomy")
    };
    error
}

pub fn assert_released(session: &AdmittedProjectionSession) {
    assert_connection_released(session);
}

pub fn assert_durable_pending(fixture: &Fixture, thread: SyndicThreadId, turn: SyndicTurnId) {
    let state = fixture
        .storage
        .turn_state(&*fixture.home(), turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Pending);
    assert_eq!(state.source_event_count(), 0);
    assert_eq!(state.end_status(), None);

    let gate = fixture
        .storage
        .input_gate(&*fixture.home(), thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::PendingTurn(turn));
    assert_eq!(gate.live_count(), 0);

    let binding = fixture
        .storage
        .current_binding(&*fixture.home(), thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(binding.binding().state(), BindingState::Valid(_)));
}

pub fn assert_durable_stream_loss(
    fixture: &Fixture,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    expected_events: u64,
) {
    let status = TurnEndStatus::incomplete(TurnIncompleteReason::StreamLost);
    let state = fixture
        .storage
        .turn_state(&*fixture.home(), turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Incomplete);
    assert_eq!(state.source_event_count(), expected_events);
    assert_eq!(state.end_status(), Some(status));
    assert_eq!(
        state.incomplete_reason(),
        Some(TurnIncompleteReason::StreamLost)
    );

    let terminal = fixture
        .storage
        .source_event(
            &*fixture.home(),
            turn,
            SourceEventSequence::new(expected_events).unwrap(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(terminal.source(), None);
    assert!(matches!(
        terminal.payload(),
        SourceEventPayload::TurnEnded(actual) if *actual == status
    ));
    assert!(
        fixture
            .storage
            .source_event(
                &*fixture.home(),
                turn,
                SourceEventSequence::new(expected_events.checked_add(1).unwrap()).unwrap(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );

    let gate = fixture
        .storage
        .input_gate(&*fixture.home(), thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);
    assert_eq!(gate.live_count(), 0);
    let binding = fixture
        .storage
        .current_binding(&*fixture.home(), thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(binding.binding().state(), BindingState::Stale(_)));
}

pub fn finish_fixture(fixture: Fixture) {
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    drop(directory);
}
