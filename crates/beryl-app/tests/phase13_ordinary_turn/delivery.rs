use beryl_app::cas_projection::{
    CasProjectionCoordinator, OrdinaryNotStartedProjection, OrdinaryTurnCaptureLoss,
    OrdinaryTurnExecutionOutcome, OrdinaryTurnNotStarted,
};
use serde_json::json;
use syndic_storage::{
    AssistantMessagePhase, BindingState, CanonicalItemKind, InputGateState, SourceEventPayload,
    TurnIncompleteReason, TurnLifecycle, TurnTerminalOutcome,
};

use crate::{
    backend::{FakeAppServer, ProjectionStep, TurnStartAction, TurnStartReply},
    support::{
        NoTools, execution_request, item_by_kind, item_text, obtain, process, source_events,
        wait_for_lifecycle,
    },
    syndic::{Fixture, execution_binding, point_limit},
};

const INPUT: &str = "phase13 delivery input";
const CAS_THREAD: &str = "phase13-delivery-thread";
const CAS_TURN: &str = "phase13-delivery-turn";

#[test]
fn buffered_turn_start_then_lost_response_is_never_replayed() {
    let mut fixture = Fixture::new(62);
    let submitted = fixture.submit_text(INPUT);
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: CAS_THREAD },
        ProjectionStep::TurnStart {
            target: CAS_THREAD,
            expected_input: INPUT,
            before_reply: vec![notify(
                "turn/started",
                json!({
                    "threadId": CAS_THREAD,
                    "turn": turn(CAS_TURN, "inProgress")
                }),
            )],
            reply: TurnStartReply::WithholdAndDisconnect,
            after_reply: vec![],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(62));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);

    let outcome = coordinator
        .execute_ordinary_turn(
            &fixture.store,
            fixture.storage,
            projection,
            &execution_request(),
            &mut NoTools,
        )
        .unwrap();
    assert!(matches!(
        outcome,
        OrdinaryTurnExecutionOutcome::Incomplete {
            reason: OrdinaryTurnCaptureLoss::StartCompletionUnknown(_)
        }
    ));
    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Incomplete);
    let events = source_events(&fixture, submitted.turn);
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[0].payload(),
        SourceEventPayload::TurnActivated
    ));
    assert_eq!(events[0].source().unwrap().thread_id().as_str(), CAS_THREAD);
    assert_eq!(events[0].source().unwrap().turn_id().as_str(), CAS_TURN);
    assert!(matches!(
        events[1].payload(),
        SourceEventPayload::TurnEnded(status)
            if status.outcome() == TurnTerminalOutcome::Incomplete
                && status.incomplete_reason() == Some(TurnIncompleteReason::StreamLost)
    ));
    assert!(events[1].source().is_none());
    assert_stale_and_idle(&fixture);
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

#[test]
fn exact_rejection_restores_the_pending_turn_and_retains_projection() {
    let mut fixture = Fixture::new(63);
    let submitted = fixture.submit_text(INPUT);
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: CAS_THREAD },
        ProjectionStep::TurnStart {
            target: CAS_THREAD,
            expected_input: INPUT,
            before_reply: vec![],
            reply: TurnStartReply::Reject {
                code: -32_123,
                message: "phase13 exact rejection",
            },
            after_reply: vec![],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(63));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);

    let outcome = coordinator
        .execute_ordinary_turn(
            &fixture.store,
            fixture.storage,
            projection,
            &execution_request(),
            &mut NoTools,
        )
        .unwrap();
    let OrdinaryTurnExecutionOutcome::NotStarted { projection, reason } = outcome else {
        panic!("expected exact not-started outcome, got {outcome:?}")
    };
    let OrdinaryNotStartedProjection::Retained(projection) = projection else {
        panic!("exact rejection must retain loaded projection authority")
    };
    assert_eq!(projection.cas_thread_id().as_str(), CAS_THREAD);
    assert!(projection.is_live().unwrap());
    let OrdinaryTurnNotStarted::ExactRejection(error) = reason else {
        panic!("expected exact CAS rejection")
    };
    assert_eq!(error.code, -32_123);
    assert_eq!(error.message, "phase13 exact rejection");
    drop(projection);

    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Pending);
    assert_eq!(state.record().source_event_count(), 0);
    let binding = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(binding.binding().state(), BindingState::Valid(_)));
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.record().state(),
        &InputGateState::PendingTurn(submitted.turn)
    );
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

#[test]
fn conflicting_start_identity_closes_source_less_without_claiming_live_capture() {
    let mut fixture = Fixture::new(68);
    let submitted = fixture.submit_text(INPUT);
    let observed_turn = "phase13-conflicting-observed-turn";
    let returned_turn = "phase13-conflicting-returned-turn";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: CAS_THREAD },
        ProjectionStep::TurnStart {
            target: CAS_THREAD,
            expected_input: INPUT,
            before_reply: vec![notify(
                "turn/started",
                json!({
                    "threadId": CAS_THREAD,
                    "turn": turn(observed_turn, "inProgress")
                }),
            )],
            reply: TurnStartReply::Exact {
                turn: returned_turn,
            },
            after_reply: vec![],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(68));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);

    let outcome = coordinator
        .execute_ordinary_turn(
            &fixture.store,
            fixture.storage,
            projection,
            &execution_request(),
            &mut NoTools,
        )
        .unwrap();
    assert!(matches!(
        outcome,
        OrdinaryTurnExecutionOutcome::Incomplete {
            reason: OrdinaryTurnCaptureLoss::TargetConfirmationFailed(_)
        }
    ));
    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Incomplete);
    let events = source_events(&fixture, submitted.turn);
    assert_eq!(events.len(), 1);
    assert!(events[0].source().is_none());
    assert!(matches!(
        events[0].payload(),
        SourceEventPayload::TurnEnded(status)
            if status.outcome() == TurnTerminalOutcome::Incomplete
                && status.incomplete_reason() == Some(TurnIncompleteReason::CompletionMismatch)
    ));
    assert_stale_and_idle(&fixture);
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

#[test]
fn partial_delta_then_target_loss_preserves_exact_bytes_and_closes_incomplete() {
    let mut fixture = Fixture::new(64);
    let submitted = fixture.submit_text(INPUT);
    let initial = "durable item-start prefix; ";
    let partial = "durable partial Δ bytes";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: CAS_THREAD },
        ProjectionStep::TurnStart {
            target: CAS_THREAD,
            expected_input: INPUT,
            before_reply: vec![],
            reply: TurnStartReply::Exact { turn: CAS_TURN },
            after_reply: vec![],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(64));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);

    let request = execution_request();
    let outcome = std::thread::scope(|scope| {
        let execution = scope.spawn(|| {
            coordinator.execute_ordinary_turn(
                &fixture.store,
                fixture.storage,
                projection,
                &request,
                &mut NoTools,
            )
        });
        wait_for_lifecycle(&fixture, submitted.turn, TurnLifecycle::Active);
        server.send_notification(
            "turn/started",
            json!({
                "threadId": CAS_THREAD,
                "turn": turn(CAS_TURN, "inProgress")
            }),
        );
        server.send_notification(
            "item/started",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": {
                    "id": "phase13-partial-item",
                    "type": "agentMessage",
                    "phase": "final_answer",
                            "text": initial
                }
            }),
        );
        server.send_notification(
            "item/agentMessage/delta",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "itemId": "phase13-partial-item",
                "delta": partial
            }),
        );
        server.disconnect();
        execution.join().unwrap().unwrap()
    });
    assert!(matches!(
        outcome,
        OrdinaryTurnExecutionOutcome::Incomplete {
            reason: OrdinaryTurnCaptureLoss::TargetClosed(_)
        }
    ));
    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Incomplete);
    let assistant = item_by_kind(
        &fixture,
        submitted.turn,
        CanonicalItemKind::AssistantMessage(AssistantMessagePhase::FinalAnswer),
    );
    assert_eq!(
        item_text(&fixture, assistant),
        format!("{initial}{partial}")
    );
    let events = source_events(&fixture, submitted.turn);
    assert!(matches!(
        events.last().unwrap().payload(),
        SourceEventPayload::TurnEnded(status)
            if status.outcome() == TurnTerminalOutcome::Incomplete
                && status.incomplete_reason() == Some(TurnIncompleteReason::StreamLost)
    ));
    assert!(events.last().unwrap().source().is_none());
    assert_stale_and_idle(&fixture);
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

fn assert_stale_and_idle(fixture: &Fixture) {
    let binding = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(binding.binding().state(), BindingState::Stale(_)));
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().state(), &InputGateState::Idle);
}

fn notify(method: &'static str, params: serde_json::Value) -> TurnStartAction {
    TurnStartAction::notification(method, params)
}

fn turn(id: &str, status: &str) -> serde_json::Value {
    json!({ "id": id, "status": status })
}
