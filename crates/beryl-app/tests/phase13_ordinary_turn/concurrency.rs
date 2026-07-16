#[cfg(feature = "test-faults")]
use std::time::Instant;
use std::{thread, time::Duration};

use beryl_app::cas_projection::{
    CasProjectionCoordinator, OrdinaryTurnExecutionError, OrdinaryTurnExecutionOutcome,
    ProjectionCoordinatorError,
};
use serde_json::json;
use syndic_storage::{BindingState, TurnLifecycle, TurnTerminalOutcome};

#[cfg(feature = "test-faults")]
use beryl_home_store::test_faults::{FaultController, FaultPoint};

use crate::{
    backend::{FakeAppServer, ProjectionStep, TurnStartReply},
    support::{NoTools, execution_request, obtain, process, wait_for_lifecycle},
    syndic::{Fixture, execution_binding, point_limit},
};

const INPUT: &str = "phase13 concurrency input";

#[test]
fn a_quiet_active_turn_excludes_another_execution_of_the_same_thread() {
    let mut fixture = Fixture::new(65);
    let submitted = fixture.submit_text(INPUT);
    let cas_thread = "phase13-same-thread";
    let cas_turn = "phase13-same-turn";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh { target: cas_thread },
        ProjectionStep::TurnStart {
            target: cas_thread,
            expected_input: INPUT,
            before_reply: vec![],
            reply: TurnStartReply::Exact { turn: cas_turn },
            after_reply: vec![],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(65));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let first = obtain(&fixture, &coordinator, &mut session, fixture.thread);
    let second = obtain(&fixture, &coordinator, &mut session, fixture.thread);
    let request = execution_request();

    thread::scope(|scope| {
        let first_execution = scope.spawn(|| {
            coordinator.execute_ordinary_turn(
                &fixture.store,
                fixture.storage,
                first,
                &request,
                &mut NoTools,
            )
        });
        thread::sleep(Duration::from_millis(50));
        if first_execution.is_finished() {
            panic!(
                "first same-thread execution ended before activation: {:?}",
                first_execution.join().unwrap()
            );
        }
        wait_for_lifecycle(&fixture, submitted.turn, TurnLifecycle::Active);
        thread::sleep(Duration::from_millis(250));
        assert_active_binding(&fixture, fixture.thread);

        let error = coordinator
            .execute_ordinary_turn(
                &fixture.store,
                fixture.storage,
                second,
                &request,
                &mut NoTools,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            OrdinaryTurnExecutionError::Coordinator(
                ProjectionCoordinatorError::ProjectionInFlight { thread_id }
            ) if thread_id == fixture.thread
        ));

        send_user_lifecycle(&server, cas_thread, cas_turn, "phase13-same-user", INPUT);
        server.send_notification(
            "turn/completed",
            json!({
                "threadId": cas_thread,
                "turn": terminal_turn(cas_turn)
            }),
        );
        let outcome = first_execution.join().unwrap().unwrap();
        assert_terminal(outcome);
    });
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

#[test]
fn different_threads_remain_simultaneously_active_during_quiet_capture() {
    let mut fixture = Fixture::new(66);
    let first_submitted = fixture.submit_text(INPUT);
    let second_thread = fixture.create_ordinary_pending(67, "phase13 second input");
    let second_turn = fixture
        .storage
        .thread(&fixture.store, second_thread, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .committed_tail()
        .unwrap();
    let first_cas_thread = "phase13-cross-first";
    let first_cas_turn = "phase13-cross-first-turn";
    let second_cas_thread = "phase13-cross-second";
    let second_cas_turn = "phase13-cross-second-turn";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh {
            target: first_cas_thread,
        },
        ProjectionStep::Fresh {
            target: second_cas_thread,
        },
        ProjectionStep::TurnStart {
            target: first_cas_thread,
            expected_input: INPUT,
            before_reply: vec![],
            reply: TurnStartReply::Exact {
                turn: first_cas_turn,
            },
            after_reply: vec![],
        },
        ProjectionStep::TurnStart {
            target: second_cas_thread,
            expected_input: "phase13 second input",
            before_reply: vec![],
            reply: TurnStartReply::Exact {
                turn: second_cas_turn,
            },
            after_reply: vec![],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(66));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let first = obtain(&fixture, &coordinator, &mut session, fixture.thread);
    let second = obtain(&fixture, &coordinator, &mut session, second_thread);
    let request = execution_request();

    thread::scope(|scope| {
        let first_execution = scope.spawn(|| {
            coordinator.execute_ordinary_turn(
                &fixture.store,
                fixture.storage,
                first,
                &request,
                &mut NoTools,
            )
        });
        thread::sleep(Duration::from_millis(50));
        if first_execution.is_finished() {
            panic!(
                "first cross-thread execution ended before activation: {:?}",
                first_execution.join().unwrap()
            );
        }
        wait_for_lifecycle(&fixture, first_submitted.turn, TurnLifecycle::Active);
        let second_execution = scope.spawn(|| {
            coordinator.execute_ordinary_turn(
                &fixture.store,
                fixture.storage,
                second,
                &request,
                &mut NoTools,
            )
        });
        thread::sleep(Duration::from_millis(50));
        if second_execution.is_finished() {
            panic!(
                "second cross-thread execution ended before activation: {:?}",
                second_execution.join().unwrap()
            );
        }
        wait_for_lifecycle(&fixture, second_turn, TurnLifecycle::Active);
        thread::sleep(Duration::from_millis(250));
        assert_active_binding(&fixture, fixture.thread);
        assert_active_binding(&fixture, second_thread);

        send_user_lifecycle(
            &server,
            first_cas_thread,
            first_cas_turn,
            "phase13-cross-first-user",
            INPUT,
        );
        server.send_notification(
            "turn/completed",
            json!({
                "threadId": first_cas_thread,
                "turn": terminal_turn(first_cas_turn)
            }),
        );
        wait_for_lifecycle(&fixture, first_submitted.turn, TurnLifecycle::Complete);
        let first_projection = terminal_projection(first_execution.join().unwrap().unwrap());
        send_user_lifecycle(
            &server,
            second_cas_thread,
            second_cas_turn,
            "phase13-cross-second-user",
            "phase13 second input",
        );
        server.send_notification(
            "turn/completed",
            json!({
                "threadId": second_cas_thread,
                "turn": terminal_turn(second_cas_turn)
            }),
        );
        let second_projection = terminal_projection(second_execution.join().unwrap().unwrap());
        drop((first_projection, second_projection));
    });
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

#[cfg(feature = "test-faults")]
#[test]
fn unrelated_live_publication_waits_for_writer_admission_without_revision_conflict() {
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(68, faults.clone());
    let first_submitted = fixture.submit_text(INPUT);
    let second_thread = fixture.create_ordinary_pending(69, "phase13 queued writer input");
    let second_turn = fixture
        .storage
        .thread(&fixture.store, second_thread, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .committed_tail()
        .unwrap();
    let first_cas_thread = "phase13-writer-first";
    let first_cas_turn = "phase13-writer-first-turn";
    let second_cas_thread = "phase13-writer-second";
    let second_cas_turn = "phase13-writer-second-turn";
    let first_item = "phase13-writer-first-item";
    let second_item = "phase13-writer-second-item";
    let server = FakeAppServer::spawn(vec![
        ProjectionStep::Fresh {
            target: first_cas_thread,
        },
        ProjectionStep::Fresh {
            target: second_cas_thread,
        },
        ProjectionStep::TurnStart {
            target: first_cas_thread,
            expected_input: INPUT,
            before_reply: vec![],
            reply: TurnStartReply::Exact {
                turn: first_cas_turn,
            },
            after_reply: vec![],
        },
        ProjectionStep::TurnStart {
            target: second_cas_thread,
            expected_input: "phase13 queued writer input",
            before_reply: vec![],
            reply: TurnStartReply::Exact {
                turn: second_cas_turn,
            },
            after_reply: vec![],
        },
    ]);
    let mut session = server.admit(execution_binding().runtime_id(), process(68));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let first = obtain(&fixture, &coordinator, &mut session, fixture.thread);
    let second = obtain(&fixture, &coordinator, &mut session, second_thread);
    let request = execution_request();

    thread::scope(|scope| {
        let first_execution = scope.spawn(|| {
            coordinator.execute_ordinary_turn(
                &fixture.store,
                fixture.storage,
                first,
                &request,
                &mut NoTools,
            )
        });
        wait_for_lifecycle(&fixture, first_submitted.turn, TurnLifecycle::Active);
        let second_execution = scope.spawn(|| {
            coordinator.execute_ordinary_turn(
                &fixture.store,
                fixture.storage,
                second,
                &request,
                &mut NoTools,
            )
        });
        wait_for_lifecycle(&fixture, second_turn, TurnLifecycle::Active);
        thread::sleep(Duration::from_millis(250));
        send_user_lifecycle(
            &server,
            first_cas_thread,
            first_cas_turn,
            "phase13-writer-first-user",
            INPUT,
        );
        wait_for_source_event_count(&fixture, first_submitted.turn, 3);
        send_user_lifecycle(
            &server,
            second_cas_thread,
            second_cas_turn,
            "phase13-writer-second-user",
            "phase13 queued writer input",
        );
        wait_for_source_event_count(&fixture, second_turn, 3);

        let first_cut = faults.block_next(FaultPoint::BeforeCommit);
        server.send_notification(
            "item/started",
            json!({
                "threadId": first_cas_thread,
                "turnId": first_cas_turn,
                "item": agent_item(first_item)
            }),
        );
        assert!(first_cut.wait_until_reached(Duration::from_secs(10)));
        server.send_notification(
            "item/started",
            json!({
                "threadId": second_cas_thread,
                "turnId": second_cas_turn,
                "item": agent_item(second_item)
            }),
        );
        thread::sleep(Duration::from_millis(100));
        first_cut.release();

        wait_for_item_count(&fixture, first_submitted.turn, 2);
        wait_for_item_count(&fixture, second_turn, 2);
        server.send_notification(
            "item/completed",
            json!({
                "threadId": first_cas_thread,
                "turnId": first_cas_turn,
                "item": agent_item(first_item)
            }),
        );
        server.send_notification(
            "item/completed",
            json!({
                "threadId": second_cas_thread,
                "turnId": second_cas_turn,
                "item": agent_item(second_item)
            }),
        );
        server.send_notification(
            "turn/completed",
            json!({
                "threadId": first_cas_thread,
                "turn": terminal_turn(first_cas_turn)
            }),
        );
        server.send_notification(
            "turn/completed",
            json!({
                "threadId": second_cas_thread,
                "turn": terminal_turn(second_cas_turn)
            }),
        );
        assert_terminal(first_execution.join().unwrap().unwrap());
        assert_terminal(second_execution.join().unwrap().unwrap());
    });
    fixture.store.validate_registered_domains().unwrap();
    server.join();
}

fn assert_active_binding(fixture: &Fixture, thread: beryl_model::SyndicThreadId) {
    let binding = fixture
        .storage
        .current_binding(&fixture.store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(binding.binding().state(), BindingState::Active(_)));
}

fn assert_terminal(outcome: OrdinaryTurnExecutionOutcome) {
    drop(terminal_projection(outcome));
}

fn terminal_projection(
    outcome: OrdinaryTurnExecutionOutcome,
) -> beryl_app::cas_projection::LoadedCasProjection {
    let OrdinaryTurnExecutionOutcome::Terminal { projection, status } = outcome else {
        panic!("expected terminal ordinary execution, got {outcome:?}")
    };
    assert_eq!(status.outcome(), TurnTerminalOutcome::Complete);
    assert_eq!(status.incomplete_reason(), None);
    *projection
}

fn terminal_turn(turn: &str) -> serde_json::Value {
    json!({ "id": turn, "status": "completed" })
}

fn send_user_lifecycle(
    server: &FakeAppServer,
    thread_id: &str,
    turn_id: &str,
    item_id: &str,
    text: &str,
) {
    let item = json!({
        "id": item_id,
        "type": "userMessage",
        "clientId": null,
        "content": [{ "type": "text", "text": text }]
    });
    for method in ["item/started", "item/completed"] {
        server.send_notification(
            method,
            json!({
                "threadId": thread_id,
                "turnId": turn_id,
                "item": item.clone()
            }),
        );
    }
}

#[cfg(feature = "test-faults")]
fn wait_for_item_count(fixture: &Fixture, turn: beryl_model::SyndicTurnId, expected: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let count = fixture
            .storage
            .turn_state(&fixture.store, turn, point_limit())
            .unwrap()
            .unwrap()
            .record()
            .item_count();
        if count == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for turn {turn} to reach {expected} items; current={count}"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(feature = "test-faults")]
fn wait_for_source_event_count(fixture: &Fixture, turn: beryl_model::SyndicTurnId, expected: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let count = fixture
            .storage
            .turn_state(&fixture.store, turn, point_limit())
            .unwrap()
            .unwrap()
            .record()
            .source_event_count();
        if count == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for turn {turn} to reach {expected} source events; current={count}"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(feature = "test-faults")]
fn agent_item(item: &str) -> serde_json::Value {
    json!({
        "id": item,
        "type": "agentMessage",
        "phase": "final_answer",
        "text": ""
    })
}
