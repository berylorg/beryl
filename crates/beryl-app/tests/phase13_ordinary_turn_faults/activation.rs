use beryl_app::cas_projection::CasProjectionCoordinator;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use syndic_storage::{BindingState, InputGateState, TurnLifecycle};

use crate::{
    backend::{FakeAppServer, ProjectionStep},
    common::{
        CAS_THREAD, ExpectedCutState, INPUT, assert_publication_failure, recover_after_writer_cut,
    },
    support::{NoTools, execution_request, obtain, process},
    syndic::{Fixture, execution_binding, point_limit},
};

#[test]
fn activation_before_commit_recovers_the_exact_pending_prior_state() {
    activation_cut(70, FaultPoint::BeforeCommit, ExpectedCutState::Prior);
}

#[test]
fn activation_after_commit_before_persist_recovers_a_whole_old_or_new_state() {
    activation_cut(
        78,
        FaultPoint::AfterCommitBeforePersist,
        ExpectedCutState::PriorOrNew,
    );
}

#[test]
fn activation_after_persist_recovers_the_exact_active_new_state() {
    activation_cut(71, FaultPoint::AfterPersist, ExpectedCutState::New);
}

fn activation_cut(seed: u8, point: FaultPoint, expected: ExpectedCutState) {
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(seed, faults.clone());
    let submitted = fixture.submit_text(INPUT);
    let server = FakeAppServer::spawn(vec![ProjectionStep::Fresh { target: CAS_THREAD }]);
    let mut session = server.admit(execution_binding().runtime_id(), process(u64::from(seed)));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);
    faults.fail_next(point);

    let result = coordinator.execute_ordinary_turn(
        &fixture.store,
        fixture.storage,
        projection,
        &execution_request(),
        &mut NoTools,
    );
    server.join();
    assert_publication_failure(result.expect_err("activation cut must fail execution"));
    recover_after_writer_cut(&fixture.store);

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
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let actual = match binding.binding().state() {
        BindingState::Active(_) => ExpectedCutState::New,
        BindingState::Valid(_) => ExpectedCutState::Prior,
        state => panic!("activation cut recovered an invalid binding state: {state:?}"),
    };
    expected.assert_allows(actual);
    match actual {
        ExpectedCutState::New => {
            let BindingState::Active(active) = binding.binding().state() else {
                unreachable!()
            };
            assert_eq!(active.turn_id(), submitted.turn);
            assert!(
                fixture
                    .storage
                    .active_cas_turn(&fixture.store, active.snapshot_id(), point_limit())
                    .unwrap()
                    .is_none()
            );
            assert!(matches!(
                gate.record().state(),
                InputGateState::AwaitingSteering(target)
                    if target.active_turn_id() == submitted.turn
            ));
        }
        ExpectedCutState::Prior => assert_eq!(
            gate.record().state(),
            &InputGateState::PendingTurn(submitted.turn)
        ),
        ExpectedCutState::PriorOrNew => unreachable!(),
    }
    fixture.store.validate_registered_domains().unwrap();
}
