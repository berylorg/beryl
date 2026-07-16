use std::thread;

use beryl_app::cas_projection::CasProjectionCoordinator;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use serde_json::json;
use syndic_storage::{
    BindingState, ContentLifecycle, InputGateState, ItemProjectionBuildPhase,
    ItemProjectionGeneration, ProviderItemLifecycle, SourceEventPayload, TurnLifecycle,
    TurnTerminalOutcome,
};

use crate::{
    backend::TIMEOUT,
    common::{
        CAS_THREAD, CAS_TURN, ExpectedCutState, INPUT, recover_after_writer_cut, turn_server,
    },
    support::{
        NoTools, execution_request, obtain, process, source_events, turn_items, wait_for_lifecycle,
    },
    syndic::{Fixture, execution_binding, point_limit},
};

#[test]
fn terminal_convergence_before_commit_keeps_the_terminal_frontier_unadvanced() {
    terminal_convergence_cut(76, FaultPoint::BeforeCommit, ExpectedCutState::Prior);
}

#[test]
fn terminal_convergence_after_commit_before_persist_recovers_a_whole_old_or_new_state() {
    terminal_convergence_cut(
        81,
        FaultPoint::AfterCommitBeforePersist,
        ExpectedCutState::PriorOrNew,
    );
}

#[test]
fn terminal_convergence_after_persist_recovers_the_exact_first_commit() {
    terminal_convergence_cut(77, FaultPoint::AfterPersist, ExpectedCutState::New);
}

fn terminal_convergence_cut(seed: u8, point: FaultPoint, expected: ExpectedCutState) {
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(seed, faults.clone());
    let submitted = fixture.submit_text(INPUT);
    let server = turn_server();
    let mut session = server.admit(execution_binding().runtime_id(), process(u64::from(seed)));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);
    let request = execution_request();
    let first_preterminal_commit = faults.block_next(FaultPoint::AfterPersist);

    let (result, home_before_terminal) = thread::scope(|scope| {
        let execution = scope.spawn(|| {
            coordinator.execute_ordinary_turn(
                &fixture.store,
                fixture.storage,
                projection,
                &request,
                &mut NoTools,
            )
        });
        let mut next_post_persist = first_preterminal_commit;
        for stage in [
            "binding activation",
            "CAS turn identity publication",
            "TurnActivated source event",
        ] {
            assert!(
                next_post_persist.wait_until_reached(TIMEOUT),
                "ordinary execution never persisted {stage}"
            );
            let following = faults.block_next(FaultPoint::AfterPersist);
            next_post_persist.release();
            next_post_persist = following;
        }
        wait_for_lifecycle(&fixture, submitted.turn, TurnLifecycle::Active);
        assert_eq!(source_events(&fixture, submitted.turn).len(), 1);
        server.send_notification(
            "item/started",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": user_item()
            }),
        );
        assert!(
            next_post_persist.wait_until_reached(TIMEOUT),
            "provider user-message start never reached its post-persist cut"
        );
        let following = faults.block_next(FaultPoint::AfterPersist);
        next_post_persist.release();
        next_post_persist = following;
        server.send_notification(
            "item/completed",
            json!({
                "threadId": CAS_THREAD,
                "turnId": CAS_TURN,
                "item": user_item()
            }),
        );
        assert!(
            next_post_persist.wait_until_reached(TIMEOUT),
            "provider user-message completion never reached its post-persist cut"
        );
        let following = faults.block_next(FaultPoint::AfterPersist);
        next_post_persist.release();
        next_post_persist = following;
        assert_eq!(source_events(&fixture, submitted.turn).len(), 3);
        let home_before_terminal = fixture.store.home_revision().unwrap().get();
        server.send_notification(
            "turn/completed",
            json!({
                "threadId": CAS_THREAD,
                "turn": {
                    "id": CAS_TURN,
                    "status": "completed"
                }
            }),
        );
        assert!(
            next_post_persist.wait_until_reached(TIMEOUT),
            "terminal source event never reached its post-persist cut"
        );
        faults.fail_next(point);
        next_post_persist.release();
        (execution.join().unwrap(), home_before_terminal)
    });
    server.join();
    let _error = result.expect_err("terminal convergence cut must fail execution");
    recover_after_writer_cut(&fixture.store);
    let actual = match fixture.store.home_revision().unwrap().get() - home_before_terminal {
        1 => ExpectedCutState::Prior,
        2 => ExpectedCutState::New,
        commits => panic!("terminal convergence cut recovered {commits} commits"),
    };
    expected.assert_allows(actual);

    let state = fixture
        .storage
        .turn_state(&fixture.store, submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Complete);
    assert_eq!(state.record().source_event_count(), 4);
    assert_eq!(state.record().item_count(), 1);
    assert_eq!(state.record().finalized_item_count(), 0);
    let events = source_events(&fixture, submitted.turn);
    assert!(matches!(
        events[0].payload(),
        SourceEventPayload::TurnActivated
    ));
    assert!(matches!(
        events[3].payload(),
        SourceEventPayload::TurnEnded(status)
            if status.outcome() == TurnTerminalOutcome::Complete
                && status.incomplete_reason().is_none()
    ));
    assert!(events[3].source().is_some());

    let binding = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(valid) = binding.binding().state() else {
        panic!("proven terminal evidence must advance the whole valid binding")
    };
    assert_eq!(valid.represented_prefix().tail(), Some(submitted.turn));
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().state(), &InputGateState::Idle);

    let user_item = turn_items(&fixture, submitted.turn)[0].item_id();
    let item = fixture
        .storage
        .canonical_item(&fixture.store, user_item, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        item.record().provider_lifecycle(),
        ProviderItemLifecycle::Completed
    );
    let manifest = fixture
        .storage
        .content_manifest(
            &fixture.store,
            item.record()
                .payload()
                .content()
                .expect("user fixture must own content")
                .id(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(manifest.record().lifecycle(), ContentLifecycle::Sealed);
    let build = fixture
        .storage
        .item_projection_build(
            &fixture.store,
            user_item,
            ItemProjectionGeneration::FIRST,
            point_limit(),
        )
        .unwrap();
    match actual {
        ExpectedCutState::New => assert!(matches!(
            build.unwrap().record().phase(),
            ItemProjectionBuildPhase::Parsing(_)
        )),
        ExpectedCutState::Prior => assert!(build.is_none()),
        ExpectedCutState::PriorOrNew => unreachable!(),
    }
    fixture.store.validate_registered_domains().unwrap();
}

fn user_item() -> serde_json::Value {
    json!({
        "id": "phase13-fault-user-item",
        "type": "userMessage",
        "clientId": null,
        "content": [{ "type": "text", "text": INPUT }]
    })
}
