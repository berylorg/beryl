use syndic_storage::test_faults::{
    AwaitingTerminalPredecessorFamily, FixtureBatch, FixtureDelete, FixtureRecord,
    inject_awaiting_terminal_predecessor,
};

use super::*;

#[test]
fn immediate_predecessor_record_versions_have_no_compatibility_decoder() {
    for (name, family) in [
        ("gate-v3", AwaitingTerminalPredecessorFamily::InputGateV3),
        (
            "leaf-v3",
            AwaitingTerminalPredecessorFamily::AcceptedRouteLeafV3,
        ),
        (
            "route-v2",
            AwaitingTerminalPredecessorFamily::AcceptedRouteGenerationV2,
        ),
    ] {
        let home = TestHome::new(&format!("phase65-awaiting-terminal-retired-{name}"));
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        inject_awaiting_terminal_predecessor(&store, storage, family).unwrap();
        store.close().unwrap();

        let mut reopened = open(home.path());
        SyndicStorage::register(&mut reopened).unwrap();
        assert!(
            reopened
                .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
                .is_err()
        );
        reopened.close().unwrap();
    }
}

#[test]
fn awaiting_terminal_gate_rejects_a_steerable_state_mismatch() {
    let fixture = active_fixture("phase65-awaiting-terminal-corrupt-gate");
    accept_text(&fixture, "queued", draft_id(43), 6);
    admit_unknown(&fixture, 8);
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let corrupt = InputGateRecord::new(
        gate.thread_id(),
        gate.revision(),
        InputGateState::Steerable(fixture.turn),
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    let mut mutation = FixtureBatch::new();
    mutation.put(FixtureRecord::InputGate(corrupt)).unwrap();
    commit(&fixture.store, fixture.storage, mutation);

    assert!(
        fixture
            .store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
}

#[test]
fn awaiting_terminal_rejects_a_half_transition_that_kept_the_ready_source() {
    let fixture = active_fixture("phase65-awaiting-terminal-corrupt-ready");
    accept_text(&fixture, "queued", draft_id(43), 6);
    admit_unknown(&fixture, 8);
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let proof = gate.selected_route().unwrap();
    let ready = AcceptedReadySourceRecord::new(
        fixture.thread,
        gate.revision(),
        proof.generation(),
        proof.revision(),
        AcceptedInputOrdinal::FIRST,
        AcceptedInputOrdinal::FIRST,
    );
    let mut mutation = FixtureBatch::new();
    mutation
        .put(FixtureRecord::AcceptedReadySource(ready))
        .unwrap();
    commit(&fixture.store, fixture.storage, mutation);

    assert!(
        fixture
            .store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
}

#[test]
fn awaiting_terminal_rejects_a_half_transition_missing_the_next_source() {
    let fixture = active_fixture("phase65-awaiting-terminal-corrupt-next");
    accept_text(&fixture, "queued", draft_id(43), 6);
    admit_unknown(&fixture, 8);
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let proof = gate.selected_route().unwrap();
    let mut mutation = FixtureBatch::new();
    mutation
        .delete(FixtureDelete::AcceptedNextSource {
            thread: fixture.thread,
            generation: proof.generation(),
        })
        .unwrap();
    commit(&fixture.store, fixture.storage, mutation);

    assert!(
        fixture
            .store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
}
