use std::thread;

use beryl_app::cas_projection::CasProjectionCoordinator;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_model::{CasThreadId, CasTurnId};
use syndic_storage::{BindingState, InputGateState, TurnLifecycle};

use crate::{
    backend::TIMEOUT,
    common::{
        CAS_THREAD, CAS_TURN, ExpectedCutState, INPUT, assert_publication_failure,
        recover_after_writer_cut, turn_server,
    },
    support::{NoTools, execution_request, obtain, process, source_events},
    syndic::{Fixture, execution_binding, point_limit},
};

#[test]
fn active_turn_identity_before_commit_recovers_the_exact_pre_identity_state() {
    active_turn_identity_cut(72, FaultPoint::BeforeCommit, ExpectedCutState::Prior);
}

#[test]
fn active_turn_identity_after_persist_recovers_the_exact_one_way_correlation() {
    active_turn_identity_cut(73, FaultPoint::AfterPersist, ExpectedCutState::New);
}

#[test]
fn active_turn_identity_after_commit_before_persist_recovers_a_whole_old_or_new_state() {
    active_turn_identity_cut(
        79,
        FaultPoint::AfterCommitBeforePersist,
        ExpectedCutState::PriorOrNew,
    );
}

fn active_turn_identity_cut(seed: u8, point: FaultPoint, expected: ExpectedCutState) {
    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(seed, faults.clone());
    let submitted = fixture.submit_text(INPUT);
    let server = turn_server();
    let mut session = server.admit(execution_binding().runtime_id(), process(u64::from(seed)));
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.store).unwrap();
    let projection = obtain(&fixture, &coordinator, &mut session, fixture.thread);
    let activation = faults.block_next(point);
    faults.fail_next(point);
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
        assert!(
            activation.wait_until_reached(TIMEOUT),
            "activation never reached {point:?}"
        );
        activation.release();
        execution.join().unwrap()
    });
    server.join();
    assert_publication_failure(result.expect_err("identity cut must fail execution"));
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
    let cas_turn_owner = fixture
        .storage
        .cas_turn_owner(
            &fixture.store,
            CasThreadId::new(CAS_THREAD).unwrap(),
            CasTurnId::new(CAS_TURN).unwrap(),
            point_limit(),
        )
        .unwrap();
    assert_eq!(state.record().lifecycle(), TurnLifecycle::Pending);
    assert_eq!(state.record().source_event_count(), 0);
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("identity cut must retain the whole activated binding")
    };
    let publication = fixture
        .storage
        .active_cas_turn(&fixture.store, active.snapshot_id(), point_limit())
        .unwrap();
    let actual = match (
        publication.as_ref(),
        cas_turn_owner.as_ref(),
        gate.record().state(),
    ) {
        (Some(_), Some(_), InputGateState::Steerable(_)) => ExpectedCutState::New,
        (None, None, InputGateState::AwaitingSteering(_)) => ExpectedCutState::Prior,
        mixed => panic!("identity cut recovered a mixed durable state: {mixed:?}"),
    };
    expected.assert_allows(actual);
    match actual {
        ExpectedCutState::New => {
            let publication = publication.unwrap();
            assert_eq!(publication.record().turn_id(), submitted.turn);
            assert_eq!(publication.record().cas_thread_id().as_str(), CAS_THREAD);
            assert_eq!(publication.record().cas_turn_id().as_str(), CAS_TURN);
            assert!(matches!(
                gate.record().state(),
                InputGateState::Steerable(target)
                    if target.pending().active_turn_id() == submitted.turn
                        && target.cas_turn_id().as_str() == CAS_TURN
            ));
        }
        ExpectedCutState::Prior => assert!(matches!(
            gate.record().state(),
            InputGateState::AwaitingSteering(target)
                if target.active_turn_id() == submitted.turn
        )),
        ExpectedCutState::PriorOrNew => unreachable!(),
    }
    assert!(source_events(&fixture, submitted.turn).is_empty());
    fixture.store.validate_registered_domains().unwrap();
}
