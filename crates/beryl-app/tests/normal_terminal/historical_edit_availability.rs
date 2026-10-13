use super::*;
use beryl_app::cas_projection::HistoricalEditUnavailable;
use syndic_storage::{
    CompactionOperationNonce, InputGateRecord, InputGateState,
    test_faults::{FixtureBatch, FixtureRecord},
};

#[test]
fn replacement_edit_refuses_running_queued_and_compacting_gates_while_submission_is_available() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new(158);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn_projection_only();
    let (session, projection) = ordinary_custody::prepare(&fixture, &server, true);
    let original = fixture
        .storage
        .input_gate(&fixture.home(), fixture.thread, syndic::point_limit())
        .unwrap()
        .unwrap();
    let worker = fixture.store.exact_stop_worker();
    for (state, queued, expected) in [
        (InputGateState::Idle, 0, Ok(())),
        (
            InputGateState::Steerable(submitted.turn),
            0,
            Err(HistoricalEditUnavailable::TurnRunning),
        ),
        (
            InputGateState::Idle,
            1,
            Err(HistoricalEditUnavailable::QueuedInput),
        ),
        (
            InputGateState::compacting(
                submitted.turn,
                CompactionOperationNonce::from_bytes([211; 16]),
            ),
            0,
            Err(HistoricalEditUnavailable::CompactionRunning),
        ),
    ] {
        let home = fixture.home();
        let gate = fixture
            .storage
            .input_gate(&home, fixture.thread, syndic::point_limit())
            .unwrap()
            .unwrap();
        let updated = InputGateRecord::new(
            fixture.thread,
            gate.revision().checked_next().unwrap(),
            state,
            queued,
            gate.route_generation_high_water(),
            gate.selected_route(),
            0,
            queued,
            queued,
        )
        .unwrap();
        replace_gate(&fixture, updated);
        assert_eq!(
            worker.selected_submission_availability(fixture.thread),
            Ok(())
        );
        let before = home.home_revision().unwrap();
        assert_eq!(
            worker.selected_historical_edit_availability(fixture.thread),
            expected
        );
        assert_eq!(home.home_revision().unwrap(), before);
    }
    replace_gate(&fixture, original);
    projection.release().unwrap();
    ordinary_custody::close(fixture, session, None, server);
}

fn replace_gate(fixture: &Fixture, gate: InputGateRecord) {
    let home = fixture.home();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::InputGate(gate)).unwrap();
    let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
    command
        .add(
            fixture
                .storage
                .fixture_contribution(fixture.storage.revision(&home).unwrap(), batch),
        )
        .unwrap();
    assert!(matches!(
        home.execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}
