use std::thread;

use beryl_app::cas_projection::CasProjectionCoordinator;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use serde_json::json;
use syndic_storage::{BindingState, InputGateState, SourceEventPayload, TurnLifecycle};

use crate::{
    common::{
        CAS_ITEM, CAS_THREAD, CAS_TURN, ExpectedCutState, INPUT, assert_publication_failure,
        recover_after_writer_cut, turn_server,
    },
    support::{
        NoTools, execution_request, obtain, process, source_events, turn_items, wait_for_lifecycle,
    },
    syndic::{Fixture, execution_binding, point_limit},
};

#[test]
fn live_item_start_before_commit_recovers_the_exact_prior_capture_state() {
    live_item_start_cut(74, FaultPoint::BeforeCommit, ExpectedCutState::Prior);
}

#[test]
fn live_item_start_after_persist_recovers_one_exact_source_event_and_item() {
    live_item_start_cut(75, FaultPoint::AfterPersist, ExpectedCutState::New);
}

#[test]
fn live_item_start_after_commit_before_persist_recovers_a_whole_old_or_new_state() {
    live_item_start_cut(
        80,
        FaultPoint::AfterCommitBeforePersist,
        ExpectedCutState::PriorOrNew,
    );
}

fn live_item_start_cut(seed: u8, point: FaultPoint, expected: ExpectedCutState) {
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(seed, faults.clone());
    let submitted = fixture.submit_text(INPUT);
    let server = turn_server();
    let mut session = server.admit(execution_binding().runtime_id(), process(u64::from(seed)));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);
    let request = execution_request();

    let result = thread::scope(|scope| {
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
        assert_eq!(source_events(&fixture, submitted.turn).len(), 1);
        faults.fail_next(point);
        server.send_notification(
            "item/started",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": {
                    "id": CAS_ITEM,
                    "type": "agentMessage",
                    "phase": "final_answer",
                    "text": ""
                }
            }),
        );
        execution.join().unwrap()
    });
    server.join();
    assert_publication_failure(result.expect_err("live source-event cut must fail execution"));
    recover_after_writer_cut(&fixture.store);

    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    let binding = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let events = source_events(&fixture, submitted.turn);

    assert!(matches!(
        events[0].payload(),
        SourceEventPayload::TurnActivated
    ));
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Active);
    assert!(matches!(binding.binding().state(), BindingState::Active(_)));
    assert!(matches!(
        gate.record().state(),
        InputGateState::Steerable(_)
    ));
    let actual = match state.record().source_event_count() {
        1 => ExpectedCutState::Prior,
        2 => ExpectedCutState::New,
        count => panic!("live-event cut recovered an invalid source frontier: {count}"),
    };
    expected.assert_allows(actual);
    match actual {
        ExpectedCutState::New => {
            assert_eq!(state.record().item_count(), 2);
            assert_eq!(events.len(), 2);
            assert!(matches!(
                events[1].payload(),
                SourceEventPayload::ItemStarted { item, .. }
                    if item.cas_item_id().as_str() == CAS_ITEM
            ));
            let source = events[1].source().unwrap();
            assert_eq!(source.thread_id().as_str(), CAS_THREAD);
            assert_eq!(source.turn_id().as_str(), CAS_TURN);
            assert_eq!(turn_items(&fixture, submitted.turn).len(), 2);
        }
        ExpectedCutState::Prior => {
            assert_eq!(state.record().item_count(), 1);
            assert_eq!(events.len(), 1);
            assert_eq!(turn_items(&fixture, submitted.turn).len(), 1);
        }
        ExpectedCutState::PriorOrNew => unreachable!(),
    }
    fixture.store.validate_registered_domains().unwrap();
}
